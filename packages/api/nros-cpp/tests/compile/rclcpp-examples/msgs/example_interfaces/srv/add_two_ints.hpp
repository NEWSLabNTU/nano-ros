// Stand-in for the codegen'd `example_interfaces/srv/add_two_ints.hpp`
// (phase-488). Shape: `packages/cli/rosidl-codegen/packs/cpp/service.hpp.jinja`;
// FFI bodies are inline stubs (see `std_msgs/msg/string.hpp` beside this).

#ifndef NROS_RCLCPP_EXAMPLES_EXAMPLE_INTERFACES_SRV_ADD_TWO_INTS_HPP
#define NROS_RCLCPP_EXAMPLES_EXAMPLE_INTERFACES_SRV_ADD_TWO_INTS_HPP

#include <cstddef>
#include <cstdint>

namespace example_interfaces {
namespace srv {

struct AddTwoInts {
    struct Request {
        int64_t a = {};
        int64_t b = {};

        static constexpr const char* TYPE_NAME =
            "example_interfaces::srv::dds_::AddTwoInts_Request_";
        static constexpr const char* TYPE_HASH = "RIHS01_stub_add_two_ints";
        static constexpr size_t SERIALIZED_SIZE_MAX = 20;

        static int ffi_publish(void*, const void*) { return 0; }
        static int ffi_serialize(const void*, uint8_t*, size_t, size_t* out_len) {
            if (out_len) *out_len = 0;
            return 0;
        }
        static int ffi_deserialize(const uint8_t*, size_t, void*) { return 0; }
    };

    struct Response {
        int64_t sum = {};

        static constexpr const char* TYPE_NAME =
            "example_interfaces::srv::dds_::AddTwoInts_Response_";
        static constexpr const char* TYPE_HASH = "RIHS01_stub_add_two_ints";
        static constexpr size_t SERIALIZED_SIZE_MAX = 12;

        static int ffi_publish(void*, const void*) { return 0; }
        static int ffi_serialize(const void*, uint8_t*, size_t, size_t* out_len) {
            if (out_len) *out_len = 0;
            return 0;
        }
        static int ffi_deserialize(const uint8_t*, size_t, void*) { return 0; }
    };

    static constexpr const char* TYPE_NAME = "example_interfaces::srv::dds_::AddTwoInts_";
};

} // namespace srv
} // namespace example_interfaces

#endif // NROS_RCLCPP_EXAMPLES_EXAMPLE_INTERFACES_SRV_ADD_TWO_INTS_HPP
