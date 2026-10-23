/*
 * NEGATIVE probe — issue 1995, the pointer shapes WITHOUT the hosted surface.
 *
 * Compiled with no `-DNROS_CPP_STD`, i.e. the freestanding API: there is no
 * allocator on the delivery path, so `NROS_CPP_HAS_SHARED_PTR` is undefined,
 * the generated messages carry no `SharedPtr` aliases, and a callback taking a
 * `std::shared_ptr<const M>` (spelled by hand, since this TU brings its own
 * `<memory>`) must be REFUSED at `create_subscription` with the text naming
 * `const M&` — not accepted, and not a conversion error inside `InplaceFn`.
 *
 * `just check cpp` requires this TU to FAIL with "REFUSED by nano-ros" and
 * "const M&".
 */

#include <memory>

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

int subscription_pointer_callback_freestanding_probe(rclcpp::Node& node);
int subscription_pointer_callback_freestanding_probe(rclcpp::Node& node) {
    auto sub = node.create_subscription<Msg>(
        "chatter", 10, [](std::shared_ptr<const Msg> msg) { (void)msg->data; });
    (void)sub;
    return 0;
}
