/* issue 1384 — an executor-bound node can create its EAGER entities, whichever
 * slot it got.
 *
 * `nros_executor_node_init` stores the slot `NodeBuilder::build()` returns, and
 * the FIRST node an executor builds takes slot 0 (`executor_param_node_keying.c`
 * asserts that in words). `nros_node_t::is_multi_session()` also required
 * `node_id != 0`, so a node bound through the very call that binds it read as a
 * LEGACY node whenever it was the only one — the commonest shape in the tree.
 *
 * The severe consequence is the one this TU is about.
 * `nros_executor_node_init` leaves `support` NULL **on purpose** (phase-156:
 * the executor-bound paths key off the executor, not off support), so the
 * legacy arm of `resolve_session_and_domain` had nothing to read and answered
 * `None`. Every C entity created EAGERLY rather than at registration —
 * `rclc_publisher_init_default` and its QoS/options siblings, the polling
 * subscription, `nros_service_init_polling`, `nros_client_init_polling` —
 * therefore failed `NROS_RET_NOT_INIT` on the primary node, BEFORE reaching the
 * backend at all.
 *
 * A compile or signature check cannot see any of that: every one of those
 * functions has the right prototype and returns an `nros_ret_t` either way.
 * Only a run reaches the session.
 *
 * TWO nodes, because the discriminator is a COMPARISON. The stub backend
 * refuses `create_*` with `NROS_RMW_RET_UNSUPPORTED`, so a correct call cannot
 * return `NROS_RET_OK` here — what it CAN do is reach the backend, and the
 * second node (slot 1) was always able to. So the assertion is: the primary
 * node's answer is the SECOND node's answer, and neither is `NROS_RET_NOT_INIT`
 * — the return that means "we never got as far as asking". The pre-fix
 * measurement in issue 1384 is exactly this pair: `-7` for the primary node and
 * `-1` for the second.
 *
 * The names go on the wire too. Each create is checked against the name the
 * backend was actually handed (`nros_stub_rmw_last_entity_name`), because
 * `resolve_entity_name_on_node` — a THIRD site of the same predicate — chooses
 * between "expand + apply this node's remap rules" and "expand only", and the
 * wrong arm is silent: it returns a plausible name and `NROS_RET_OK`. The remap
 * TABLE has no C entry point, so the rule-application half is asserted in the
 * `nros-c` unit tests (`node::accessor_tests`); what a C image can prove is that
 * the resolved name — not the source spelling — is what the backend sees, for
 * BOTH slots.
 *
 * Same stub backend and same archive as the other run probes in this lane, so
 * it needs no router, no agent and no network.
 */

#include "stub_rmw_backend.h"

#include <nros/nros.h>

#include <stdbool.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

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

#define CHECK_STR(actual, expected, msg)                                                           \
    do {                                                                                           \
        const char* _a = (actual);                                                                 \
        if (_a == NULL || strcmp(_a, (expected)) != 0) {                                           \
            fprintf(stderr, "FAIL: %s -- got '%s', expected '%s' (%s:%d)\n", (msg),                \
                    _a == NULL ? "(null)" : _a, (expected), __FILE__, __LINE__);                   \
            s_failures++;                                                                          \
        }                                                                                          \
    } while (0)

/* ---- Types the eager creates need --------------------------------------- */

static const nros_message_type_t MSG_TYPE = {
    .type_name = "std_msgs::msg::dds_::Int32_",
    .type_hash = "RIHS01_0000000000000000000000000000000000000000000000000000000000000000",
    .serialized_size_max = 8,
};

static const nros_service_type_t SRV_TYPE = {
    .type_name = "example_interfaces::srv::dds_::AddTwoInts_",
    .type_hash = "RIHS01_0000000000000000000000000000000000000000000000000000000000000000",
};

/* ---- One node's four eager creates --------------------------------------
 *
 * Returns the four return codes through `out`, and asserts the wire name each
 * create handed the backend. `label` names the slot in a failure.
 */

typedef struct {
    nros_ret_t publisher;
    nros_ret_t subscription;
    nros_ret_t service;
    nros_ret_t client;
} eager_results_t;

static void eager_creates(const nros_node_t* node, const char* label, const char* expected_topic,
                          const char* expected_service, eager_results_t* out) {
    char msg[192];

    nros_publisher_t publisher = rcl_get_zero_initialized_publisher();
    nros_stub_rmw_clear_last_entity_name();
    out->publisher = rclc_publisher_init_default(&publisher, node, &MSG_TYPE, "chatter");
    snprintf(msg, sizeof(msg), "%s: publisher reached the backend under its RESOLVED name", label);
    CHECK_STR(nros_stub_rmw_last_entity_name(), expected_topic, msg);

    nros_subscription_t subscription = rcl_get_zero_initialized_subscription();
    nros_stub_rmw_clear_last_entity_name();
    out->subscription = nros_subscription_init_polling(&subscription, node, &MSG_TYPE, "chatter");
    snprintf(msg, sizeof(msg),
             "%s: polling subscription reached the backend under its RESOLVED name", label);
    CHECK_STR(nros_stub_rmw_last_entity_name(), expected_topic, msg);

    nros_service_t service = rcl_get_zero_initialized_service();
    nros_stub_rmw_clear_last_entity_name();
    out->service = nros_service_init_polling(&service, node, &SRV_TYPE, "add");
    snprintf(msg, sizeof(msg), "%s: polling service reached the backend under its RESOLVED name",
             label);
    CHECK_STR(nros_stub_rmw_last_entity_name(), expected_service, msg);

    nros_client_t client = rcl_get_zero_initialized_client();
    nros_stub_rmw_clear_last_entity_name();
    out->client = nros_client_init_polling(&client, node, &SRV_TYPE, "add");
    snprintf(msg, sizeof(msg), "%s: polling client reached the backend under its RESOLVED name",
             label);
    CHECK_STR(nros_stub_rmw_last_entity_name(), expected_service, msg);
}

int main(void) {
    /* The stub must be the backend this image opens against. `$NROS_RMW` is the
     * hosted rung of precedence model A and beats the baked selector. */
    setenv("NROS_RMW", NROS_STUB_RMW_NAME, 1);

    struct nros_support_t support = nros_support_get_zero_initialized();
    CHECK_RET(nros_support_init_rmw(&support, "stub://none", 44, "executor_bound_node",
                                    NROS_STUB_RMW_NAME),
              NROS_RET_OK, "support opened on the stub backend");

    struct nros_executor_t executor = rclc_executor_get_zero_initialized_executor();
    CHECK_RET(nros_executor_init(&executor, &support, 8), NROS_RET_OK, "executor initialised");

    /* Both nodes take the same namespace, so the only thing that differs
     * between them is the slot. That is the point. */
    nros_node_options_t opts = rcl_node_get_default_options();
    const char* ns = "/sensing";
    memcpy(opts.namespace_, ns, strlen(ns));
    opts.namespace_len = strlen(ns);

    struct nros_node_t primary = rcl_get_zero_initialized_node();
    CHECK_RET(nros_executor_node_init(&executor, &primary, "filter", &opts), NROS_RET_OK,
              "the first node bound to the executor");

    struct nros_node_t second = rcl_get_zero_initialized_node();
    CHECK_RET(nros_executor_node_init(&executor, &second, "shaper", &opts), NROS_RET_OK,
              "the second node bound to the executor");

    /* ---- 1. The fact the whole issue turns on --------------------------- */

    CHECK(primary.node_id == 0,
          "the FIRST node an executor builds takes slot 0 -- if this ever stops being true, the "
          "premise of issue 1384 has moved and the assertions below need re-deriving");
    CHECK(second.node_id == 1, "the second node takes slot 1");
    CHECK(primary.executor != NULL && second.executor != NULL,
          "both nodes reach the executor that built them");
    CHECK(primary.support == NULL,
          "nros_executor_node_init leaves support NULL on purpose -- this is WHY routing an "
          "executor-bound node down the legacy arm cannot work");

    CHECK(rcl_node_is_valid(&primary), "the primary node is valid");
    CHECK(rcl_node_is_valid(&second), "the second node is valid");

    /* ---- 2. The remap table is READABLE from either slot ----------------
     *
     * `only_expand = false` is the arm that consults the executor's rules, so
     * it must answer for any node that reaches an executor. The primary node
     * used to return NROS_RET_NOT_INIT here -- the reported symptom, and the
     * mildest of the three outcomes.
     */

    /* Zeroed before every call so a REFUSED resolve reads as '' rather than as
     * whatever was on the stack — a refusal must not look like a name. */
    char resolved[128] = {0};
    CHECK_RET(nros_node_resolve_name(&primary, "chatter", false, resolved, sizeof(resolved)),
              NROS_RET_OK, "the primary node can read its remap table");
    CHECK_STR(resolved, "/sensing/chatter", "the primary node's resolved name");

    memset(resolved, 0, sizeof(resolved));
    CHECK_RET(nros_node_resolve_name(&second, "chatter", false, resolved, sizeof(resolved)),
              NROS_RET_OK, "the second node can read its remap table");
    CHECK_STR(resolved, "/sensing/chatter", "the second node's resolved name");

    /* ---- 3. The eager entity creates REACH the backend ------------------ */

    eager_results_t primary_rc;
    eager_results_t second_rc;
    eager_creates(&primary, "primary(slot 0)", "/sensing/chatter", "/sensing/add", &primary_rc);
    eager_creates(&second, "second(slot 1)", "/sensing/chatter", "/sensing/add", &second_rc);

    /* NOT_INIT is the discriminator: it is what resolve_session_and_domain
     * returns when it found no session, i.e. the call never reached the
     * backend. Anything else means it did. */
    CHECK(primary_rc.publisher != NROS_RET_NOT_INIT,
          "publisher_init on the PRIMARY node must reach the backend, not die at the session "
          "lookup (issue 1384: this was -7)");
    CHECK(primary_rc.subscription != NROS_RET_NOT_INIT,
          "polling subscription_init on the PRIMARY node must reach the backend");
    CHECK(primary_rc.service != NROS_RET_NOT_INIT,
          "polling service_init on the PRIMARY node must reach the backend");
    CHECK(primary_rc.client != NROS_RET_NOT_INIT,
          "polling client_init on the PRIMARY node must reach the backend");

    /* And the positive control beside it: two nodes on one executor, identical
     * in everything a caller can see, must answer identically. This is the
     * assertion that keeps the fix from being "make both fail the same way" --
     * the line above already refuses NOT_INIT, so agreeing means agreeing on a
     * value that reached the backend. */
    CHECK(primary_rc.publisher == second_rc.publisher,
          "the two slots must answer publisher_init identically");
    CHECK(primary_rc.subscription == second_rc.subscription,
          "the two slots must answer subscription_init identically");
    CHECK(primary_rc.service == second_rc.service,
          "the two slots must answer service_init identically");
    CHECK(primary_rc.client == second_rc.client,
          "the two slots must answer client_init identically");

    /* ---- 4. A legacy node is unchanged ----------------------------------
     *
     * `rclc_node_init_default` reaches no executor, so it keeps the legacy arm
     * and keeps refusing the remap read. The fix must not have turned every
     * node into an executor-bound one.
     */

    struct nros_node_t legacy = rcl_get_zero_initialized_node();
    CHECK_RET(rclc_node_init_default(&legacy, "legacy", "/sensing", &support), NROS_RET_OK,
              "a legacy node still initialises");
    CHECK(legacy.executor == NULL, "a legacy node reaches no executor");
    memset(resolved, 0, sizeof(resolved));
    CHECK_RET(nros_node_resolve_name(&legacy, "chatter", false, resolved, sizeof(resolved)),
              NROS_RET_NOT_INIT,
              "a node that reaches no executor cannot read remap rules and must SAY so rather "
              "than returning the bare expansion");
    memset(resolved, 0, sizeof(resolved));
    CHECK_RET(nros_node_resolve_name(&legacy, "chatter", true, resolved, sizeof(resolved)),
              NROS_RET_OK, "expansion alone needs no executor");
    CHECK_STR(resolved, "/sensing/chatter", "the legacy node's expansion");

    (void)rcl_node_fini(&legacy);
    (void)rcl_node_fini(&second);
    (void)rcl_node_fini(&primary);
    (void)rclc_executor_fini(&executor);
    (void)rclc_support_fini(&support);

    if (s_failures != 0) {
        fprintf(stderr, "executor_bound_node: %d check(s) FAILED\n", s_failures);
        return 1;
    }
    printf("executor_bound_node: all checks passed\n");
    return 0;
}
