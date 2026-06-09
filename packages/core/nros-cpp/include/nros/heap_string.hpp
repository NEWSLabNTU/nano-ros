// nros-cpp: HeapString — heap-backed (alloc) growable string container
// Freestanding C++14 — no exceptions, no STL required
//
// Memory layout is repr(C) compatible: identical to
// `struct { char* data; size_t size; size_t capacity; }` (rclc's
// `rosidl_runtime_c__String` shape). Used by codegen-generated message structs
// for `mode = "heap"` string fields (RFC-0033) — the bridgeable analog of
// rclcpp's `std::string` (whose layout is not repr(C), so it cannot cross the
// Rust↔C++ FFI directly).
//
// `data` is kept NUL-terminated, `size` is the string length, and `capacity`
// includes the NUL (`size + 1`) — matching `rosidl_runtime_c__String`. Allocation
// goes through the C-ABI nros_platform_malloc/free so the SAME allocator spans
// both FFI sides (the Rust deserializer allocates; this destructor frees).

/**
 * @file heap_string.hpp
 * @ingroup grp_support
 * @brief `nros::HeapString` — heap-backed growable string container.
 */

#ifndef NROS_CPP_HEAP_STRING_HPP
#define NROS_CPP_HEAP_STRING_HPP

#include <cstddef>

#include <nros/platform.h>

namespace nros {

/// Heap-backed string container (`mode = "heap"`, RFC-0033).
///
/// Layout `{ char* data; size_t size; size_t capacity; }` — C-ABI compatible
/// with the runtime and the Rust FFI mirror. Owns its buffer (freed in the
/// destructor via `nros_platform_free`); non-copyable, movable. `data` is
/// NUL-terminated when non-null.
struct HeapString {
    char* data;
    size_t size;
    size_t capacity;

    HeapString() : data(nullptr), size(0), capacity(0) {}
    ~HeapString() { nros_platform_free(data); }

    HeapString(const HeapString&) = delete;
    HeapString& operator=(const HeapString&) = delete;
    HeapString(HeapString&& o) noexcept : data(o.data), size(o.size), capacity(o.capacity) {
        o.data = nullptr;
        o.size = 0;
        o.capacity = 0;
    }
    HeapString& operator=(HeapString&& o) noexcept {
        if (this != &o) {
            nros_platform_free(data);
            data = o.data;
            size = o.size;
            capacity = o.capacity;
            o.data = nullptr;
            o.size = 0;
            o.capacity = 0;
        }
        return *this;
    }

    /// NUL-terminated contents (`""` when empty).
    const char* c_str() const { return data ? data : ""; }
    size_t length() const { return size; }
    bool empty() const { return size == 0; }

    /// Copy `n` bytes from `src` (which need not be NUL-terminated), storing a
    /// fresh NUL-terminated buffer. Returns false on alloc failure.
    bool assign(const char* src, size_t n) {
        char* fresh = static_cast<char*>(nros_platform_malloc(n + 1));
        if (fresh == nullptr) return false;
        for (size_t i = 0; i < n; ++i) fresh[i] = src[i];
        fresh[n] = '\0';
        nros_platform_free(data);
        data = fresh;
        size = n;
        capacity = n + 1;
        return true;
    }

    void clear() {
        nros_platform_free(data);
        data = nullptr;
        size = 0;
        capacity = 0;
    }
};

} // namespace nros

#endif // NROS_CPP_HEAP_STRING_HPP
