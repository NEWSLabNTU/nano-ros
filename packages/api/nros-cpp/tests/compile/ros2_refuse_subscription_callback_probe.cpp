/*
 * NEGATIVE probe — issue 1995, a subscription callback the executor cannot call.
 *
 * `create_subscription<M>(topic, qos, cb)` takes `void(const M&)` everywhere,
 * and on a hosted build (`-DNROS_CPP_STD`, which this lane compiles with) also
 * upstream's pointer shapes — `M::UniquePtr`, `M::SharedPtr`,
 * `M::ConstSharedPtr`, `std::shared_ptr<const M>` — by handing the callback a
 * heap copy per delivery (`subscription_pointer_callback_runtime.cpp`). A
 * callable taking NONE of those used to fail deep inside `InplaceFn` as a
 * conversion error; it is now refused at the call, naming the shapes. The
 * shape below is a two-argument callback, which is what upstream's
 * `MessageInfo` overload looks like to a port.
 *
 * `just check cpp` requires this TU to FAIL with "REFUSED by nano-ros".
 */

#include <rclcpp/rclcpp.hpp>

#include <cstddef>
#include <cstdint>

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

int ros2_refuse_subscription_callback_probe(rclcpp::Node& node);
int ros2_refuse_subscription_callback_probe(rclcpp::Node& node) {
    auto sub = node.create_subscription<Msg>(
        "chatter", 10, [](const Msg& msg, size_t info) { (void)msg, (void)info; });
    (void)sub;
    return 0;
}
