/* Issue 1385 — a C executor INSTALLS the backend wake callback, and HANDS IT
 * BACK at teardown.
 *
 * Two coupled facts, and the order between them is the content of the issue.
 *
 *   1. `nros_executor_init` reaches `Executor::from_session_ptr_in` — the
 *      borrowed-session constructor — which installed nothing, so
 *      `has_async_wake` was false for the entire life of EVERY C executor, on
 *      EVERY backend. The three Rust `Executor::open*` paths call
 *      `install_wake_signal_on_primary`; the C path reached none of them.
 *      What that costs is the `else` arm of `spin_once`'s wait decision: the
 *      transport is driven for the caller's FULL timeout, so an arrival
 *      signalled from a backend worker thread or an ISR — rather than from the
 *      recv we are parked in — cannot cut the wait short.
 *
 *   2. `Executor::drop` did not clear the callback, although the runtime cb's
 *      own SAFETY comment said it must. The context is
 *      `Arc::as_ptr(&self.wake_ctx)`, executor-owned; `rclc_executor_fini`
 *      drops the executor and then ZERO-FILLS `_opaque`, while the borrowed
 *      session lives on in `nros_support_t` until `rclc_support_fini`. So
 *      installing without clearing converts a missed optimisation into a
 *      use-after-free with an arbitrarily wide window.
 *
 * A latency number alone cannot separate these: "the spin came back early" is
 * true of an install with no clear, and of a spin that never waited at all.
 * So this probe asserts the BACKEND'S OWN SLOT in both directions —
 * `nros_stub_rmw_wake_cb_installed()` reads back what `set_wake_callback` was
 * last handed — and only then measures what the install buys, against a
 * negative control and with a LOWER bound as well as an upper one.
 *
 * The RMW is `stub_rmw_backend.c`: an executor needs a session and a session
 * needs a backend, but nothing here publishes. No router, no agent, no
 * network — it runs in `just check c`.
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

/* ---- The backend's async notify path -------------------------------------
 *
 * A real backend fires the runtime wake callback from a worker thread on
 * datagram arrival, or from an ISR. This thread is that path: it waits until
 * the executor is already parked, then invokes the callback the backend was
 * handed. Nothing here touches the executor directly — that is the whole
 * point, since `nros_executor_cancel` and `Executor::wake` reach the
 * `was_woken` arm without any backend involvement and are NOT what 1385 is
 * about. */

#define ARRIVAL_AFTER_MS 60u

static bool s_arrival_fired = false;

static void* arrival_thread(void* unused) {
    (void)unused;
    sleep_ms(ARRIVAL_AFTER_MS);
    s_arrival_fired = nros_stub_rmw_invoke_wake();
    return NULL;
}

int main(void) {
    /* The stub must be the backend this image opens against. `$NROS_RMW` is the
     * hosted rung of precedence model A and beats the baked selector. */
    setenv("NROS_RMW", NROS_STUB_RMW_NAME, 1);

    CHECK(!nros_stub_rmw_wake_cb_installed(),
          "the backend holds no wake callback before any executor exists -- if it did, this "
          "probe would be reading somebody else's install");

    struct nros_support_t support = nros_support_get_zero_initialized();
    CHECK_RET(nros_support_init_rmw(&support, "stub://none", 44, "executor_backend_wake",
                                    NROS_STUB_RMW_NAME),
              NROS_RET_OK, "support opened on the stub backend");

    /* Opening a SESSION must not install anything: the wake callback belongs to
     * an executor's wake state, and no executor exists yet. */
    CHECK(!nros_stub_rmw_wake_cb_installed(),
          "opening the session alone installs no callback -- the context is executor-owned");

    struct nros_executor_t executor = rclc_executor_get_zero_initialized_executor();
    CHECK_RET(nros_executor_init(&executor, &support, 8), NROS_RET_OK, "executor initialised");

    /* ---- 1. The install ------------------------------------------------- */

    CHECK(nros_stub_rmw_wake_cb_installed(),
          "nros_executor_init INSTALLED the runtime wake callback on the backend -- this is the "
          "defect: the C path built its executor through from_session_ptr_in and installed "
          "nothing, so every C executor on every backend waited out its full budget");

    /* ---- 2a. The negative control: what an idle spin costs ---------------
     *
     * Without this number "the woken spin returned quickly" is unfalsifiable.
     * With the callback installed the executor parks in the platform wake
     * primitive rather than in `drive_io`, so the budget is spent either way —
     * which is exactly why the install cannot be measured by wall time alone
     * and part 1 above reads the slot instead. */
    const uint64_t control_start = now_ms();
    (void)rclc_executor_spin_some(&executor, 400000000ull); /* 400 ms */
    const uint64_t control_ms = now_ms() - control_start;
    CHECK(control_ms >= 300,
          "an idle spin must actually SPEND its budget -- if it returns instantly the wake "
          "measurement below proves nothing");

    /* ---- 2b. The wake: an arrival signalled from the backend -------------
     *
     * The arrival lands while the executor is ALREADY parked. Both bounds are
     * asserted, and the lower one is not decoration: a `spin_once` that never
     * waited would sail through the upper bound and report a wake it never
     * performed. */
    pthread_t th;
    CHECK(pthread_create(&th, NULL, arrival_thread, NULL) == 0, "arrival thread started");
    const uint64_t woken_start = now_ms();
    (void)rclc_executor_spin_some(&executor, 400000000ull); /* the same 400 ms budget */
    const uint64_t woken_ms = now_ms() - woken_start;
    CHECK(pthread_join(th, NULL) == 0, "arrival thread joined");

    CHECK(s_arrival_fired,
          "the backend HAD a callback to fire -- a stub with an empty slot would make the "
          "timing below a measurement of nothing");
    CHECK(woken_ms >= ARRIVAL_AFTER_MS - 10,
          "LOWER BOUND: the spin must have been WAITING when the arrival came. A spin that "
          "returns before the arrival was even signalled passes every upper bound while doing "
          "none of the waiting this probe is about");
    CHECK(woken_ms * 2 < control_ms,
          "UPPER BOUND: an arrival signalled from the backend mid-spin must CUT THE WAIT "
          "SHORT -- compared against the idle spin rather than a constant, so a slow machine "
          "moves both numbers together");
    if (woken_ms * 2 >= control_ms || woken_ms + 10 < ARRIVAL_AFTER_MS) {
        fprintf(stderr, "       idle spin %llu ms, woken spin %llu ms, arrival at %u ms\n",
                (unsigned long long)control_ms, (unsigned long long)woken_ms, ARRIVAL_AFTER_MS);
    }

    /* ---- 2c. A second arrival, after one that landed BETWEEN spins ------
     *
     * The runtime wake callback sets BOTH halves of one signal: the executor's
     * `wake_flag` and the platform wake primitive. `spin_once`'s fast arm
     * consumes the flag (`was_woken`) and returns without entering the wait,
     * so unless it also drains the primitive, the primitive stays posted and
     * the NEXT spin's `wait_ms` returns instantly on a signal already acted
     * on. That spin then does not wait at all — which is precisely what the
     * lower bound below exists to catch, and it is invisible to any upper
     * bound.
     *
     * Reachable only now: before the C executor installed a callback, no C
     * spin ever entered the wait arm. */
    CHECK(nros_stub_rmw_invoke_wake(), "an arrival BETWEEN spins");
    const uint64_t between_start = now_ms();
    (void)rclc_executor_spin_some(&executor, 400000000ull);
    const uint64_t between_ms = now_ms() - between_start;
    CHECK(between_ms < 100,
          "an arrival that landed between spins is taken by the fast arm, which does not wait");

    s_arrival_fired = false;
    CHECK(pthread_create(&th, NULL, arrival_thread, NULL) == 0, "second arrival thread started");
    const uint64_t second_start = now_ms();
    (void)rclc_executor_spin_some(&executor, 400000000ull);
    const uint64_t second_ms = now_ms() - second_start;
    CHECK(pthread_join(th, NULL) == 0, "second arrival thread joined");
    CHECK(s_arrival_fired, "the second arrival had a callback to fire");
    CHECK(second_ms >= ARRIVAL_AFTER_MS - 10,
          "LOWER BOUND: the spin AFTER a fast-arm wake must still WAIT. A stale post left on "
          "the wake primitive by the fast arm makes this spin return instantly, so the work it "
          "should have waited for is dispatched a whole spin late");
    CHECK(second_ms * 2 < control_ms, "UPPER BOUND: and it is still cut short by the arrival");
    if (second_ms + 10 < ARRIVAL_AFTER_MS || second_ms * 2 >= control_ms) {
        fprintf(stderr, "       between-spins %llu ms, spin after it %llu ms\n",
                (unsigned long long)between_ms, (unsigned long long)second_ms);
    }

    /* ---- 3. The teardown: the callback is handed back -------------------
     *
     * `rclc_executor_fini` drops the executor in place and then zero-fills
     * `_opaque` — the very storage `wake_ctx` was carved from — while the
     * session it borrowed lives on until `rclc_support_fini`. A callback still
     * installed here is a backend holding a pointer into freed, overwritten
     * memory, and any arrival in that window calls it.
     *
     * This is the half that must land FIRST. Observed, not asserted in prose:
     * the backend's own slot, read back. */
    CHECK_RET(rclc_executor_fini(&executor), NROS_RET_OK, "executor finalised");

    if (nros_stub_rmw_wake_cb_installed()) {
        fprintf(stderr,
                "FAIL: the backend STILL holds a wake callback after rclc_executor_fini -- its "
                "context points into `_opaque`, which fini has just zero-filled (%s:%d)\n",
                __FILE__, __LINE__);
        s_failures++;
        /* Deliberately NOT invoking it: on a tree with this defect that call
         * IS the use-after-free, and a probe should report a fault rather than
         * perform one. */
    } else {
        CHECK(!nros_stub_rmw_invoke_wake(),
              "an arrival after teardown reaches NOTHING -- there is no callback left to call, "
              "which is the whole obligation the runtime cb's SAFETY comment states");
    }

    /* ---- 4. The clear is not a one-shot ---------------------------------
     *
     * The session outlives the executor by design, so a second executor over
     * the same support must get its own install. A teardown that latched the
     * backend off for good would pass part 3 and silently cost every later
     * executor its wake. */
    struct nros_executor_t second = rclc_executor_get_zero_initialized_executor();
    CHECK_RET(nros_executor_init(&second, &support, 8), NROS_RET_OK, "second executor initialised");
    CHECK(nros_stub_rmw_wake_cb_installed(),
          "a second executor over the same session installs its OWN callback -- the clear must "
          "not latch the backend off");
    CHECK_RET(rclc_executor_fini(&second), NROS_RET_OK, "second executor finalised");
    CHECK(!nros_stub_rmw_wake_cb_installed(), "and hands it back again");

    (void)rclc_support_fini(&support);

    CHECK(nros_stub_rmw_drive_io_calls() > 0,
          "the spins actually reached the backend -- a probe whose executor never drove I/O "
          "measured nothing");

    if (s_failures != 0) {
        fprintf(stderr, "executor_backend_wake: %d failure(s)\n", s_failures);
        return 1;
    }
    printf("executor_backend_wake: OK (idle spin %llu ms, backend-woken spin %llu ms, "
           "arrival at %u ms)\n",
           (unsigned long long)control_ms, (unsigned long long)woken_ms, ARRIVAL_AFTER_MS);
    return 0;
}
