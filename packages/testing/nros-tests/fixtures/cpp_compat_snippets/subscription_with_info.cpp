// Compile-only coverage for the callback-style subscription-with-attachment
// path (`Node::create_subscription_with_info`). Instantiating the template
// (including `Subscription<M>::message_info_trampoline`, whose address the
// register call takes) proves the header method type-checks against the
// public C++ API. Runtime delivery of the attachment is covered by the
// cross-RMW bridge e2e tests; this snippet exists because phase-277 W5
// removed the `if (false)` instantiation block from the
// examples/native/cpp/listener example (examples stay demo-only).
//
// The message type is a minimal stand-in with the same static surface the
// generated C++ bindings expose (`TYPE_NAME` / `TYPE_HASH` /
// `ffi_deserialize` / `SERIALIZED_SIZE_MAX`) so the snippet needs no generated
// headers. `SERIALIZED_SIZE_MAX` became part of that surface when phase-456 W7
// made every C++ subscription registration state its receive bound
// (`rx_buffer_capacity<M>`): an unmarked stand-in is the `legacy` arm of
// `nros/size_bound.hpp`, which reads it. Without it this snippet stopped
// compiling and its consuming test reported a MISSING in-lane fixture
// (issue 1684's tier-1 run, measured 2026-10-06).

#include <nros/nros.hpp>

#include <cstddef>
#include <cstdint>

namespace {

struct FakeString {
    static constexpr const char* TYPE_NAME = "test_msgs::msg::dds_::Fake_";
    static constexpr const char* TYPE_HASH = "TypeHashNotSupported";
    static constexpr size_t SERIALIZED_SIZE_MAX = 64;
    static int ffi_deserialize(const uint8_t* data, size_t len, void* out) {
        (void)data;
        (void)len;
        (void)out;
        return 0;
    }
};

} // namespace

int main() {
    rclcpp::Node node;
    rclcpp::Subscription<FakeString> info_sub;
    (void)node.create_subscription_with_info<FakeString>(
        info_sub, "/chatter_info", ::nros::QoS(10),
        [](const FakeString& m, const uint8_t* attachment, size_t attachment_len) {
            (void)m;
            (void)attachment;
            (void)attachment_len;
        });
    return 0;
}
