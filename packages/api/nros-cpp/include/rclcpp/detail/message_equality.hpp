// nros-cpp: field-wise equality for generated messages
// Freestanding C++ — no STL

/**
 * @file message_equality.hpp
 * @brief `rclcpp::detail::msg_field_eq` — what a generated message's
 *        `operator==` compares each field with (phase-488).
 *
 * rosidl's generated C++ messages have `operator==` / `operator!=`, and ported
 * code compares them (Nav2's velocity smoother:
 * `last_cmd_.twist == geometry_msgs::msg::Twist()`). A nano-ros message's
 * fields are primitives, nested messages, C arrays, and the four containers
 * below. Each overload compares by VALUE (length, then elements), so two
 * containers holding the same text or elements are equal however much
 * capacity each has. Floating-point fields compare with `==`, as rosidl's do.
 */

#ifndef NROS_RCLCPP_DETAIL_MESSAGE_EQUALITY_HPP
#define NROS_RCLCPP_DETAIL_MESSAGE_EQUALITY_HPP

#include <stddef.h>

#include "rclcpp/fixed_sequence.hpp"
#include "rclcpp/fixed_string.hpp"
#include "rclcpp/heap_sequence.hpp"
#include "rclcpp/heap_string.hpp"

namespace rclcpp {
namespace detail {

template <typename T> inline bool msg_field_eq(const T& a, const T& b);
template <typename T, size_t N> inline bool msg_field_eq(const T (&a)[N], const T (&b)[N]);
template <size_t N> inline bool msg_field_eq(const FixedString<N>& a, const FixedString<N>& b);
inline bool msg_field_eq(const HeapString& a, const HeapString& b);
template <typename T, size_t N>
inline bool msg_field_eq(const FixedSequence<T, N>& a, const FixedSequence<T, N>& b);
template <typename T> inline bool msg_field_eq(const HeapSequence<T>& a, const HeapSequence<T>& b);

inline bool msg_text_eq(const char* a, size_t na, const char* b, size_t nb) {
    if (na != nb) return false;
    for (size_t i = 0; i < na; ++i)
        if (a[i] != b[i]) return false;
    return true;
}

/// A primitive or a nested message (whose own generated `operator==` this
/// reaches).
template <typename T> inline bool msg_field_eq(const T& a, const T& b) {
    return a == b;
}
template <typename T, size_t N> inline bool msg_field_eq(const T (&a)[N], const T (&b)[N]) {
    for (size_t i = 0; i < N; ++i)
        if (!msg_field_eq(a[i], b[i])) return false;
    return true;
}
template <size_t N> inline bool msg_field_eq(const FixedString<N>& a, const FixedString<N>& b) {
    return msg_text_eq(a.c_str(), a.length(), b.c_str(), b.length());
}
inline bool msg_field_eq(const HeapString& a, const HeapString& b) {
    return msg_text_eq(a.c_str(), a.length(), b.c_str(), b.length());
}
template <typename T, size_t N>
inline bool msg_field_eq(const FixedSequence<T, N>& a, const FixedSequence<T, N>& b) {
    if (a.length() != b.length()) return false;
    for (size_t i = 0; i < a.length(); ++i)
        if (!msg_field_eq(a[i], b[i])) return false;
    return true;
}
template <typename T> inline bool msg_field_eq(const HeapSequence<T>& a, const HeapSequence<T>& b) {
    if (a.length() != b.length()) return false;
    for (size_t i = 0; i < a.length(); ++i)
        if (!msg_field_eq(a[i], b[i])) return false;
    return true;
}

} // namespace detail
} // namespace rclcpp

#endif // NROS_RCLCPP_DETAIL_MESSAGE_EQUALITY_HPP
