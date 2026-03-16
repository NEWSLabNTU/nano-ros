// nros-cpp: Lightweight non-owning view types (freestanding C++14)
//
// Provides nros::Span<T> and nros::StringView as zero-overhead alternatives
// to std::span (C++20) and std::string_view (C++17). Compatible with GCC 5+,
// Clang 3.5+, and all embedded toolchains.
//
// These types are used by generated borrowed message structs to reference
// variable-length data in the CDR receive buffer without copying.

#ifndef NROS_CPP_SPAN_HPP
#define NROS_CPP_SPAN_HPP

#include <cstddef>
#include <cstring>

namespace nros {

/// Non-owning view over a contiguous sequence of `T` values.
///
/// Same semantics as `std::span<const T>` but requires only C++14.
/// The data pointer is valid only for the lifetime of the source buffer
/// (typically the subscription callback scope).
template <typename T> struct Span {
    const T* ptr;
    size_t len;

    constexpr const T* data() const { return ptr; }
    constexpr size_t size() const { return len; }
    constexpr bool empty() const { return len == 0; }
    constexpr const T& operator[](size_t i) const { return ptr[i]; }
    constexpr const T* begin() const { return ptr; }
    constexpr const T* end() const { return ptr + len; }
};

/// Non-owning view over a UTF-8 string (not null-terminated).
///
/// Same semantics as `std::string_view` but requires only C++14.
/// The data pointer is valid only for the lifetime of the source buffer.
struct StringView {
    const char* ptr;
    size_t len;

    constexpr const char* data() const { return ptr; }
    constexpr size_t size() const { return len; }
    constexpr bool empty() const { return len == 0; }
    constexpr char operator[](size_t i) const { return ptr[i]; }
    constexpr const char* begin() const { return ptr; }
    constexpr const char* end() const { return ptr + len; }

    /// Compare with a null-terminated C string.
    bool equals(const char* cstr) const {
        size_t clen = strlen(cstr);
        return clen == len && memcmp(ptr, cstr, len) == 0;
    }
};

} // namespace nros

#endif // NROS_CPP_SPAN_HPP
