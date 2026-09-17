/* phase-417 W4.e — a C guard condition is created on the NODE, in one call,
 * with its callback bound at creation; a trigger from ANOTHER THREAD reaches
 * the executor.
 *
 * A compile-and-run probe, not a signature probe, because every shape of this
 * defect compiles. The shape it replaced was three calls —
 * `nros_guard_condition_init(guard, support)`, `_set_callback(...)`,
 * `nros_executor_add_guard_condition(exec, guard)` — and the first two produced
 * an object that was INERT: nothing reached the executor's arena until the
 * third ran, so `handle_id` stayed `SIZE_MAX` and a trigger landed on a local
 * byte no executor watches. The one in-tree C caller did the first two and
 * never the third, for four phases, and it read like working code. A header
 * check cannot see any of that: `nros_ret_t` is `NROS_RET_OK` at every step.
 *
 * So this asserts BEHAVIOUR, in four parts:
 *
 *   1. creation REGISTERS — `handle_id != SIZE_MAX` and the executor's handle
 *      count went up by one. That is the whole of defect 1.
 *   2. a trigger from another thread runs the callback ON THE EXECUTOR'S
 *      THREAD, with the context the creation call bound.
 *   3. the trigger travels through the ARENA, not the local fallback byte:
 *      `nros_guard_condition_is_triggered` sees the arena flag set before the
 *      spin and cleared by the dispatch that consumed it.
 *   4. the trigger WAKES a spin that would otherwise sleep out its budget.
 *      Measured against a negative control — an identical spin with nothing
 *      pending — because "returned quickly" means nothing without the number a
 *      sleeping spin costs.
 *
 * Both trigger spellings are exercised: `nros_guard_condition_trigger`, which
 * did not exist until this item (the ledger, the crate's docs and the
 * custom-platform README all named it while only rcl's verb-first spelling was
 * exported), and rcl's `rcl_trigger_guard_condition`, which forwards to it.
 *
 * The RMW is `stub_rmw_backend.c`: an executor needs a session and a session
 * needs a backend, but nothing here publishes. That keeps the probe in
 * `just check c` with no router, no agent and no network.
 */

#include "stub_rmw_backend.h"

#include <nros/nros.h>

#include <pthread.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <time.h>

/* The registration hook every C image owes `nros_support_init` — there is no
 * weak default on the C path. */
void nros_app_register_backends(void);
void nros_app_register_backends(void) {
    (void)nros_stub_rmw_register();
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

/* ---- The callback ------------------------------------------------------- */

/* The context the creation call binds. A wrong or dropped context reads as a
 * zeroed struct here rather than as a plausible one. */
typedef struct {
    unsigned magic;
    unsigned fires;
    pthread_t ran_on;
} guard_ctx_t;

#define GUARD_CTX_MAGIC 0x6d61676eu

static guard_ctx_t s_ctx = {GUARD_CTX_MAGIC, 0, 0};

static void on_guard(void* context) {
    guard_ctx_t* ctx = (guard_ctx_t*)context;
    if (ctx == NULL) return;
    ctx->fires++;
    ctx->ran_on = pthread_self();
}

/* ---- Timing ------------------------------------------------------------- */

static uint64_t now_ms(void) {
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return (uint64_t)ts.tv_sec * 1000u + (uint64_t)(ts.tv_nsec / 1000000L);
}

static void sleep_ms(unsigned ms) {
    struct timespec req;
    req.tv_sec = (time_t)(ms / 1000u);
    req.tv_nsec = (long)(ms % 1000u) * 1000000L;
    (void)nanosleep(&req, NULL);
}

/* ---- The other thread --------------------------------------------------- */

static struct nros_guard_condition_t s_guard;
static unsigned s_trigger_delay_ms = 0;
static bool s_trigger_uses_rcl_spelling = false;

static void* trigger_thread(void* unused) {
    (void)unused;
    if (s_trigger_delay_ms > 0) sleep_ms(s_trigger_delay_ms);
    if (s_trigger_uses_rcl_spelling) {
        (void)rcl_trigger_guard_condition(&s_guard);
    } else {
        (void)nros_guard_condition_trigger(&s_guard);
    }
    return NULL;
}

int main(void) {
    /* The stub must be the backend this image opens against. `$NROS_RMW` is the
     * hosted rung of precedence model A and beats the baked selector. */
    setenv("NROS_RMW", NROS_STUB_RMW_NAME, 1);

    struct nros_support_t support = nros_support_get_zero_initialized();
    CHECK_RET(nros_support_init_rmw(&support, "stub://none", 43, "node_guard_condition",
                                    NROS_STUB_RMW_NAME),
              NROS_RET_OK, "support opened on the stub backend");

    struct nros_executor_t executor = rclc_executor_get_zero_initialized_executor();
    CHECK_RET(nros_executor_init(&executor, &support, 8), NROS_RET_OK, "executor initialised");

    /* The node is BOUND to the executor — the executor exists first, then the
     * node. That ordering is what makes the node an owner that can reach an
     * arena; a node from the legacy `rclc_node_init_default` path reaches no
     * executor and the creation verb says so rather than half-working. */
    struct nros_node_t node = rcl_get_zero_initialized_node();
    CHECK_RET(nros_executor_node_init(&executor, &node, "guard_owner", NULL), NROS_RET_OK,
              "node bound to the executor");

    const int handles_before = nros_executor_get_handle_count(&executor);

    /* ---- 1. One call, and it REGISTERS ---------------------------------- */

    s_guard = rcl_get_zero_initialized_guard_condition();
    CHECK(s_guard.handle_id == SIZE_MAX, "a zero-initialised guard is not registered");

    CHECK_RET(nros_node_create_guard_condition(&node, &s_guard, on_guard, &s_ctx), NROS_RET_OK,
              "the node created the guard condition");
    CHECK(nros_guard_condition_is_valid(&s_guard), "the created guard condition is valid");
    CHECK(s_guard.handle_id != SIZE_MAX,
          "creation REGISTERED the guard -- this is the defect the three-call shape had, where "
          "the object stayed inert until a third call nobody had to make");
    CHECK(nros_executor_get_handle_count(&executor) == handles_before + 1,
          "the executor took one more handle");

    /* A second create into the same storage is a sequencing error, not a
     * silent second registration. */
    CHECK_RET(nros_node_create_guard_condition(&node, &s_guard, on_guard, &s_ctx),
              NROS_RET_BAD_SEQUENCE, "a double create is refused");

    /* ---- 4a. The negative control: what a spin with nothing pending costs --
     *
     * Without this number, "the woken spin returned quickly" is unfalsifiable.
     * The stub's `drive_io` sleeps out whatever timeout it is handed, which is
     * what a real backend's blocking read does. */
    const uint64_t control_start = now_ms();
    (void)rclc_executor_spin_some(&executor, 400000000ull); /* 400 ms */
    const uint64_t control_ms = now_ms() - control_start;
    CHECK(control_ms >= 300,
          "the control spin must actually SLEEP its budget -- if it returns instantly the wake "
          "measurement below proves nothing");
    CHECK(s_ctx.fires == 0, "no callback fires without a trigger");

    /* ---- 2 + 3 + 4b. A trigger from another thread ----------------------- */

    pthread_t th;
    s_trigger_delay_ms = 0;
    s_trigger_uses_rcl_spelling = false;
    CHECK(pthread_create(&th, NULL, trigger_thread, NULL) == 0, "trigger thread started");
    CHECK(pthread_join(th, NULL) == 0, "trigger thread joined");

    /* 3. It travelled through the ARENA. The local byte in the struct is the
     * fallback an UNREGISTERED guard used, and nothing writes it now; the
     * reader answers the arena flag, which is the only reason it is not
     * permanently false. */
    CHECK(nros_guard_condition_is_triggered(&s_guard),
          "the cross-thread trigger is visible to the polling reader BEFORE any spin");

    const uint64_t woken_start = now_ms();
    (void)rclc_executor_spin_some(&executor, 400000000ull); /* same 400 ms budget */
    const uint64_t woken_ms = now_ms() - woken_start;

    CHECK(s_ctx.fires == 1, "the callback ran exactly once for one trigger");
    CHECK(s_ctx.magic == GUARD_CTX_MAGIC,
          "the callback got the context the CREATION call bound, unchanged");
    CHECK(pthread_equal(s_ctx.ran_on, pthread_self()),
          "the callback ran on the EXECUTOR's thread, not on the thread that triggered");
    CHECK(!nros_guard_condition_is_triggered(&s_guard), "the dispatch CONSUMED the flag");

    /* 4b. The wake, case ONE: the trigger landed BETWEEN two spins. The flag is
     * already set when the spin enters, so the spin must poll and dispatch
     * rather than park. Compared against the control rather than against a
     * constant, so a slow machine moves both numbers together. */
    CHECK(woken_ms * 2 < control_ms,
          "a trigger that lands BETWEEN spins must not wait out the next budget -- the woken "
          "spin has to come back well inside the time the idle spin sleeps");
    if (woken_ms * 2 >= control_ms) {
        fprintf(stderr, "       idle spin %llu ms, woken spin %llu ms\n",
                (unsigned long long)control_ms, (unsigned long long)woken_ms);
    }

    /* 4c. Case TWO: the trigger lands while the executor is ALREADY parked.
     *
     * It is still dispatched, by the spin it landed in, and the bound is the
     * spin's own budget — NOT sooner, and this probe asserts the bound rather
     * than pretending to a latency C cannot deliver today.
     *
     * Why not sooner, measured rather than assumed: the executor only parks in
     * an interruptible wait when the BACKEND has been told the runtime wake
     * callback (`supports_wake_callback` → `has_async_wake`), and the three
     * Rust `Executor::open*` paths install it while the C path does not —
     * `nros_executor_init` reaches `from_session_ptr_in`, which assembles an
     * executor over a session it BORROWS from the support context and installs
     * nothing. So a C executor parks inside the transport, where a guard's
     * signal cannot reach it, whatever backend it runs on. Installing it there
     * is not a one-liner: `Executor::drop` does not clear the callback (the
     * runtime cb's own safety comment says it must), and on the C path the
     * session OUTLIVES the executor — `nros_executor_fini` then
     * `nros_support_fini` — so a backend would be left holding a callback into
     * freed wake state. That is a separate defect and it is named, not
     * smuggled into this item. */
    s_trigger_delay_ms = 50;
    CHECK(pthread_create(&th, NULL, trigger_thread, NULL) == 0, "parked-wake thread started");
    const uint64_t parked_start = now_ms();
    (void)rclc_executor_spin_some(&executor, 400000000ull);
    const uint64_t parked_ms = now_ms() - parked_start;
    CHECK(pthread_join(th, NULL) == 0, "parked-wake thread joined");
    s_trigger_delay_ms = 0;

    CHECK(s_ctx.fires == 2, "the trigger that arrived mid-spin was dispatched by that spin");
    CHECK(parked_ms <= control_ms + 100,
          "a mid-spin trigger must be dispatched within the spin's own budget");

    /* ---- rcl's spelling reaches the same place --------------------------- */

    s_trigger_uses_rcl_spelling = true;
    CHECK(pthread_create(&th, NULL, trigger_thread, NULL) == 0, "second trigger thread started");
    CHECK(pthread_join(th, NULL) == 0, "second trigger thread joined");
    (void)rclc_executor_spin_some(&executor, 400000000ull);
    CHECK(s_ctx.fires == 3, "rcl_trigger_guard_condition reaches the same registered guard");

    /* ---- The polling half: clear without dispatching --------------------- */

    CHECK_RET(nros_guard_condition_trigger(&s_guard), NROS_RET_OK, "triggered for the poller");
    CHECK(nros_guard_condition_is_triggered(&s_guard), "the poller sees it");
    CHECK_RET(nros_guard_condition_clear(&s_guard), NROS_RET_OK, "the poller cleared it");
    CHECK(!nros_guard_condition_is_triggered(&s_guard), "clear() reached the same flag");
    (void)rclc_executor_spin_some(&executor, 10000000ull);
    CHECK(s_ctx.fires == 3, "a cleared flag dispatches nothing");

    /* ---- Teardown -------------------------------------------------------- */

    CHECK_RET(rcl_guard_condition_fini(&s_guard), NROS_RET_OK, "guard condition finalised");
    CHECK_RET(rcl_guard_condition_fini(&s_guard), NROS_RET_OK, "fini is idempotent, as in rcl");
    (void)rclc_executor_fini(&executor);
    (void)rcl_node_fini(&node);
    (void)rclc_support_fini(&support);

    CHECK(nros_stub_rmw_drive_io_calls() > 0,
          "the spins actually reached the backend -- a probe whose executor never drove I/O "
          "measured nothing");

    if (s_failures != 0) {
        fprintf(stderr, "node_guard_condition: %d failure(s)\n", s_failures);
        return 1;
    }
    printf("node_guard_condition: OK (idle spin %llu ms, pre-spin wake %llu ms, "
           "parked wake %llu ms)\n",
           (unsigned long long)control_ms, (unsigned long long)woken_ms,
           (unsigned long long)parked_ms);
    return 0;
}
