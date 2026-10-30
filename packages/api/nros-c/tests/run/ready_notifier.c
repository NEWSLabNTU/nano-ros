/* Issue 2144 (RFC-0112 D2.2, "Readiness out") — a C host learns that work
 * arrived through ONE executor-level notifier, instead of polling.
 *
 * `rclc_executor_set_ready_notifier` installs a `(notifier, ctx)` pair that
 * the executor calls whenever its backend signals an arrival. This probe
 * checks, against the backend's own wake path (`nros_stub_rmw_invoke_wake`,
 * which is what a real backend's worker thread does on a datagram):
 *
 *   1. before an install, an arrival calls nothing (the negative control);
 *   2. after it, an arrival calls the notifier with its context — with
 *      nothing spinning, which is the point;
 *   3. a replacement redirects the next arrival to the new context only;
 *   4. clearing stops the calls, and clearing again is still OK;
 *   5. after `rclc_executor_fini` the notifier is never called again.
 *
 * The RMW is `stub_rmw_backend.c`: no router, no agent — it runs in
 * `just check c`. The poll-only refusal (`RCL_RET_UNSUPPORTED`) needs a backend
 * without a wake callback; the Rust test `xrce_guard_wake.rs` measures it on
 * XRCE.
 */

#include "stub_rmw_backend.h"

#include <rclc/rclc.h>

#include <stdatomic.h>
#include <stdio.h>
#include <stdlib.h>

void nros_app_register_backends(void);
void nros_app_register_backends(void) {
    (void)nros_stub_rmw_register();
}

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

static atomic_int s_calls[2];

static void on_ready(void* ctx) {
    atomic_fetch_add(&s_calls[(int)(intptr_t)ctx], 1);
}

static int calls(int i) {
    return atomic_load(&s_calls[i]);
}

int main(void) {
    setenv("NROS_RMW", NROS_STUB_RMW_NAME, 1);

    CHECK_RET(rclc_executor_set_ready_notifier(NULL, on_ready, NULL), RCL_RET_INVALID_ARGUMENT,
              "a NULL executor is refused");
    struct rclc_executor_t uninit = rclc_executor_get_zero_initialized_executor();
    CHECK_RET(rclc_executor_set_ready_notifier(&uninit, on_ready, NULL), RCL_RET_NOT_INIT,
              "an uninitialised executor is refused");

    struct rclc_support_t support = rclc_support_get_zero_initialized();
    CHECK_RET(rclc_support_init_rmw(&support, "stub://none", 45, "ready_notifier",
                                    NROS_STUB_RMW_NAME),
              RCL_RET_OK, "support opened on the stub backend");
    struct rclc_executor_t executor = rclc_executor_get_zero_initialized_executor();
    CHECK_RET(rclc_executor_init(&executor, &support.context, 8, NULL), RCL_RET_OK,
              "executor initialised");

    /* 1. Negative control: no notifier, an arrival calls nothing. */
    CHECK(nros_stub_rmw_invoke_wake(), "the backend holds the executor's wake callback");
    CHECK(calls(0) + calls(1) == 0, "an arrival before any install called a notifier");

    /* 2. Installed: the next arrival calls it, with nothing spinning. */
    CHECK_RET(rclc_executor_set_ready_notifier(&executor, on_ready, (void*)(intptr_t)0),
              RCL_RET_OK, "the stub signals arrivals asynchronously, so the install succeeds");
    CHECK(nros_stub_rmw_invoke_wake(), "arrival");
    CHECK(calls(0) == 1, "an arrival with nothing spinning did not call the notifier");

    /* 3. Replaced: only the new context hears the next arrival. */
    CHECK_RET(rclc_executor_set_ready_notifier(&executor, on_ready, (void*)(intptr_t)1),
              RCL_RET_OK, "replaced");
    CHECK(nros_stub_rmw_invoke_wake(), "arrival");
    CHECK(calls(0) == 1 && calls(1) == 1, "a replaced notifier's context was called again");

    /* 4. Cleared: nobody. Clearing twice is fine. */
    CHECK_RET(rclc_executor_set_ready_notifier(&executor, NULL, NULL), RCL_RET_OK, "cleared");
    CHECK_RET(rclc_executor_set_ready_notifier(&executor, NULL, NULL), RCL_RET_OK,
              "cleared again");
    CHECK(nros_stub_rmw_invoke_wake(), "arrival");
    CHECK(calls(0) == 1 && calls(1) == 1, "a cleared notifier was called");

    /* The spin that the notification would prompt still works. */
    CHECK_RET(rclc_executor_spin_some(&executor, 0), RCL_RET_OK, "spin_some(0)");

    /* 5. Installed, then finalised: the executor takes it down. */
    CHECK_RET(rclc_executor_set_ready_notifier(&executor, on_ready, (void*)(intptr_t)0),
              RCL_RET_OK, "installed again");
    CHECK_RET(rclc_executor_fini(&executor), RCL_RET_OK, "executor finalised");
    CHECK(!nros_stub_rmw_invoke_wake(), "the backend holds no callback after fini");
    CHECK(calls(0) == 1, "the notifier was called after fini");

    CHECK_RET(rclc_support_fini(&support), RCL_RET_OK, "support finalised");

    if (s_failures != 0) {
        fprintf(stderr, "ready_notifier: %d failure(s)\n", s_failures);
        return 1;
    }
    printf("ready_notifier: OK\n");
    return 0;
}
