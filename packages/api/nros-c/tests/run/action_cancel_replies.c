/* issue 2030 — the C callback tier TAKES cancel replies, so cancels do not
 * run out of request slots.
 *
 * Since issue 2009 each action sub-client keeps its requests in a bounded
 * table (`MAX_CONCURRENT_GOALS` = 4) and a slot comes back only when its reply
 * is taken. Nothing on the C callback tier took a cancel reply, so after four
 * `rclc_action_cancel_goal` the table was full and the fifth was refused, with
 * a healthy server answering every one.
 *
 * This sends SIX cancels, each answered by a staged reply, through both C
 * readers:
 *
 *   - `rclc_executor_spin_some` (the arena dispatch, via the trampoline), and
 *   - `rclc_action_client_poll` (the same client polled by hand),
 *
 * and checks every send succeeds and the new cancel-response callback sees
 * each reply with its goal, return code and goals_canceling count.
 *
 * Negative control, measured: registering the arena entry without the
 * cancel-response trampoline (the pre-2030 shape) -> the fifth cancel FAILs.
 */

#include "stub_rmw_backend.h"

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

static const rclc_action_type_t ACTION_TYPE = {
    .type_name = "test_msgs::action::dds_::Count_",
    .type_hash = "RIHS01_0000000000000000000000000000000000000000000000000000000000000000",
    .goal_serialized_size_max = 64,
    .result_serialized_size_max = 64,
    .feedback_serialized_size_max = 64,
};

/* More than the sub-client's 4 request slots. */
#define CANCELS 6

typedef struct {
    int calls;
    uint8_t goal[16];
    enum rclc_cancel_return_code_t code;
    size_t canceling;
} seen_t;

static void on_cancel_response(const struct rclc_goal_uuid_t* goal_uuid,
                               enum rclc_cancel_return_code_t return_code, size_t goals_canceling,
                               void* context) {
    seen_t* s = (seen_t*)context;
    s->calls++;
    memcpy(s->goal, goal_uuid->uuid, 16);
    s->code = return_code;
    s->canceling = goals_canceling;
}

/* A CancelGoal reply: `[CDR header 4][return_code i8][pad 3][count u32]` then
 * `count` GoalInfo `[uuid 16][stamp sec i32][nanosec u32]`. */
static void stage_cancel_reply(int64_t seq, const uint8_t goal[16], int8_t code, bool listed) {
    uint8_t r[12 + 24] = {0x00, 0x01, 0x00, 0x00};
    r[4] = (uint8_t)code;
    uint32_t n = listed ? 1u : 0u;
    memcpy(r + 8, &n, sizeof(n));
    if (listed) memcpy(r + 12, goal, 16);
    CHECK(nros_stub_rmw_stage_reply_for(seq, r, listed ? sizeof(r) : 12),
          "stage a cancel reply");
}

static void goal_for(int i, uint8_t out[16]) {
    for (int b = 0; b < 16; ++b) out[b] = (uint8_t)(0x40 + i * 16 + b);
}

int main(void) {
    setenv("NROS_RMW", NROS_STUB_RMW_NAME, 1);
    nros_stub_rmw_set_accept_entities(true);

    struct rclc_support_t support = rclc_support_get_zero_initialized();
    if (rclc_support_init_rmw(&support, "stub://none", 47, "action_cancel_replies",
                              NROS_STUB_RMW_NAME) != RCL_RET_OK) {
        fprintf(stderr, "action_cancel_replies: support init failed\n");
        return 1;
    }
    struct rclc_executor_t executor = rclc_executor_get_zero_initialized_executor();
    CHECK(rclc_executor_init(&executor, &support.context, 4, NULL) == RCL_RET_OK, "executor");
    rcl_node_options_t opts = rcl_node_get_default_options();
    struct rcl_node_t node = rcl_get_zero_initialized_node();
    CHECK(rclc_executor_node_init(&executor, &node, "cancel_c", &opts) == RCL_RET_OK, "node");

    seen_t seen;
    memset(&seen, 0, sizeof(seen));
    struct rcl_action_client_t client = rcl_action_get_zero_initialized_client();
    CHECK(rcl_action_client_init(&client, &node, &ACTION_TYPE, "cancels", NULL) == RCL_RET_OK,
          "a callback-tier action client");
    CHECK(rclc_executor_attach_action_client(&executor, &client) == RCL_RET_OK, "attached");
    CHECK(rclc_action_client_set_cancel_response_callback(&client, on_cancel_response, &seen) ==
              RCL_RET_OK,
          "set the cancel-response callback");

    /* --- 1. the executor's spin takes each reply ------------------------ */
    for (int i = 0; i < CANCELS; ++i) {
        uint8_t goal[16];
        goal_for(i, goal);
        struct rclc_goal_uuid_t uuid;
        memcpy(uuid.uuid, goal, 16);
        rcl_ret_t r = rclc_action_cancel_goal(&client, &uuid);
        if (r != RCL_RET_OK) {
            fprintf(stderr, "FAIL: spin: cancel %d of %d refused (rc=%d)\n", i + 1, CANCELS, (int)r);
            s_failures++;
            break;
        }
        stage_cancel_reply(nros_stub_rmw_last_request_seq(), goal, 0, true);
        (void)rclc_executor_spin_some(&executor, 0);
        CHECK(seen.calls == i + 1, "spin: the cancel-response callback ran for this reply");
        CHECK(memcmp(seen.goal, goal, 16) == 0, "spin: ...about the goal the request named");
        CHECK(seen.code == RCLC_CANCEL_RC_OK && seen.canceling == 1,
              "spin: ...with ERROR_NONE and one goal canceling");
    }
    CHECK(nros_stub_rmw_staged_replies() == 0, "spin: every staged reply was taken");

    /* --- 2. rclc_action_client_poll takes each reply too ----------------- */
    int base = seen.calls;
    for (int i = 0; i < CANCELS; ++i) {
        uint8_t goal[16];
        goal_for(i + CANCELS, goal);
        struct rclc_goal_uuid_t uuid;
        memcpy(uuid.uuid, goal, 16);
        rcl_ret_t r = rclc_action_cancel_goal(&client, &uuid);
        if (r != RCL_RET_OK) {
            fprintf(stderr, "FAIL: poll: cancel %d of %d refused (rc=%d)\n", i + 1, CANCELS, (int)r);
            s_failures++;
            break;
        }
        /* A refused cancel: REJECTED, nothing listed. */
        stage_cancel_reply(nros_stub_rmw_last_request_seq(), goal, 1, false);
        CHECK(rclc_action_client_poll(&client) == RCL_RET_OK, "rclc_action_client_poll");
        CHECK(seen.calls == base + i + 1, "poll: the cancel-response callback ran");
        CHECK(seen.code == RCLC_CANCEL_RC_REJECTED && seen.canceling == 0,
              "poll: ...with ERROR_REJECTED and nothing canceling");
    }

    CHECK(rclc_executor_remove_action_client(&executor, &client) == RCL_RET_OK, "removed");
    CHECK(rcl_action_client_fini(&client, &node) == RCL_RET_OK, "fini");
    (void)rcl_node_fini(&node);
    (void)rclc_executor_fini(&executor);
    (void)rclc_support_fini(&support);

    if (s_failures != 0) {
        fprintf(stderr, "action_cancel_replies: %d failure(s)\n", s_failures);
        return 1;
    }
    printf("action_cancel_replies: OK\n");
    return 0;
}
