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
#include <new> // placement new (precedent: component_node.hpp factories)

#include <nros/platform.h>

namespace nros {

/// Heap-backed growable sequence container (`mode = "heap"`, RFC-0033).
///
/// Layout is `{ T* data; size_t size; size_t capacity; }` — C-ABI compatible
/// with the runtime and the Rust FFI mirror. Owns its buffer (freed in the
/// destructor via `nros_platform_free`); non-copyable, movable.
///
/// **Element lifetime (issue #201).** Destruction paths (destructor,
/// move-assign, `clear()`) run each element's destructor before freeing the
/// buffer, so elements that OWN heap memory (`nros::HeapString`, a nested
/// `HeapSequence`, generated message structs containing either) are torn down
/// recursively — a two-level `mode = "heap"` config no longer leaks. The
/// pseudo-destructor call compiles to nothing for trivially destructible
/// element types.
///
/// **Element relocation contract.** `reserve()` moves elements to the grown
/// buffer by byte copy WITHOUT running destructors on the old slots
/// (ownership relocates). Every permitted element type is trivially
/// relocatable: standard-layout, self-contained owning pointers, no
/// self-references — the same contract the C runtime's
/// `rosidl_runtime_c__*__Sequence` realloc path assumes.
///
/// Usage:
/// ```cpp
/// nros::HeapSequence<uint8_t> pixels;
/// pixels.push_back(0xAB);
/// for (size_t i = 0; i < pixels.length(); ++i) { use(pixels[i]); }
///
/// // Owning (non-copyable) element types are built in place:
/// nros::HeapSequence<pkg::msg::DiagnosticStatus> statuses;
/// if (auto* s = statuses.emplace_back()) { s->name.assign("motor", 5); }
/// ```
template <typename T> struct HeapSequence {
    T* data;
    size_t size;
    size_t capacity;

    HeapSequence() : data(nullptr), size(0), capacity(0) {}
    ~HeapSequence() {
        destroy_elements();
        nros_platform_free(data);
    }

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
            destroy_elements();
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
    ///
    /// Grows by BYTE RELOCATION (see the element-relocation contract above):
    /// live elements move to the new buffer without destructor/constructor
    /// pairs, so owning element types stay valid across growth.
    bool reserve(size_t n) {
        if (n <= capacity) return true;
        T* fresh = static_cast<T*>(nros_platform_malloc(n * sizeof(T)));
        if (fresh == nullptr) return false;
        const unsigned char* src = reinterpret_cast<const unsigned char*>(data);
        unsigned char* dst = reinterpret_cast<unsigned char*>(fresh);
        for (size_t b = 0; b < size * sizeof(T); ++b)
            dst[b] = src[b];
        nros_platform_free(data);
        data = fresh;
        capacity = n;
        return true;
    }

    /// Append a copy of `val`. Returns false on alloc failure. Placement-new
    /// copy-constructs into the (uninitialized) slot — assignment would invoke
    /// `operator=` on garbage, which is UB for owning element types. Only
    /// available for copy-constructible `T`; owning (non-copyable) element
    /// types use [`emplace_back`].
    bool push_back(const T& val) {
        if (size >= capacity) {
            size_t next = capacity == 0 ? 4 : capacity * 2;
            if (!reserve(next)) return false;
        }
        new (data + size) T(val);
        ++size;
        return true;
    }

    /// Append a default-constructed element in place and return it, or
    /// `nullptr` on alloc failure. The way to BUILD sequences of owning
    /// (non-copyable) element types: fill the returned element through its
    /// own mutators (issue #201 — the two-level heap use case).
    T* emplace_back() {
        if (size >= capacity) {
            size_t next = capacity == 0 ? 4 : capacity * 2;
            if (!reserve(next)) return nullptr;
        }
        T* slot = new (data + size) T();
        ++size;
        return slot;
    }

    /// Drop all elements (running their destructors) and release the buffer.
    void clear() {
        destroy_elements();
        nros_platform_free(data);
        data = nullptr;
        size = 0;
        capacity = 0;
    }

    T& operator[](size_t i) { return data[i]; }
    const T& operator[](size_t i) const { return data[i]; }

    /// Run every live element's destructor (issue #201). A pseudo-destructor
    /// call is valid for scalar `T` too and compiles to nothing when `T` is
    /// trivially destructible — no `<type_traits>` needed.
    void destroy_elements() {
        for (size_t i = 0; i < size; ++i)
            data[i].~T();
    }

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
