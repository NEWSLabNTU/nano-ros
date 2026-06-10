// nros-cpp: HeapSequence — heap-backed (alloc) growable sequence container
// Freestanding C++14 — no exceptions, no STL required
//
// Memory layout is repr(C) compatible: identical to
// `struct { T* data; size_t size; size_t capacity; }` (rclc's
// `rosidl_runtime_c__<T>__Sequence` shape). Used by codegen-generated message
// structs for `mode = "heap"` sequence fields (RFC-0033).
//
// Allocation goes through the C-ABI `nros_platform_malloc` / `nros_platform_free`
// so the SAME allocator is used on both sides of the Rust↔C++ FFI: the Rust FFI
// deserializer allocates `data`, and this type's destructor frees it. Mixing a
// Rust-side allocator with a C++ `delete` would be undefined behaviour — using
// the shared platform allocator avoids that.

/**
 * @file heap_sequence.hpp
 * @ingroup grp_support
 * @brief `nros::HeapSequence<T>` — heap-backed growable sequence container.
 */

#ifndef NROS_CPP_HEAP_SEQUENCE_HPP
#define NROS_CPP_HEAP_SEQUENCE_HPP

#include <cstddef>
#include <cstdint>

#include <nros/platform.h>

namespace nros {

/// Heap-backed growable sequence container (`mode = "heap"`, RFC-0033).
///
/// Layout is `{ T* data; size_t size; size_t capacity; }` — C-ABI compatible
/// with the runtime and the Rust FFI mirror. Owns its buffer (freed in the
/// destructor via `nros_platform_free`); non-copyable, movable.
///
/// Usage:
/// ```cpp
/// nros::HeapSequence<uint8_t> pixels;
/// pixels.push_back(0xAB);
/// for (size_t i = 0; i < pixels.length(); ++i) { use(pixels[i]); }
/// ```
template <typename T> struct HeapSequence {
    T* data;
    size_t size;
    size_t capacity;

    HeapSequence() : data(nullptr), size(0), capacity(0) {}
    ~HeapSequence() { nros_platform_free(data); }

    // Owns memory → non-copyable, movable.
    HeapSequence(const HeapSequence&) = delete;
    HeapSequence& operator=(const HeapSequence&) = delete;
    HeapSequence(HeapSequence&& o) noexcept : data(o.data), size(o.size), capacity(o.capacity) {
        o.data = nullptr;
        o.size = 0;
        o.capacity = 0;
    }
    HeapSequence& operator=(HeapSequence&& o) noexcept {
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

    /// Ensure capacity for at least `n` elements. Returns false on alloc failure.
    bool reserve(size_t n) {
        if (n <= capacity) return true;
        T* fresh = static_cast<T*>(nros_platform_malloc(n * sizeof(T)));
        if (fresh == nullptr) return false;
        for (size_t i = 0; i < size; ++i)
            fresh[i] = data[i];
        nros_platform_free(data);
        data = fresh;
        capacity = n;
        return true;
    }

    /// Append an element. Returns false on alloc failure.
    bool push_back(const T& val) {
        if (size >= capacity) {
            size_t next = capacity == 0 ? 4 : capacity * 2;
            if (!reserve(next)) return false;
        }
        data[size++] = val;
        return true;
    }

    /// Drop all elements and release the buffer.
    void clear() {
        nros_platform_free(data);
        data = nullptr;
        size = 0;
        capacity = 0;
    }

    T& operator[](size_t i) { return data[i]; }
    const T& operator[](size_t i) const { return data[i]; }

    /// Current number of elements.
    size_t length() const { return size; }

    /// Iterator support.
    T* begin() { return data; }
    T* end() { return data + size; }
    const T* begin() const { return data; }
    const T* end() const { return data + size; }
};

} // namespace nros

#endif // NROS_CPP_HEAP_SEQUENCE_HPP
