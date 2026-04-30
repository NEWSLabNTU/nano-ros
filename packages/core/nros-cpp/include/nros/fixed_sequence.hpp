// nros-cpp: FixedSequence — fixed-capacity sequence container
// Freestanding C++14 — no exceptions, no STL required
//
// Memory layout is repr(C) compatible: identical to { uint32_t size; T data[N]; }.
// Used by codegen-generated message structs for sequence fields.

/**
 * @file fixed_sequence.hpp
 * @ingroup grp_support
 * @brief `nros::FixedSequence<T,N>` — fixed-capacity sequence container.
 */

#ifndef NROS_CPP_FIXED_SEQUENCE_HPP
#define NROS_CPP_FIXED_SEQUENCE_HPP

#include <cstddef>
#include <cstdint>

namespace nros {

/// Fixed-capacity sequence container.
///
/// Wraps a `uint32_t size` + `T data[N]` pair with push/access methods.
/// Memory layout is identical to `struct { uint32_t size; T data[N]; }`,
/// so the type is C-ABI compatible with the runtime.
///
/// Usage:
/// ```cpp
/// nros::FixedSequence<int32_t, 64> values;
/// values.push_back(42);
/// values.push_back(7);
/// for (uint32_t i = 0; i < values.length(); ++i) {
///     printf("values[%u] = %d\n", i, values[i]);
/// }
/// ```
template <typename T, size_t N> struct FixedSequence {
    uint32_t size;
    T data[N];

    /// Default constructor — empty sequence.
    FixedSequence() : size(0), data{} {}

    /// Append an element. Returns false if the sequence is full.
    bool push_back(const T& val) {
        if (size >= N) return false;
        data[size++] = val;
        return true;
    }

    /// Access element by index (no bounds check).
    T& operator[](size_t i) { return data[i]; }
    const T& operator[](size_t i) const { return data[i]; }

    /// Current number of elements.
    uint32_t length() const { return size; }

    /// Maximum capacity.
    static constexpr size_t max_size() { return N; }

    /// Iterator support.
    T* begin() { return data; }
    T* end() { return data + size; }
    const T* begin() const { return data; }
    const T* end() const { return data + size; }
};

} // namespace nros

#endif // NROS_CPP_FIXED_SEQUENCE_HPP
