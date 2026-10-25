// issue 2034 — the C++ action client's cancel replies are taken by the
// EXECUTOR's spin, as C's are (issue 2030), and handed on from there:
//
//   * to the `async_cancel_*` future and callback waiting on that request —
//     the callback runs inside the spin, with the future never polled;
//   * otherwise kept for `try_recv_cancel_response`, oldest first, at most
//     `CANCEL_STASH_CAP` (a further unread reply evicts the oldest).
//
// Taking the reply is what frees its request's slot (`MAX_CONCURRENT_GOALS`
// = 4 per client). Before the fix the spin left cancel replies alone, so a
// caller that cancelled and only SPUN — never polling a future, never calling
// `poll()` or `try_recv_cancel_response` — had its fifth cancel refused.
//
// Real `libnros_cpp.a` + the stub backend, whose replies the test stages by
// request id. Every pump below is `rclcpp::spin_some(node)`: nothing here polls
// a future or calls `Client::poll()`.

#include "rclcpp/rclcpp.hpp"
#include "rclcpp_action/rclcpp_action.hpp"

extern "C" {
#include "stub_rmw_backend.h"
}

#include <chrono>
#include <cstdio>
#include <cstring>
#include <functional>
#include <future>
#include <memory>

extern "C" void nros_app_register_backends(void);
extern "C" void nros_app_register_backends(void) {
    (void)nros_stub_rmw_register();
}

namespace {

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

using Client = rclcpp_action::Client<Count>;
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

const uint8_t kHdr[4] = {0x00, 0x01, 0x00, 0x00};

void stage_accept(int64_t seq, bool accepted) {
    uint8_t reply[16] = {0x00, 0x01, 0x00, 0x00, static_cast<uint8_t>(accepted ? 1 : 0)};
    check(nros_stub_rmw_stage_reply_for(seq, reply, sizeof(reply)), "stage an acceptance");
}

// CancelGoal reply: [encap][code][pad][count u32][GoalInfo{uuid 16, sec, nsec} x count]
void stage_cancel_reply(int64_t seq, int8_t code, const uint8_t* uuid, int32_t sec,
                        uint32_t nsec) {
    uint8_t reply[12 + 24] = {0x00, 0x01, 0x00, 0x00, static_cast<uint8_t>(code), 0xEE, 0xEE,
                              0xEE};
    const uint32_t count = uuid != nullptr ? 1 : 0;
    std::memcpy(reply + 8, &count, 4);
    size_t len = 12;
    if (uuid != nullptr) {
        std::memcpy(reply + 12, uuid, 16);
        std::memcpy(reply + 28, &sec, 4);
        std::memcpy(reply + 32, &nsec, 4);
        len += 24;
    }
    check(nros_stub_rmw_stage_reply_for(seq, reply, len), "stage a cancel reply");
}

// Send one goal through `async_send_goal`, accept it, and return its handle.
ClientGoalHandle::SharedPtr accepted_handle(const rclcpp::Node::SharedPtr& node, Client& client,
                                            int64_t v) {
    ClientGoalHandle::SharedPtr got;
    Client::SendGoalOptions opts;
    opts.goal_response_callback = [&got](ClientGoalHandle::SharedPtr h) { got = h; };
    Value goal;
    goal.v = v;
    auto fut = client.async_send_goal(goal, opts);
    check(fut.valid(), "async_send_goal sent the goal");
    stage_accept(nros_stub_rmw_last_request_seq(), true);
    pump(node);
    check(static_cast<bool>(got), "the goal was accepted with a handle");
    return got;
}


// --- cancels taken by the spin free their slots ------------------------------
void spin_frees_cancel_slots(const rclcpp::Node::SharedPtr& node) {
    Client client;
    check(node->create_action_client(client, "count_spin_cancel").ok(), "a callback-tier client");
    const uint8_t raw_goal[16] = {7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7};
    const rclcpp_action::GoalUUID goal(raw_goal); // any id: the stub answers what it is told
    // Six plain cancels, each answered and only SPUN for. Codes 0,1,2,3,0,1.
    int sent = 0;
    for (int i = 0; i < 6; ++i) {
        if (!client.cancel_goal(goal).ok()) continue;
        ++sent;
        stage_cancel_reply(nros_stub_rmw_last_request_seq(), static_cast<int8_t>(i % 4), nullptr,
                           0, 0);
        pump(node);
    }
    check(sent == 6, "the spin took each cancel reply, so a fifth and sixth cancel are sent");

    // The kept codes: the last CANCEL_STASH_CAP (4) of the six, oldest first.
    const int8_t want[4] = {2, 3, 0, 1};
    for (int k = 0; k < 4; ++k) {
        rclcpp_action::CancelReturnCode code = rclcpp_action::CancelReturnCode::Ok;
        check(client.try_recv_cancel_response(code).ok() && static_cast<int8_t>(code) == want[k],
              "try_recv_cancel_response reads the kept replies, oldest first");
    }
    rclcpp_action::CancelReturnCode code = rclcpp_action::CancelReturnCode::Ok;
    check(!client.try_recv_cancel_response(code).ok(),
          "...and only CANCEL_STASH_CAP of them: the oldest two were evicted");
}

// --- the spin hands a cancel reply to its waiting future + callback ---------
void spin_feeds_cancel_futures(const rclcpp::Node::SharedPtr& node) {
    auto client = rclcpp_action::create_client<Count>(node, "count_spin_future");
    auto h = accepted_handle(node, *client, 21);
    int cbs = 0;
    int8_t cb_code = -1;
    auto fut = client->async_cancel_goal(h, [&](Client::CancelResponse::SharedPtr r) {
        ++cbs;
        cb_code = r ? r->return_code : -1;
    });
    check(fut.valid(), "async_cancel_goal sent the cancel");
    // A bare std::shared_future copy: waiting on it drives nothing, so it is
    // ready only if something else fulfilled it.
    std::shared_future<Client::CancelResponse::SharedPtr> bare = fut;
    stage_cancel_reply(nros_stub_rmw_last_request_seq(), 0, h ? h->get_goal_id().data() : kHdr,
                       4, 5);
    pump(node);
    check(cbs == 1 && cb_code == 0, "the cancel callback ran inside the spin, once, with ERROR_NONE");
    const bool fulfilled = bare.wait_for(std::chrono::seconds(0)) == std::future_status::ready;
    check(fulfilled, "the spin fulfilled the cancel future without it being polled");
    // A bare std::shared_future's get() blocks for ever on an unfulfilled
    // promise, so read it only once it is ready.
    check(fulfilled && bare.get() && bare.get()->goals_canceling.size() == 1 &&
              bare.get()->goals_canceling[0].stamp.sec == 4,
          "...with the reply's goals_canceling");
    rclcpp_action::CancelReturnCode code = rclcpp_action::CancelReturnCode::Ok;
    check(!client->try_recv_cancel_response(code).ok(),
          "a reply a future claimed is not also kept for try_recv_cancel_response");

    // Six async cancels whose futures are DISCARDED: the spin still takes each
    // reply, so the slots come back.
    int sent = 0;
    for (int i = 0; i < 6; ++i) {
        auto f = client->async_cancel_all_goals();
        if (!f.valid()) continue;
        ++sent;
        stage_cancel_reply(nros_stub_rmw_last_request_seq(), 0, nullptr, 0, 0);
        pump(node);
    }
    check(sent == 6, "discarded async_cancel_* futures do not keep their slots once the spin runs");
}

} // namespace

int main() {
    if (!rclcpp::init_with_rmw(NROS_STUB_RMW_NAME, nullptr, 0, "i2034").ok()) {
        std::fprintf(stderr, "action_cancel_reply_spin_runtime: init on the stub backend failed\n");
        return 1;
    }
    nros_stub_rmw_set_accept_entities(true);
    auto node = rclcpp::Node::make_shared("i2034");
    check(node && node->initialized(), "a node comes up on the stub backend");
    nros_stub_rmw_reset_client();

    spin_frees_cancel_slots(node);
    spin_feeds_cancel_futures(node);

    if (g_failures != 0) {
        std::fprintf(stderr, "action_cancel_reply_spin_runtime: %d failure(s)\n", g_failures);
        return 1;
    }
    std::printf("action_cancel_reply_spin_runtime: OK\n");
    return 0;
}
