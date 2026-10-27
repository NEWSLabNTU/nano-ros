/* `rclc_node_get_rmw_implementation_identifier` — ROS 2's
 * `rmw_get_implementation_identifier()`, asked per node.
 *
 * It reads the backend's `get_implementation_identifier` vtable slot, and for
 * a backend that leaves the slot NULL (the stub does) answers the name the
 * vtable was REGISTERED under. This probe checks:
 *
 *   1. a node on the stub's session answers the stub's registered name;
 *   2. a node on a session of a DIFFERENT backend in the same image answers
 *      that backend's name — per session, never one image-wide value;
 *   3. a NULL or uninitialised node answers NULL.
 */

#include "stub_rmw_backend.h"

#include <rclc/rclc.h>

#include <stdio.h>
#include <string.h>

void nros_app_register_backends(void);
void nros_app_register_backends(void) {
    (void)nros_stub_rmw_register();
    /* A second, distinct vtable (no message-info slots), so the image links
     * two backends that must answer two names. */
    (void)nros_stub_rmw_register_without_message_info("stub_other");
}

static int s_failures = 0;

#define CHECK(cond, msg)                                                                           \
    do {                                                                                           \
        if (!(cond)) {                                                                             \
            fprintf(stderr, "FAIL: %s (%s:%d)\n", (msg), __FILE__, __LINE__);                      \
            s_failures++;                                                                          \
        }                                                                                          \
    } while (0)

static bool name_is(const char* got, const char* want) {
    return got != NULL && strcmp(got, want) == 0;
}

int main(void) {
    nros_stub_rmw_set_accept_entities(true);

    CHECK(rclc_node_get_rmw_implementation_identifier(NULL) == NULL, "NULL node answers NULL");
    rcl_node_t never_init = rcl_get_zero_initialized_node();
    CHECK(rclc_node_get_rmw_implementation_identifier(&never_init) == NULL,
          "an uninitialised node answers NULL");

    struct rclc_support_t support = rclc_support_get_zero_initialized();
    CHECK(rclc_support_init_rmw(&support, "stub://none", 51, "rmw_ident", NROS_STUB_RMW_NAME) ==
              RCL_RET_OK,
          "support opened on the stub backend");
    rclc_executor_t executor = rclc_executor_get_zero_initialized_executor();
    CHECK(rclc_executor_init(&executor, &support.context, 2, NULL) == RCL_RET_OK,
          "executor initialised");

    rcl_node_options_t opts = rcl_node_get_default_options();
    rcl_node_t node = rcl_get_zero_initialized_node();
    CHECK(rclc_executor_node_init(&executor, &node, "on_stub", &opts) == RCL_RET_OK,
          "node bound to the stub session");
    const char* name = rclc_node_get_rmw_implementation_identifier(&node);
    CHECK(name_is(name, NROS_STUB_RMW_NAME),
          "a NULL slot answers the name the vtable was registered under");

    /* Two calls, one address: the answer is static storage. */
    CHECK(name == rclc_node_get_rmw_implementation_identifier(&node),
          "the name is static storage, not a per-call buffer");

    struct rclc_support_t other = rclc_support_get_zero_initialized();
    CHECK(rclc_support_init_rmw(&other, "stub://none", 52, "rmw_ident_other", "stub_other") ==
              RCL_RET_OK,
          "support opened on the second backend");
    rcl_node_t other_node = rcl_get_zero_initialized_node();
    CHECK(rclc_node_init_default(&other_node, "on_other", "", &other) == RCL_RET_OK,
          "node on the second backend");
    CHECK(name_is(rclc_node_get_rmw_implementation_identifier(&other_node), "stub_other"),
          "a node on another backend's session answers that backend's name");
    CHECK(name_is(rclc_node_get_rmw_implementation_identifier(&node), NROS_STUB_RMW_NAME),
          "...and the first node's answer did not change");

    (void)rcl_node_fini(&other_node);
    (void)rclc_support_fini(&other);
    (void)rcl_node_fini(&node);
    (void)rclc_executor_fini(&executor);
    (void)rclc_support_fini(&support);

    if (s_failures != 0) {
        fprintf(stderr, "rmw_implementation_identifier: %d failure(s)\n", s_failures);
        return 1;
    }
    printf("rmw_implementation_identifier: OK\n");
    return 0;
}
