/* phase-515 C6 — rcl's `rcl_*_get_options` on the six entity kinds.
 *
 * Each handle keeps the options it was initialised with, and
 * `rcl_{publisher,subscription,service,client,action_client,action_server}_get_options`
 * returns a pointer to that copy:
 *
 *   - initialised WITH options: the getter answers what was passed (a
 *     non-zero scheduling context, and the subscription's message-info flag),
 *     and the pointer is into the handle, not the caller's struct;
 *   - initialised WITHOUT options: every field is zero;
 *   - a NULL or zero-initialised (never initialised) handle: NULL.
 *
 * Same stub backend as the other run probes in `just check c`.
 */

#include "stub_rmw_backend.h"

#include <rclc/action.h>
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

static const rclc_message_type_t MSG_TYPE = {
    .type_name = "std_msgs::msg::dds_::Int32_",
    .type_hash = "RIHS01_0000000000000000000000000000000000000000000000000000000000000000",
    .serialized_size_max = 64,
};

static const rclc_service_type_t SRV_TYPE = {
    .type_name = "example_interfaces::srv::dds_::AddTwoInts_",
    .type_hash = "RIHS01_0000000000000000000000000000000000000000000000000000000000000000",
};

static const rclc_action_type_t ACTION_TYPE = {
    .type_name = "example_interfaces::action::dds_::Fibonacci_",
    .type_hash = "RIHS01_0000000000000000000000000000000000000000000000000000000000000000",
    .goal_serialized_size_max = 64,
    .result_serialized_size_max = 64,
    .feedback_serialized_size_max = 64,
};

static enum rclc_goal_response_t reject_goal(struct rcl_action_server_t* server,
                                             const struct rclc_goal_handle_t* goal,
                                             const uint8_t* request, size_t len, void* ctx) {
    (void)server;
    (void)goal;
    (void)request;
    (void)len;
    (void)ctx;
    return RCLC_GOAL_REJECT;
}

/* A scheduling context nothing binds until the executor attaches the entity,
 * which this probe never does -- init only stores it. */
#define SC 3

static void publisher(rcl_node_t* node) {
    CHECK(rcl_publisher_get_options(NULL) == NULL, "publisher: NULL handle -> NULL");
    rcl_publisher_t p = rcl_get_zero_initialized_publisher();
    CHECK(rcl_publisher_get_options(&p) == NULL, "publisher: uninitialised -> NULL");

    rcl_publisher_options_t opts = rcl_publisher_get_default_options();
    CHECK_RET(rclc_publisher_init_with_options(&p, node, &MSG_TYPE, "/opts_pub", NULL, &opts),
              RCL_RET_OK, "publisher init with options");
    const rcl_publisher_options_t* got = rcl_publisher_get_options(&p);
    CHECK(got != NULL && got != &opts, "publisher: a pointer into the handle");
    CHECK(got != NULL && memcmp(got, &opts, sizeof opts) == 0, "publisher: the options passed");
    (void)rcl_publisher_fini(&p, node);
}

static void subscription(rcl_node_t* node) {
    CHECK(rcl_subscription_get_options(NULL) == NULL, "subscription: NULL handle -> NULL");
    rcl_subscription_t s = rcl_get_zero_initialized_subscription();
    CHECK(rcl_subscription_get_options(&s) == NULL, "subscription: uninitialised -> NULL");

    rcl_subscription_options_t opts = rcl_subscription_get_default_options();
    opts.sched_context = SC;
    opts.message_info = 1;
    CHECK_RET(rclc_subscription_init_with_options(&s, node, &MSG_TYPE, "/opts_sub", NULL, &opts),
              RCL_RET_OK, "subscription init with options");
    const rcl_subscription_options_t* got = rcl_subscription_get_options(&s);
    CHECK(got != NULL && got != &opts, "subscription: a pointer into the handle");
    CHECK(got != NULL && got->sched_context == SC, "subscription: the sched context passed");
    CHECK(got != NULL && got->message_info == 1, "subscription: the message-info flag passed");
    (void)rcl_subscription_fini(&s, node);

    rcl_subscription_t d = rcl_get_zero_initialized_subscription();
    CHECK_RET(rclc_subscription_init_default(&d, node, &MSG_TYPE, "/opts_sub_default"), RCL_RET_OK,
              "subscription init without options");
    got = rcl_subscription_get_options(&d);
    CHECK(got != NULL && got->sched_context == 0 && got->message_info == 0,
          "subscription: no options -> all zero");
    (void)rcl_subscription_fini(&d, node);
}

static void service(rcl_node_t* node) {
    CHECK(rcl_service_get_options(NULL) == NULL, "service: NULL handle -> NULL");
    rcl_service_t s = rcl_get_zero_initialized_service();
    CHECK(rcl_service_get_options(&s) == NULL, "service: uninitialised -> NULL");

    rcl_service_options_t opts = rcl_service_get_default_options();
    opts.sched_context = SC;
    CHECK_RET(rclc_service_init_with_options(&s, node, &SRV_TYPE, "/opts_srv", NULL, &opts),
              RCL_RET_OK, "service init with options");
    const rcl_service_options_t* got = rcl_service_get_options(&s);
    CHECK(got != NULL && got->sched_context == SC, "service: the sched context passed");
    (void)rcl_service_fini(&s, node);

    rcl_service_t d = rcl_get_zero_initialized_service();
    CHECK_RET(rclc_service_init_default(&d, node, &SRV_TYPE, "/opts_srv_default"), RCL_RET_OK,
              "service init without options");
    got = rcl_service_get_options(&d);
    CHECK(got != NULL && got->sched_context == 0, "service: no options -> zero");
    (void)rcl_service_fini(&d, node);
}

static void client(rcl_node_t* node) {
    CHECK(rcl_client_get_options(NULL) == NULL, "client: NULL handle -> NULL");
    rcl_client_t c = rcl_get_zero_initialized_client();
    CHECK(rcl_client_get_options(&c) == NULL, "client: uninitialised -> NULL");

    rcl_client_options_t opts = rcl_client_get_default_options();
    opts.sched_context = SC;
    CHECK_RET(rclc_client_init_with_options(&c, node, &SRV_TYPE, "/opts_cli", NULL, &opts),
              RCL_RET_OK, "client init with options");
    const rcl_client_options_t* got = rcl_client_get_options(&c);
    CHECK(got != NULL && got->sched_context == SC, "client: the sched context passed");
    (void)rcl_client_fini(&c, node);
}

static void action_client(rcl_node_t* node) {
    CHECK(rcl_action_client_get_options(NULL) == NULL, "action client: NULL handle -> NULL");
    struct rcl_action_client_t c = rcl_action_get_zero_initialized_client();
    CHECK(rcl_action_client_get_options(&c) == NULL, "action client: uninitialised -> NULL");

    rcl_action_client_options_t opts = rcl_action_client_get_default_options();
    opts.sched_context = SC;
    CHECK_RET(rcl_action_client_init(&c, node, &ACTION_TYPE, "opts_fib", &opts), RCL_RET_OK,
              "action client init with options");
    const rcl_action_client_options_t* got = rcl_action_client_get_options(&c);
    CHECK(got != NULL && got->sched_context == SC, "action client: the sched context passed");
    (void)rcl_action_client_fini(&c, node);
}

static void action_server(rcl_node_t* node) {
    CHECK(rcl_action_server_get_options(NULL) == NULL, "action server: NULL handle -> NULL");
    struct rcl_action_server_t s = rcl_action_get_zero_initialized_server();
    CHECK(rcl_action_server_get_options(&s) == NULL, "action server: uninitialised -> NULL");

    rcl_action_server_options_t opts = rcl_action_server_get_default_options();
    opts.sched_context = SC;
    CHECK_RET(rclc_action_server_init_with_options(&s, node, "opts_fib_srv", &ACTION_TYPE,
                                                   reject_goal, NULL, NULL, NULL, NULL, &opts),
              RCL_RET_OK, "action server init with options");
    const rcl_action_server_options_t* got = rcl_action_server_get_options(&s);
    CHECK(got != NULL && got->sched_context == SC, "action server: the sched context passed");
    (void)rcl_action_server_fini(&s, node);
}

int main(void) {
    setenv("NROS_RMW", NROS_STUB_RMW_NAME, 1);
    nros_stub_rmw_set_accept_entities(true);

    struct rclc_support_t support = rclc_support_get_zero_initialized();
    CHECK_RET(rclc_support_init_rmw(&support, "stub://none", 47, "entity_get_options",
                                    NROS_STUB_RMW_NAME),
              RCL_RET_OK, "support opened on the stub backend");
    rclc_executor_t executor = rclc_executor_get_zero_initialized_executor();
    CHECK_RET(rclc_executor_init(&executor, &support.context, 4, NULL), RCL_RET_OK,
              "executor initialised");
    rcl_node_options_t node_opts = rcl_node_get_default_options();
    rcl_node_t node = rcl_get_zero_initialized_node();
    CHECK_RET(rclc_executor_node_init(&executor, &node, "opts", &node_opts), RCL_RET_OK,
              "node bound to the executor");
    if (s_failures != 0) {
        return 1;
    }

    publisher(&node);
    subscription(&node);
    service(&node);
    client(&node);
    action_client(&node);
    action_server(&node);

    (void)rcl_node_fini(&node);
    (void)rclc_executor_fini(&executor);
    if (s_failures != 0) {
        fprintf(stderr, "entity_get_options: %d failure(s)\n", s_failures);
        return 1;
    }
    printf("entity_get_options: OK\n");
    return 0;
}
