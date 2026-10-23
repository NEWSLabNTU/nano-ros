// issue 2011 — a C++ action client's futures are keyed by their GOAL.
//
// Before this, `get_result_future` read `RESULT_STASH`, a process-global
// one-slot buffer shared by every C++ action client, and `send_goal_future`
// raced the executor's dispatch for its own reply: a reply the arena took
// during `Future::wait`'s `spin_once` went to a callback and never reached the
// future. No test drove either one end to end.
//
// RUNTIME, because every property here is behaviour: which future an
// acceptance or a result reaches when replies arrive out of order, on which
// path (the executor's dispatch or the future's own poll), what happens to a
// result nobody waits for any more, what two clients in one process see, and
// what the client does when its table is full.
//
// Real `libnros_cpp.a` + the stub backend, whose replies the test STAGES BY
// REQUEST ID (`nros_stub_rmw_stage_reply_for`) — and which hands a reply only
// to the client that sent the request, so an action client's three
// sub-clients and two action clients stay apart, as on a wire.

#include "rclcpp/rclcpp.hpp"

extern "C" {
#include "stub_rmw_backend.h"
}

#include <chrono>
#include <cstdio>
#include <cstring>
#include <memory>
#include <utility>

extern "C" void nros_app_register_backends(void);
extern "C" void nros_app_register_backends(void) {
    (void)nros_stub_rmw_register();
}

namespace {

// One int64 behind a CDR encapsulation header — the shape `ffi_serialize`
// writes and the action dispatch delivers (header spliced back in front).
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
using Accept = Client::GoalAccept;
using AcceptFut = rclcpp::Future<Accept>;
using ResultFut = rclcpp::Future<Value>;

int g_failures = 0;

void check(bool cond, const char* what) {
    if (!cond) {
        ++g_failures;
        std::fprintf(stderr, "FAIL: %s\n", what);
    }
}

// send_goal reply: [encap][accepted u8][pad][stamp i32 + u32].
void stage_accept_for(int64_t seq, bool accepted) {
    uint8_t wire[16] = {0x00, 0x01, 0x00, 0x00};
    wire[4] = accepted ? 1 : 0;
    check(nros_stub_rmw_stage_reply_for(seq, wire, sizeof(wire)), "stage an acceptance");
}

// get_result reply: [encap][status i8][pad to 8][int64 fields].
void stage_result_for(int64_t seq, int64_t value, uint8_t status = 4) {
    uint8_t wire[16] = {0x00, 0x01, 0x00, 0x00};
    wire[4] = status;
    std::memcpy(wire + 8, &value, sizeof(value));
    check(nros_stub_rmw_stage_reply_for(seq, wire, sizeof(wire)), "stage a result");
}

// The goal id the client generates for its n-th goal (counter, little-endian).
void goal_n(uint8_t out[16], uint64_t n) {
    std::memset(out, 0, 16);
    std::memcpy(out, &n, sizeof(n));
}

// Poll a future's own path (no executor) a bounded number of times.
template <typename F> bool poll_ready(F& f, int tries = 16) {
    for (int i = 0; i < tries; ++i)
        if (f.is_ready()) return true;
    return false;
}

int64_t take_value(ResultFut& f) {
    Value out;
    if (!f.try_take(out).ok()) return -999;
    return out.v;
}

struct CallbackLog {
    int results = 0;
    int accepts = 0;
    int64_t last_result = 0;
};

void on_result(const uint8_t[16], int32_t, const uint8_t* data, size_t len, void* ctx) {
    auto* log = static_cast<CallbackLog*>(ctx);
    ++log->results;
    Value v;
    if (Value::ffi_deserialize(data, len, &v) == 0) log->last_result = v.v;
}

void on_accept(bool, const uint8_t[16], void* ctx) {
    ++static_cast<CallbackLog*>(ctx)->accepts;
}

bool make_client(rclcpp::Node& node, Client& c, const char* name) {
    return node.create_action_client(c, name).ok() && c.is_valid();
}

} // namespace

int main() {
    if (!rclcpp::init_with_rmw(NROS_STUB_RMW_NAME, nullptr, 0, "i2011").ok()) {
        std::fprintf(stderr, "action_future_keying_runtime: init on the stub backend failed\n");
        return 1;
    }
    nros_stub_rmw_set_accept_entities(true);
    auto node = rclcpp::Node::make_shared("i2011_client");
    check(node && node->initialized(), "a node comes up on the stub backend");
    Value goal;

    // --- 1. two goals, ACCEPTANCES out of order, the futures' own poll ------
    {
        nros_stub_rmw_reset_client();
        Client client;
        check(make_client(*node, client, "count1"), "an action client");
        goal.v = 1;
        AcceptFut fa = client.send_goal_future(goal);
        const int64_t seq_a = nros_stub_rmw_last_request_seq();
        goal.v = 2;
        AcceptFut fb = client.send_goal_future(goal);
        const int64_t seq_b = nros_stub_rmw_last_request_seq();
        check(fa.valid() && fb.valid(), "two goals in flight on one client");
        stage_accept_for(seq_b, false); // b's arrives first, and says REJECTED
        stage_accept_for(seq_a, true);
        check(poll_ready(fa), "a's acceptance is found behind b's");
        check(fb.is_ready(), "b was handed its acceptance when a's poll took it");
        Accept aa, ab;
        check(fa.try_take(aa).ok() && fb.try_take(ab).ok(), "both acceptances decode");
        uint8_t g1[16], g2[16];
        goal_n(g1, 1);
        goal_n(g2, 2);
        check(std::memcmp(aa.goal_id, g1, 16) == 0, "a's acceptance names a's goal");
        check(std::memcmp(ab.goal_id, g2, 16) == 0, "b's acceptance names b's goal");
        check(aa.accepted && !ab.accepted, "each future gets its OWN verdict");
        check(nros_stub_rmw_staged_replies() == 0, "both replies were consumed");
    }

    // --- 1b. the same, the OLDER goal's acceptance first, the NEWER polls ---
    {
        nros_stub_rmw_reset_client();
        Client client;
        check(make_client(*node, client, "count1b"), "an action client");
        AcceptFut fa = client.send_goal_future(goal);
        const int64_t seq_a = nros_stub_rmw_last_request_seq();
        AcceptFut fb = client.send_goal_future(goal);
        const int64_t seq_b = nros_stub_rmw_last_request_seq();
        stage_accept_for(seq_a, true);
        stage_accept_for(seq_b, false);
        check(poll_ready(fb), "b's acceptance is found behind a's");
        check(fa.is_ready(), "a was handed its own when b's poll took it");
        Accept aa, ab;
        check(fa.try_take(aa).ok() && fb.try_take(ab).ok(), "both decode");
        check(aa.accepted && !ab.accepted, "neither took the other's verdict");
    }

    // --- 2. send_goal_future with the EXECUTOR dispatching ------------------
    // The arena takes the reply during `wait_for`'s `spin_once`; it used to go
    // to a callback and never reach the future.
    {
        nros_stub_rmw_reset_client();
        Client client;
        check(make_client(*node, client, "count2"), "an action client");
        AcceptFut f = client.send_goal_future(goal);
        stage_accept_for(nros_stub_rmw_last_request_seq(), true);
        check(f.wait_for(std::chrono::milliseconds(500)) == std::future_status::ready,
              "the acceptance the executor dispatched reaches the future");
        Accept a;
        check(f.try_take(a).ok() && a.accepted, "and says accepted");
        check(nros_stub_rmw_staged_replies() == 0, "the reply was consumed");
    }

    // --- 3. two goals, RESULTS out of order, through the executor -----------
    {
        nros_stub_rmw_reset_client();
        Client client;
        check(make_client(*node, client, "count3"), "an action client");
        CallbackLog log;
        Client::SendGoalOptions opts;
        opts.result = &on_result;
        opts.context = &log;
        check(client.set_callbacks(opts).ok(), "a user result callback");
        uint8_t ga[16], gb[16];
        goal_n(ga, 7);
        goal_n(gb, 8);
        ResultFut ra = client.get_result_future(ga);
        const int64_t seq_a = nros_stub_rmw_last_request_seq();
        ResultFut rb = client.get_result_future(gb);
        const int64_t seq_b = nros_stub_rmw_last_request_seq();
        check(ra.valid() && rb.valid(), "two results awaited on one client");
        stage_result_for(seq_a, 100); // a's first, while b waits
        stage_result_for(seq_b, 200);
        check(rb.wait_for(std::chrono::milliseconds(500)) == std::future_status::ready,
              "b's result arrives through the executor");
        check(take_value(rb) == 200, "b gets b's result");
        check(ra.is_ready() ||
                  ra.wait_for(std::chrono::milliseconds(500)) == std::future_status::ready,
              "a's result arrives too");
        check(take_value(ra) == 100, "a gets a's result, not b's");
        check(log.results == 2, "the user's result callback still sees both");
    }

    // --- 4. TWO action clients in one process do not cross ------------------
    // Both clients' first goal has the SAME id (each counts from 1), so only
    // the client a reply arrived on can tell them apart.
    {
        nros_stub_rmw_reset_client();
        Client x, y;
        check(make_client(*node, x, "count4x") && make_client(*node, y, "count4y"),
              "two action clients");
        uint8_t g[16];
        goal_n(g, 1);
        ResultFut rx = x.get_result_future(g);
        const int64_t seq_x = nros_stub_rmw_last_request_seq();
        ResultFut ry = y.get_result_future(g);
        const int64_t seq_y = nros_stub_rmw_last_request_seq();
        stage_result_for(seq_y, 22);
        stage_result_for(seq_x, 11);
        check(rx.wait_for(std::chrono::milliseconds(500)) == std::future_status::ready,
              "x's result arrives");
        check(ry.is_ready() ||
                  ry.wait_for(std::chrono::milliseconds(500)) == std::future_status::ready,
              "y's result arrives");
        check(take_value(rx) == 11, "x gets x's result");
        check(take_value(ry) == 22, "y gets y's result");
    }

    // --- 5. a LATE result after the future was cancelled / destroyed --------
    {
        nros_stub_rmw_reset_client();
        Client client;
        check(make_client(*node, client, "count5"), "an action client");
        CallbackLog log;
        Client::SendGoalOptions opts;
        opts.result = &on_result;
        opts.context = &log;
        check(client.set_callbacks(opts).ok(), "a user result callback");
        uint8_t gc[16], gd[16], gl[16];
        goal_n(gc, 3);
        goal_n(gd, 4);
        goal_n(gl, 5);
        ResultFut cancelled = client.get_result_future(gc);
        const int64_t seq_c = nros_stub_rmw_last_request_seq();
        cancelled.cancel();
        int64_t seq_d = 0;
        {
            ResultFut gone = client.get_result_future(gd);
            seq_d = nros_stub_rmw_last_request_seq();
        }
        ResultFut live = client.get_result_future(gl);
        const int64_t seq_l = nros_stub_rmw_last_request_seq();
        stage_result_for(seq_c, 301);
        stage_result_for(seq_d, 302);
        check(!poll_ready(live, 4), "neither late result answers the live future");
        check(!cancelled.valid() && !cancelled.is_ready(), "the cancelled future stays empty");
        check(log.results == 2, "the late results reached the callback, nobody's future");
        stage_result_for(seq_l, 303);
        check(poll_ready(live) && take_value(live) == 303, "the live future gets its own");
    }

    // --- 6. a FULL table refuses; it never misroutes ------------------------
    {
        nros_stub_rmw_reset_client();
        Client client;
        check(make_client(*node, client, "count6"), "an action client");
        const uint32_t sent0 = nros_stub_rmw_requests_sent();
        ResultFut f[4];
        int64_t seq[4];
        for (int i = 0; i < 4; ++i) {
            uint8_t g[16];
            goal_n(g, static_cast<uint64_t>(10 + i));
            f[i] = client.get_result_future(g);
            seq[i] = nros_stub_rmw_last_request_seq();
        }
        uint8_t g5[16];
        goal_n(g5, 99);
        ResultFut refused = client.get_result_future(g5);
        check(f[0].valid() && f[1].valid() && f[2].valid() && f[3].valid(),
              "MAX_CONCURRENT_GOALS (4) results may be awaited");
        check(!refused.valid(), "a fifth is refused: an empty future");
        check(nros_stub_rmw_requests_sent() == sent0 + 4, "the refused request was never sent");
        for (int i = 3; i >= 0; --i)
            stage_result_for(seq[i], 1000 + i);
        bool all = true;
        for (int i = 0; i < 4; ++i)
            all = all && poll_ready(f[i]) && take_value(f[i]) == 1000 + i;
        check(all, "the four still get their own results, in any order");
        ResultFut after = client.get_result_future(g5);
        check(after.valid(), "an answered table frees room for the next send");
    }

    // --- 7. the BLOCKING calls wait for their own goal and forward the rest -
    // They used to wait on a process-global flag set by ANY acceptance/result,
    // with every other callback unplugged meanwhile.
    {
        nros_stub_rmw_reset_client();
        Client client;
        check(make_client(*node, client, "count7"), "an action client");
        AcceptFut pending = client.send_goal_future(goal);
        const int64_t seq_pending = nros_stub_rmw_last_request_seq();
        stage_accept_for(seq_pending, false);    // the future's own (rejected)
        stage_accept_for(seq_pending + 1, true); // the blocking call's
        uint8_t id[16];
        check(client.send_goal(goal, id).ok(), "send_goal returns ITS acceptance (accepted)");
        check(pending.is_ready(), "the pending future was handed its own meanwhile");
        Accept a;
        check(pending.try_take(a).ok() && !a.accepted, "with its own verdict (rejected)");
        uint8_t g2[16];
        goal_n(g2, 2);
        check(std::memcmp(id, g2, 16) == 0, "send_goal reports its own goal id");

        uint8_t gx[16];
        goal_n(gx, 42);
        ResultFut rf = client.get_result_future(gx);
        const int64_t seq_rf = nros_stub_rmw_last_request_seq();
        stage_result_for(seq_rf, 55);
        stage_result_for(seq_rf + 1, 66);
        Value blocking;
        check(client.get_result(id, blocking).ok(), "get_result completes");
        check(blocking.v == 66, "get_result returns ITS goal's result");
        check(rf.is_ready() && take_value(rf) == 55, "the future got its own meanwhile");
    }

    // --- 8. a client MOVED with futures pending: they follow it -------------
    {
        nros_stub_rmw_reset_client();
        Client original;
        check(make_client(*node, original, "count8"), "an action client");
        uint8_t g[16];
        goal_n(g, 1);
        ResultFut r = original.get_result_future(g);
        const int64_t seq = nros_stub_rmw_last_request_seq();
        Client moved(std::move(original));
        stage_result_for(seq, 77);
        check(r.wait_for(std::chrono::milliseconds(500)) == std::future_status::ready,
              "the future is still delivered to after its client moved");
        check(take_value(r) == 77, "with its own result");
    }

    // --- 9. a future outliving its client: invalid, not dangling ------------
    {
        nros_stub_rmw_reset_client();
        ResultFut orphan;
        {
            Client scoped;
            check(make_client(*node, scoped, "count9"), "a scoped action client");
            uint8_t g[16];
            goal_n(g, 1);
            orphan = scoped.get_result_future(g);
            check(orphan.valid(), "a result awaited on the scoped client");
        }
        check(!orphan.valid() && !orphan.is_ready(), "the client is gone: the future is empty");
    }

    // --- 10. a wait with nothing staged lasts at least its budget -----------
    {
        nros_stub_rmw_reset_client();
        Client client;
        check(make_client(*node, client, "count10"), "an action client");
        AcceptFut f = client.send_goal_future(goal);
        const auto t0 = std::chrono::steady_clock::now();
        check(f.wait_for(std::chrono::milliseconds(50)) == std::future_status::timeout,
              "no reply staged: the wait times out");
        const auto waited = std::chrono::steady_clock::now() - t0;
        check(waited >= std::chrono::milliseconds(50), "and waited at least its budget");
    }

    // --- 11. L1 polling client: each reply names its goal -------------------
    {
        nros_stub_rmw_reset_client();
        rclcpp::PollingActionClient<Count> poll;
        check(node->create_polling_action_client(poll, "count11").ok(), "an L1 polling client");
        uint8_t ga[16], gb[16];
        check(poll.send_goal(goal, ga).ok(), "goal a");
        const int64_t seq_a = nros_stub_rmw_last_request_seq();
        check(poll.send_goal(goal, gb).ok(), "goal b");
        const int64_t seq_b = nros_stub_rmw_last_request_seq();
        stage_accept_for(seq_b, false);
        stage_accept_for(seq_a, true);
        uint8_t buf[32];
        size_t len = 0;
        uint8_t about[16];
        check(poll.try_recv_goal_response(buf, sizeof(buf), len, about).ok(), "first reply");
        check(std::memcmp(about, gb, 16) == 0 && buf[4] == 0, "names goal b (rejected)");
        check(poll.try_recv_goal_response(buf, sizeof(buf), len, about).ok(), "second reply");
        check(std::memcmp(about, ga, 16) == 0 && buf[4] == 1, "names goal a (accepted)");
    }

    rclcpp::shutdown();
    if (g_failures != 0) {
        std::fprintf(stderr, "action_future_keying_runtime: %d failure(s)\n", g_failures);
        return 1;
    }
    std::printf("action_future_keying_runtime: OK\n");
    return 0;
}
