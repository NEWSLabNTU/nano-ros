// Stand-in for the codegen'd `example_interfaces/action/fibonacci.hpp`
// (phase-488). Shape: `packages/cli/rosidl-codegen/packs/cpp/action.hpp.jinja`
// with the default fixed-capacity sequence field; FFI bodies are inline stubs
// (see `std_msgs/msg/string.hpp`).

#ifndef NROS_RCLCPP_EXAMPLES_EXAMPLE_INTERFACES_ACTION_FIBONACCI_HPP
#define NROS_RCLCPP_EXAMPLES_EXAMPLE_INTERFACES_ACTION_FIBONACCI_HPP

#include <cstddef>
#include <cstdint>

#include "rclcpp/fixed_sequence.hpp"

#define NROS_RCLCPP_EXAMPLES_STUB_FFI                                                              \
    static int ffi_publish(void*, const void*) {                                                   \
        return 0;                                                                                  \
    }                                                                                              \
    static int ffi_serialize(const void*, uint8_t*, size_t, size_t* out_len) {                     \
        if (out_len) *out_len = 0;                                                                 \
        return 0;                                                                                  \
    }                                                                                              \
    static int ffi_deserialize(const uint8_t*, size_t, void*) {                                    \
        return 0;                                                                                  \
    }

namespace example_interfaces {
namespace action {

struct Fibonacci {
    struct Goal {
        int32_t order = {};
        static constexpr const char* TYPE_NAME =
            "example_interfaces::action::dds_::Fibonacci_Goal_";
        static constexpr const char* TYPE_HASH = "RIHS01_stub_fibonacci_goal";
        static constexpr size_t SERIALIZED_SIZE_MAX = 8;
        NROS_RCLCPP_EXAMPLES_STUB_FFI
    };

    struct Result {
        ::rclcpp::FixedSequence<int32_t, 64> sequence = {};
        static constexpr const char* TYPE_NAME =
            "example_interfaces::action::dds_::Fibonacci_Result_";
        static constexpr const char* TYPE_HASH = "RIHS01_stub_fibonacci_result";
        static constexpr size_t SERIALIZED_SIZE_MAX = 712;
        NROS_RCLCPP_EXAMPLES_STUB_FFI
    };

    struct Feedback {
        ::rclcpp::FixedSequence<int32_t, 64> sequence = {};
        static constexpr const char* TYPE_NAME =
            "example_interfaces::action::dds_::Fibonacci_Feedback_";
        static constexpr const char* TYPE_HASH = "RIHS01_stub_fibonacci_feedback";
        static constexpr size_t SERIALIZED_SIZE_MAX = 712;
        NROS_RCLCPP_EXAMPLES_STUB_FFI
    };

    static constexpr const char* TYPE_NAME = "example_interfaces::action::dds_::Fibonacci_";
    static constexpr const char* TYPE_HASH = "RIHS01_stub_fibonacci";
    static constexpr const char* SEND_GOAL_SERVICE_HASH = "RIHS01_stub_fibonacci_send_goal";
    static constexpr const char* CANCEL_GOAL_SERVICE_HASH = "RIHS01_stub_cancel_goal";
    static constexpr const char* GET_RESULT_SERVICE_HASH = "RIHS01_stub_fibonacci_get_result";
    static constexpr const char* FEEDBACK_MESSAGE_HASH = "RIHS01_stub_fibonacci_feedback_msg";
    static constexpr const char* STATUS_MESSAGE_HASH = "RIHS01_stub_goal_status_array";
};

} // namespace action
} // namespace example_interfaces

#undef NROS_RCLCPP_EXAMPLES_STUB_FFI

#endif // NROS_RCLCPP_EXAMPLES_EXAMPLE_INTERFACES_ACTION_FIBONACCI_HPP
