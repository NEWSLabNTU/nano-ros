// nros-cpp: the API's own minimal type traits
// Freestanding C++ — no exceptions, no STL required

/**
 * @file traits.hpp
 * @ingroup grp_support
 * @brief `nros::tr` — the handful of traits this API needs, carried rather
 *        than borrowed.
 */

#ifndef NROS_CPP_TRAITS_HPP
#define NROS_CPP_TRAITS_HPP

namespace nros {

/// The API's own traits — RFC-0096 D3.
///
/// WHY THIS EXISTS RATHER THAN `#include <type_traits>`
///
/// A freestanding API cannot assume a standard-library shim's SHAPE, and this
/// is not a hypothetical: the threadx-qemu-riscv64 board's `<type_traits>` was
/// a 58-line stub with `enable_if`, `integral_constant` and `is_convertible`
/// and nothing else, and its `remove_reference` lived in `<utility>`. A header
/// that leans on `std::decay` there compiles on every host and fails on one
/// cross build, which is the reach-narrower-than-the-rule shape this repo keeps
/// paying for.
///
/// phase-442 W4 filled both shims in, and that fix is worth having on its own —
/// but it is a fix to OUR shims, and the rule is about targets generally. An
/// API whose metaprogramming depends on a shim is one board away from the same
/// failure. So the traits live here, in our namespace, where their shape is a
/// property of this repository.
///
/// The set is deliberately the minimum the two W3 mechanisms need, not a
/// re-implementation of `<type_traits>`. A trait nothing here uses is not
/// carried; add one when a mechanism needs it, with the mechanism.
namespace tr {

/// `sizeof`'s type, without `<cstddef>`. `__SIZE_TYPE__` is the compiler's own
/// answer, which is what makes it right on a 32-bit target where a hand-written
/// `unsigned long` is not (measured, phase-442 W0).
using size_type = __SIZE_TYPE__;

template <typename T, T V> struct integral_constant {
    static constexpr T value = V;
    using value_type = T;
    using type = integral_constant;
    constexpr operator value_type() const { return value; }
};

using true_type = integral_constant<bool, true>;
using false_type = integral_constant<bool, false>;

template <typename T, typename U> struct is_same : false_type {};
template <typename T> struct is_same<T, T> : true_type {};

template <bool B, typename T, typename F> struct conditional {
    using type = T;
};
template <typename T, typename F> struct conditional<false, T, F> {
    using type = F;
};

template <bool B, typename T = void> struct enable_if {};
template <typename T> struct enable_if<true, T> {
    using type = T;
};

template <typename T> struct remove_reference {
    using type = T;
};
template <typename T> struct remove_reference<T&> {
    using type = T;
};
template <typename T> struct remove_reference<T&&> {
    using type = T;
};

template <typename T> struct remove_const {
    using type = T;
};
template <typename T> struct remove_const<const T> {
    using type = T;
};

template <typename T> struct remove_volatile {
    using type = T;
};
template <typename T> struct remove_volatile<volatile T> {
    using type = T;
};

template <typename T> struct remove_cv {
    using type = typename remove_const<typename remove_volatile<T>::type>::type;
};

template <typename T> struct remove_extent {
    using type = T;
};
template <typename T> struct remove_extent<T[]> {
    using type = T;
};
template <typename T, size_type N> struct remove_extent<T[N]> {
    using type = T;
};

template <typename T> struct is_array : false_type {};
template <typename T> struct is_array<T[]> : true_type {};
template <typename T, size_type N> struct is_array<T[N]> : true_type {};

template <typename T> struct is_function : false_type {};
template <typename R, typename... A> struct is_function<R(A...)> : true_type {};
template <typename R, typename... A> struct is_function<R(A..., ...)> : true_type {};

/// What a by-value parameter does to a type.
///
/// Written out rather than approximated as `remove_cv<remove_reference<T>>`,
/// because the two differ exactly where a callback signature is most likely to
/// be written — a function type, or an array — and an approximation that is
/// right for scalars is the gap the ThreadX shim already had.
template <typename T> struct decay {
  private:
    using U = typename remove_reference<T>::type;

  public:
    using type = typename conditional<
        is_array<U>::value, typename remove_extent<U>::type*,
        typename conditional<is_function<U>::value, typename remove_reference<U>::type*,
                             typename remove_cv<U>::type>::type>::type;
};

/// `static_cast<T&&>`, without `<utility>`.
///
/// Named `forward_rvalue` rather than `move` on purpose: `nros::tr::move` beside
/// a `using namespace std` would be an overload-resolution coin flip in a ported
/// file, and this namespace is not a `std` replacement.
template <typename T> constexpr typename remove_reference<T>::type&& forward_rvalue(T&& v) {
    return static_cast<typename remove_reference<T>::type&&>(v);
}

} // namespace tr
} // namespace nros

#endif // NROS_CPP_TRAITS_HPP
