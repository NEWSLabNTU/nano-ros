// phase-515 P4 -- `rclcpp::GenericPublisher` and the generic subscription,
// keyed by a type NAME and HASH the caller supplies (rclcpp's name-only form
// is refused at compile time: `ros2_refuse_generic_by_name_probe.cpp`).
//
// A RUN on the stub backend:
//   1. the publisher reaches the backend under the type name and hash given,
//      and `publish_serialized` puts exactly the caller's bytes on the wire;
//   2. the subscription reaches it under its type too, its callback receives
//      a staged sample's bytes unchanged during spin, and after `reset()` it
//      receives nothing.

#include "rclcpp/rclcpp.hpp"

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

int g_failures = 0;

void check(bool cond, const char* what) {
    if (!cond) {
        ++g_failures;
        std::fprintf(stderr, "FAIL: %s\n", what);
    }
}

constexpr const char* kType = "std_msgs::msg::dds_::Int64_";
constexpr const char* kHash = "RIHS01_generic_probe_hash";

bool last_type_is(const char* type, const char* hash) {
    const char* got_hash = nullptr;
    const char* got_type = nros_stub_rmw_last_entity_type(&got_hash);
    return std::strcmp(got_type, type) == 0 && got_hash && std::strcmp(got_hash, hash) == 0;
}

struct Seen {
    int calls = 0;
    size_t len = 0;
    uint8_t bytes[32] = {};
};

void pump(const rclcpp::Node::SharedPtr& node) {
    for (int i = 0; i < 4; ++i)
        rclcpp::spin_some(node);
}

} // namespace

int main() {
    nros_stub_rmw_set_accept_entities(true);
    nros_stub_rmw_set_entity_wake_supported(false);
    if (!rclcpp::init_with_rmw(NROS_STUB_RMW_NAME, nullptr, 0, "p4").ok()) {
        std::fprintf(stderr, "generic_pubsub_runtime: init on the stub failed\n");
        return 1;
    }
    auto node = rclcpp::Node::make_shared("p4");
    check(node && node->initialized(), "a node comes up on the stub backend");

    // 1 -- publisher.
    auto pub = node->create_generic_publisher("gen_pub", kType, kHash, 10);
    check(pub->is_valid(), "create_generic_publisher returns a live publisher");
    check(last_type_is(kType, kHash), "the publisher reached the backend under the given type");
    check(std::strcmp(pub->get_topic_name(), "gen_pub") == 0, "get_topic_name");

    const uint8_t frame[12] = {0x00, 0x01, 0x00, 0x00, 7, 0, 0, 0, 0, 0, 0, 0};
    const uint32_t before = nros_stub_rmw_publish_count();
    check(pub->publish_serialized(frame, sizeof(frame)).ok(), "publish_serialized succeeds");
    size_t wire_len = 0;
    const uint8_t* wire = nros_stub_rmw_last_payload(&wire_len);
    check(nros_stub_rmw_publish_count() == before + 1, "one publish reached the backend");
    check(wire_len == sizeof(frame) && std::memcmp(wire, frame, sizeof(frame)) == 0,
          "...carrying exactly the caller's bytes");

    // 2 -- subscription.
    Seen seen;
    auto sub = node->create_generic_subscription("gen_sub", kType, kHash, rclcpp::QoS(10),
                                                 [&seen](const uint8_t* data, size_t len) {
                                                     ++seen.calls;
                                                     seen.len = len;
                                                     if (len <= sizeof(seen.bytes))
                                                         std::memcpy(seen.bytes, data, len);
                                                 });
    check(static_cast<bool>(sub), "create_generic_subscription returns a live handle");
    check(last_type_is(kType, kHash), "the subscription reached the backend under the given type");

    const uint8_t sample[12] = {0x00, 0x01, 0x00, 0x00, 42, 0, 0, 0, 0, 0, 0, 0};
    check(nros_stub_rmw_stage_sample("gen_sub", sample, sizeof(sample)), "stage a sample");
    pump(node);
    check(seen.calls == 1, "the callback ran once during spin");
    check(seen.len == sizeof(sample) && std::memcmp(seen.bytes, sample, sizeof(sample)) == 0,
          "...with the sample's bytes unchanged");

    sub.reset();
    check(!static_cast<bool>(sub), "reset() releases the handle");
    (void)nros_stub_rmw_stage_sample("gen_sub", sample, sizeof(sample));
    pump(node);
    check(seen.calls == 1, "a released subscription receives nothing");

    if (g_failures != 0) {
        std::fprintf(stderr, "generic_pubsub_runtime: %d failure(s)\n", g_failures);
        return 1;
    }
    std::printf("generic_pubsub_runtime: OK\n");
    return 0;
}
