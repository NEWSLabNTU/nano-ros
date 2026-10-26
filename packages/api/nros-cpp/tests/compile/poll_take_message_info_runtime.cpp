// issue 2074 -- the POLLING take with metadata: upstream's
// `rclcpp::Subscription::take(msg, message_info)`, spelled on our caller-owned
// `rclcpp::PollSubscription<M>` as `take(M&, rclcpp::MessageInfo&)`.
//
// A RUN on the stub backend, built TWICE by `just check cpp`: hosted
// (`-DNROS_CPP_STD=1`, C++17) and freestanding (no `NROS_CPP_STD`, C++14) --
// `MessageInfo` is plain data, so the polling take needs no heap and is offered
// on both. Two modes, one per process, because a process opens one session:
//
//   (no argument)  the stub reports metadata through its `take_with_info` slot:
//                  the take returns the message AND the staged timestamps,
//                  sequence numbers and GID, `is_reported()` true; a sample
//                  staged without metadata reports zeros, still `is_reported()`
//                  (the backend HAS a channel and said zero);
//   no-info        the stub is registered with no `take_with_info` slot (the
//                  XRCE / uORB case): every field zero and `is_reported()`
//                  false -- the case an all-zero record could not tell apart.
//
// In both: an empty subscription answers TryAgain and leaves `info` untouched,
// and the one-argument `take(M&)` still works beside it.
//
// Mutation-checked: `from_info(None)` in `nros_cpp_subscription_take_with_info`
// fails 3 checks (info run); `message_info_from_ffi` passing `false` fails 2
// (info run), and passing nothing (always reported) fails 2 (no-info run).

#include "rclcpp/rclcpp.hpp"

extern "C" {
#include "stub_rmw_backend.h"
}

#include <stdio.h>
#include <string.h>

extern "C" void nros_app_register_backends(void);
extern "C" void nros_app_register_backends(void) {
    (void)nros_stub_rmw_register();
    (void)nros_stub_rmw_register_without_message_info("stub_noinfo");
}

namespace {

int g_failures = 0;

void check(bool cond, const char* what) {
    if (!cond) {
        ++g_failures;
        fprintf(stderr, "FAIL: %s\n", what);
    }
}

struct Int64 {
    int64_t data = 0;
    static constexpr const char* TYPE_NAME = "std_msgs::msg::dds_::Int64_";
    static constexpr const char* TYPE_HASH = "RIHS01_stub";
    static const size_t SERIALIZED_SIZE_MAX = 4 + sizeof(int64_t);
    static int ffi_serialize(const Int64* msg, uint8_t* buf, size_t cap, size_t* out) {
        if (cap < SERIALIZED_SIZE_MAX) return -1;
        const uint8_t hdr[4] = {0x00, 0x01, 0x00, 0x00};
        memcpy(buf, hdr, 4);
        memcpy(buf + 4, &msg->data, sizeof(int64_t));
        *out = SERIALIZED_SIZE_MAX;
        return 0;
    }
    static int ffi_deserialize(const uint8_t* data, size_t len, void* out) {
        if (len != SERIALIZED_SIZE_MAX) return -1;
        memcpy(&static_cast<Int64*>(out)->data, data + 4, sizeof(int64_t));
        return 0;
    }
};

void frame_of(int64_t v, uint8_t (&frame)[12]) {
    const uint8_t hdr[4] = {0x00, 0x01, 0x00, 0x00};
    memcpy(frame, hdr, 4);
    memcpy(frame + 4, &v, sizeof(v));
}

void stage_info(const char* topic, int64_t v, int64_t src_ns, int64_t pub_seq, uint8_t gid0) {
    uint8_t frame[12];
    frame_of(v, frame);
    nros_stub_message_info_t info;
    memset(&info, 0, sizeof(info));
    info.source_timestamp = src_ns;
    info.received_timestamp = src_ns + 5;
    info.publication_sequence_number = static_cast<uint64_t>(pub_seq);
    info.reception_sequence_number = static_cast<uint64_t>(pub_seq) + 100;
    for (int i = 0; i < 16; ++i)
        info.publisher_gid[i] = static_cast<uint8_t>(gid0 + i);
    check(nros_stub_rmw_stage_sample_info(topic, frame, sizeof(frame), &info),
          "stage a sample with metadata");
}

void stage_bare(const char* topic, int64_t v) {
    uint8_t frame[12];
    frame_of(v, frame);
    check(nros_stub_rmw_stage_sample(topic, frame, sizeof(frame)), "stage a bare sample");
}

bool all_zero(const rclcpp::RmwMessageInfo& i) {
    bool zero = i.source_timestamp == 0 && i.received_timestamp == 0 &&
                i.publication_sequence_number == 0 && i.reception_sequence_number == 0;
    for (int k = 0; k < 24; ++k)
        zero = zero && i.publisher_gid.data[k] == 0;
    return zero;
}

bool same_info(const rclcpp::RmwMessageInfo& i, int64_t src_ns, int64_t pub_seq, uint8_t gid0) {
    bool gid = true;
    for (int k = 0; k < 16; ++k)
        gid = gid && i.publisher_gid.data[k] == static_cast<uint8_t>(gid0 + k);
    for (int k = 16; k < 24; ++k)
        gid = gid && i.publisher_gid.data[k] == 0;
    return i.source_timestamp == src_ns && i.received_timestamp == src_ns + 5 &&
           i.publication_sequence_number == static_cast<uint64_t>(pub_seq) &&
           i.reception_sequence_number == static_cast<uint64_t>(pub_seq) + 100 && gid;
}

/// A record the take must not touch when nothing was waiting.
rclcpp::MessageInfo sentinel() {
    rclcpp::RmwMessageInfo r;
    r.source_timestamp = 777;
    return rclcpp::MessageInfo(r);
}

} // namespace

int main(int argc, char** argv) {
    const bool no_info = argc > 1 && strcmp(argv[1], "no-info") == 0;
    const char* rmw = no_info ? "stub_noinfo" : NROS_STUB_RMW_NAME;
    nros_stub_rmw_set_accept_entities(true);
    if (!rclcpp::init_with_rmw(rmw, nullptr, 0, "i2074").ok()) {
        fprintf(stderr, "poll_take_message_info_runtime: init on %s failed\n", rmw);
        return 1;
    }
    rclcpp::Node node;
    check(rclcpp::create_node(node, "i2074", nullptr).ok(), "a node comes up on the stub");
    rclcpp::PollSubscription<Int64> sub;
    check(node.create_subscription(sub, "pt_info").ok(), "a poll subscription is created");
    if (g_failures != 0) return 1;

    Int64 m;
    rclcpp::MessageInfo info = sentinel();

    // Nothing waiting: TryAgain, `info` untouched.
    rclcpp::Result r = sub.take(m, info);
    check(!r.ok() && r.code() == rclcpp::ErrorCode::TryAgain, "an empty take answers TryAgain");
    check(info.get_rmw_message_info().source_timestamp == 777,
          "an empty take leaves the caller's MessageInfo untouched");

    // A sample staged with metadata.
    stage_info("pt_info", 41, 5000, 12, 0x50);
    r = sub.take(m, info);
    check(r.ok() && m.data == 41, "take(M&, MessageInfo&) returns the message");
    if (no_info) {
        check(all_zero(info.get_rmw_message_info()) && !info.is_reported(),
              "no metadata channel: every field zero and is_reported() false");
    } else {
        check(same_info(info.get_rmw_message_info(), 5000, 12, 0x50),
              "take(M&, MessageInfo&) returns the sample's metadata");
        check(info.is_reported(), "metadata reported: is_reported() true");
    }

    // A sample staged with none.
    stage_bare("pt_info", 42);
    info = sentinel();
    r = sub.take_sized<64>(m, info);
    check(r.ok() && m.data == 42, "take_sized<Cap>(M&, MessageInfo&) returns the message");
    check(all_zero(info.get_rmw_message_info()),
          "a sample with no metadata arrives with every field zero, never stale");
    check(info.is_reported() == !no_info,
          "is_reported() follows the backend's channel, not the field values");

    // The one-argument take still works beside it.
    stage_bare("pt_info", 43);
    r = sub.take(m);
    check(r.ok() && m.data == 43, "take(M&) still works");

    rclcpp::shutdown();
    if (g_failures != 0) {
        fprintf(stderr, "poll_take_message_info_runtime(%s): %d failure(s)\n",
                no_info ? "no-info" : "info", g_failures);
        return 1;
    }
    printf("poll_take_message_info_runtime(%s): OK\n", no_info ? "no-info" : "info");
    return 0;
}
