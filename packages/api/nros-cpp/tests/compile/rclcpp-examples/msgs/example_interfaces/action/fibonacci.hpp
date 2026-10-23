// Stand-in for the codegen'd `example_interfaces/action/fibonacci.hpp`
// (phase-488). Shape: `packages/cli/rosidl-codegen/packs/cpp/action.hpp.jinja`
// with the default fixed-capacity sequence field; FFI bodies are inline stubs
// (see `std_msgs/msg/string.hpp`).

#ifndef NROS_RCLCPP_EXAMPLES_EXAMPLE_INTERFACES_ACTION_FIBONACCI_HPP
#define NROS_RCLCPP_EXAMPLES_EXAMPLE_INTERFACES_ACTION_FIBONACCI_HPP

#include <cstddef>
#include <cstdint>

#include "rclcpp/detail/std_detect.hpp" // issue 1995 — the pointer aliases

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
#if defined(NROS_CPP_HAS_SHARED_PTR) // hosted-family: shared-ptr-interop
        // rosidl's smart-pointer aliases (issue 1995) — hosted builds only.
        using SharedPtr = ::std::shared_ptr<Goal>;
        using ConstSharedPtr = ::std::shared_ptr<const Goal>;
        using UniquePtr = ::std::unique_ptr<Goal>;
        using ConstUniquePtr = ::std::unique_ptr<const Goal>;
        using WeakPtr = ::std::weak_ptr<Goal>;
        using ConstWeakPtr = ::std::weak_ptr<const Goal>;
#endif

        int32_t order = {};
        static constexpr const char* TYPE_NAME =
            "example_interfaces::action::dds_::Fibonacci_Goal_";
        static constexpr const char* TYPE_HASH = "RIHS01_stub_fibonacci_goal";
        static constexpr size_t SERIALIZED_SIZE_MAX = 8;
        NROS_RCLCPP_EXAMPLES_STUB_FFI
    };

    struct Result {
#if defined(NROS_CPP_HAS_SHARED_PTR) // hosted-family: shared-ptr-interop
        // rosidl's smart-pointer aliases (issue 1995) — hosted builds only.
        using SharedPtr = ::std::shared_ptr<Result>;
        using ConstSharedPtr = ::std::shared_ptr<const Result>;
        using UniquePtr = ::std::unique_ptr<Result>;
        using ConstUniquePtr = ::std::unique_ptr<const Result>;
        using WeakPtr = ::std::weak_ptr<Result>;
        using ConstWeakPtr = ::std::weak_ptr<const Result>;
#endif

        ::rclcpp::FixedSequence<int32_t, 64> sequence = {};
        static constexpr const char* TYPE_NAME =
            "example_interfaces::action::dds_::Fibonacci_Result_";
        static constexpr const char* TYPE_HASH = "RIHS01_stub_fibonacci_result";
        static constexpr size_t SERIALIZED_SIZE_MAX = 712;
        NROS_RCLCPP_EXAMPLES_STUB_FFI
    };

    struct Feedback {
#if defined(NROS_CPP_HAS_SHARED_PTR) // hosted-family: shared-ptr-interop
        // rosidl's smart-pointer aliases (issue 1995) — hosted builds only.
        using SharedPtr = ::std::shared_ptr<Feedback>;
        using ConstSharedPtr = ::std::shared_ptr<const Feedback>;
        using UniquePtr = ::std::unique_ptr<Feedback>;
        using ConstUniquePtr = ::std::unique_ptr<const Feedback>;
        using WeakPtr = ::std::weak_ptr<Feedback>;
        using ConstWeakPtr = ::std::weak_ptr<const Feedback>;
#endif

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
