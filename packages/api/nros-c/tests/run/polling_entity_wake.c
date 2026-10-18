/* Issue 1940 — `rclc_polling_subscription_set_wake_callback` must reach the
 * backend, and must not answer OK when the backend cannot wake.
 *
 * The C function built a waker from the caller's (cb, ctx) and handed it to
 * the cffi subscription, whose `register_waker` was the trait's empty default:
 * no per-entity slot existed in `rmw_vtable.h` to forward it to. So the call
 * returned `RCL_RET_OK` and the callback never fired, on every backend.
 *
 * The RMW is `stub_rmw_backend.c` in accept mode, with its
 * `subscription_set_wake` slot recorded so the probe can play the backend's
 * arrival path. Every assertion about a FIRED callback fails on the pre-1940
 * tree: nothing was installed, so `nros_stub_rmw_invoke_subscription_wake`
 * finds no callback.
 */

#include "stub_rmw_backend.h"

#include <rclc/rclc.h>

#include <stdbool.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>

void nros_app_register_backends(void);
void nros_app_register_backends(void) {
    (void)nros_stub_rmw_register();
}

/* Hand-rolled rather than <assert.h>: NDEBUG would compile the probe away and
 * it would still exit 0 (issue 0196's class). */
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

static const rclc_message_type_t MSG_TYPE = {
    .type_name = "std_msgs::msg::dds_::Int32_",
    .type_hash = "RIHS01_0000000000000000000000000000000000000000000000000000000000000000",
    .serialized_size_max = 64,
};

static void on_wake(void* ctx) {
    (*(int*)ctx)++;
}

int main(void) {
    setenv("NROS_RMW", NROS_STUB_RMW_NAME, 1);
    nros_stub_rmw_set_accept_entities(true);

    struct rclc_support_t support = rclc_support_get_zero_initialized();
    CHECK_RET(rclc_support_init_rmw(&support, "stub://none", 47, "polling_entity_wake",
                                    NROS_STUB_RMW_NAME),
              RCL_RET_OK, "support opened on the stub backend");
    struct rcl_node_t node = rcl_get_zero_initialized_node();
    CHECK_RET(rclc_node_init_default(&node, "polling_entity_wake", "", &support), RCL_RET_OK,
              "node initialised");
    if (s_failures != 0) return 1;

    /* ---- 1. A backend with the slot: installed, and an arrival fires ---- */
    struct rclc_polling_subscription_t sub = rclc_get_zero_initialized_polling_subscription();
    CHECK_RET(rclc_polling_subscription_init(&sub, &node, &MSG_TYPE, "/entity_wake"), RCL_RET_OK,
              "polling subscription created");

    CHECK(!nros_stub_rmw_invoke_subscription_wake(),
          "no wake callback is installed before the caller asks for one");

    struct rclc_wake_state_t state = rclc_wake_state_get_zero_initialized();
    int fired = 0;
    CHECK_RET(rclc_polling_subscription_set_wake_callback(&sub, &state, on_wake, &fired),
              RCL_RET_OK, "set_wake_callback on a backend that can wake");

    CHECK(nros_stub_rmw_invoke_subscription_wake(),
          "the call must INSTALL a callback in the backend -- issue 1940 installed nothing");
    CHECK(fired == 1, "the backend's arrival must reach the caller's callback");
    (void)nros_stub_rmw_invoke_subscription_wake();
    CHECK(fired == 2, "every arrival reaches it, not just the first");

    CHECK_RET(rclc_polling_subscription_fini(&sub), RCL_RET_OK, "subscription finalised");

    /* ---- 2. A backend that cannot wake: the call says so ---------------- */
    nros_stub_rmw_set_entity_wake_supported(false);
    struct rclc_polling_subscription_t deaf = rclc_get_zero_initialized_polling_subscription();
    CHECK_RET(rclc_polling_subscription_init(&deaf, &node, &MSG_TYPE, "/entity_wake_deaf"),
              RCL_RET_OK, "second polling subscription created");
    struct rclc_wake_state_t deaf_state = rclc_wake_state_get_zero_initialized();
    int deaf_fired = 0;
    CHECK_RET(rclc_polling_subscription_set_wake_callback(&deaf, &deaf_state, on_wake,
                                                         &deaf_fired),
              RCL_RET_UNSUPPORTED,
              "a backend that refuses the slot must not be reported as OK");
    CHECK_RET(rclc_polling_subscription_fini(&deaf), RCL_RET_OK, "second subscription finalised");
    nros_stub_rmw_set_entity_wake_supported(true);

    (void)rcl_node_fini(&node);
    (void)rclc_support_fini(&support);

    if (s_failures != 0) {
        fprintf(stderr, "polling_entity_wake: %d failure(s)\n", s_failures);
        return 1;
    }
    printf("polling_entity_wake: OK\n");
    return 0;
}
