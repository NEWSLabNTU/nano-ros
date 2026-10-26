/*
 * phase-515 C8 -- rcl's `rcl_service_server_is_available(node, client,
 * &is_available)`, called with rcl's signature on a real executor over the
 * stub backend: NULL arguments refused, an unattached client NOT_INIT, a
 * backend that cannot tell UNSUPPORTED with the answer untouched, and the
 * backend's yes/no reported as OK + the bool.
 */

#include "stub_rmw_backend.h"

#include <rcl/graph.h>
#include <rclc/rclc.h>

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

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

static const rclc_service_type_t SRV_TYPE = {
    .type_name = "example_interfaces::srv::dds_::AddTwoInts_",
    .type_hash = "RIHS01_0000000000000000000000000000000000000000000000000000000000000000",
};

int main(void) {
    setenv("NROS_RMW", NROS_STUB_RMW_NAME, 1);
    nros_stub_rmw_set_accept_entities(true);

    struct rclc_support_t support = rclc_support_get_zero_initialized();
    CHECK_RET(rclc_support_init_rmw(&support, "stub://none", 52, "service_server_is_available",
                                    NROS_STUB_RMW_NAME),
              RCL_RET_OK, "support opened on the stub backend");
    if (s_failures != 0) return 1;

    rclc_executor_t executor = rclc_executor_get_zero_initialized_executor();
    CHECK_RET(rclc_executor_init(&executor, &support.context, 4, NULL), RCL_RET_OK,
              "executor initialised");
    rcl_node_options_t opts = rcl_node_get_default_options();
    rcl_node_t node = rcl_get_zero_initialized_node();
    CHECK_RET(rclc_executor_node_init(&executor, &node, "availability", &opts), RCL_RET_OK,
              "node bound to the executor");
    rcl_client_t client = rcl_get_zero_initialized_client();
    CHECK_RET(rclc_client_init_default(&client, &node, &SRV_TYPE, "/availability"), RCL_RET_OK,
              "client created");
    if (s_failures != 0) return 1;

    /* rcl's signature, taken as a pointer so a drift fails to compile. */
    rcl_ret_t (*avail)(const rcl_node_t*, const rcl_client_t*, bool*) =
        rcl_service_server_is_available;
    bool is_available = true;

    CHECK_RET(avail(NULL, &client, &is_available), RCL_RET_INVALID_ARGUMENT, "NULL node refused");
    CHECK_RET(avail(&node, NULL, &is_available), RCL_RET_INVALID_ARGUMENT, "NULL client refused");
    CHECK_RET(avail(&node, &client, NULL), RCL_RET_INVALID_ARGUMENT, "NULL out refused");
    CHECK_RET(avail(&node, &client, &is_available), RCL_RET_NOT_INIT,
              "a client not attached to an executor is NOT_INIT");

    CHECK_RET(rclc_executor_attach_client(&executor, &client), RCL_RET_OK, "client attached");

    /* The backend cannot tell: UNSUPPORTED, and the answer is not written. */
    nros_stub_rmw_set_server_available(-1);
    is_available = true;
    CHECK_RET(avail(&node, &client, &is_available), RCL_RET_UNSUPPORTED,
              "a backend that cannot tell is UNSUPPORTED");
    CHECK(is_available, "UNSUPPORTED leaves *is_available untouched");

    nros_stub_rmw_set_server_available(0);
    CHECK_RET(avail(&node, &client, &is_available), RCL_RET_OK, "no server: OK");
    CHECK(!is_available, "no server: false");

    nros_stub_rmw_set_server_available(1);
    CHECK_RET(avail(&node, &client, &is_available), RCL_RET_OK, "a server: OK");
    CHECK(is_available, "a server: true");

    nros_stub_rmw_set_server_available(-1);
    if (s_failures != 0) {
        fprintf(stderr, "service_server_is_available: %d failure(s)\n", s_failures);
        return 1;
    }
    printf("service_server_is_available: OK\n");
    return 0;
}
