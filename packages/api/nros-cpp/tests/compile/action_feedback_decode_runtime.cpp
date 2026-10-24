// issue 2027 — a staged action FEEDBACK frame decodes to its payload and its
// goal id on every C++ client path.
//
// The frame is `[CDR header 4][GoalId 16][feedback fields]`. Issue 2014 found
// `PollingActionClient<A>::try_recv_feedback_sized` handing the WHOLE frame to
// the deserializer — the goal id read as the payload, into a buffer 16 bytes
// short — and fixed it with no runtime test, because the stub backend could
// not stage a subscription sample. Since issue 2015 it can. This drives:
//
//   * `PollingActionClient<A>::try_recv_feedback` (the L1 polling path, both
//     the raw-array and the `GoalUUID` overloads, and `_sized`);
//   * `rclcpp_action::Client<A>`: feedback taken by the executor's dispatch
//     (the client-wide `set_callbacks` feedback hook gets the goal id and the
//     spliced bytes) and read back through `try_recv_feedback`.
//
// The payload's first field byte is chosen so a decode from the wrong offset
// cannot pass: the strict deserializer checks the length AND the header, and
// the goal id's bytes are not a header.
//
// Negative control, measured: `try_recv_feedback_sized` decoding from byte 0
// (issue 2014's defect) -> FAIL lines below.

#include "rclcpp/rclcpp.hpp"
#include "rclcpp/polling_action_client.hpp"
#include "rclcpp_action/rclcpp_action.hpp"

extern "C" {
#include "stub_rmw_backend.h"
}

#include <cstdio>
#include <cstring>

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
    // Strict: exactly a little-endian header plus one int64.
    static int ffi_deserialize(const uint8_t* data, size_t len, Value* out) {
        if (len != 4 + sizeof(int64_t)) return -1;
        if (data[0] != 0x00 || data[1] != 0x01) return -1;
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

int g_failures = 0;

void check(bool cond, const char* what) {
    if (!cond) {
        ++g_failures;
        std::fprintf(stderr, "FAIL: %s\n", what);
    }
}

// The goal id's first bytes are NOT `00 01`, so a decode that reads the goal
// id as a header fails the strict deserializer.
const uint8_t kGoal[16] = {0xA5, 0x5A, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16};

void stage_feedback(const char* topic_suffix, const uint8_t goal[16], int64_t v) {
    uint8_t frame[4 + 16 + sizeof(int64_t)] = {0x00, 0x01, 0x00, 0x00};
    std::memcpy(frame + 4, goal, 16);
    std::memcpy(frame + 20, &v, sizeof(v));
    check(nros_stub_rmw_stage_sample(topic_suffix, frame, sizeof(frame)), "stage a feedback frame");
}

struct Seen {
    int calls = 0;
    uint8_t goal[16] = {};
    uint8_t bytes[64] = {};
    size_t len = 0;
};
Seen g_seen;

void on_feedback(const uint8_t goal_id[16], const uint8_t* data, size_t len, void* ctx) {
    Seen* s = static_cast<Seen*>(ctx);
    ++s->calls;
    std::memcpy(s->goal, goal_id, 16);
    s->len = len < sizeof(s->bytes) ? len : sizeof(s->bytes);
    std::memcpy(s->bytes, data, s->len);
}

} // namespace

int main() {
    check(NROS_CPP_ACTION_FEEDBACK_PAYLOAD_OFFSET == 20, "the feedback payload starts at byte 20");
    if (!rclcpp::init_with_rmw(NROS_STUB_RMW_NAME, nullptr, 0, "i2027fb").ok()) {
        std::fprintf(stderr, "action_feedback_decode_runtime: init failed\n");
        return 1;
    }
    nros_stub_rmw_set_accept_entities(true);
    auto node = rclcpp::Node::make_shared("i2027fb");
    check(node && node->initialized(), "a node comes up on the stub backend");
    nros_stub_rmw_reset_client();

    // --- 1. the polling client --------------------------------------------
    {
        rclcpp::PollingActionClient<Count> cli;
        check(node->create_polling_action_client(cli, "fb_poll").ok(), "a polling action client");
        uint8_t about[16] = {};
        Value fb;
        check(cli.try_recv_feedback(about, fb).code() == rclcpp::ErrorCode::TryAgain,
              "nothing staged: TryAgain");

        stage_feedback("fb_poll/_action/feedback", kGoal, 0x0102030405060708LL);
        check(cli.try_recv_feedback(about, fb).ok(), "try_recv_feedback decodes the frame");
        check(fb.v == 0x0102030405060708LL, "the payload is the fields at byte 20");
        check(std::memcmp(about, kGoal, 16) == 0, "the goal id is the frame's");

        stage_feedback("fb_poll/_action/feedback", kGoal, -7);
        rclcpp_action::GoalUUID id;
        Value fb2;
        check(cli.try_recv_feedback_sized<sizeof(int64_t)>(id, fb2).ok() && fb2.v == -7,
              "the _sized / GoalUUID overload: exactly the fields' capacity is enough");
        check(id == rclcpp_action::GoalUUID(kGoal), "...and names the goal");

        // A body that looks like a header is still a body.
        stage_feedback("fb_poll/_action/feedback", kGoal, 256); // `00 01 00 00 …`
        Value fb3;
        check(cli.try_recv_feedback(about, fb3).ok() && fb3.v == 256,
              "a payload starting 00 01 00 00 is decoded as the payload");
    }

    // --- 2. the callback client, through the executor's dispatch ----------
    {
        rclcpp_action::Client<Count> client;
        check(node->create_action_client(client, "fb_cb").ok(), "a callback action client");
        rclcpp_action::Client<Count>::SendGoalOptions raw;
        raw.feedback = &on_feedback;
        raw.context = &g_seen;
        check(client.set_callbacks(raw).ok(), "a client-wide feedback hook");

        stage_feedback("fb_cb/_action/feedback", kGoal, 0x1122334455667788LL);
        for (int i = 0; i < 4; ++i)
            rclcpp::spin_some(node);
        check(g_seen.calls == 1, "the executor's dispatch delivered the feedback once");
        check(std::memcmp(g_seen.goal, kGoal, 16) == 0, "...attributed to the frame's goal");
        int64_t v = 0;
        if (g_seen.len == 12) std::memcpy(&v, g_seen.bytes + 4, sizeof(v));
        check(g_seen.len == 12 && g_seen.bytes[0] == 0x00 && g_seen.bytes[1] == 0x01 &&
                  v == 0x1122334455667788LL,
              "...as [frame header][fields]: 12 bytes, the payload intact");
        Value fb;
        check(client.try_recv_feedback(fb).ok() && fb.v == 0x1122334455667788LL,
              "try_recv_feedback reads the same feedback back");

        // The client's own poll takes a frame no spin has taken yet.
        stage_feedback("fb_cb/_action/feedback", kGoal, 99);
        Value fb2;
        check(client.try_recv_feedback(fb2).ok() && fb2.v == 99,
              "try_recv_feedback polls the client for a frame no spin took");
        check(g_seen.calls == 2, "...and the hook saw it too");
    }

    if (g_failures != 0) {
        std::fprintf(stderr, "action_feedback_decode_runtime: %d failure(s)\n", g_failures);
        return 1;
    }
    std::printf("action_feedback_decode_runtime: OK\n");
    return 0;
}
