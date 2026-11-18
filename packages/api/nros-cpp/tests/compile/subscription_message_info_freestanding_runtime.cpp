// Issue 2717 -- a `(const M&, const rclcpp::MessageInfo&)` subscription
// callback on a FREESTANDING build (no NROS_CPP_STD, no allocator).
//
// The callable is copied into the executor arena, as the one-argument
// callback's is, so nothing needs a heap. A RUN on the stub backend, twice:
// with its `take_with_info` slot (metadata reported) and without it.
//
//   1. the callback gets the message AND the sample's metadata: both
//      timestamps, both sequence numbers, the publisher GID;
//   2. with no metadata channel it still runs, every field zero and
//      `is_reported()` false;
//   3. a capture with state survives the copy into the arena (the callback
//      writes through a captured pointer and reads a captured value).
//
// Mutation-checked: handing the callback a default `MessageInfo` instead of
// `message_info_from_ffi(*info)` (`subscription.hpp`) fails (1).

#include "rclcpp/rclcpp.hpp"

extern "C" {
#include "stub_rmw_backend.h"
}

#include <stdio.h>
#include <string.h>

#if defined(NROS_CPP_HAS_SHARED_PTR)
#error "this test must be compiled WITHOUT -DNROS_CPP_STD"
#endif

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

struct Seen {
    int calls = 0;
    int64_t value = 0;
    int64_t tag = 0;
    bool reported = false;
    rclcpp::RmwMessageInfo info;
};

void stage(const char* topic, int64_t v) {
    uint8_t frame[12] = {0x00, 0x01, 0x00, 0x00};
    memcpy(frame + 4, &v, sizeof(v));
    nros_stub_message_info_t info = {};
    info.source_timestamp = 9000;
    info.received_timestamp = 9005;
    info.publication_sequence_number = 31;
    info.reception_sequence_number = 131;
    for (int i = 0; i < 16; ++i)
        info.publisher_gid[i] = static_cast<uint8_t>(0x70 + i);
    check(nros_stub_rmw_stage_sample_info(topic, frame, sizeof(frame), &info),
          "stage a sample with metadata");
}

} // namespace

int main(int argc, char** argv) {
    const bool no_info = argc > 1 && strcmp(argv[1], "no-info") == 0;
    const char* rmw = no_info ? "stub_noinfo" : NROS_STUB_RMW_NAME;
    nros_stub_rmw_set_accept_entities(true);
    nros_stub_rmw_set_entity_wake_supported(false);
    if (!rclcpp::init_with_rmw(rmw, nullptr, 0, "i2717").ok()) {
        fprintf(stderr, "subscription_message_info_freestanding_runtime: init on %s failed\n", rmw);
        return 1;
    }
    rclcpp::Node node;
    check(rclcpp::create_node(node, "i2717", nullptr).ok(), "a node comes up on the stub");

    Seen seen;
    Seen* out = &seen;
    const int64_t tag = 4242;
    auto sub = node.create_subscription<Int64>(
        "mi_fs", 10, [out, tag](const Int64& m, const rclcpp::MessageInfo& mi) {
            ++out->calls;
            out->value = m.data;
            out->tag = tag;
            out->reported = mi.is_reported();
            out->info = mi.get_rmw_message_info();
        });
    (void)sub;
    if (g_failures != 0) return 1;

    stage("mi_fs", 77);
    rclcpp::Node::SharedPtr handle = node.shared_from_this();
    for (int i = 0; i < 4; ++i)
        rclcpp::spin_some(handle);

    check(seen.calls == 1 && seen.value == 77, "the callback gets the message");
    check(seen.tag == tag, "the capture survived its copy into the arena");
    const rclcpp::RmwMessageInfo& i = seen.info;
    if (no_info) {
        bool zero = i.source_timestamp == 0 && i.received_timestamp == 0 &&
                    i.publication_sequence_number == 0 && i.reception_sequence_number == 0;
        for (int k = 0; k < 24; ++k)
            zero = zero && i.publisher_gid.data[k] == 0;
        check(zero && !seen.reported, "no metadata channel: every field zero, not reported");
    } else {
        bool gid = true;
        for (int k = 0; k < 16; ++k)
            gid = gid && i.publisher_gid.data[k] == static_cast<uint8_t>(0x70 + k);
        check(seen.reported, "metadata reported: is_reported() true");
        check(i.source_timestamp == 9000 && i.received_timestamp == 9005,
              "both timestamps are the sample's");
        check(i.publication_sequence_number == 31 && i.reception_sequence_number == 131,
              "both sequence numbers are the sample's");
        check(gid, "the publisher GID is the sample's");
    }

    rclcpp::shutdown();
    if (g_failures != 0) {
        fprintf(stderr, "subscription_message_info_freestanding_runtime: %d failure(s)\n",
                g_failures);
        return 1;
    }
    printf("subscription_message_info_freestanding_runtime (%s): OK\n", rmw);
    return 0;
}
