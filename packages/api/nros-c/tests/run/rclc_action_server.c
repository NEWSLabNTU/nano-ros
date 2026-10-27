/*
 * phase-515 C4b -- rclc's typed action server, with rclc's signatures, on a
 * real executor over the stub RMW backend:
 *
 *     rclc_action_server_init_default(&server, &node, &support, &type, "count");
 *     rclc_executor_add_action_server(&exec, &server, N, reqs, sizeof(reqs[0]),
 *                                     goal_cb, cancel_cb, ctx);
 *     rclc_action_publish_feedback(goal_handle, &feedback_message);
 *     rclc_action_send_result(goal_handle, GOAL_STATE_SUCCEEDED, &get_result_response);
 *
 * Asserted here:
 *   1. a goal reaches the goal callback deserialised into the caller's
 *      `SendGoal_Request` array, with its id, and an accept reaches the client;
 *   2. feedback is published with the goal's id written into the message;
 *   3. a result sent through the handle answers the client's result request,
 *      `status` written by the call, and frees the handle;
 *   4. a rejected goal reaches the client as rejected and frees its handle;
 *   5. a cancel goes CANCELING while the cancel callback decides, and an
 *      accepted one sets `goal_cancelled`;
 *   6. with every handle in use the next goal is rejected without a callback;
 *   7. the refusals: bad sizes, NULL callbacks, a descriptor with no layout,
 *      an uninitialised support.
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

/* The action's three payloads and the envelopes a generated header declares. */
typedef struct {
    int64_t order;
} count_goal_t;
typedef struct {
    int64_t value;
} count_result_t;
typedef struct {
    int64_t progress;
} count_feedback_t;
typedef struct {
    unique_identifier_msgs__msg__UUID goal_id;
    count_goal_t goal;
} count_send_goal_request_t;
typedef struct {
    int8_t status;
    count_result_t result;
} count_get_result_response_t;
typedef struct {
    unique_identifier_msgs__msg__UUID goal_id;
    count_feedback_t feedback;
} count_feedback_message_t;

/* CDR: a 4-byte encapsulation header, then the int64. */
static int32_t i64_serialize(const void* msg, uint8_t* buf, size_t cap, size_t* out_len) {
    if (cap < 12) return -1;
    const uint8_t hdr[4] = {0x00, 0x01, 0x00, 0x00};
    memcpy(buf, hdr, 4);
    memcpy(buf + 4, msg, 8);
    *out_len = 12;
    return 0;
}

static int32_t i64_deserialize(const uint8_t* buf, size_t len, void* msg) {
    if (len < 12) return -1;
    memcpy(msg, buf + 4, 8);
    return 0;
}

static const rclc_action_type_t COUNT = {
    .type_name = "test_msgs::action::dds_::Count_",
    .type_hash = "RIHS01_0000000000000000000000000000000000000000000000000000000000000000",
    .goal_serialized_size_max = 64,
    .result_serialized_size_max = 64,
    .feedback_serialized_size_max = 64,
    .goal_serialize = i64_serialize,
    .goal_deserialize = i64_deserialize,
    .result_serialize = i64_serialize,
    .result_deserialize = i64_deserialize,
    .feedback_serialize = i64_serialize,
    .feedback_deserialize = i64_deserialize,
    .send_goal_request_size = sizeof(count_send_goal_request_t),
    .send_goal_request_goal_offset = offsetof(count_send_goal_request_t, goal),
    .get_result_response_size = sizeof(count_get_result_response_t),
    .get_result_response_result_offset = offsetof(count_get_result_response_t, result),
    .feedback_message_size = sizeof(count_feedback_message_t),
    .feedback_message_feedback_offset = offsetof(count_feedback_message_t, feedback),
};

/* A descriptor from an older generator: codecs, no envelope layout. */
static const rclc_action_type_t COUNT_NO_LAYOUT = {
    .type_name = "test_msgs::action::dds_::Count_",
    .type_hash = "RIHS01_0000000000000000000000000000000000000000000000000000000000000000",
    .goal_serialize = i64_serialize,
    .goal_deserialize = i64_deserialize,
    .result_serialize = i64_serialize,
    .result_deserialize = i64_deserialize,
    .feedback_serialize = i64_serialize,
    .feedback_deserialize = i64_deserialize,
};

#define HANDLES 2

typedef struct {
    int goals, cancels;
    rcl_action_goal_state_t status_in_goal_cb, status_in_cancel_cb;
    rclc_action_goal_handle_t* last;
    bool cancel_answer;
} app_t;

static rcl_ret_t on_goal(rclc_action_goal_handle_t* goal_handle, void* context) {
    app_t* app = (app_t*)context;
    app->goals++;
    app->last = goal_handle;
    app->status_in_goal_cb = goal_handle->status;
    const count_send_goal_request_t* req =
        (const count_send_goal_request_t*)goal_handle->ros_goal_request;
    return req->goal.order < 1000 ? RCL_RET_ACTION_GOAL_ACCEPTED : RCL_RET_ACTION_GOAL_REJECTED;
}

static bool on_cancel(rclc_action_goal_handle_t* goal_handle, void* context) {
    app_t* app = (app_t*)context;
    app->cancels++;
    app->status_in_cancel_cb = goal_handle->status;
    return app->cancel_answer;
}

static void pump(rclc_executor_t* executor) {
    for (int i = 0; i < 4; i++)
        (void)rclc_executor_spin_some(executor, 0);
}

static void stage_goal(int64_t seq, const uint8_t uuid[16], int64_t order) {
    uint8_t wire[28] = {0x00, 0x01, 0x00, 0x00};
    memcpy(wire + 4, uuid, 16);
    memcpy(wire + 20, &order, 8);
    CHECK(nros_stub_rmw_stage_request("/_action/send_goal", seq, wire, sizeof wire),
          "stage a goal");
}

static int goal_reply(int64_t want_seq) {
    int64_t seq = -1;
    size_t len = 0;
    const uint8_t* r = nros_stub_rmw_last_response("/_action/send_goal", &seq, &len);
    if (r == NULL || seq != want_seq || len <= 4) return -1;
    return r[4];
}

int main(void) {
    setenv("NROS_RMW", NROS_STUB_RMW_NAME, 1);
    nros_stub_rmw_set_accept_entities(true);
    nros_stub_rmw_set_entity_wake_supported(false);

    struct rclc_support_t support = rclc_support_get_zero_initialized();
    if (rclc_support_init_rmw(&support, "stub://none", 53, "rclc_action_server",
                              NROS_STUB_RMW_NAME) != RCL_RET_OK) {
        fprintf(stderr, "rclc_action_server: support init failed\n");
        return 1;
    }
    rclc_executor_t executor = rclc_executor_get_zero_initialized_executor();
    CHECK(rclc_executor_init(&executor, &support.context, 4, NULL) == RCL_RET_OK, "executor");
    rcl_node_options_t opts = rcl_node_get_default_options();
    rcl_node_t node = rcl_get_zero_initialized_node();
    CHECK(rclc_executor_node_init(&executor, &node, "rclc_srv", &opts) == RCL_RET_OK, "node");

    /* rclc's signatures, taken as pointers so a drift fails to compile. */
    rcl_ret_t (*init)(rclc_action_server_t*, rcl_node_t*, rclc_support_t*,
                      const rclc_action_type_t*, const char*) = rclc_action_server_init_default;
    rcl_ret_t (*add)(rclc_executor_t*, rclc_action_server_t*, size_t, void*, size_t,
                     rclc_action_server_handle_goal_callback_t,
                     rclc_action_server_handle_cancel_callback_t, void*) =
        rclc_executor_add_action_server;
    rcl_ret_t (*send_result)(rclc_action_goal_handle_t*, rcl_action_goal_state_t, void*) =
        rclc_action_send_result;
    rcl_ret_t (*publish_feedback)(rclc_action_goal_handle_t*, void*) = rclc_action_publish_feedback;

    app_t app;
    memset(&app, 0, sizeof app);
    count_send_goal_request_t reqs[HANDLES];
    memset(reqs, 0, sizeof reqs);

    /* --- 7. refusals ----------------------------------------------------- */
    {
        struct rclc_support_t cold = rclc_support_get_zero_initialized();
        rclc_action_server_t s = rcl_action_get_zero_initialized_server();
        CHECK(init(&s, &node, &cold, &COUNT, "cold") == RCL_RET_NOT_INIT,
              "an uninitialised support is refused");
        rclc_action_server_t comp = rcl_action_get_zero_initialized_server();
        CHECK(init(&comp, &node, NULL, &COUNT, "component") == RCL_RET_OK,
              "a NULL support is accepted, as rclc_timer_init_default accepts it");
        CHECK(rclc_action_server_fini(&comp, &node) == RCL_RET_OK, "component fini");
        rclc_action_server_t old = rcl_action_get_zero_initialized_server();
        CHECK(init(&old, &node, &support, &COUNT_NO_LAYOUT, "old") == RCL_RET_OK, "old init");
        CHECK(add(&executor, &old, HANDLES, reqs, sizeof reqs[0], on_goal, on_cancel, &app) ==
                  RCL_RET_UNSUPPORTED,
              "a descriptor with no envelope layout is UNSUPPORTED");
        CHECK(rclc_action_server_fini(&old, &node) == RCL_RET_OK, "old fini");
    }
    rclc_action_server_t server = rcl_action_get_zero_initialized_server();
    CHECK(init(&server, &node, &support, &COUNT, "count") == RCL_RET_OK, "init_default");
    CHECK(add(&executor, &server, 0, reqs, sizeof reqs[0], on_goal, on_cancel, &app) ==
              RCL_RET_INVALID_ARGUMENT,
          "zero handles is refused");
    CHECK(add(&executor, &server, 1000, reqs, sizeof reqs[0], on_goal, on_cancel, &app) ==
              RCL_RET_INVALID_ARGUMENT,
          "more handles than NROS_MAX_CONCURRENT_GOALS is refused");
    CHECK(add(&executor, &server, HANDLES, reqs, 0, on_goal, on_cancel, &app) == RCL_RET_ERROR,
          "a zero request size is rclc's RCL_RET_ERROR");
    CHECK(add(&executor, &server, HANDLES, reqs, 4, on_goal, on_cancel, &app) ==
              RCL_RET_INVALID_ARGUMENT,
          "a request size below the envelope's is refused");
    CHECK(add(&executor, &server, HANDLES, reqs, sizeof reqs[0], NULL, on_cancel, &app) ==
              RCL_RET_INVALID_ARGUMENT,
          "a NULL goal callback is refused");
    CHECK(add(&executor, &server, HANDLES, reqs, sizeof reqs[0], on_goal, on_cancel, &app) ==
              RCL_RET_OK,
          "rclc_executor_add_action_server");
    if (s_failures != 0) return 1;

    /* --- 1. an accepted goal --------------------------------------------- */
    const uint8_t g1[16] = {1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1};
    stage_goal(100, g1, 5);
    pump(&executor);
    CHECK(app.goals == 1, "the goal callback ran once");
    rclc_action_goal_handle_t* h1 = app.last;
    CHECK(h1 != NULL && h1->ros_goal_request == (void*)&reqs[0],
          "the goal landed in the caller's first SendGoal_Request");
    CHECK(reqs[0].goal.order == 5, "...deserialised");
    CHECK(memcmp(reqs[0].goal_id.uuid, g1, 16) == 0 && memcmp(h1->goal_id.uuid, g1, 16) == 0,
          "...with the client's goal id, in the request and on the handle");
    CHECK(app.status_in_goal_cb == GOAL_STATE_UNKNOWN, "UNKNOWN while the callback decides");
    CHECK(h1->status == GOAL_STATE_EXECUTING && h1->goal_accepted, "EXECUTING once accepted");
    CHECK(h1->action_server == &server, "the handle names its server");
    CHECK(goal_reply(100) == 1, "the client was told: accepted");

    /* --- 2. feedback ------------------------------------------------------ */
    count_feedback_message_t fb;
    memset(&fb, 0, sizeof fb);
    fb.feedback.progress = 3;
    const uint32_t before = nros_stub_rmw_publish_count();
    CHECK(publish_feedback(h1, &fb) == RCL_RET_OK, "rclc_action_publish_feedback");
    CHECK(nros_stub_rmw_publish_count() > before, "...reached the feedback publisher");
    CHECK(memcmp(fb.goal_id.uuid, g1, 16) == 0, "...after writing the goal id into the message");
    {
        size_t len = 0;
        const uint8_t* p = nros_stub_rmw_last_payload(&len);
        int64_t v = 0;
        if (p != NULL && len >= 28) memcpy(&v, p + 20, 8);
        CHECK(p != NULL && len >= 28 && memcmp(p + 4, g1, 16) == 0 && v == 3,
              "...as [goal id][progress] on the wire");
    }

    /* --- 3. the result ---------------------------------------------------- */
    count_get_result_response_t res;
    memset(&res, 0, sizeof res);
    res.result.value = 105;
    CHECK(send_result(h1, GOAL_STATE_EXECUTING, &res) == RCL_RET_INVALID_ARGUMENT,
          "a non-terminal status is refused");
    CHECK(send_result(h1, GOAL_STATE_SUCCEEDED, &res) == RCL_RET_OK, "rclc_action_send_result");
    CHECK(res.status == GOAL_STATE_SUCCEEDED, "...writes status into the response");
    CHECK(send_result(h1, GOAL_STATE_SUCCEEDED, &res) == RCL_RET_INVALID_ARGUMENT,
          "...and frees the handle: a second send is refused");
    {
        uint8_t wire[20] = {0x00, 0x01, 0x00, 0x00};
        memcpy(wire + 4, g1, 16);
        CHECK(nros_stub_rmw_stage_request("/_action/get_result", 101, wire, sizeof wire),
              "stage a get_result");
        pump(&executor);
        int64_t seq = -1, v = 0;
        size_t len = 0;
        const uint8_t* r = nros_stub_rmw_last_response("/_action/get_result", &seq, &len);
        if (r != NULL && len >= 16) memcpy(&v, r + 8, 8);
        CHECK(r != NULL && seq == 101 && r[4] == GOAL_STATE_SUCCEEDED && v == 105,
              "get_result answers SUCCEEDED with the sent result");
    }

    /* --- 4. a rejected goal ---------------------------------------------- */
    const uint8_t g2[16] = {2};
    stage_goal(102, g2, 9001);
    pump(&executor);
    CHECK(app.goals == 2, "the goal callback saw the second goal");
    CHECK(goal_reply(102) == 0, "the client was told: rejected");

    /* --- 5. a cancel ------------------------------------------------------ */
    const uint8_t g3[16] = {3, 3};
    stage_goal(103, g3, 7);
    pump(&executor);
    rclc_action_goal_handle_t* h3 = app.last;
    CHECK(goal_reply(103) == 1 && h3 != NULL && h3->ros_goal_request == (void*)&reqs[0],
          "goal 3 accepted into the first handle the rejected goal left free");
    app.cancel_answer = true;
    {
        uint8_t wire[28] = {0x00, 0x01, 0x00, 0x00};
        memcpy(wire + 4, g3, 16);
        CHECK(nros_stub_rmw_stage_request("/_action/cancel_goal", 104, wire, sizeof wire),
              "stage a cancel");
        pump(&executor);
    }
    CHECK(app.cancels == 1, "the cancel callback ran");
    CHECK(app.status_in_cancel_cb == GOAL_STATE_CANCELING, "CANCELING while it decides");
    CHECK(h3->goal_cancelled && h3->status == GOAL_STATE_CANCELING,
          "an accepted cancel sets goal_cancelled");
    memset(&res, 0, sizeof res);
    CHECK(send_result(h3, GOAL_STATE_CANCELED, &res) == RCL_RET_OK, "the canceled goal ends");

    /* --- 6. every handle in use ------------------------------------------ */
    const uint8_t g4[16] = {4}, g5[16] = {5}, g6[16] = {6};
    stage_goal(105, g4, 1);
    pump(&executor);
    CHECK(goal_reply(105) == 1, "goal 4 accepted");
    stage_goal(106, g5, 2);
    pump(&executor);
    CHECK(goal_reply(106) == 1, "goal 5 accepted: both handles now held");
    const int seen = app.goals;
    stage_goal(107, g6, 3);
    pump(&executor);
    CHECK(app.goals == seen, "with both handles held, the third goal reaches no callback");
    CHECK(goal_reply(107) == 0, "...and is rejected");

    CHECK(rclc_executor_remove_action_server(&executor, &server) == RCL_RET_OK, "removed");
    CHECK(rclc_action_server_fini(&server, &node) == RCL_RET_OK, "rclc_action_server_fini");
    (void)rcl_node_fini(&node);
    (void)rclc_executor_fini(&executor);
    (void)rclc_support_fini(&support);

    if (s_failures != 0) {
        fprintf(stderr, "rclc_action_server: %d failure(s)\n", s_failures);
        return 1;
    }
    printf("rclc_action_server: OK\n");
    return 0;
}
