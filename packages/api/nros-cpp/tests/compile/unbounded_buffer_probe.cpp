// Issue 0964 — EXPECTED-FAILURE probe. This TU must NOT compile.
//
// The `unbounded` arm of `detail::buffer_bounds` poisons: a type whose
// derivation ran and found no bound cannot size a buffer, and the estimate it
// used to fall back on is invented — `compute_serialized_size_max` is a flat
// 512 per nested message and a default capacity per string, which over-stated
// 38 of 39 bounded types and UNDER-stated one. A buffer smaller than the
// message a user legitimately sends cannot accept it, and no run-time check can
// repair a size already baked into a stack array. So it is a build error.
//
// Written as an expected failure because a static_assert cannot say "this
// expression must not compile" — the sibling `rx_size_bound.cpp` asserts
// everything that must still SUCCEED, and this asserts the one thing that must
// not. If this TU ever compiles clean, the poison has been disarmed and the
// estimate is back in a buffer, silently.

#include <cstddef>
#include <cstdint>

#include "nros/size_bound.hpp"

namespace {

/// The shape codegen emits for a type whose derivation found no bound: the
/// marker is present, the bound CONSTANTS are absent, and the nested templates
/// carry the diagnostic. Mirrors the `Unbounded` stub in `rx_size_bound.cpp`.
struct Unbounded {
    int32_t data{0};
    static constexpr const char* TYPE_NAME = "p::msg::dds_::Unbounded_";
    static constexpr const char* TYPE_HASH = "RIHS01_unbounded_stub";
    static const size_t SERIALIZED_SIZE_MAX = 264;
    using nros_derived_size_bounds = void;
    template <class NROS_size_bound_required = void> struct tx_size_bound {
        static_assert(::nros::detail::size_bound_dependent_false<NROS_size_bound_required>::value,
                      "NROS_UNBOUNDED__p_msg_unbounded__field_data: p/Unbounded states no "
                      "serialized-size bound -- unbounded member: data (string).");
        static constexpr size_t value = 0;
    };
    template <class NROS_size_bound_required = void> struct rx_size_bound {
        static_assert(::nros::detail::size_bound_dependent_false<NROS_size_bound_required>::value,
                      "NROS_UNBOUNDED__p_msg_unbounded__field_data: p/Unbounded states no "
                      "serialized-size bound -- unbounded member: data (string).");
        static constexpr size_t value = 0;
    };
};

// THE ASSERTION OF THIS FILE: asking an unbounded type to size a buffer is a
// build error. Both directions, because both stack an array.
constexpr size_t rx = ::nros::detail::buffer_bounds<Unbounded>::rx;
constexpr size_t tx = ::nros::detail::buffer_bounds<Unbounded>::tx;

} // namespace

int main() { return static_cast<int>(rx + tx); }
