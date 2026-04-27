// nros-cpp: Lightweight non-owning view types (freestanding C++14)
//
// Provides nros::Span<T> and nros::StringView as zero-overhead alternatives
// to std::span (C++20) and std::string_view (C++17). Compatible with GCC 5+,
// Clang 3.5+, and all embedded toolchains.
//
// These types are used by generated borrowed message structs to reference
// variable-length data in the CDR receive buffer without copying.

/**
 * @file span.hpp
 * @ingroup grp_support
 * @brief `nros::Span<T>` and `nros::StringView` — non-owning views.
 */

#ifndef NROS_CPP_SPAN_HPP
#define NROS_CPP_SPAN_HPP

#include <cstddef>
#include <string.h>

namespace nros {

/// Non-owning view over a contiguous sequence of `T` values.
///
/// Same semantics as `std::span<const T>` but requires only C++14.
/// The data pointer is valid only for the lifetime of the source buffer
/// (typically the subscription callback scope).
template <typename T> struct Span {
    /// Pointer to the first element. Borrowed — caller-owned storage.
    const T* ptr;
    /// Number of elements in the view.
    size_t len;

    /// Pointer to the underlying storage.
    constexpr const T* data() const { return ptr; }
    /// Number of elements (alias for `len`).
    constexpr size_t size() const { return len; }
    /// True if the view contains zero elements.
    constexpr bool empty() const { return len == 0; }
    /// Element access; no bounds check.
    constexpr const T& operator[](size_t i) const { return ptr[i]; }
    /// Iterator to the first element.
    constexpr const T* begin() const { return ptr; }
    /// Iterator past the last element.
    constexpr const T* end() const { return ptr + len; }
};

/// Non-owning view over a UTF-8 string (not null-terminated).
///
/// Same semantics as `std::string_view` but requires only C++14.
/// The data pointer is valid only for the lifetime of the source buffer.
struct StringView {
    /// Pointer to the first byte. Not null-terminated.
    const char* ptr;
    /// Number of bytes in the view.
    size_t len;

    /// Pointer to the underlying bytes.
    constexpr const char* data() const { return ptr; }
    /// Number of bytes.
    constexpr size_t size() const { return len; }
    /// True if the view contains zero bytes.
    constexpr bool empty() const { return len == 0; }
    /// Byte access; no bounds check.
    constexpr char operator[](size_t i) const { return ptr[i]; }
    /// Iterator to the first byte.
    constexpr const char* begin() const { return ptr; }
    /// Iterator past the last byte.
    constexpr const char* end() const { return ptr + len; }

    /// True if `cstr` (null-terminated) has the same length and bytes.
    bool equals(const char* cstr) const {
        size_t clen = strlen(cstr);
        return clen == len && memcmp(ptr, cstr, len) == 0;
    }
};

} // namespace nros

#endif // NROS_CPP_SPAN_HPP
