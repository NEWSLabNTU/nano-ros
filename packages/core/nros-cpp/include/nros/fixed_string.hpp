// nros-cpp: FixedString — fixed-capacity null-terminated string
// Freestanding C++14 — no exceptions, no STL required
//
// Memory layout is repr(C) compatible: identical to char[N].
// Used by codegen-generated message structs for string fields.

#ifndef NROS_CPP_FIXED_STRING_HPP
#define NROS_CPP_FIXED_STRING_HPP

#include <cstddef>
#include <string.h>

namespace nros {

/// Fixed-capacity null-terminated string.
///
/// Wraps a `char[N]` buffer with safe assignment and query methods.
/// Memory layout is identical to `char[N]`, making it repr(C) compatible
/// with the Rust side (`[u8; N]`).
///
/// Usage:
/// ```cpp
/// nros::FixedString<256> name;
/// name = "hello world";
/// printf("%s (len=%zu)\n", name.c_str(), name.length());
/// ```
template <size_t N> struct FixedString {
    char data[N];

    /// Default constructor — empty string.
    FixedString() { data[0] = '\0'; }

    /// Assign from a C string. Truncates if longer than capacity.
    FixedString& operator=(const char* s) {
        if (s == nullptr) {
            data[0] = '\0';
            return *this;
        }
        size_t i = 0;
        for (; i < N - 1 && s[i] != '\0'; ++i) {
            data[i] = s[i];
        }
        data[i] = '\0';
        return *this;
    }

    /// Get a pointer to the null-terminated string.
    const char* c_str() const { return data; }

    /// Get the length of the string (up to N-1).
    size_t length() const {
        size_t len = 0;
        while (len < N && data[len] != '\0')
            ++len;
        return len;
    }

    /// Maximum number of characters (excluding null terminator).
    static constexpr size_t capacity() { return N - 1; }

    /// Compare with a C string.
    bool operator==(const char* s) const {
        if (s == nullptr) return data[0] == '\0';
        return strncmp(data, s, N) == 0;
    }

    bool operator!=(const char* s) const { return !(*this == s); }
};

} // namespace nros

#endif // NROS_CPP_FIXED_STRING_HPP
