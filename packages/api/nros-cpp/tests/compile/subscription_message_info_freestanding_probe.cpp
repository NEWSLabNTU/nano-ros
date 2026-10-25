/*
 * NEGATIVE probe — issue 2036, a MessageInfo callback WITHOUT the hosted surface.
 *
 * Compiled with no `-DNROS_CPP_STD`, i.e. the freestanding API. The metadata
 * path keeps its callable on the heap, so a
 * `(const M&, const rclcpp::MessageInfo&)` callback must be REFUSED at
 * `create_subscription` with the text naming `MessageInfo` — not accepted, and
 * not a conversion error inside `InplaceFn`.
 *
 * `just check cpp` requires this TU to FAIL with "REFUSED by nano-ros" and
 * "rclcpp::MessageInfo".
 */

#include <rclcpp/rclcpp.hpp>

#include <cstddef>
#include <cstdint>

#if defined(NROS_CPP_HAS_SHARED_PTR)
#error "this probe must be compiled WITHOUT -DNROS_CPP_STD"
#endif

namespace {

struct Msg {
    int32_t data = 0;
    static const size_t SERIALIZED_SIZE_MAX = 16;
    static constexpr const char* TYPE_NAME = "std_msgs::msg::dds_::Int32_";
    static constexpr const char* TYPE_HASH = "RIHS01_int32_stub";
    static int ffi_publish(void*, const void*) { return 0; }
    static int ffi_serialize(const void*, uint8_t*, size_t, size_t* out) {
        if (out) *out = 0;
        return 0;
    }
    static int ffi_deserialize(const uint8_t*, size_t, void*) { return 0; }
};

} // namespace

int subscription_message_info_freestanding_probe(rclcpp::Node& node);
int subscription_message_info_freestanding_probe(rclcpp::Node& node) {
    auto sub = node.create_subscription<Msg>(
        "chatter", 10, [](const Msg& msg, const rclcpp::MessageInfo& info) {
            (void)msg.data;
            (void)info.get_rmw_message_info().source_timestamp;
        });
    (void)sub;
    return 0;
}
