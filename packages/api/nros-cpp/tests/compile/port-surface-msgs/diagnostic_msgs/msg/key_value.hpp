// Stand-in for the codegen'd `diagnostic_msgs/msg/key_value.hpp` (phase-488).
//
// The SHAPE is `packages/cli/rosidl-codegen/packs/cpp/message.hpp.jinja`'s:
// same namespace, member types and static metadata. Two things differ, both
// on purpose. The capacities are smaller than the codegen defaults (64
// elements, 256-byte strings), which would make a DiagnosticArray about 2 MiB;
// and the FFI bodies are inline stubs, because the runtime check only counts
// publishes and never reads the bytes. Fields `diagnostic_updater` does not
// touch (the array's `header`) are left out.

#ifndef NROS_PORT_SURFACE_DIAGNOSTIC_MSGS_MSG_KEY_VALUE_HPP
#define NROS_PORT_SURFACE_DIAGNOSTIC_MSGS_MSG_KEY_VALUE_HPP

#include <cstddef>
#include <cstdint>

#include "rclcpp/detail/serialization_format.hpp"
#include "rclcpp/detail/std_detect.hpp"
#include "rclcpp/fixed_sequence.hpp"
#include "rclcpp/fixed_string.hpp"

namespace diagnostic_msgs {
namespace msg {

struct KeyValue {
#if defined(NROS_CPP_HAS_SHARED_PTR)
    using SharedPtr = ::std::shared_ptr<KeyValue>;
    using ConstSharedPtr = ::std::shared_ptr<const KeyValue>;
#endif
    ::rclcpp::FixedString<32> key = {};
    ::rclcpp::FixedString<32> value = {};

    static constexpr const char* TYPE_NAME = "diagnostic_msgs::msg::dds_::KeyValue_";
    static constexpr const char* TYPE_HASH = "RIHS01_stub_diagnostic_msgs_key_value";
    static constexpr size_t SERIALIZED_SIZE_MAX = 64;

    static int ffi_publish(void*, const void*) { return 0; }
    static int ffi_serialize(const void*, uint8_t* buf, size_t cap, size_t* out_len) {
        // An empty CDR body: the encapsulation header only.
        if (cap < 4) return -1;
        buf[0] = 0x00;
        buf[1] = 0x01;
        buf[2] = 0x00;
        buf[3] = 0x00;
        if (out_len) *out_len = 4;
        return 0;
    }
    static int ffi_deserialize(const uint8_t*, size_t, void*) { return 0; }
};

} // namespace msg
} // namespace diagnostic_msgs

namespace rclcpp {
template <> struct format_of<::diagnostic_msgs::msg::KeyValue> {
    static constexpr SerializationFormat value = SerializationFormat::Cdr;
};
} // namespace rclcpp

#endif // NROS_PORT_SURFACE_DIAGNOSTIC_MSGS_MSG_KEY_VALUE_HPP
