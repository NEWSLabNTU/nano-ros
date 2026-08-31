// nros-cpp: ask a message type how big it can get (freestanding C++14)
//
// phase-408 W1/W4, issue 0896 — one spelling, `nros::rx_size_bound<M>::value`,
// for "how many bytes must a receive buffer for M hold". Every C++ subscribe
// path fills `rx_buffer_hint` from it, so the number reaching the executor
// arena and the zenoh payload class is the type's OWN derived bound and not a
// figure the caller typed.

/**
 * @file size_bound.hpp
 * @ingroup grp_support
 * @brief `nros::rx_size_bound<M>` / `nros::tx_size_bound<M>` — a message
 *        type's derived serialized-size bound, or a compile error saying why
 *        it has none.
 */

#ifndef NROS_CPP_SIZE_BOUND_HPP
#define NROS_CPP_SIZE_BOUND_HPP

#include <cstddef>

namespace nros {
namespace detail {

/// A `false` that DEPENDS on a template parameter.
///
/// `static_assert(false, ...)` inside a template fires when the template is
/// PARSED; `static_assert(size_bound_dependent_false<T>::value, ...)` fires
/// when it is INSTANTIATED. Generated message headers use the second form for
/// the poisoned `tx_size_bound`/`rx_size_bound` of an unbounded type, so
/// including such a header stays fine and only ASKING it for a number is the
/// error.
template <class...> struct size_bound_dependent_false {
    static constexpr bool value = false;
};

template <class...> struct make_void {
    using type = void;
};
/// `std::void_t` is C++17; nros-cpp is C++14.
template <class... Ts> using void_t = typename make_void<Ts...>::type;

/// Fallback: a type from a codegen with no derivation, or a hand-written
/// message-shaped struct (the `tests/compile/*.cpp` stubs are these). All it
/// has is the older `SERIALIZED_SIZE_MAX` ESTIMATE, so that is what it gets —
/// unchanged behaviour, and the only place the estimate still reaches a
/// receive-buffer hint.
template <class M, class = void> struct size_bounds_of {
    static constexpr size_t tx = M::SERIALIZED_SIZE_MAX;
    static constexpr size_t rx = M::SERIALIZED_SIZE_MAX;
};

/// A generated type that states DERIVED bounds, marked by the
/// `nros_derived_size_bounds` member the C++ pack emits for every message.
///
/// `tx_size_bound`/`rx_size_bound` are class TEMPLATES on that type, not plain
/// constants, precisely so an unbounded type can carry a poison here: naming
/// the number is then a `static_assert` that names the type and the member
/// costing it the bound, rather than a silently substituted estimate.
template <class M> struct size_bounds_of<M, void_t<typename M::nros_derived_size_bounds>> {
    static constexpr size_t tx = M::template tx_size_bound<>::value;
    static constexpr size_t rx = M::template rx_size_bound<>::value;
};

} // namespace detail

/// Bytes a TRANSMIT buffer for `M` must hold — XCDR1, the only encoding this
/// stack writes.
template <class M> struct tx_size_bound {
    static constexpr size_t value = detail::size_bounds_of<M>::tx;
};

/// Bytes a RECEIVE buffer for `M` must hold — `max(XCDR1, XCDR2)`, because
/// `data_representation` is negotiable and a non-default peer can send either
/// (RFC-0055). Under-sizing this drops samples at the transport with no
/// diagnostic, which is the failure issue 0896 exists to remove; over-sizing
/// only wastes bytes.
template <class M> struct rx_size_bound {
    static constexpr size_t value = detail::size_bounds_of<M>::rx;
};

} // namespace nros

#endif // NROS_CPP_SIZE_BOUND_HPP
