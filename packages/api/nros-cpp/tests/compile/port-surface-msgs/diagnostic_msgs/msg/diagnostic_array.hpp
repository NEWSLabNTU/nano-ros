// Stand-in for the codegen'd `diagnostic_msgs/msg/diagnostic_array.hpp` (phase-488).
//
// The SHAPE is `packages/cli/rosidl-codegen/packs/cpp/message.hpp.jinja`'s:
// same namespace, member types and static metadata. Two things differ, both
// on purpose. The capacities are smaller than the codegen defaults (64
// elements, 256-byte strings), which would make a DiagnosticArray about 2 MiB;
// and the FFI bodies are inline stubs that put an empty body on the wire,
// because the runtime checks only count publishes and never read the bytes. Fields `diagnostic_updater` does not
// touch (the array's `header`) are left out.

#ifndef NROS_PORT_SURFACE_DIAGNOSTIC_MSGS_MSG_DIAGNOSTIC_ARRAY_HPP
#define NROS_PORT_SURFACE_DIAGNOSTIC_MSGS_MSG_DIAGNOSTIC_ARRAY_HPP

#include <cstddef>
#include <cstdint>

#include "rclcpp/detail/nros_cpp_ffi.h"
#include "rclcpp/detail/serialization_format.hpp"
#include "rclcpp/detail/std_detect.hpp"
#include "rclcpp/fixed_sequence.hpp"
#include "rclcpp/fixed_string.hpp"
#include "diagnostic_msgs/msg/diagnostic_status.hpp"

namespace diagnostic_msgs {
namespace msg {

struct DiagnosticArray {
#if defined(NROS_CPP_HAS_SHARED_PTR)
    using SharedPtr = ::std::shared_ptr<DiagnosticArray>;
    using ConstSharedPtr = ::std::shared_ptr<const DiagnosticArray>;
#endif
    ::rclcpp::FixedSequence<::diagnostic_msgs::msg::DiagnosticStatus, 4> status = {};

    static constexpr const char* TYPE_NAME = "diagnostic_msgs::msg::dds_::DiagnosticArray_";
    static constexpr const char* TYPE_HASH = "RIHS01_stub_diagnostic_msgs_diagnostic_array";
    static constexpr size_t SERIALIZED_SIZE_MAX = 64;

    static int ffi_publish(void* handle, const void* m) {
        uint8_t buf[4];
        size_t len = 0;
        if (ffi_serialize(m, buf, sizeof(buf), &len) != 0) return -1;
        return nros_cpp_publish_raw(handle, buf, len);
    }
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
template <> struct format_of<::diagnostic_msgs::msg::DiagnosticArray> {
    static constexpr SerializationFormat value = SerializationFormat::Cdr;
};
} // namespace rclcpp

#endif // NROS_PORT_SURFACE_DIAGNOSTIC_MSGS_MSG_DIAGNOSTIC_ARRAY_HPP
