// issue 2015 — upstream rclcpp_action spellings as drop-ins (hosted build).
//
// The stock ros2/examples action client and server used to need ~164 edited
// lines between them: goal handles, `create_client` / `create_server`, the
// `std::bind`-ed callback shapes, `SendGoalOptions`' upstream fields,
// `async_send_goal`, `WrappedResult` / `ResultCode`. Each one is a drop-in on a
// hosted build now. This drives every one of them through the real runtime:
//
//   server: `create_server<A>(node interfaces…, name, goal, cancel, accepted)`
//           with `std::bind`-ed members; the goal callback's
//           `std::shared_ptr<const Goal>`; a `ServerGoalHandle` that answers
//           `get_goal_id`, `get_goal`, `is_active`, `is_canceling`, and whose
//           `publish_feedback` / `succeed` / `canceled` reach the wire; a
//           rejected goal; a cancel.
//   client: `create_client<A>(node, name)`; `async_send_goal` with
//           `goal_response_callback` (a handle, or null on reject),
//           `feedback_callback` (`std::shared_ptr<const Feedback>`) and
//           `result_callback` (`WrappedResult` with `ResultCode`), the result
//           requested automatically once the goal is accepted.
//
// Real `libnros_cpp.a` + the stub backend, which since issue 2015 also STAGES
// requests for a named service server and samples for a named subscription,
// and keeps what each server answered.

#include "rclcpp/rclcpp.hpp"
#include "rclcpp_action/rclcpp_action.hpp"

extern "C" {
#include "stub_rmw_backend.h"
}

#include <cstdio>
#include <cstring>
#include <functional>
#include <memory>

extern "C" void nros_app_register_backends(void);
extern "C" void nros_app_register_backends(void) {
    (void)nros_stub_rmw_register();
}

namespace {

// One int64 behind a CDR encapsulation header — the shape `ffi_serialize`
// writes and every action dispatch delivers (header spliced back in front).
struct Value {
    int64_t v = 0;
    static constexpr const char* TYPE_NAME = "test_msgs::action::dds_::Count_";
    static constexpr const char* TYPE_HASH = "RIHS01_stub";
    static const size_t SERIALIZED_SIZE_MAX = 4 + sizeof(int64_t);
    static int ffi_serialize(const Value* msg, uint8_t* buf, size_t cap, size_t* out) {
        if (cap < 4 + sizeof(int64_t)) return -1;
        const uint8_t hdr[4] = {0x00, 0x01, 0x00, 0x00};
        std::memcpy(buf, hdr, 4);
        std::memcpy(buf + 4, &msg->v, sizeof(int64_t));
        *out = 4 + sizeof(int64_t);
        return 0;
    }
    static int ffi_deserialize(const uint8_t* data, size_t len, Value* out) {
        if (len != 4 + sizeof(int64_t)) return -1;
        std::memcpy(&out->v, data + 4, sizeof(int64_t));
        return 0;
    }
};

struct Count {
    using Goal = Value;
    using Result = Value;
    using Feedback = Value;
    static constexpr const char* TYPE_NAME = "test_msgs::action::dds_::Count_";
};

using GoalHandle = rclcpp_action::ServerGoalHandle<Count>;
using ClientGoalHandle = rclcpp_action::ClientGoalHandle<Count>;

int g_failures = 0;

void check(bool cond, const char* what) {
    if (!cond) {
        ++g_failures;
        std::fprintf(stderr, "FAIL: %s\n", what);
    }
}

void pump(const rclcpp::Node::SharedPtr& node) {
    for (int i = 0; i < 4; ++i)
        rclcpp::spin_some(node);
}

// [encap][uuid 16][int64] — a send_goal request, and a feedback message.
size_t frame_with_uuid(uint8_t* out, const uint8_t uuid[16], int64_t v) {
    const uint8_t hdr[4] = {0x00, 0x01, 0x00, 0x00};
    std::memcpy(out, hdr, 4);
    std::memcpy(out + 4, uuid, 16);
    std::memcpy(out + 20, &v, sizeof(v));
    return 28;
}

// --- the server half: a stock-shaped node class ------------------------------
struct ServerFixture {
    int goals = 0, cancels = 0, accepted = 0;
    int64_t last_goal = 0;
    bool run_to_completion = true;
    GoalHandle::SharedPtr kept;

    rclcpp_action::GoalResponse handle_goal(const rclcpp_action::GoalUUID& uuid,
                                            std::shared_ptr<const Count::Goal> goal) {
        (void)uuid;
        ++goals;
        last_goal = goal->v;
        return goal->v > 9000 ? rclcpp_action::GoalResponse::REJECT
                              : rclcpp_action::GoalResponse::ACCEPT_AND_EXECUTE;
    }
    rclcpp_action::CancelResponse handle_cancel(const std::shared_ptr<GoalHandle> goal_handle) {
        ++cancels;
        check(kept && goal_handle->get_goal_id() == kept->get_goal_id(),
              "the cancel callback's handle names the goal being cancelled");
        check(goal_handle->get_goal() && goal_handle->get_goal()->v == 7,
              "...and carries that goal's message");
        return rclcpp_action::CancelResponse::ACCEPT;
    }
    void handle_accepted(const std::shared_ptr<GoalHandle> goal_handle) {
        ++accepted;
        kept = goal_handle;
        check(goal_handle->get_goal() && goal_handle->get_goal()->v == last_goal,
              "get_goal() is the goal the goal callback accepted");
        check(goal_handle->is_active() && !goal_handle->is_canceling(),
              "an accepted goal is active and not canceling");
        if (!run_to_completion) return;
        auto feedback = std::make_shared<Count::Feedback>();
        feedback->v = goal_handle->get_goal()->v * 10;
        const uint32_t before = nros_stub_rmw_publish_count();
        goal_handle->publish_feedback(feedback);
        check(nros_stub_rmw_publish_count() > before, "publish_feedback reached the publisher");
        auto result = std::make_shared<Count::Result>();
        result->v = goal_handle->get_goal()->v + 100;
        goal_handle->succeed(result);
        check(!goal_handle->is_active(), "a succeeded goal is no longer active");
    }
};

// --- the client half ---------------------------------------------------------
struct ClientLog {
    int responses = 0, rejected = 0, feedbacks = 0, results = 0;
    ClientGoalHandle::SharedPtr handle;
    int64_t feedback = 0, result = 0;
    rclcpp_action::ResultCode code = rclcpp_action::ResultCode::UNKNOWN;
    rclcpp_action::GoalUUID result_goal;
};

} // namespace

int main() {
    using namespace std::placeholders;
    if (!rclcpp::init_with_rmw(NROS_STUB_RMW_NAME, nullptr, 0, "i2015").ok()) {
        std::fprintf(stderr, "action_upstream_dropins_runtime: init on the stub backend failed\n");
        return 1;
    }
    nros_stub_rmw_set_accept_entities(true);
    auto node = rclcpp::Node::make_shared("i2015");
    check(node && node->initialized(), "a node comes up on the stub backend");
    nros_stub_rmw_reset_client();

    // ===================== server =====================
    ServerFixture fx;
    rclcpp_action::Server<Count>::SharedPtr server = rclcpp_action::create_server<Count>(
        node->get_node_base_interface(), node->get_node_clock_interface(),
        node->get_node_logging_interface(), node->get_node_waitables_interface(), "count",
        std::bind(&ServerFixture::handle_goal, &fx, _1, _2),
        std::bind(&ServerFixture::handle_cancel, &fx, _1),
        std::bind(&ServerFixture::handle_accepted, &fx, _1));
    check(static_cast<bool>(server) && server->is_valid(), "create_server returns a live server");

    uint8_t wire[64];
    const uint8_t g1[16] = {1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1};
    // --- 1. a goal accepted, executed, fed back and succeeded ---------------
    {
        size_t n = frame_with_uuid(wire, g1, 5);
        check(nros_stub_rmw_stage_request("/_action/send_goal", 100, wire, n), "stage a goal");
        pump(node);
        check(fx.goals == 1 && fx.last_goal == 5, "handle_goal got the goal message");
        check(fx.accepted == 1, "handle_accepted ran once");
        size_t len = 0;
        int64_t seq = 0;
        const uint8_t* r = nros_stub_rmw_last_response("/_action/send_goal", &seq, &len);
        check(r != nullptr && seq == 100 && len > 4 && r[4] == 1, "the client was told: accepted");
        // Ask for the result.
        const uint8_t hdr[4] = {0x00, 0x01, 0x00, 0x00};
        std::memcpy(wire, hdr, 4);
        std::memcpy(wire + 4, g1, 16);
        check(nros_stub_rmw_stage_request("/_action/get_result", 101, wire, 20),
              "stage a get_result");
        pump(node);
        r = nros_stub_rmw_last_response("/_action/get_result", &seq, &len);
        int64_t v = 0;
        if (r != nullptr && len >= 16) std::memcpy(&v, r + 8, sizeof(v));
        check(r != nullptr && seq == 101 && len >= 16 && r[4] == 4 && v == 105,
              "get_result answers SUCCEEDED with the handle's result");
    }
    // --- 2. a rejected goal never reaches handle_accepted --------------------
    {
        const uint8_t g2[16] = {2};
        size_t n = frame_with_uuid(wire, g2, 9001);
        check(nros_stub_rmw_stage_request("/_action/send_goal", 102, wire, n), "stage goal 2");
        pump(node);
        size_t len = 0;
        int64_t seq = 0;
        const uint8_t* r = nros_stub_rmw_last_response("/_action/send_goal", &seq, &len);
        check(fx.goals == 2 && fx.accepted == 1, "a REJECT runs no accepted callback");
        check(r != nullptr && seq == 102 && len > 4 && r[4] == 0, "the client was told: rejected");
    }
    // --- 3. a cancel: the handle reports is_canceling, then canceled() ------
    {
        fx.run_to_completion = false;
        const uint8_t g3[16] = {3, 3};
        size_t n = frame_with_uuid(wire, g3, 7);
        check(nros_stub_rmw_stage_request("/_action/send_goal", 103, wire, n), "stage goal 3");
        pump(node);
        check(fx.accepted == 2 && fx.kept, "goal 3 accepted and its handle kept");
        // CancelGoal request: [encap][GoalInfo: uuid 16, stamp i32 + u32]
        const uint8_t hdr[4] = {0x00, 0x01, 0x00, 0x00};
        std::memcpy(wire, hdr, 4);
        std::memcpy(wire + 4, g3, 16);
        std::memset(wire + 20, 0, 8);
        check(nros_stub_rmw_stage_request("/_action/cancel_goal", 104, wire, 28),
              "stage a cancel");
        pump(node);
        check(fx.cancels == 1, "handle_cancel ran");
        check(fx.kept && fx.kept->is_canceling(), "the kept handle now says is_canceling");
        auto result = std::make_shared<Count::Result>();
        fx.kept->canceled(result);
        check(!fx.kept->is_active(), "a canceled goal is no longer active");
    }

    // ===================== client =====================
    ClientLog log;
    auto client = rclcpp_action::create_client<Count>(node, "count_c");
    check(static_cast<bool>(client) && client->is_valid(), "create_client returns a live client");
    auto options = rclcpp_action::Client<Count>::SendGoalOptions();
    options.goal_response_callback = [&log](ClientGoalHandle::SharedPtr h) {
        ++log.responses;
        if (!h) ++log.rejected;
        log.handle = h;
    };
    options.feedback_callback = [&log](ClientGoalHandle::SharedPtr h,
                                       const std::shared_ptr<const Count::Feedback> fb) {
        ++log.feedbacks;
        check(h && log.handle && h->get_goal_id() == log.handle->get_goal_id(),
              "feedback names the goal's handle");
        log.feedback = fb->v;
    };
    options.result_callback = [&log](const rclcpp_action::Client<Count>::WrappedResult& wr) {
        ++log.results;
        log.code = wr.code;
        log.result_goal = wr.goal_id;
        log.result = wr.result ? wr.result->v : -1;
    };
    // --- 4. accepted: handle, feedback, the result requested by itself -------
    {
        Value goal;
        goal.v = 42;
        auto fut = client->async_send_goal(goal, options);
        check(fut.valid(), "async_send_goal sent the goal");
        const int64_t seq_goal = nros_stub_rmw_last_request_seq();
        uint8_t accept[16] = {0x00, 0x01, 0x00, 0x00, 1};
        check(nros_stub_rmw_stage_reply_for(seq_goal, accept, sizeof(accept)), "stage accept");
        pump(node);
        check(log.responses == 1 && log.rejected == 0 && log.handle,
              "goal_response_callback got a handle");
        check(log.handle && log.handle->is_result_aware() && log.handle->is_feedback_aware(),
              "the handle knows its result and feedback callbacks");
        const int64_t seq_result = nros_stub_rmw_last_request_seq();
        check(seq_result != seq_goal, "a result callback requested the result");
        size_t n = frame_with_uuid(wire, log.handle ? log.handle->get_goal_id().data() : g1, 77);
        check(nros_stub_rmw_stage_sample("/_action/feedback", wire, n), "stage feedback");
        pump(node);
        check(log.feedbacks == 1 && log.feedback == 77,
              "feedback_callback got the feedback as shared_ptr<const Feedback>");
        uint8_t res[16] = {0x00, 0x01, 0x00, 0x00, 6};
        const int64_t value = 1234;
        std::memcpy(res + 8, &value, sizeof(value));
        check(nros_stub_rmw_stage_reply_for(seq_result, res, sizeof(res)), "stage result");
        pump(node);
        check(log.results == 1 && log.code == rclcpp_action::ResultCode::ABORTED &&
                  log.result == 1234,
              "result_callback got WrappedResult{ABORTED, result}");
        check(log.handle && log.result_goal == log.handle->get_goal_id(),
              "...for the goal it was sent for");
    }
    // --- 5. rejected: the callback gets a null handle -----------------------
    {
        Value goal;
        goal.v = 1;
        auto fut = client->async_send_goal(goal, options);
        const int64_t seq_goal = nros_stub_rmw_last_request_seq();
        uint8_t reject[16] = {0x00, 0x01, 0x00, 0x00, 0};
        check(nros_stub_rmw_stage_reply_for(seq_goal, reject, sizeof(reject)), "stage reject");
        pump(node);
        check(log.responses == 2 && log.rejected == 1, "a rejected goal's callback gets nullptr");
        check(nros_stub_rmw_last_request_seq() == seq_goal, "...and no result is requested");
    }

    if (g_failures != 0) {
        std::fprintf(stderr, "action_upstream_dropins_runtime: %d failure(s)\n", g_failures);
        return 1;
    }
    std::printf("action_upstream_dropins_runtime: OK\n");
    return 0;
}
