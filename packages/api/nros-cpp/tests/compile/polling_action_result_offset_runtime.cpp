// issue 2014 — `PollingActionClient<A>::try_recv_result_sized` decodes the
// result fields from byte 8 of a get_result reply.
//
// The reply is `[CDR header 4][status i8][pad to 4 → byte 8][result fields]`.
// The header skipped 5 bytes, so the deserializer was handed the three pad
// bytes plus the fields, with pad byte 5..8 and the first field byte read as
// the encapsulation header: every typed result on the polling client failed
// to decode (or, for a type that does not check its length, decoded garbage).
// The executor dispatch and the C/C++ callback paths already used 8; the
// offset is now spelled once (`nros_node::action_frame`), and this header
// reads it as `NROS_CPP_ACTION_RESULT_PAYLOAD_OFFSET`.
//
// RUNTIME, because the property is the bytes that come back: real
// `libnros_cpp.a` + the stub backend, whose replies the test stages by
// request id (`nros_stub_rmw_stage_reply_for`).

#include "rclcpp/rclcpp.hpp"
#include "rclcpp/polling_action_client.hpp"

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

// One int64 behind a CDR encapsulation header — the shape `ffi_serialize`
// writes and every action dispatch delivers (header spliced back in front).
// Strict about the length, so a decode from the wrong offset cannot pass.
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
        // A little-endian encapsulation header, or this is not one.
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

// get_result reply: [encap][status i8][pad to 8][int64 fields]. The pad bytes
// are NOT zero, so a decode that reads them shows it.
void stage_result_for(int64_t seq, int64_t value, uint8_t status) {
    uint8_t wire[16] = {0x00, 0x01, 0x00, 0x00};
    wire[4] = status;
    wire[5] = 0xEE;
    wire[6] = 0xEE;
    wire[7] = 0xEE;
    std::memcpy(wire + 8, &value, sizeof(value));
    check(nros_stub_rmw_stage_reply_for(seq, wire, sizeof(wire)), "stage a result");
}

} // namespace

int main() {
    check(NROS_CPP_ACTION_RESULT_PAYLOAD_OFFSET == 8, "the result payload starts at byte 8");
    if (!rclcpp::init_with_rmw(NROS_STUB_RMW_NAME, nullptr, 0, "i2014").ok()) {
        std::fprintf(stderr, "polling_action_result_offset_runtime: init failed\n");
        return 1;
    }
    nros_stub_rmw_set_accept_entities(true);
    auto node = rclcpp::Node::make_shared("i2014_client");
    check(node && node->initialized(), "a node comes up on the stub backend");

    nros_stub_rmw_reset_client();
    rclcpp::PollingActionClient<Count> cli;
    check(node->create_polling_action_client(cli, "count").ok(), "a polling action client");

    uint8_t goal[16] = {7};

    // --- 1. a succeeded result round-trips, bytes intact -------------------
    {
        check(cli.send_get_result_request(goal).ok(), "get_result requested");
        stage_result_for(nros_stub_rmw_last_request_seq(), 0x1122334455667788LL, 4);
        Value out;
        rclcpp::Result r = cli.try_recv_result_sized<sizeof(int64_t)>(out);
        check(r.ok(), "try_recv_result_sized decodes the result");
        check(out.v == 0x1122334455667788LL, "the result's bytes are the fields at byte 8");
    }

    // --- 2. the goal-reporting overload, and a non-success status -----------
    {
        check(cli.send_get_result_request(goal).ok(), "a second get_result requested");
        stage_result_for(nros_stub_rmw_last_request_seq(), -42, 6);
        Value out;
        uint8_t about[16] = {};
        rclcpp::Result r = cli.try_recv_result_sized<sizeof(int64_t)>(out, about);
        check(r.ok() && out.v == -42, "an aborted goal's result decodes too");
        check(std::memcmp(about, goal, 16) == 0, "it names the goal it is for");
    }

    // --- 3. the default-capacity spelling takes the same path ---------------
    {
        check(cli.send_get_result_request(goal).ok(), "a third get_result requested");
        stage_result_for(nros_stub_rmw_last_request_seq(), 256, 4); // `00 01 00 00 …`
        Value out;
        check(cli.try_recv_result(out).ok() && out.v == 256,
              "try_recv_result: a body that looks like a header is still a body");
    }

    // --- 4. a reply too short to hold a result is an error, not a value -----
    {
        check(cli.send_get_result_request(goal).ok(), "a fourth get_result requested");
        const uint8_t shorty[6] = {0x00, 0x01, 0x00, 0x00, 4, 0};
        check(nros_stub_rmw_stage_reply_for(nros_stub_rmw_last_request_seq(), shorty,
                                            sizeof(shorty)),
              "stage a short reply");
        Value out;
        check(!cli.try_recv_result_sized<sizeof(int64_t)>(out).ok(), "a short reply is refused");
    }

    if (g_failures != 0) {
        std::fprintf(stderr, "polling_action_result_offset_runtime: %d failure(s)\n", g_failures);
        return 1;
    }
    std::printf("polling_action_result_offset_runtime: OK\n");
    return 0;
}
