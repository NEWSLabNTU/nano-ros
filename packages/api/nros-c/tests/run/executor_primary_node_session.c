/* issue 1405 — an entity registered through the executor lands on its NODE's
 * session, whichever slot the node took.
 *
 * Every `rclc_executor_add_*` / `attach_*` path decoded the node's slot as
 *
 *     node_id = (node_raw_id != 0).then(|| NodeId::from_raw(node_raw_id));
 *
 * and `None` registers under the executor's own identity on session 0. The
 * FIRST node an executor builds takes slot 0 (`executor_bound_node.c` asserts
 * it), so the primary node was registered as if it were a legacy node — and a
 * primary node whose options name ANOTHER backend lives on an EXTRA session
 * (`NodeBuilder::resolve_session_slot`), so its executor-registered entities
 * went to the primary session while its eager ones (`rclc_publisher_init_*`,
 * which resolve the session through `node_session_mut`) went to the right one.
 * One node, two sessions, and every call returning RCL_RET_OK.
 *
 * Measured before the fix with this probe: the publisher on session 1, every
 * executor-registered entity on session 0.
 *
 * TWO stub registrations of the same vtable, under two names, give the image
 * two sessions without a second backend; the stub reports which session each
 * `create_*` was handed (`nros_stub_rmw_last_entity_session`). The reference is
 * the eager publisher: it already resolved the node's session correctly, so
 * "correct" here is "the same session the publisher got".
 *
 * And the legacy half, because it is why the `!= 0` was written: a node from
 * `rclc_node_init_default` also carries slot 0, reaches no executor record, and
 * must keep registering under the executor's own identity. A fix that decoded
 * every slot-0 node as `NodeId::PRIMARY` would fail it with a lookup error.
 */

#include "stub_rmw_backend.h"

#include <rclc/rclc.h>

#include <stdbool.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#define OTHER_RMW_NAME "stub_b"

void nros_app_register_backends(void);
void nros_app_register_backends(void) {
    (void)nros_stub_rmw_register();
    (void)nros_stub_rmw_register_as(OTHER_RMW_NAME);
}

/* Hand-rolled rather than <assert.h>: NDEBUG would compile the probe away and
 * it would still exit 0 (issue 0196's class one level down). */
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

#define CHECK_SESSION(expected, msg)                                                               \
    do {                                                                                           \
        int32_t _s = nros_stub_rmw_last_entity_session();                                          \
        if (_s != (int32_t)(expected)) {                                                           \
            fprintf(stderr, "FAIL: %s -- landed on session %d, expected %d (%s:%d)\n", (msg),      \
                    (int)_s, (int)(expected), __FILE__, __LINE__);                                 \
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

static void on_message(const uint8_t* data, size_t len, void* ctx) {
    (void)data;
    (void)len;
    (void)ctx;
}

static void on_message_info(const uint8_t* data, size_t len, const uint8_t* attachment,
                            size_t attachment_len, void* ctx) {
    (void)data;
    (void)len;
    (void)attachment;
    (void)attachment_len;
    (void)ctx;
}

static bool on_request(const uint8_t* req, size_t req_len, uint8_t* resp, size_t resp_cap,
                       size_t* resp_len, void* ctx) {
    (void)req;
    (void)req_len;
    (void)resp;
    (void)resp_cap;
    (void)resp_len;
    (void)ctx;
    return false;
}

/* Register one of each executor-path entity on `node` and assert each landed on
 * `want`. `label` names the scenario in a failure. */
static void register_all(struct rclc_executor_t* executor, struct rcl_node_t* node, uint32_t want,
                         const char* label) {
    char msg[192];

    struct rcl_subscription_t sub = rcl_get_zero_initialized_subscription();
    CHECK_RET(rclc_subscription_init_default(&sub, node, &MSG_TYPE, "/primary_session"), RCL_RET_OK,
              "subscription_init");
    snprintf(msg, sizeof(msg), "%s: rclc_executor_add_subscription_raw", label);
    CHECK_RET(rclc_executor_add_subscription_raw(executor, &sub, on_message, NULL, ON_NEW_DATA),
              RCL_RET_OK, msg);
    CHECK_SESSION(want, msg);

    snprintf(msg, sizeof(msg), "%s: rclc_executor_add_subscription_raw_with_info", label);
    CHECK_RET(rclc_executor_add_subscription_raw_with_info(executor, node, "/primary_session_info",
                                                           MSG_TYPE.type_name, MSG_TYPE.type_hash,
                                                           NULL, on_message_info, NULL),
              RCL_RET_OK, msg);
    CHECK_SESSION(want, msg);

    struct rcl_service_t service = rcl_get_zero_initialized_service();
    CHECK_RET(rclc_service_init_default(&service, node, &SRV_TYPE, "/primary_session"), RCL_RET_OK,
              "service_init");
    snprintf(msg, sizeof(msg), "%s: rclc_executor_add_service_raw", label);
    CHECK_RET(rclc_executor_add_service_raw(executor, &service, on_request, NULL), RCL_RET_OK, msg);
    CHECK_SESSION(want, msg);

    struct rcl_client_t client = rcl_get_zero_initialized_client();
    CHECK_RET(rclc_client_init_default(&client, node, &SRV_TYPE, "/primary_session"), RCL_RET_OK,
              "client_init");
    snprintf(msg, sizeof(msg), "%s: rclc_executor_attach_client", label);
    CHECK_RET(rclc_executor_attach_client(executor, &client), RCL_RET_OK, msg);
    CHECK_SESSION(want, msg);
}

int main(void) {
    setenv("NROS_RMW", NROS_STUB_RMW_NAME, 1);
    /* Every create succeeds and the stub honours the core QoS policies, so a
     * registration reaches `create_*` instead of stopping at QoS validation. */
    nros_stub_rmw_set_accept_entities(true);

    /* ---- 1. A primary node bound to an EXTRA session -------------------- */
    {
        struct rclc_support_t support = rclc_support_get_zero_initialized();
        CHECK_RET(rclc_support_init_rmw(&support, "stub://none", 45, "primary_session",
                                        NROS_STUB_RMW_NAME),
                  RCL_RET_OK, "support opened on the stub backend");
        struct rclc_executor_t executor = rclc_executor_get_zero_initialized_executor();
        CHECK_RET(rclc_executor_init(&executor, &support.context, 16, NULL), RCL_RET_OK,
                  "executor initialised");
        uint32_t before = nros_stub_rmw_sessions_opened();
        CHECK(before >= 1, "the executor opened its primary session");
        uint32_t primary_session = before - 1;

        rcl_node_options_t opts = rcl_node_get_default_options();
        memcpy(opts.rmw_name, OTHER_RMW_NAME, strlen(OTHER_RMW_NAME));
        opts.rmw_name_len = strlen(OTHER_RMW_NAME);

        struct rcl_node_t node = rcl_get_zero_initialized_node();
        CHECK_RET(rclc_executor_node_init(&executor, &node, "bridged", &opts), RCL_RET_OK,
                  "the first node bound to the executor, naming the other backend");
        CHECK(node.node_id == 0, "the FIRST node takes slot 0 -- the premise of issue 1405");
        CHECK(nros_stub_rmw_sessions_opened() == before + 1,
              "a node naming another backend opens an EXTRA session");
        uint32_t node_session = before;
        CHECK(node_session != primary_session,
              "the probe needs the node's session to differ from the primary one, or it "
              "measures nothing");

        /* The reference: the eager publisher resolves the node's session by
         * `node_session_mut(node_id)`, which was always right. */
        struct rcl_publisher_t publisher = rcl_get_zero_initialized_publisher();
        CHECK_RET(rclc_publisher_init_default(&publisher, &node, &MSG_TYPE, "/primary_session"),
                  RCL_RET_OK, "publisher_init");
        CHECK_SESSION(node_session, "the eager publisher lands on the node's session");

        register_all(&executor, &node, node_session, "primary node on an extra session");

        (void)rclc_executor_fini(&executor);
        (void)rclc_support_fini(&support);
    }

    /* ---- 2. A legacy node keeps the executor's own identity ------------- */
    {
        struct rclc_support_t support = rclc_support_get_zero_initialized();
        CHECK_RET(rclc_support_init_rmw(&support, "stub://none", 46, "legacy_session",
                                        NROS_STUB_RMW_NAME),
                  RCL_RET_OK, "support opened on the stub backend");
        struct rclc_executor_t executor = rclc_executor_get_zero_initialized_executor();
        CHECK_RET(rclc_executor_init(&executor, &support.context, 16, NULL), RCL_RET_OK,
                  "executor initialised");
        uint32_t primary_session = nros_stub_rmw_sessions_opened() - 1;

        struct rcl_node_t legacy = rcl_get_zero_initialized_node();
        CHECK_RET(rclc_node_init_default(&legacy, "legacy", "", &support), RCL_RET_OK,
                  "a legacy node initialises");
        CHECK(legacy.executor == NULL, "a legacy node reaches no executor");
        CHECK(legacy.node_id == 0, "a legacy node carries slot 0 too -- why `!= 0` existed");

        register_all(&executor, &legacy, primary_session, "legacy node");

        (void)rclc_executor_fini(&executor);
        (void)rclc_support_fini(&support);
    }

    if (s_failures != 0) {
        fprintf(stderr, "executor_primary_node_session: %d check(s) FAILED\n", s_failures);
        return 1;
    }
    printf("executor_primary_node_session: all checks passed\n");
    return 0;
}
