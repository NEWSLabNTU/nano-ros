// issue 2036 -- `rclcpp::MessageInfo` and the upstream subscription callback
// shapes that take it, on a hosted build:
//
//   (const M&, const rclcpp::MessageInfo&)
//   (std::shared_ptr<const M>, const rclcpp::MessageInfo&)
//   (std::shared_ptr<M>, const rclcpp::MessageInfo&)
//   (std::unique_ptr<M>, const rclcpp::MessageInfo&)
//
// Each is handed the message AND what the backend reported for that sample:
// both timestamps, both sequence numbers and the publisher GID, through
// `get_rmw_message_info()`. A sample the backend reports nothing for arrives
// with every field zero.
//
// A RUN on the stub backend, which reports the metadata the test stages with
// each sample (`nros_stub_rmw_stage_sample_info`) through its `take_with_info`
// slot -- the path a real backend's report takes.

#include "rclcpp/rclcpp.hpp"

extern "C" {
#include "stub_rmw_backend.h"
}

#include <cstdio>
#include <cstring>
#include <memory>

extern "C" void nros_app_register_backends(void);
extern "C" void nros_app_register_backends(void) {
    (void)nros_stub_rmw_register();
}

namespace {

int g_failures = 0;

void check(bool cond, const char* what) {
    if (!cond) {
        ++g_failures;
        std::fprintf(stderr, "FAIL: %s\n", what);
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
        std::memcpy(buf, hdr, 4);
        std::memcpy(buf + 4, &msg->data, sizeof(int64_t));
        *out = SERIALIZED_SIZE_MAX;
        return 0;
    }
    static int ffi_deserialize(const uint8_t* data, size_t len, void* out) {
        if (len != SERIALIZED_SIZE_MAX) return -1;
        std::memcpy(&static_cast<Int64*>(out)->data, data + 4, sizeof(int64_t));
        return 0;
    }
};

void stage(const char* topic, int64_t v, int64_t src_ns, int64_t pub_seq, uint8_t gid0) {
    uint8_t frame[12] = {0x00, 0x01, 0x00, 0x00};
    std::memcpy(frame + 4, &v, sizeof(v));
    nros_stub_message_info_t info = {};
    info.source_timestamp = src_ns;
    info.received_timestamp = src_ns + 5;
    info.publication_sequence_number = static_cast<uint64_t>(pub_seq);
    info.reception_sequence_number = static_cast<uint64_t>(pub_seq) + 100;
    for (int i = 0; i < 16; ++i)
        info.publisher_gid[i] = static_cast<uint8_t>(gid0 + i);
    check(nros_stub_rmw_stage_sample_info(topic, frame, sizeof(frame), &info),
          "stage a sample with metadata");
}

struct Seen {
    int calls = 0;
    int64_t value = 0;
    rclcpp::RmwMessageInfo info;
};

void pump(const rclcpp::Node::SharedPtr& node) {
    for (int i = 0; i < 4; ++i)
        rclcpp::spin_some(node);
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

} // namespace

int main() {
    nros_stub_rmw_set_accept_entities(true);
    // The stub stages samples without an arrival callback: the executor polls.
    nros_stub_rmw_set_entity_wake_supported(false);
    if (!rclcpp::init_with_rmw(NROS_STUB_RMW_NAME, nullptr, 0, "i2036").ok()) {
        std::fprintf(stderr, "subscription_message_info_runtime: init on the stub failed\n");
        return 1;
    }
    auto node = rclcpp::Node::make_shared("i2036");
    check(node && node->initialized(), "a node comes up on the stub backend");

    Seen ref, csp, sp, up;
    // The (const M&, MessageInfo) shape through the SubscriptionOptions
    // overload, with the flag set (which a MessageInfo callback satisfies);
    // the other three through the plain overload it forwards to.
    rclcpp::SubscriptionOptions so;
    so.message_info = true;
    auto s1 = node->create_subscription<Int64>(
        "mi_ref", 10,
        [&ref](const Int64& m, const rclcpp::MessageInfo& mi) {
            ++ref.calls;
            ref.value = m.data;
            ref.info = mi.get_rmw_message_info();
        },
        so);
    auto s2 = node->create_subscription<Int64>(
        "mi_csp", 10,
        [&csp](std::shared_ptr<const Int64> m, const rclcpp::MessageInfo& mi) {
            ++csp.calls;
            csp.value = m ? m->data : -1;
            csp.info = mi.get_rmw_message_info();
        });
    auto s3 = node->create_subscription<Int64>(
        "mi_sp", 10, [&sp](std::shared_ptr<Int64> m, const rclcpp::MessageInfo& mi) {
            ++sp.calls;
            sp.value = m ? m->data : -1;
            sp.info = mi.get_rmw_message_info();
        });
    auto s4 = node->create_subscription<Int64>(
        "mi_up", 10, [&up](std::unique_ptr<Int64> m, const rclcpp::MessageInfo& mi) {
            ++up.calls;
            up.value = m ? m->data : -1;
            up.info = mi.get_rmw_message_info();
        });
    (void)s1;
    (void)s2;
    (void)s3;
    (void)s4;

    stage("mi_ref", 11, 1000, 7, 0x10);
    stage("mi_csp", 22, 2000, 8, 0x20);
    stage("mi_sp", 33, 3000, 9, 0x30);
    stage("mi_up", 44, 4000, 10, 0x40);
    pump(node);

    check(ref.calls == 1 && ref.value == 11, "(const M&, const MessageInfo&) gets the message");
    check(same_info(ref.info, 1000, 7, 0x10), "...and the sample's metadata");
    check(csp.calls == 1 && csp.value == 22 && same_info(csp.info, 2000, 8, 0x20),
          "(shared_ptr<const M>, const MessageInfo&) gets both");
    check(sp.calls == 1 && sp.value == 33 && same_info(sp.info, 3000, 9, 0x30),
          "(shared_ptr<M>, const MessageInfo&) gets both");
    check(up.calls == 1 && up.value == 44 && same_info(up.info, 4000, 10, 0x40),
          "(unique_ptr<M>, const MessageInfo&) gets both");

    // A sample staged with NO metadata: every field zero, never stale.
    {
        uint8_t frame[12] = {0x00, 0x01, 0x00, 0x00};
        const int64_t v = 55;
        std::memcpy(frame + 4, &v, sizeof(v));
        check(nros_stub_rmw_stage_sample("mi_ref", frame, sizeof(frame)), "stage a bare sample");
        pump(node);
        const rclcpp::RmwMessageInfo& i = ref.info;
        bool zero = i.source_timestamp == 0 && i.received_timestamp == 0 &&
                    i.publication_sequence_number == 0 && i.reception_sequence_number == 0;
        for (int k = 0; k < 24; ++k)
            zero = zero && i.publisher_gid.data[k] == 0;
        check(ref.calls == 2 && ref.value == 55 && zero,
              "a sample the backend reports nothing for arrives with every field zero");
    }

    rclcpp::shutdown();
    if (g_failures != 0) {
        std::fprintf(stderr, "subscription_message_info_runtime: %d failure(s)\n", g_failures);
        return 1;
    }
    std::printf("subscription_message_info_runtime: OK\n");
    return 0;
}
