/* phase-430 W1 — the C timer verb takes a clock, and the clock DECIDES.
 *
 * `nros_timer_init` never took one, so a C caller could not make a ROS-time
 * timer at all while `rcl_timer_init` and our own C++ `Node::create_timer`
 * both could. `nros_timer_init_on_clock` closes that, taking its
 * `nros_clock_t *` in the position `rcl_timer_init` puts it.
 *
 * This is a compile-AND-RUN probe rather than a signature probe because the
 * defect it pins is a BEHAVIOUR: a clock argument that is accepted, stored and
 * then ignored would compile, link and read as a ROS-time timer while running
 * on the wall — RFC-0089's "compiles and differs". So the two timers are
 * created side by side and the simulated clock is held still:
 *
 *   * the ROS-time timer must NOT fire while `/clock` is stopped, and
 *   * the wall timer beside it must keep firing, and
 *   * the ROS-time timer must fire once the simulated clock advances,
 *     at whatever wall rate the advance happens.
 *
 * No `sim-time` feature is needed and none is used. `sim-time` gates the
 * `/clock` SUBSCRIPTION (the message crate and the topic), which is one way to
 * drive the override; the override itself is `nros_core::Clock`'s and its C
 * face is rcl's own `rcl_set_ros_time_override`, which nros-c has exported
 * unconditionally since issue 0789. A bag player driving `/clock` and this
 * test drive the same global, so what is asserted here is what a simulator
 * gets.
 *
 * The RMW is `stub_rmw_backend.c`: a timer needs an executor, an executor
 * needs a session, and a session needs a backend — but nothing here publishes
 * or subscribes. That keeps the probe in `just check c`, with no router, no
 * agent and no network.
 */

#include "stub_rmw_backend.h"

#include <nros/nros.h>

#include <stdbool.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>

/* The registration hook every C image owes `nros_support_init` — there is no
 * weak default on the C path (`nros_cpp_init` has the C++ one). This is where a
 * real image names its backend; here it names the stub. */
void nros_app_register_backends(void);
void nros_app_register_backends(void) {
    (void)nros_stub_rmw_register();
}

/* ---- Timer callbacks ---------------------------------------------------- */

static unsigned s_ros_fires = 0;
static unsigned s_wall_fires = 0;

static void ros_timer_cb(struct nros_timer_t* timer, void* context) {
    (void)timer;
    (void)context;
    s_ros_fires++;
}

static void wall_timer_cb(struct nros_timer_t* timer, void* context) {
    (void)timer;
    (void)context;
    s_wall_fires++;
}

/* ---- Assertions --------------------------------------------------------
 *
 * Hand-rolled rather than <assert.h>: NDEBUG would compile the whole probe
 * away and it would still exit 0, which is issue 0196's class one level down.
 */

static int s_failures = 0;

#define CHECK(cond, msg)                                                                           \
    do {                                                                                           \
        if (!(cond)) {                                                                             \
            fprintf(stderr, "FAIL: %s (%s:%d)\n", (msg), __FILE__, __LINE__);                      \
            s_failures++;                                                                          \
        }                                                                                          \
    } while (0)

#define CHECK_RET(expr, expected, msg)                                                             \
    do {                                                                                           \
        nros_ret_t _r = (expr);                                                                    \
        if (_r != (expected)) {                                                                    \
            fprintf(stderr, "FAIL: %s -- got %d, expected %d (%s:%d)\n", (msg), (int)_r,           \
                    (int)(expected), __FILE__, __LINE__);                                          \
            s_failures++;                                                                          \
        }                                                                                          \
    } while (0)

/* Spin `n` times with a 1 ms budget each. Long enough for the wall timer's 1 ms
 * period to elapse repeatedly; the return is ignored because a spin that
 * dispatched nothing answers TIMEOUT, which is a legitimate outcome here — it
 * is exactly what the paused ROS timer produces. */
static void spin_n(struct nros_executor_t* executor, int n) {
    for (int i = 0; i < n; i++) {
        (void)rclc_executor_spin_some(executor, 1000000ull);
    }
}

int main(void) {
    /* The stub must be the backend this image opens against. `$NROS_RMW` is the
     * hosted rung of precedence model A and beats the baked selector, so set it
     * rather than hoping the ambient environment names nothing. */
    setenv("NROS_RMW", NROS_STUB_RMW_NAME, 1);

    struct nros_support_t support = nros_support_get_zero_initialized();
    CHECK_RET(nros_support_init_rmw(&support, "stub://none", 42, "timer_clock_source",
                                    NROS_STUB_RMW_NAME),
              NROS_RET_OK, "support opened on the stub backend");

    struct nros_executor_t executor = rclc_executor_get_zero_initialized_executor();
    CHECK_RET(nros_executor_init(&executor, &support, 8), NROS_RET_OK, "executor initialised");

    /* Two clocks, and the simulated one is STOPPED at a fixed instant before
     * either timer exists — the state a paused simulator leaves behind. */
    struct nros_clock_t ros_clock = nros_clock_get_zero_initialized();
    struct nros_clock_t steady_clock = nros_clock_get_zero_initialized();
    CHECK_RET(nros_clock_init(&ros_clock, NROS_CLOCK_ROS_TIME), NROS_RET_OK, "ROS clock init");
    CHECK_RET(nros_clock_init(&steady_clock, NROS_CLOCK_STEADY_TIME), NROS_RET_OK,
              "steady clock init");

    const int64_t sim_epoch_ns = 1000000000ll; /* 1 s, arbitrary and frozen */
    CHECK_RET(rcl_set_ros_time_override(&ros_clock, sim_epoch_ns), NROS_RET_OK,
              "the simulated clock is stopped at a known instant");
    bool enabled = false;
    CHECK_RET(rcl_is_enabled_ros_time_override(&ros_clock, &enabled), NROS_RET_OK,
              "override state readable");
    CHECK(enabled, "the ROS time override is in effect");

    /* The verb under test, and the clock-less verb beside it. Both 1 ms, so the
     * wall timer's period is short against the spin loop below. */
    struct nros_timer_t ros_timer = rcl_get_zero_initialized_timer();
    struct nros_timer_t wall_timer = rcl_get_zero_initialized_timer();
    CHECK_RET(
        nros_timer_init_on_clock(&ros_timer, &ros_clock, &support, 1000000ull, ros_timer_cb, NULL),
        NROS_RET_OK, "ROS-time timer created on the ROS clock");
    CHECK_RET(nros_timer_init(&wall_timer, &support, 1000000ull, wall_timer_cb, NULL), NROS_RET_OK,
              "the pre-existing clock-less verb still creates a wall timer");
    CHECK(ros_timer.clock == &ros_clock, "the ROS timer kept its clock, as rcl's does");
    CHECK(wall_timer.clock == NULL, "the wall timer has no clock");

    CHECK_RET(rclc_executor_add_timer(&executor, &ros_timer), NROS_RET_OK, "ROS timer registered");
    CHECK_RET(rclc_executor_add_timer(&executor, &wall_timer), NROS_RET_OK,
              "wall timer registered");

    /* --- 1. The simulator is stopped. -------------------------------------
     * Real wall time passes (200 spins with a 1 ms budget each), so the wall
     * timer fires repeatedly; the ROS-time timer must not fire at all. */
    spin_n(&executor, 200);
    CHECK(s_ros_fires == 0, "a ROS-time timer must NOT fire while the simulated clock is stopped");
    CHECK(s_wall_fires > 0, "the wall timer beside it keeps its cadence");
    if (s_ros_fires != 0 || s_wall_fires == 0) {
        fprintf(stderr, "       ros=%u wall=%u after 200 spins with /clock stopped\n", s_ros_fires,
                s_wall_fires);
    }

    /* --- 2. The simulator advances. ---------------------------------------
     * 100 ms of SIMULATED time in one jump, against a 1 ms period. The default
     * overrun policy is Skip, so a backlog coalesces into ONE activation —
     * this asserts "it fires", not a count, which is phase-425's documented
     * behaviour and not a resolution claim. */
    const unsigned wall_before = s_wall_fires;
    CHECK_RET(rcl_set_ros_time_override(&ros_clock, sim_epoch_ns + 100000000ll), NROS_RET_OK,
              "the simulated clock advances 100 ms");
    spin_n(&executor, 5);
    CHECK(s_ros_fires > 0, "a ROS-time timer fires when the simulated clock advances");

    /* And the advance was not what drove the wall timer: it was already
     * running, and it does not care about /clock either way. */
    CHECK(s_wall_fires >= wall_before, "the wall timer is unaffected by the simulated clock");

    /* --- 3. The refusals the verb owes its caller. ------------------------- */
    struct nros_timer_t rejected = rcl_get_zero_initialized_timer();
    struct nros_clock_t uninit_clock = nros_clock_get_zero_initialized();
    CHECK_RET(nros_timer_init_on_clock(&rejected, &uninit_clock, &support, 1000000ull, ros_timer_cb,
                                       NULL),
              NROS_RET_NOT_INIT,
              "a clock the caller never initialised is REFUSED, not treated as a wall clock");
    CHECK_RET(nros_timer_init_on_clock(&rejected, NULL, &support, 1000000ull, ros_timer_cb, NULL),
              NROS_RET_INVALID_ARGUMENT,
              "the clock-taking verb requires a clock; the clock-less form is nros_timer_init");
    CHECK(rejected.state == NROS_TIMER_STATE_UNINITIALIZED, "a refused init left the timer alone");

    CHECK(nros_stub_rmw_drive_io_calls() > 0, "the stub backend was actually driven");

    if (s_failures != 0) {
        fprintf(stderr, "timer_clock_source: %d failure(s)\n", s_failures);
        return 1;
    }
    printf("timer_clock_source: OK (ros=%u wall=%u drive_io=%u)\n", s_ros_fires, s_wall_fires,
           nros_stub_rmw_drive_io_calls());
    return 0;
}
