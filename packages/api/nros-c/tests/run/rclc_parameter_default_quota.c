/**
 * rclc's default parameter server in a store smaller than rclc's default
 * quota.
 *
 * The build sizes the image's parameter store from what the image declares,
 * so a node that declares one parameter lives in a store of two (its own and
 * the seeded `use_sim_time`). `rclc_parameter_server_init_default` asks for
 * rclc's default quota of 4. The caller stated no quota, so the server must
 * start with a quota capped at the store, and the node's parameter must fit.
 * A quota the caller STATES over the store is still refused.
 *
 * Built against an nros-c whose store holds exactly two parameters
 * (`NROS_PARAMS_MAX_PARAMETERS=2`, `just check c`).
 */

#include <rclc/rclc.h>
#include <nros/rmw_vtable.h>
#include <rclc/detail/rcl_compat.h>
#include <rclc_parameter/rclc_parameter.h>

#include <stdio.h>
#include <string.h>

#include "stub_rmw_backend.h"

static int g_failures;

#define CHECK(cond, ...)                                                                           \
    do {                                                                                           \
        if (!(cond)) {                                                                             \
            g_failures++;                                                                          \
            printf("FAIL %s:%d: ", __FILE__, __LINE__);                                            \
            printf(__VA_ARGS__);                                                                   \
            printf("\n");                                                                          \
        }                                                                                          \
    } while (0)

void nros_app_register_backends(void) {
    (void)nros_stub_rmw_register();
}

int main(void) {
    struct rclc_support_t support;
    struct rclc_executor_t executor = rclc_executor_get_zero_initialized_executor();
    struct rcl_node_t talker;
    rclc_parameter_server_t ps, stated;
    int64_t got = 0;

    memset(&support, 0, sizeof(support));
    memset(&talker, 0, sizeof(talker));
    memset(&ps, 0, sizeof(ps));
    memset(&stated, 0, sizeof(stated));

    nros_stub_rmw_set_accept_entities(true);
    CHECK(rclc_support_init_rmw(&support, NULL, 0, "rclc_param_quota", NROS_STUB_RMW_NAME) ==
              RCL_RET_OK,
          "support init");
    CHECK(rclc_executor_init(&executor, &support.context, 4, NULL) == RCL_RET_OK, "executor init");
    CHECK(rclc_executor_node_init(&executor, &talker, "talker", NULL) == RCL_RET_OK,
          "talker node init");
    if (g_failures != 0) {
        printf("rclc_parameter_default_quota: %d setup failure(s)\n", g_failures);
        return 1;
    }

    /* A quota the caller states over the store is refused, as before. */
    rclc_parameter_options_t four = {.notify_changed_over_dds = false,
                                     .max_params = 4,
                                     .allow_undeclared_parameters = false,
                                     .low_mem_mode = false};
    CHECK(rclc_parameter_server_init_with_option(&stated, &talker, &four) == RCLC_RET_FULL,
          "a stated max_params of 4 was accepted by a store of 2");

    /* The default asks for rclc's 4 and gets what the store holds. */
    CHECK(rclc_parameter_server_init_default(&ps, &talker) == RCL_RET_OK,
          "the default server was refused by a store smaller than rclc's default quota");
    CHECK(ps._max_params == 2, "the default quota is %zu, expected the store's 2",
          (size_t)ps._max_params);

    /* The node's own parameter fits. */
    CHECK(rclc_parameter_declare_int(&ps, "publish_period_ms", 250) == RCL_RET_OK,
          "the node's one parameter did not fit");
    CHECK(rclc_parameter_get_int(&ps, "publish_period_ms", &got) == RCL_RET_OK && got == 250,
          "publish_period_ms reads %lld, expected 250", (long long)got);

    CHECK(rclc_parameter_server_fini(&ps, &talker) == RCL_RET_OK, "fini");

    if (g_failures != 0) {
        printf("rclc_parameter_default_quota: %d failure(s)\n", g_failures);
        return 1;
    }
    printf("rclc_parameter_default_quota: OK\n");
    return 0;
}
