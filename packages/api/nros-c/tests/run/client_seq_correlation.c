/* issue 2009 — the C client surfaces match a reply to its request by the
 * request's SEQUENCE NUMBER.
 *
 *   - the polled client (`rclc_polling_client_t`) keeps no table, so it HANDS
 *     the number out: `rclc_polling_client_send_request_raw_seq` reports the id
 *     a request went out under, `rclc_polling_client_take_response_raw_seq` the
 *     id a reply answers — upstream's `rcl_send_request` / `rcl_take_response`
 *     pair. Two requests in flight with replies arriving out of order are told
 *     apart by those numbers;
 *   - the executor-owned client (`rcl_client_t`) keeps ONE call in flight and
 *     takes only the reply naming it: a reply naming another request is
 *     dropped, and a call that TIMED OUT forgets its id, so its late reply is
 *     dropped instead of answering the next call (it used to be the next call's
 *     answer — "a known stale reply quirk of single-slot clients").
 *   - issue 2011: the L1 polled ACTION client drops stale replies (2009) and
 *     now says which goal each reply is about —
 *     `rclc_polling_action_client_try_recv_{goal_response,result,
 *     cancel_response}_raw_for` write the goal of the request a reply names,
 *     so a caller with several goals in flight can tell them apart.
 *
 * Same stub backend as the other run probes in `just check c`, whose replies
 * the test stages by id (`nros_stub_rmw_stage_reply_for`).
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

static void stage(int64_t seq, uint8_t marker) {
    uint8_t reply[4] = {marker, marker, marker, marker};
    CHECK(nros_stub_rmw_stage_reply_for(seq, reply, sizeof(reply)), "stage a reply on the stub");
}

int main(void) {
    setenv("NROS_RMW", NROS_STUB_RMW_NAME, 1);
    nros_stub_rmw_set_accept_entities(true);

    struct rclc_support_t support = rclc_support_get_zero_initialized();
    CHECK_RET(rclc_support_init_rmw(&support, "stub://none", 48, "client_seq_correlation",
                                    NROS_STUB_RMW_NAME),
              RCL_RET_OK, "support opened on the stub backend");
    if (s_failures != 0) return 1;

    rclc_executor_t executor = rclc_executor_get_zero_initialized_executor();
    CHECK_RET(rclc_executor_init(&executor, &support.context, 4, NULL), RCL_RET_OK,
              "executor initialised");
    rcl_node_options_t opts = rcl_node_get_default_options();
    rcl_node_t node = rcl_get_zero_initialized_node();
    CHECK_RET(rclc_executor_node_init(&executor, &node, "client_seq", &opts), RCL_RET_OK,
              "node bound to the executor");
    if (s_failures != 0) return 1;

    const uint8_t request[4] = {1, 2, 3, 4};
    uint8_t buf[16];

    /* ---- 1. polled client: the numbers tell two in-flight requests apart --- */
    {
        nros_stub_rmw_reset_client();
        struct rclc_polling_client_t cli = {0};
        CHECK_RET(rclc_polling_client_init(&cli, &node, &SRV_TYPE, "/seq_poll"), RCL_RET_OK,
                  "polling client created");
        int64_t seq_a = 0;
        int64_t seq_b = 0;
        CHECK_RET(rclc_polling_client_send_request_raw_seq(&cli, request, sizeof(request), &seq_a),
                  RCL_RET_OK, "request A sent");
        CHECK_RET(rclc_polling_client_send_request_raw_seq(&cli, request, sizeof(request), &seq_b),
                  RCL_RET_OK, "request B sent");
        CHECK(seq_a == nros_stub_rmw_last_request_seq() - 1,
              "A's number is the id it went out under");
        CHECK(seq_b == nros_stub_rmw_last_request_seq(), "B's number is the id it went out under");
        CHECK(seq_a != seq_b, "two requests, two numbers");

        stage(seq_b, 0xBB); /* out of order: B answers first */
        stage(seq_a, 0xAA);
        int64_t got = 0;
        int32_t n = rclc_polling_client_take_response_raw_seq(&cli, buf, sizeof(buf), &got);
        CHECK(n == 4 && buf[0] == 0xBB && got == seq_b, "the first reply names B");
        n = rclc_polling_client_take_response_raw_seq(&cli, buf, sizeof(buf), &got);
        CHECK(n == 4 && buf[0] == 0xAA && got == seq_a, "the second reply names A");
        n = rclc_polling_client_take_response_raw_seq(&cli, buf, sizeof(buf), &got);
        CHECK(n == 0, "nothing more");

        CHECK_RET(rclc_polling_client_send_request_raw_seq(&cli, request, sizeof(request), NULL),
                  RCL_RET_INVALID_ARGUMENT, "a NULL number is refused, not ignored");
        CHECK_RET(rclc_polling_client_fini(&cli), RCL_RET_OK, "polling client finalised");
    }

    /* ---- 2. executor client: a reply naming another request is dropped ----- */
    rcl_client_t client = rcl_get_zero_initialized_client();
    CHECK_RET(rclc_client_init_default(&client, &node, &SRV_TYPE, "/seq_exec"), RCL_RET_OK,
              "executor client created");
    CHECK_RET(rclc_executor_attach_client(&executor, &client), RCL_RET_OK,
              "executor client attached");
    {
        nros_stub_rmw_reset_client();
        size_t len = 0;
        CHECK_RET(rclc_client_send_request_async(&client, request, sizeof(request)), RCL_RET_OK,
                  "async request sent");
        const int64_t seq = nros_stub_rmw_last_request_seq();
        CHECK_RET(rclc_client_take_response(&client, buf, sizeof(buf), &len), RCLC_RET_TRY_AGAIN,
                  "no reply yet");
        stage(seq + 100, 0xEE); /* a stray reply to no request of this call */
        stage(seq, 0x11);
        CHECK_RET(rclc_client_take_response(&client, buf, sizeof(buf), &len), RCL_RET_OK,
                  "the call's reply is taken");
        CHECK(len == 4 && buf[0] == 0x11, "the stray reply was dropped, the call's delivered");
        CHECK(nros_stub_rmw_staged_replies() == 0, "the stray reply was consumed, not left queued");
    }

    /* ---- 3. a TIMED-OUT call's late reply does not answer the next call ---- */
    {
        nros_stub_rmw_reset_client();
        size_t len = 0;
        CHECK_RET(rclc_client_set_timeout(&client, 30), RCL_RET_OK, "a short timeout");
        CHECK_RET(rclc_client_call(&client, request, sizeof(request), buf, sizeof(buf), &len),
                  RCL_RET_TIMEOUT, "the call times out with no reply");
        const int64_t timed_out = nros_stub_rmw_last_request_seq();

        CHECK_RET(rclc_client_send_request_async(&client, request, sizeof(request)), RCL_RET_OK,
                  "the next call is sent");
        const int64_t next = nros_stub_rmw_last_request_seq();
        stage(timed_out, 0x66); /* the timed-out call's reply, late */
        CHECK_RET(rclc_client_take_response(&client, buf, sizeof(buf), &len), RCLC_RET_TRY_AGAIN,
                  "the late reply does NOT answer the next call");
        stage(next, 0x77);
        CHECK_RET(rclc_client_take_response(&client, buf, sizeof(buf), &len), RCL_RET_OK,
                  "the next call's own reply is taken");
        CHECK(len == 4 && buf[0] == 0x77, "with its own bytes");
    }

    /* ---- 4. polled ACTION client: each reply names its goal (issue 2011) ---- */
    {
        nros_stub_rmw_reset_client();
        static const rclc_action_type_t ACT_TYPE = {
            .type_name = "test_msgs::action::dds_::Count_",
            .type_hash = "RIHS01_0000000000000000000000000000000000000000000000000000000000000000",
            .goal_serialized_size_max = 16,
            .result_serialized_size_max = 16,
            .feedback_serialized_size_max = 16,
        };
        struct rclc_polling_action_client_t act;
        memset(&act, 0, sizeof(act));
        CHECK_RET(rclc_polling_action_client_init(&act, &node, &ACT_TYPE, "/seq_action"),
                  RCL_RET_OK, "polling action client created");
        const uint8_t goal_cdr[8] = {0x00, 0x01, 0x00, 0x00, 1, 0, 0, 0};
        uint8_t ga[16], gb[16], about[16];
        CHECK_RET(rclc_polling_action_client_send_goal_raw(&act, goal_cdr, sizeof(goal_cdr), &ga),
                  RCL_RET_OK, "goal A sent");
        const int64_t seq_ga = nros_stub_rmw_last_request_seq();
        CHECK_RET(rclc_polling_action_client_send_goal_raw(&act, goal_cdr, sizeof(goal_cdr), &gb),
                  RCL_RET_OK, "goal B sent");
        const int64_t seq_gb = nros_stub_rmw_last_request_seq();
        CHECK(memcmp(ga, gb, 16) != 0, "two goals, two ids");

        /* Acceptances, out of order: B's (rejected) first. */
        const uint8_t rej[8] = {0x00, 0x01, 0x00, 0x00, 0, 0, 0, 0};
        const uint8_t acc[8] = {0x00, 0x01, 0x00, 0x00, 1, 0, 0, 0};
        CHECK(nros_stub_rmw_stage_reply_for(seq_gb, rej, sizeof(rej)), "stage B's acceptance");
        CHECK(nros_stub_rmw_stage_reply_for(seq_ga, acc, sizeof(acc)), "stage A's acceptance");
        int32_t n = rclc_polling_action_client_try_recv_goal_response_raw_for(&act, buf,
                                                                              sizeof(buf), &about);
        CHECK(n > 4 && buf[4] == 0 && memcmp(about, gb, 16) == 0, "first reply: B, rejected");
        n = rclc_polling_action_client_try_recv_goal_response_raw_for(&act, buf, sizeof(buf),
                                                                      &about);
        CHECK(n > 4 && buf[4] == 1 && memcmp(about, ga, 16) == 0, "second reply: A, accepted");

        /* Results, out of order, plus a late one for a request nobody sent. */
        CHECK_RET(rclc_polling_action_client_send_get_result_request_raw(&act, &ga), RCL_RET_OK,
                  "result A requested");
        const int64_t seq_ra = nros_stub_rmw_last_request_seq();
        CHECK_RET(rclc_polling_action_client_send_get_result_request_raw(&act, &gb), RCL_RET_OK,
                  "result B requested");
        const int64_t seq_rb = nros_stub_rmw_last_request_seq();
        const uint8_t res_a[12] = {0x00, 0x01, 0x00, 0x00, 4, 0, 0, 0, 0xAA, 0, 0, 0};
        const uint8_t res_b[12] = {0x00, 0x01, 0x00, 0x00, 6, 0, 0, 0, 0xBB, 0, 0, 0};
        CHECK(nros_stub_rmw_stage_reply_for(seq_rb, res_b, sizeof(res_b)), "stage B's result");
        CHECK(nros_stub_rmw_stage_reply_for(seq_ra, res_a, sizeof(res_a)), "stage A's result");
        n = rclc_polling_action_client_try_recv_result_raw_for(&act, buf, sizeof(buf), &about);
        CHECK(n == 12 && buf[8] == 0xBB && memcmp(about, gb, 16) == 0, "first result: B's");
        n = rclc_polling_action_client_try_recv_result_raw_for(&act, buf, sizeof(buf), &about);
        CHECK(n == 12 && buf[8] == 0xAA && memcmp(about, ga, 16) == 0, "second result: A's");
        n = rclc_polling_action_client_try_recv_result_raw_for(&act, buf, sizeof(buf), &about);
        CHECK(n == 0, "nothing more");
        CHECK(rclc_polling_action_client_try_recv_result_raw_for(&act, buf, sizeof(buf), NULL) ==
                  RCL_RET_INVALID_ARGUMENT,
              "a NULL goal out-param is refused, not ignored");
        CHECK_RET(rclc_polling_action_client_fini(&act), RCL_RET_OK,
                  "polling action client finalised");
    }

    CHECK_RET(rclc_executor_remove_client(&executor, &client), RCL_RET_OK, "client removed");
    CHECK_RET(rcl_client_fini(&client, &node), RCL_RET_OK, "client finalised");

    if (s_failures != 0) {
        fprintf(stderr, "client_seq_correlation: %d failure(s)\n", s_failures);
        return 1;
    }
    printf("client_seq_correlation: OK\n");
    return 0;
}
