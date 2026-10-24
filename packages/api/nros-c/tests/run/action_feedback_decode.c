/* issue 2027 — a staged action FEEDBACK frame reaches a C client as its
 * payload and its goal id, on both C polling paths.
 *
 * The frame is `[CDR header 4][GoalId 16][feedback fields]`. Issue 2014 found
 * `rclc_action_client_poll`'s feedback arm handing the callback the BARE
 * fields — no encapsulation header — while the executor's dispatch handed the
 * same callback `[header][fields]`, and fixed it with no runtime test (the
 * stub backend could not stage a subscription sample; since issue 2015 it
 * can). This drives:
 *
 *   - `rclc_action_client_poll` (the callback tier, polled by hand): the
 *     feedback callback gets the frame's goal id and `[header][fields]`;
 *   - `rclc_polling_action_client_try_recv_feedback_raw` (the L1 polling
 *     tier): the whole frame and its goal id, the fields at
 *     `RCLC_ACTION_FEEDBACK_PAYLOAD_OFFSET`.
 *
 * Negative control, measured: the poll arm handing the bare fields (issue
 * 2014's defect) -> FAIL lines below.
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

/* The goal id's first bytes are not a CDR header. */
static const uint8_t GOAL[16] = {0xA5, 0x5A, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16};

static void stage_feedback(const char* suffix, int64_t v) {
    uint8_t frame[4 + 16 + sizeof(int64_t)] = {0x00, 0x01, 0x00, 0x00};
    memcpy(frame + 4, GOAL, 16);
    memcpy(frame + 20, &v, sizeof(v));
    CHECK(nros_stub_rmw_stage_sample(suffix, frame, sizeof(frame)), "stage a feedback frame");
}

typedef struct {
    int calls;
    uint8_t goal[16];
    uint8_t bytes[64];
    size_t len;
} seen_t;

static void on_feedback(const struct rclc_goal_uuid_t* goal_uuid, const uint8_t* feedback,
                        size_t feedback_len, void* context) {
    seen_t* s = (seen_t*)context;
    s->calls++;
    memcpy(s->goal, goal_uuid->uuid, 16);
    s->len = feedback_len < sizeof(s->bytes) ? feedback_len : sizeof(s->bytes);
    memcpy(s->bytes, feedback, s->len);
}

int main(void) {
    setenv("NROS_RMW", NROS_STUB_RMW_NAME, 1);
    nros_stub_rmw_set_accept_entities(true);
    CHECK(RCLC_ACTION_FEEDBACK_PAYLOAD_OFFSET == 20, "the feedback payload starts at byte 20");

    struct rclc_support_t support = rclc_support_get_zero_initialized();
    if (rclc_support_init_rmw(&support, "stub://none", 46, "action_feedback_decode",
                              NROS_STUB_RMW_NAME) != RCL_RET_OK) {
        fprintf(stderr, "action_feedback_decode: support init failed\n");
        return 1;
    }
    struct rclc_executor_t executor = rclc_executor_get_zero_initialized_executor();
    CHECK(rclc_executor_init(&executor, &support.context, 4, NULL) == RCL_RET_OK, "executor");
    rcl_node_options_t opts = rcl_node_get_default_options();
    struct rcl_node_t node = rcl_get_zero_initialized_node();
    CHECK(rclc_executor_node_init(&executor, &node, "fb_c", &opts) == RCL_RET_OK, "node");

    /* --- 1. rclc_action_client_poll ------------------------------------- */
    {
        seen_t seen;
        memset(&seen, 0, sizeof(seen));
        struct rcl_action_client_t client = rcl_action_get_zero_initialized_client();
        CHECK(rcl_action_client_init(&client, &node, &ACTION_TYPE, "fb_cb", NULL) == RCL_RET_OK,
              "a callback-tier action client");
        client.feedback_callback = on_feedback;
        client.context = &seen;
        CHECK(rclc_executor_attach_action_client(&executor, &client) == RCL_RET_OK, "attached");

        stage_feedback("fb_cb/_action/feedback", 0x0102030405060708LL);
        CHECK(rclc_action_client_poll(&client) == RCL_RET_OK, "rclc_action_client_poll");
        CHECK(seen.calls == 1, "the feedback callback ran once");
        CHECK(memcmp(seen.goal, GOAL, 16) == 0, "...with the frame's goal id");
        int64_t v = 0;
        if (seen.len == 12) memcpy(&v, seen.bytes + 4, sizeof(v));
        CHECK(seen.len == 12 && seen.bytes[0] == 0x00 && seen.bytes[1] == 0x01,
              "...and [frame header][fields]: 12 bytes, header first");
        CHECK(v == 0x0102030405060708LL, "...the payload intact behind the header");

        CHECK(rclc_executor_remove_action_client(&executor, &client) == RCL_RET_OK, "removed");
        CHECK(rcl_action_client_fini(&client, &node) == RCL_RET_OK, "fini");
    }

    /* --- 2. rclc_polling_action_client_try_recv_feedback_raw ------------ */
    {
        struct rclc_polling_action_client_t poll;
        memset(&poll, 0, sizeof(poll));
        CHECK(rclc_polling_action_client_init(&poll, &node, &ACTION_TYPE, "fb_poll") == RCL_RET_OK,
              "a polling action client");
        uint8_t buf[64];
        uint8_t goal[16] = {0};
        CHECK(rclc_polling_action_client_try_recv_feedback_raw(&poll, buf, sizeof(buf), &goal) == 0,
              "nothing staged: 0 bytes");
        stage_feedback("fb_poll/_action/feedback", -42);
        int32_t n = rclc_polling_action_client_try_recv_feedback_raw(&poll, buf, sizeof(buf), &goal);
        CHECK(n == 28, "the whole frame: header + goal id + fields");
        CHECK(memcmp(goal, GOAL, 16) == 0, "the goal id is the frame's");
        int64_t v = 0;
        if (n >= (int32_t)(RCLC_ACTION_FEEDBACK_PAYLOAD_OFFSET + sizeof(v)))
            memcpy(&v, buf + RCLC_ACTION_FEEDBACK_PAYLOAD_OFFSET, sizeof(v));
        CHECK(v == -42, "the fields sit at RCLC_ACTION_FEEDBACK_PAYLOAD_OFFSET");
        CHECK(rclc_polling_action_client_fini(&poll) == RCL_RET_OK, "polling fini");
    }

    (void)rcl_node_fini(&node);
    (void)rclc_executor_fini(&executor);
    (void)rclc_support_fini(&support);

    if (s_failures != 0) {
        fprintf(stderr, "action_feedback_decode: %d failure(s)\n", s_failures);
        return 1;
    }
    printf("action_feedback_decode: OK\n");
    return 0;
}
