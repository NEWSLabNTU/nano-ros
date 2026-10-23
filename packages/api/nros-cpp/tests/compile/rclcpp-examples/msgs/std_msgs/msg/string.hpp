// Stand-in for the codegen'd `std_msgs/msg/string.hpp` (phase-488).
//
// The SHAPE is `packages/cli/rosidl-codegen/packs/cpp/message.hpp.jinja`'s for
// `std_msgs/String` with the default fixed-capacity string field: same
// namespace, same member type, same static metadata, same `ffi_*` hooks. Only
// the FFI bodies differ — they are inline stubs here, so the ported upstream
// examples link without the codegen'd Rust FFI glue. Nothing in the fixture
// runs the wire, so no byte these produce is ever read.
//
// If the template's public shape changes (a field type, a nested alias), this
// file must follow, or the fixture measures a message type no user has.

#ifndef NROS_RCLCPP_EXAMPLES_STD_MSGS_MSG_STRING_HPP
#define NROS_RCLCPP_EXAMPLES_STD_MSGS_MSG_STRING_HPP

#include <cstddef>
#include <cstdint>

#include "rclcpp/detail/std_detect.hpp" // issue 1995 — the pointer aliases

#include "rclcpp/detail/serialization_format.hpp"
#include "rclcpp/fixed_string.hpp"

namespace std_msgs {
namespace msg {

struct String {
#if defined(NROS_CPP_HAS_SHARED_PTR) // hosted-family: shared-ptr-interop
    // rosidl's smart-pointer aliases (issue 1995) — hosted builds only.
    using SharedPtr = ::std::shared_ptr<String>;
    using ConstSharedPtr = ::std::shared_ptr<const String>;
    using UniquePtr = ::std::unique_ptr<String>;
    using ConstUniquePtr = ::std::unique_ptr<const String>;
    using WeakPtr = ::std::weak_ptr<String>;
    using ConstWeakPtr = ::std::weak_ptr<const String>;
#endif

    ::rclcpp::FixedString<256> data = {};

    static constexpr const char* TYPE_NAME = "std_msgs::msg::dds_::String_";
    static constexpr const char* TYPE_HASH = "RIHS01_stub_std_msgs_string";
    static constexpr size_t SERIALIZED_SIZE_MAX = 265;
    using nros_derived_size_bounds = void;
    static constexpr size_t TX_MAX_SERIALIZED_SIZE = 265;
    static constexpr size_t RX_MAX_SERIALIZED_SIZE = 272;
    template <class = void> struct tx_size_bound {
        static constexpr size_t value = 265;
    };
    template <class = void> struct rx_size_bound {
        static constexpr size_t value = 272;
    };

    static int ffi_publish(void*, const void*) { return 0; }
    static int ffi_serialize(const void*, uint8_t*, size_t, size_t* out_len) {
        if (out_len) *out_len = 0;
        return 0;
    }
    static int ffi_deserialize(const uint8_t*, size_t, void*) { return 0; }
};

} // namespace msg
} // namespace std_msgs

namespace rclcpp {
template <> struct format_of<::std_msgs::msg::String> {
    static constexpr SerializationFormat value = SerializationFormat::Cdr;
};
} // namespace rclcpp

#endif // NROS_RCLCPP_EXAMPLES_STD_MSGS_MSG_STRING_HPP
