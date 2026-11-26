/*
 * The C device extension hook, run: `nros_executor_register_device` registers a
 * device event source, and the completion handle it returns is signalled from
 * a POSIX signal handler (the host's interrupt context) with
 * `nros_completion_signal_from_isr`.
 *
 * What it asserts, each against a control:
 *   1. registration claims one handle and returns a usable completion handle;
 *      null arguments are refused;
 *   2. a completion signalled from a signal handler WAKES a parked spin, which
 *      an identical spin with nothing pending does not (the stub backend sleeps
 *      out its budget);
 *   3. the device's continuation runs at its bound priority: a device bound to
 *      a critical sched context runs before a best-effort guard condition that
 *      was triggered first and registered first;
 *   4. a null completion handle is ignored by both signal functions.
 *
 * Same stub backend and archive as the other run probes: no router, no network.
 */

#include "stub_rmw_backend.h"

#include <nros/device_hook.h>
#include <rclc/rclc.h>

#include <pthread.h>
#include <signal.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/types.h>
#include <time.h>
#include <unistd.h>

/* The registration hook every C image owes `rclc_support_init`. */
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

/* The order continuations ran in: 'D' for the device, 'G' for the guard. */
static char s_order[8];
static volatile int s_order_len = 0;
static volatile int s_device_fires = 0;

static void on_device(void* context) {
    (void)context;
    s_device_fires++;
    if (s_order_len < (int)sizeof s_order - 1) s_order[s_order_len++] = 'D';
}

static void on_guard(void* context) {
    (void)context;
    if (s_order_len < (int)sizeof s_order - 1) s_order[s_order_len++] = 'G';
}

/* The handle the signal handler signals, as a port's interrupt handler would
 * hold it. */
static nros_completion_t s_done;

static void on_sigusr2(int sig) {
    (void)sig;
    nros_completion_signal_from_isr(s_done);
}

static void* raise_after_50ms(void* unused) {
    (void)unused;
    sleep_ms(50);
    /* Deliver to the process; the handler runs on whichever thread takes it,
     * as an interrupt does. */
    (void)kill(getpid(), SIGUSR2);
    return NULL;
}

int main(void) {
    setenv("NROS_RMW", NROS_STUB_RMW_NAME, 1);

    struct rclc_support_t support = rclc_support_get_zero_initialized();
    CHECK_RET(rclc_support_init_rmw(&support, "stub://none", 44, "executor_device_hook",
                                    NROS_STUB_RMW_NAME),
              RCL_RET_OK, "support opened on the stub backend");

    struct rclc_executor_t executor = rclc_executor_get_zero_initialized_executor();
    CHECK_RET(rclc_executor_init(&executor, &support.context, 8, NULL), RCL_RET_OK,
              "executor initialised");
    struct rcl_node_t node = rcl_get_zero_initialized_node();
    CHECK_RET(rclc_executor_node_init(&executor, &node, "device_owner", NULL), RCL_RET_OK,
              "node bound to the executor");

    /* ---- 1. Registration -------------------------------------------------- */

    /* The guard first, so registration order alone would run it first. */
    struct rcl_guard_condition_t guard = rcl_get_zero_initialized_guard_condition();
    CHECK_RET(rclc_node_create_guard_condition(&node, &guard, on_guard, NULL), RCL_RET_OK,
              "the best-effort guard condition is created");

    size_t handle = SIZE_MAX;
    nros_completion_t null_done;
    memset(&null_done, 0, sizeof null_done);
    CHECK_RET(nros_executor_register_device(NULL, on_device, NULL, &handle, &s_done),
              RCL_RET_INVALID_ARGUMENT, "a null executor is refused");
    CHECK_RET(nros_executor_register_device(&executor, NULL, NULL, &handle, &s_done),
              RCL_RET_INVALID_ARGUMENT, "a null callback is refused");
    CHECK_RET(nros_executor_register_device(&executor, on_device, NULL, NULL, &s_done),
              RCL_RET_INVALID_ARGUMENT, "a null handle out-parameter is refused");

    const int handles_before = rclc_executor_get_handle_count(&executor);
    CHECK_RET(nros_executor_register_device(&executor, on_device, NULL, &handle, &s_done),
              RCL_RET_OK, "the device is registered");
    CHECK(handle != SIZE_MAX, "registration returned the device's handle");
    CHECK(s_done._slot != NULL, "registration returned a completion handle");
    CHECK(rclc_executor_get_handle_count(&executor) == handles_before + 1,
          "the device claimed one handle");

    /* ---- 4. A null handle is ignored --------------------------------------- */
    nros_completion_signal(null_done);
    nros_completion_signal_from_isr(null_done);

    /* Priorities: the device critical, the guard best-effort. */
    rclc_sched_context_t cfg;
    memset(&cfg, 0, sizeof cfg);
    cfg.class_ = RCLC_SCHED_CLASS_FIFO;
    cfg.priority = RCLC_SCHED_PRIORITY_CRITICAL;
    rclc_sched_context_id_t sc_dev = 0, sc_low = 0;
    CHECK_RET(rclc_executor_create_sched_context(&executor, &cfg, &sc_dev), RCL_RET_OK,
              "the critical sched context is created");
    cfg.priority = RCLC_SCHED_PRIORITY_BEST_EFFORT;
    CHECK_RET(rclc_executor_create_sched_context(&executor, &cfg, &sc_low), RCL_RET_OK,
              "the best-effort sched context is created");
    CHECK_RET(rclc_executor_bind_handle_to_sched_context(&executor, handle, sc_dev), RCL_RET_OK,
              "the device's handle binds to a sched context");
    CHECK_RET(rclc_executor_bind_handle_to_sched_context(&executor, guard.handle_id, sc_low),
              RCL_RET_OK, "the guard binds to a sched context");

    struct sigaction sa;
    memset(&sa, 0, sizeof sa);
    sa.sa_handler = on_sigusr2;
    sigemptyset(&sa.sa_mask);
    CHECK(sigaction(SIGUSR2, &sa, NULL) == 0, "the signal handler is installed");

    /* ---- 2a. The negative control: an idle spin sleeps its budget ---------- */
    const uint64_t control_start = now_ms();
    (void)rclc_executor_spin_some(&executor, 400000000ull); /* 400 ms */
    const uint64_t control_ms = now_ms() - control_start;
    CHECK(control_ms >= 300, "an idle spin must spend its budget, or the wake below proves "
                             "nothing");
    CHECK(s_device_fires == 0, "no continuation runs without a completion");

    /* ---- 2b. A completion from the signal handler wakes the parked spin ---- */
    pthread_t th;
    CHECK(pthread_create(&th, NULL, raise_after_50ms, NULL) == 0, "the raising thread starts");
    const uint64_t woken_start = now_ms();
    for (int i = 0; i < 3 && s_device_fires == 0; i++) {
        (void)rclc_executor_spin_some(&executor, 400000000ull);
    }
    const uint64_t woken_ms = now_ms() - woken_start;
    CHECK(pthread_join(th, NULL) == 0, "the raising thread joined");
    CHECK(s_device_fires == 1, "the completion ran the device's continuation once");
    CHECK(woken_ms >= 40, "the continuation did not run before the signal");
    CHECK(woken_ms < 250, "the completion WOKE the parked spin rather than waiting out its "
                          "400 ms budget");

    /* ---- 3. Priority: the device ahead of a lower-priority ready guard ----- */
    s_order_len = 0;
    memset(s_order, 0, sizeof s_order);
    CHECK_RET(rclc_guard_condition_trigger(&guard), RCL_RET_OK, "the guard is triggered first");
    CHECK(raise(SIGUSR2) == 0, "the completion is signalled from the handler second");
    (void)rclc_executor_spin_some(&executor, 0);
    for (int i = 0; i < 3 && s_order_len < 2; i++) {
        (void)rclc_executor_spin_some(&executor, 0);
    }
    CHECK(s_order_len == 2 && s_order[0] == 'D' && s_order[1] == 'G',
          "the critical device ran before the best-effort guard that was ready first");
    if (!(s_order_len == 2 && s_order[0] == 'D')) {
        fprintf(stderr, "       order: \"%s\"\n", s_order);
    }

    (void)rclc_executor_fini(&executor);
    (void)rclc_support_fini(&support);

    if (s_failures != 0) {
        fprintf(stderr, "executor_device_hook: %d failure(s)\n", s_failures);
        return 1;
    }
    printf("executor_device_hook: OK\n");
    return 0;
}
