/* Issue 1940 — a per-entity wake callback reaches the backend, and a backend
 * that cannot serve one says so.
 *
 * `rclc_polling_{subscription,service,client}_set_wake_callback` used to end
 * in the cffi route's no-op `register_waker`: each returned `RCL_RET_OK` and
 * its callback never fired, on every backend. They now forward to the
 * vtable's `*_set_on_new_*_callback` slots. This probe drives both halves of
 * that contract against the stub backend:
 *
 *   1. the backend STORES a registration for each kind, and invoking it the
 *      way an arrival path would calls the C callback with its own context —
 *      more than once, because the ABI says a registration persists;
 *   2. with the slots answering UNSUPPORTED, each setter returns
 *      `RCL_RET_UNSUPPORTED` rather than `RCL_RET_OK`.
 *
 * Negative control for (1): before any registration the stub holds nothing,
 * so `nros_stub_rmw_invoke_entity_wake` returns false — a probe that passed
 * without the forward would be reading a callback nobody installed. */

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

static const rclc_service_type_t SRV_TYPE = {
    .type_name = "example_interfaces::srv::dds_::AddTwoInts_",
    .type_hash = "RIHS01_0000000000000000000000000000000000000000000000000000000000000000",
};

/* One counter per kind; the callback's ctx says which. */
static int s_fired[3] = {0, 0, 0};

static void on_wake(void* ctx) {
    int kind = *(const int*)ctx;
    s_fired[kind]++;
}

static const int KIND_IDS[3] = {NROS_STUB_ENTITY_SUBSCRIPTION, NROS_STUB_ENTITY_SERVICE,
                                NROS_STUB_ENTITY_CLIENT};
static const char* const KIND_NAMES[3] = {"subscription", "service", "client"};

int main(void) {
    setenv("NROS_RMW", NROS_STUB_RMW_NAME, 1);
    nros_stub_rmw_set_accept_entities(true);

    struct rclc_support_t support = rclc_support_get_zero_initialized();
    CHECK_RET(rclc_support_init_rmw(&support, "stub://none", 47, "entity_wake_callback",
                                    NROS_STUB_RMW_NAME),
              RCL_RET_OK, "support opened on the stub backend");
    if (s_failures != 0) return 1;

    rclc_executor_t executor = rclc_executor_get_zero_initialized_executor();
    CHECK_RET(rclc_executor_init(&executor, &support.context, 4, NULL), RCL_RET_OK,
              "executor initialised");
    rcl_node_options_t opts = rcl_node_get_default_options();
    rcl_node_t node = rcl_get_zero_initialized_node();
    CHECK_RET(rclc_executor_node_init(&executor, &node, "entity_wake", &opts), RCL_RET_OK,
              "node bound to the executor");

    struct rclc_polling_subscription_t sub = {0};
    struct rclc_polling_service_t srv = {0};
    struct rclc_polling_client_t cli = {0};
    CHECK_RET(rclc_polling_subscription_init(&sub, &node, &MSG_TYPE, "/entity_wake"), RCL_RET_OK,
              "polling subscription created");
    CHECK_RET(rclc_polling_service_init(&srv, &node, &SRV_TYPE, "/entity_wake_srv"), RCL_RET_OK,
              "polling service created");
    CHECK_RET(rclc_polling_client_init(&cli, &node, &SRV_TYPE, "/entity_wake_srv"), RCL_RET_OK,
              "polling client created");
    if (s_failures != 0) return 1;

    for (int k = 0; k < 3; k++) {
        CHECK(!nros_stub_rmw_invoke_entity_wake(KIND_IDS[k]),
              "negative control: no callback is stored before a registration");
    }

    /* ---- 1. a registration reaches the backend and persists ---- */
    struct rclc_wake_state_t states[3] = {{{0}}, {{0}}, {{0}}};
    CHECK_RET(
        rclc_polling_subscription_set_wake_callback(&sub, &states[0], on_wake, (void*)&KIND_IDS[0]),
        RCL_RET_OK, "subscription wake callback accepted");
    CHECK_RET(
        rclc_polling_service_set_wake_callback(&srv, &states[1], on_wake, (void*)&KIND_IDS[1]),
        RCL_RET_OK, "service wake callback accepted");
    CHECK_RET(rclc_polling_client_set_wake_callback(&cli, &states[2], on_wake, (void*)&KIND_IDS[2]),
              RCL_RET_OK, "client wake callback accepted");

    for (int k = 0; k < 3; k++) {
        for (int arrival = 0; arrival < 3; arrival++) {
            CHECK(nros_stub_rmw_invoke_entity_wake(KIND_IDS[k]),
                  "the backend holds a registration for this entity");
        }
        if (s_fired[k] != 3) {
            fprintf(stderr, "FAIL: %s callback fired %d time(s) for 3 arrivals\n", KIND_NAMES[k],
                    s_fired[k]);
            s_failures++;
        }
    }

    /* ---- 2. a backend without the capability is reported, not accepted ---- */
    nros_stub_rmw_set_entity_wake_supported(false);
    CHECK_RET(
        rclc_polling_subscription_set_wake_callback(&sub, &states[0], on_wake, (void*)&KIND_IDS[0]),
        RCL_RET_UNSUPPORTED, "subscription: unsupported is reported");
    CHECK_RET(
        rclc_polling_service_set_wake_callback(&srv, &states[1], on_wake, (void*)&KIND_IDS[1]),
        RCL_RET_UNSUPPORTED, "service: unsupported is reported");
    CHECK_RET(rclc_polling_client_set_wake_callback(&cli, &states[2], on_wake, (void*)&KIND_IDS[2]),
              RCL_RET_UNSUPPORTED, "client: unsupported is reported");
    nros_stub_rmw_set_entity_wake_supported(true);

    (void)rclc_polling_client_fini(&cli);
    (void)rclc_polling_service_fini(&srv);
    (void)rclc_polling_subscription_fini(&sub);
    (void)rcl_node_fini(&node);
    (void)rclc_executor_fini(&executor);
    (void)rclc_support_fini(&support);

    if (s_failures != 0) {
        fprintf(stderr, "entity_wake_callback: %d failure(s)\n", s_failures);
        return 1;
    }
    printf("entity_wake_callback: OK (3 kinds x 3 arrivals delivered; unsupported reported)\n");
    return 0;
}
