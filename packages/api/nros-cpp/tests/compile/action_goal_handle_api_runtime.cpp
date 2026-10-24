// issue 2027 — the rest of upstream rclcpp_action's goal-handle API, and the
// goal-slot leak, on a hosted build.
//
//   client: `async_get_result(goal_handle, result_callback)` and
//           `async_cancel_goal(goal_handle, cancel_callback)` returning
//           upstream's `std::shared_future`s, which `spin_until_future_complete`
//           then `get()` complete; `async_cancel_all_goals` /
//           `async_cancel_goals_before`; a cancel response's `goals_canceling`.
//   server: `ServerGoalHandle::execute()` — ACCEPTED -> EXECUTING with the
//           status array published — after `ACCEPT_AND_DEFER`, and the server
//           executing an `ACCEPT_AND_EXECUTE` goal itself before
//           `handle_accepted`.
//   leak:   a future dropped before its reply frees the request's slot in the
//           Rust core (`MAX_CONCURRENT_GOALS` = 4 per sub-client), so more
//           than four abandoned goals — and get_result requests — still leave
//           the client able to send; a late reply for a freed slot is dropped.
//           An abandoned future whose reply an upstream callback still waits
//           for does NOT free it.
//
// Real `libnros_cpp.a` + the stub backend, whose replies the test stages by
// request id and whose requests it stages per service.

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
using ServerGoalHandle = rclcpp_action::ServerGoalHandle<Count>;

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

void stage_result(int64_t seq, uint8_t status, int64_t value) {
    uint8_t reply[16] = {0x00, 0x01, 0x00, 0x00, status, 0xEE, 0xEE, 0xEE};
    std::memcpy(reply + 8, &value, sizeof(value));
    check(nros_stub_rmw_stage_reply_for(seq, reply, sizeof(reply)), "stage a result");
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

// --- server fixture ----------------------------------------------------------
struct ServerFixture {
    bool defer = true;
    int accepted = 0;
    bool executing_at_accept = true;
    ServerGoalHandle::SharedPtr kept;
    rclcpp_action::GoalResponse handle_goal(const rclcpp_action::GoalUUID&,
                                            std::shared_ptr<const Count::Goal>) {
        return defer ? rclcpp_action::GoalResponse::ACCEPT_AND_DEFER
                     : rclcpp_action::GoalResponse::ACCEPT_AND_EXECUTE;
    }
    rclcpp_action::CancelResponse handle_cancel(const std::shared_ptr<ServerGoalHandle>) {
        return rclcpp_action::CancelResponse::ACCEPT;
    }
    void handle_accepted(const std::shared_ptr<ServerGoalHandle> h) {
        ++accepted;
        executing_at_accept = h->is_executing();
        kept = h; // a deferred goal is executed later, from outside the callback
    }
};

size_t goal_request(uint8_t* out, const uint8_t uuid[16], int64_t v) {
    std::memcpy(out, kHdr, 4);
    std::memcpy(out + 4, uuid, 16);
    std::memcpy(out + 20, &v, sizeof(v));
    return 28;
}

void server_execute(const rclcpp::Node::SharedPtr& node) {
    using namespace std::placeholders;
    ServerFixture fx;
    auto server = rclcpp_action::create_server<Count>(
        node, "count_srv", std::bind(&ServerFixture::handle_goal, &fx, _1, _2),
        std::bind(&ServerFixture::handle_cancel, &fx, _1),
        std::bind(&ServerFixture::handle_accepted, &fx, _1));
    check(static_cast<bool>(server) && server->is_valid(), "create_server returns a live server");
    uint8_t wire[32];

    // --- ACCEPT_AND_DEFER: ACCEPTED until execute() -------------------------
    const uint8_t gd[16] = {0xD0, 1};
    check(nros_stub_rmw_stage_request("count_srv/_action/send_goal", 500, wire,
                                      goal_request(wire, gd, 3)),
          "stage a deferred goal");
    pump(node);
    check(fx.accepted == 1 && fx.kept, "handle_accepted ran for the deferred goal");
    check(!fx.executing_at_accept, "a deferred goal is not executing in handle_accepted");
    check(fx.kept && fx.kept->is_active() && !fx.kept->is_executing(),
          "...and stays ACCEPTED after the callback returned");
    const uint32_t before = nros_stub_rmw_publish_count();
    if (fx.kept) fx.kept->execute();
    check(fx.kept && fx.kept->is_executing(), "execute() moved the goal to EXECUTING");
    check(nros_stub_rmw_publish_count() > before, "...and published the status array");
    size_t len = 0;
    const uint8_t* status = nros_stub_rmw_last_payload(&len);
    // GoalStatusArray: [encap][count u32][GoalInfo{uuid 16, sec, nsec}][status i8] ...
    bool found = false;
    for (size_t at = 8; status != nullptr && at + 25 <= len; ++at) {
        if (std::memcmp(status + at, gd, 16) == 0 && status[at + 24] == 2) found = true;
    }
    check(found, "the published status array says the goal is EXECUTING (2)");
    const uint32_t after = nros_stub_rmw_publish_count();
    rclcpp::Result again = server->execute(gd);
    check(!again.ok() && again.code() == rclcpp::ErrorCode::NotAllowed,
          "a second execute() is refused (NotAllowed): the goal is not ACCEPTED");
    check(nros_stub_rmw_publish_count() == after, "...and publishes nothing");
    const uint8_t nobody[16] = {0x77};
    check(server->execute(nobody).code() == rclcpp::ErrorCode::NotFound,
          "execute() on a goal the server does not hold is NotFound");

    // --- ACCEPT_AND_EXECUTE: executing by the time handle_accepted runs -----
    fx.defer = false;
    const uint8_t ge[16] = {0xE0, 2};
    check(nros_stub_rmw_stage_request("count_srv/_action/send_goal", 501, wire,
                                      goal_request(wire, ge, 4)),
          "stage an execute-now goal");
    pump(node);
    check(fx.accepted == 2, "handle_accepted ran for the execute-now goal");
    check(fx.executing_at_accept,
          "ACCEPT_AND_EXECUTE: the server executed the goal before handle_accepted");
    auto result = std::make_shared<Count::Result>();
    if (fx.kept) fx.kept->succeed(result);
}

// --- client: async_get_result / async_cancel_* ------------------------------
void client_handle_api(const rclcpp::Node::SharedPtr& node) {
    auto client = rclcpp_action::create_client<Count>(node, "count_api");
    check(static_cast<bool>(client) && client->is_valid(), "create_client returns a live client");

    // --- async_get_result: requests once, future + callback -----------------
    {
        auto h = accepted_handle(node, *client, 11);
        check(h && !h->is_result_aware(), "no result callback yet: not result-aware");
        const int64_t before = nros_stub_rmw_last_request_seq();
        int callbacks = 0;
        int64_t cb_value = 0;
        auto fut = client->async_get_result(h, [&](const Client::WrappedResult& wr) {
            ++callbacks;
            cb_value = wr.result ? wr.result->v : -1;
        });
        const int64_t seq = nros_stub_rmw_last_request_seq();
        check(fut.valid(), "async_get_result returns a valid shared_future");
        check(seq != before && h && h->is_result_aware(),
              "async_get_result requested the result and made the goal result-aware");
        std::shared_future<Client::WrappedResult> same = client->async_get_result(h);
        check(nros_stub_rmw_last_request_seq() == seq, "a second call requests nothing more");
        check(!rclcpp::detail::future_is_ready(fut), "the future waits for the result");
        stage_result(seq, 4, 4242);
        check(rclcpp::spin_until_future_complete(node, fut, 1000) ==
                  rclcpp::FutureReturnCode::SUCCESS,
              "spin_until_future_complete completes the result future");
        const Client::WrappedResult& wr = fut.get();
        check(wr.code == rclcpp_action::ResultCode::SUCCEEDED && wr.result &&
                  wr.result->v == 4242 && h && wr.goal_id == h->get_goal_id(),
              "get() is WrappedResult{SUCCEEDED, result} for this goal");
        check(callbacks == 1 && cb_value == 4242, "the result callback ran once with it");
        check(same.valid() && rclcpp::detail::future_is_ready(same) && same.get().result &&
                  same.get().result->v == 4242,
              "every future for one goal is the handle's future (a bare std::shared_future too)");
        check(h && h->get_status() == 4, "the handle's status is SUCCEEDED");
    }

    // --- async_get_result on a goal whose result callback was given ----------
    {
        ClientGoalHandle::SharedPtr got;
        int result_cbs = 0;
        Client::SendGoalOptions opts;
        opts.goal_response_callback = [&got](ClientGoalHandle::SharedPtr h) { got = h; };
        opts.result_callback = [&result_cbs](const Client::WrappedResult&) { ++result_cbs; };
        Value goal;
        goal.v = 12;
        (void)client->async_send_goal(goal, opts); // the future is DISCARDED, as the stock client does
        stage_accept(nros_stub_rmw_last_request_seq(), true);
        pump(node);
        check(static_cast<bool>(got), "a discarded async_send_goal future still delivers the handle");
        const int64_t seq_result = nros_stub_rmw_last_request_seq();
        auto fut = client->async_get_result(got);
        check(nros_stub_rmw_last_request_seq() == seq_result,
              "already result-aware: async_get_result sends no second request");
        stage_result(seq_result, 6, -5);
        check(fut.wait_for(std::chrono::milliseconds(500)) == std::future_status::ready,
              "wait_for drives the client until the result arrives");
        check(fut.get().code == rclcpp_action::ResultCode::ABORTED && result_cbs == 1,
              "the future and the send-time result callback both see ABORTED");
    }

    // --- async_cancel_goal ---------------------------------------------------
    {
        auto h = accepted_handle(node, *client, 13);
        int cancel_cbs = 0;
        auto fut = client->async_cancel_goal(h, [&](Client::CancelResponse::SharedPtr r) {
            cancel_cbs += (r && r->return_code == Client::CancelResponse::ERROR_NONE) ? 1 : 100;
        });
        check(fut.valid(), "async_cancel_goal sent the cancel");
        const int64_t seq = nros_stub_rmw_last_request_seq();
        check(!rclcpp::detail::future_is_ready(fut), "the cancel future waits for its reply");
        stage_cancel_reply(seq, 0, h ? h->get_goal_id().data() : kHdr, 9, 10);
        check(rclcpp::spin_until_future_complete(node, fut, 1000) ==
                  rclcpp::FutureReturnCode::SUCCESS,
              "spin_until_future_complete completes the cancel future");
        Client::CancelResponse::SharedPtr r = fut.get();
        check(r && r->return_code == Client::CancelResponse::ERROR_NONE,
              "the cancel response says ERROR_NONE");
        check(r && r->goals_canceling.size() == 1 && h &&
                  std::memcmp(r->goals_canceling[0].goal_id.uuid, h->get_goal_id().data(), 16) ==
                      0 &&
                  r->goals_canceling[0].stamp.sec == 9 && r->goals_canceling[0].stamp.nanosec == 10,
              "goals_canceling names the goal and its stamp");
        check(cancel_cbs == 1, "the cancel callback ran once with the response");
        check(!client->async_cancel_goal(nullptr).valid(), "a null handle cancels nothing");
    }

    // --- async_cancel_all_goals / async_cancel_goals_before ------------------
    {
        auto all = client->async_cancel_all_goals();
        const int64_t seq_all = nros_stub_rmw_last_request_seq();
        auto before = client->async_cancel_goals_before(rclcpp::Time(static_cast<int32_t>(5), static_cast<uint32_t>(7)));
        const int64_t seq_before = nros_stub_rmw_last_request_seq();
        check(all.valid() && before.valid() && seq_all != seq_before,
              "cancel-all and cancel-before each sent a request");
        // Out of order: the second request's reply first.
        stage_cancel_reply(seq_before, 1, nullptr, 0, 0);
        stage_cancel_reply(seq_all, 0, nullptr, 0, 0);
        check(rclcpp::spin_until_future_complete(node, all, 1000) ==
                  rclcpp::FutureReturnCode::SUCCESS,
              "cancel-all completes");
        check(rclcpp::detail::future_is_ready(before), "cancel-before completed in the same pass");
        check(all.get() && all.get()->return_code == 0 && all.get()->goals_canceling.empty(),
              "cancel-all got its own reply (ERROR_NONE)");
        check(before.get() && before.get()->return_code == 1,
              "cancel-before got its own reply (ERROR_REJECTED)");
    }

    // --- a cancel reply no handle-taking cancel waits for is kept -----------
    {
        auto h = accepted_handle(node, *client, 14);
        auto waiting = client->async_cancel_goal(h);
        const int64_t seq_waiting = nros_stub_rmw_last_request_seq();
        check(client->cancel_goal(h->get_goal_id()).ok(), "a goal-id cancel_goal");
        const int64_t seq_plain = nros_stub_rmw_last_request_seq();
        // `cancel_goal`'s request names the same goal; the dispatch takes the
        // first reply for that goal to the waiter, the second to the stash.
        stage_cancel_reply(seq_waiting, 0, nullptr, 0, 0);
        stage_cancel_reply(seq_plain, 3, nullptr, 0, 0);
        check(rclcpp::spin_until_future_complete(node, waiting, 1000) ==
                  rclcpp::FutureReturnCode::SUCCESS,
              "the waiting cancel completes");
        client->poll();
        rclcpp_action::CancelReturnCode code = rclcpp_action::CancelReturnCode::Ok;
        check(client->try_recv_cancel_response(code).ok() &&
                  code == rclcpp_action::CancelReturnCode::GoalTerminated,
              "try_recv_cancel_response still gets the reply no waiter took");
    }
}

// --- the goal-slot leak ------------------------------------------------------
void slot_leak(const rclcpp::Node::SharedPtr& node) {
    Client client;
    check(node->create_action_client(client, "count_leak").ok(), "a goal-id client");
    Value goal;
    goal.v = 1;

    // Ten goals whose futures are dropped before any acceptance arrives.
    int64_t abandoned_seq = 0;
    int sent = 0;
    for (int i = 0; i < 10; ++i) {
        auto fut = client.send_goal_future(goal);
        if (fut.valid()) ++sent;
        if (i == 0) abandoned_seq = nros_stub_rmw_last_request_seq();
    }
    check(sent == 10, "ten abandoned goals each freed their send_goal slot (MAX_CONCURRENT_GOALS = 4)");
    auto live = client.send_goal_future(goal);
    check(live.valid(), "an eleventh goal is still sent");
    const int64_t live_seq = nros_stub_rmw_last_request_seq();
    // A late acceptance for an abandoned goal is dropped, then the live one lands.
    stage_accept(abandoned_seq, true);
    stage_accept(live_seq, true);
    Client::GoalAccept accept{};
    check(live.wait(node->executor_handle(), 1000, accept).ok() && accept.accepted,
          "the live goal gets its own acceptance past the late one");

    // The same for get_result: ten dropped futures, then one that completes.
    rclcpp_action::GoalUUID id(accept.goal_id);
    int requested = 0;
    for (int i = 0; i < 10; ++i) {
        auto fut = client.get_result_future(id);
        if (fut.valid()) ++requested;
    }
    check(requested == 10, "ten abandoned get_result requests each freed their slot");
    auto res = client.get_result_future(id);
    check(res.valid(), "an eleventh get_result is still sent");
    stage_result(nros_stub_rmw_last_request_seq(), 4, 99);
    Value out;
    check(res.wait(node->executor_handle(), 1000, out).ok() && out.v == 99,
          "...and completes");

    // An explicitly cancelled future frees its slot too.
    for (int i = 0; i < 6; ++i) {
        auto fut = client.send_goal_future(goal);
        check(fut.valid(), "a goal sent after cancelled ones");
        fut.cancel();
    }

    // A client-wide callback still waiting on acceptances keeps the slots:
    // after four abandoned futures the table is full.
    Client::SendGoalOptions raw;
    raw.goal_response = [](bool, const uint8_t*, void*) {};
    check(client.set_callbacks(raw).ok(), "a client-wide acceptance callback");
    int kept = 0;
    for (int i = 0; i < 5; ++i) {
        auto fut = client.send_goal_future(goal);
        if (fut.valid()) ++kept;
    }
    check(kept == 4, "with a client-wide callback waiting, abandoned futures keep their slots");
}

} // namespace

int main() {
    if (!rclcpp::init_with_rmw(NROS_STUB_RMW_NAME, nullptr, 0, "i2027").ok()) {
        std::fprintf(stderr, "action_goal_handle_api_runtime: init on the stub backend failed\n");
        return 1;
    }
    nros_stub_rmw_set_accept_entities(true);
    auto node = rclcpp::Node::make_shared("i2027");
    check(node && node->initialized(), "a node comes up on the stub backend");
    nros_stub_rmw_reset_client();

    server_execute(node);
    client_handle_api(node);
    slot_leak(node);

    if (g_failures != 0) {
        std::fprintf(stderr, "action_goal_handle_api_runtime: %d failure(s)\n", g_failures);
        return 1;
    }
    std::printf("action_goal_handle_api_runtime: OK\n");
    return 0;
}
