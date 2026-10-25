/* phase-430 W1 — the C timer verb takes a clock, and the clock DECIDES.
 *
 * The clock-less init never took one, so a C caller could not make a ROS-time
 * timer at all while our own C++ `Node::create_timer` could. phase-430 W1
 * added a clock-taking verb with the clock where rcl puts it; phase-515 C1 gave
 * it rcl's name and contract, `rcl_timer_init`, and the clock-less verb rclc's
 * `rclc_timer_init_default`.
 *
 * phase-515 C1 also pins rcl's CALLBACK contract, `(timer, last_call_time)`:
 * the second argument is the time since the previous call (or the reset), and
 * the timer pointer is the caller's own, so `RCLC_CONTAINER_OF` reaches the
 * state the timer is embedded in.
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

#include <rclc/rclc.h>

#include <stdbool.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>

/* The registration hook every C image owes `rclc_support_init` — there is no
 * weak default on the C path (`nros_cpp_init` has the C++ one). This is where a
 * real image names its backend; here it names the stub. */
void nros_app_register_backends(void);
void nros_app_register_backends(void) {
    (void)nros_stub_rmw_register();
}

/* ---- Timer callbacks ---------------------------------------------------- */

static unsigned s_ros_fires = 0;
static unsigned s_wall_fires = 0;

static void ros_timer_cb(struct rcl_timer_t* timer, int64_t last_call_time) {
    (void)timer;
    (void)last_call_time;
    s_ros_fires++;
}

static void wall_timer_cb(struct rcl_timer_t* timer, int64_t last_call_time) {
    (void)timer;
    (void)last_call_time;
    s_wall_fires++;
}

/* issue 1974 — the call-info flavour and the autostart flag. */
static unsigned s_info_fires = 0;
static struct rcl_timer_call_info_t s_last_info;
static unsigned s_late_fires = 0;
static unsigned s_group_fires = 0;

static void info_timer_cb(struct rcl_timer_t* timer, const struct rcl_timer_call_info_t* info) {
    (void)timer;
    s_last_info = *info;
    s_info_fires++;
}

static int64_t s_late_last_call_time = -1;

static void late_timer_cb(struct rcl_timer_t* timer, int64_t last_call_time) {
    (void)timer;
    s_late_last_call_time = last_call_time;
    s_late_fires++;
}

/* phase-515 C1 — state reached through the timer, not a context pointer. The
 * timer is deliberately NOT the first member, so a callback that cast the
 * timer pointer straight to the struct would read the wrong bytes. */
typedef struct {
    uint32_t marker;
    struct rcl_timer_t timer;
    unsigned fires;
} grouped_state_t;

static grouped_state_t s_group = {.marker = 0x5eed5eedu};

static void group_timer_cb(struct rcl_timer_t* timer, int64_t last_call_time) {
    (void)last_call_time;
    grouped_state_t* self = RCLC_CONTAINER_OF(timer, grouped_state_t, timer);
    if (self == &s_group && self->marker == 0x5eed5eedu) {
        self->fires++;
    }
    s_group_fires++;
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
        rcl_ret_t _r = (expr);                                                                     \
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
static void spin_n(struct rclc_executor_t* executor, int n) {
    for (int i = 0; i < n; i++) {
        (void)rclc_executor_spin_some(executor, 1000000ull);
    }
}

int main(void) {
    /* The stub must be the backend this image opens against. `$NROS_RMW` is the
     * hosted rung of precedence model A and beats the baked selector, so set it
     * rather than hoping the ambient environment names nothing. */
    setenv("NROS_RMW", NROS_STUB_RMW_NAME, 1);

    struct rclc_support_t support = rclc_support_get_zero_initialized();
    CHECK_RET(rclc_support_init_rmw(&support, "stub://none", 42, "timer_clock_source",
                                    NROS_STUB_RMW_NAME),
              RCL_RET_OK, "support opened on the stub backend");

    struct rclc_executor_t executor = rclc_executor_get_zero_initialized_executor();
    CHECK_RET(rclc_executor_init(&executor, &support.context, 8, NULL), RCL_RET_OK,
              "executor initialised");

    /* Two clocks, and the simulated one is STOPPED at a fixed instant before
     * either timer exists — the state a paused simulator leaves behind. */
    struct rcl_clock_t ros_clock = rclc_clock_get_zero_initialized();
    struct rcl_clock_t steady_clock = rclc_clock_get_zero_initialized();
    CHECK_RET(rcl_clock_init(RCL_ROS_TIME, &ros_clock, NULL), RCL_RET_OK, "ROS clock init");
    CHECK_RET(rcl_clock_init(RCL_STEADY_TIME, &steady_clock, NULL), RCL_RET_OK,
              "steady clock init");

    const int64_t sim_epoch_ns = 1000000000ll; /* 1 s, arbitrary and frozen */
    CHECK_RET(rcl_set_ros_time_override(&ros_clock, sim_epoch_ns), RCL_RET_OK,
              "the simulated clock is stopped at a known instant");
    bool enabled = false;
    CHECK_RET(rcl_is_enabled_ros_time_override(&ros_clock, &enabled), RCL_RET_OK,
              "override state readable");
    CHECK(enabled, "the ROS time override is in effect");

    /* The verb under test, and the clock-less verb beside it. Both 1 ms, so the
     * wall timer's period is short against the spin loop below. */
    struct rcl_timer_t ros_timer = rcl_get_zero_initialized_timer();
    struct rcl_timer_t wall_timer = rcl_get_zero_initialized_timer();
    CHECK_RET(rcl_timer_init(&ros_timer, &ros_clock, &support.context, 1000000ll, ros_timer_cb,
                             rcl_get_default_allocator()),
              RCL_RET_OK, "ROS-time timer created on the ROS clock");
    CHECK_RET(rclc_timer_init_default(&wall_timer, NULL, 1000000ull, wall_timer_cb), RCL_RET_OK,
              "the pre-existing clock-less verb still creates a wall timer");
    CHECK(ros_timer.clock == &ros_clock, "the ROS timer kept its clock, as rcl's does");
    CHECK(wall_timer.clock == NULL, "the wall timer has no clock");

    CHECK_RET(rclc_executor_add_timer(&executor, &ros_timer), RCL_RET_OK, "ROS timer registered");
    CHECK_RET(rclc_executor_add_timer(&executor, &wall_timer), RCL_RET_OK, "wall timer registered");

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
    CHECK_RET(rcl_set_ros_time_override(&ros_clock, sim_epoch_ns + 100000000ll), RCL_RET_OK,
              "the simulated clock advances 100 ms");
    spin_n(&executor, 5);
    CHECK(s_ros_fires > 0, "a ROS-time timer fires when the simulated clock advances");

    /* And the advance was not what drove the wall timer: it was already
     * running, and it does not care about /clock either way. */
    CHECK(s_wall_fires >= wall_before, "the wall timer is unaffected by the simulated clock");

    /* --- 3. The refusals the verb owes its caller. ------------------------- */
    struct rcl_timer_t rejected = rcl_get_zero_initialized_timer();
    struct rcl_clock_t uninit_clock = rclc_clock_get_zero_initialized();
    CHECK_RET(rcl_timer_init(&rejected, &uninit_clock, NULL, 1000000ll, ros_timer_cb,
                             rcl_get_default_allocator()),
              RCL_RET_NOT_INIT,
              "a clock the caller never initialised is REFUSED, not treated as a wall clock");
    CHECK_RET(
        rcl_timer_init(&rejected, NULL, NULL, 1000000ll, ros_timer_cb, rcl_get_default_allocator()),
        RCL_RET_INVALID_ARGUMENT,
        "the clock-taking verb requires a clock; the clock-less form is "
        "rclc_timer_init_default");
    CHECK_RET(rcl_timer_init(&rejected, &ros_clock, NULL, -1ll, ros_timer_cb,
                             rcl_get_default_allocator()),
              RCL_RET_INVALID_ARGUMENT, "a negative period is refused");
    struct rcl_allocator_t foreign = rcl_get_default_allocator();
    foreign.allocate = NULL;
    CHECK_RET(rcl_timer_init(&rejected, &ros_clock, NULL, 1000000ll, ros_timer_cb, foreign),
              RCL_RET_INVALID_ARGUMENT,
              "a caller-supplied allocator is refused rather than silently unused");
    CHECK(rejected.state == RCLC_TIMER_STATE_UNINITIALIZED, "a refused init left the timer alone");

    /* --- 4. issue 1974: call info, autostart, and a grouped clock timer. ---
     * All on the ROS clock, so every time is the override this test writes and
     * every assertion is an equality. The two timers above are removed first:
     * the builtin executor holds four handles (NROS_EXECUTOR_MAX_HANDLES). */
    CHECK_RET(rclc_executor_remove_timer(&executor, &ros_timer), RCL_RET_OK, "ROS timer removed");
    CHECK_RET(rclc_executor_remove_timer(&executor, &wall_timer), RCL_RET_OK, "wall timer removed");
    const int64_t t0 = sim_epoch_ns + 1000000000ll; /* 2 s */
    CHECK_RET(rcl_set_ros_time_override(&ros_clock, t0), RCL_RET_OK, "freeze /clock at t0");

    struct rcl_timer_t info_timer = rcl_get_zero_initialized_timer();
    CHECK_RET(rclc_timer_init_with_info(&info_timer, &ros_clock, 10000000ull, info_timer_cb, true),
              RCL_RET_OK, "a call-info timer on the ROS clock");
    CHECK_RET(rclc_executor_add_timer(&executor, &info_timer), RCL_RET_OK,
              "call-info timer registered");

    struct rcl_timer_t late_timer = rcl_get_zero_initialized_timer();
    CHECK_RET(rcl_timer_init2(&late_timer, &ros_clock, &support.context, 10000000ll, late_timer_cb,
                              rcl_get_default_allocator(), false),
              RCL_RET_OK, "autostart=false timer on the ROS clock");
    CHECK(late_timer.state == RCLC_TIMER_STATE_CANCELED, "autostart=false is created CANCELLED");
    CHECK_RET(rclc_executor_add_timer(&executor, &late_timer), RCL_RET_OK,
              "a cancelled timer can be added; it is registered cancelled");
    bool canceled = false;
    CHECK_RET(rcl_timer_is_canceled(&late_timer, &canceled), RCL_RET_OK, "is_canceled readable");
    CHECK(canceled, "the registered timer is cancelled until reset");

    /* The class fix: the in-group verb used to drop the clock and register a
     * WALL timer, so this one would fire on every spin while /clock is frozen. */
    s_group.timer = rcl_get_zero_initialized_timer();
    CHECK_RET(rcl_timer_init(&s_group.timer, &ros_clock, &support.context, 1000000ll,
                             group_timer_cb, rcl_get_default_allocator()),
              RCL_RET_OK, "grouped ROS-time timer");
    CHECK_RET(rclc_executor_add_timer_in_group(&executor, &s_group.timer, "ctrl"), RCL_RET_OK,
              "grouped timer registered");

    spin_n(&executor, 50);
    CHECK(s_info_fires == 0, "the call-info timer waits for /clock");
    CHECK(s_late_fires == 0, "a cancelled timer does not fire");
    CHECK(s_group_fires == 0, "a grouped ROS-time timer follows the ROS clock, not the wall");

    /* 35 ms of simulated time against a 10 ms period: due at t0 + 10 ms, run
     * at t0 + 35 ms. */
    CHECK_RET(rcl_set_ros_time_override(&ros_clock, t0 + 35000000ll), RCL_RET_OK, "/clock +35 ms");
    spin_n(&executor, 3);
    CHECK(s_info_fires == 1, "the call-info timer fired once (Skip coalesces the backlog)");
    CHECK(s_last_info.actual_call_time == t0 + 35000000ll,
          "actual_call_time is now on the timer's ROS clock");
    CHECK(s_last_info.expected_call_time == t0 + 10000000ll,
          "expected_call_time is the deadline it was serving");
    CHECK(s_last_info.actual_call_time >= s_last_info.expected_call_time,
          "an activation never runs before its deadline");
    CHECK(s_late_fires == 0, "still cancelled after /clock moved");
    CHECK(s_group_fires > 0, "the grouped ROS-time timer fires when /clock advances");
    CHECK(s_group.fires == s_group_fires,
          "RCLC_CONTAINER_OF reached the struct embedding the timer on every call");

    /* reset starts it, and the first period counts from the reset — not from
     * creation, which would make it fire at once. */
    CHECK_RET(rcl_timer_reset(&late_timer), RCL_RET_OK, "reset starts the timer");
    spin_n(&executor, 3);
    CHECK(s_late_fires == 0, "the first period counts from the reset");
    CHECK_RET(rcl_set_ros_time_override(&ros_clock, t0 + 45000000ll), RCL_RET_OK, "/clock +10 ms");
    spin_n(&executor, 3);
    CHECK(s_late_fires == 1, "fires one period after the reset");
    CHECK(s_late_last_call_time == 10000000ll,
          "rcl's last_call_time: the 10 ms since the reset, not the 45 ms since creation");
    if (s_failures != 0) {
        fprintf(stderr, "       info=%u late=%u group=%u expected=%lld actual=%lld\n", s_info_fires,
                s_late_fires, s_group_fires, (long long)s_last_info.expected_call_time,
                (long long)s_last_info.actual_call_time);
    }

    CHECK_RET(rclc_timer_init_with_info(&rejected, NULL, 1000000ull, NULL, true),
              RCL_RET_INVALID_ARGUMENT, "a call-info timer needs a callback");

    CHECK(nros_stub_rmw_drive_io_calls() > 0, "the stub backend was actually driven");

    if (s_failures != 0) {
        fprintf(stderr, "timer_clock_source: %d failure(s)\n", s_failures);
        return 1;
    }
    printf("timer_clock_source: OK (ros=%u wall=%u drive_io=%u)\n", s_ros_fires, s_wall_fires,
           nros_stub_rmw_drive_io_calls());
    return 0;
}
