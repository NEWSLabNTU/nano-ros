/*
 * Phase 11W.3 — implement the nothrow placement-new operators
 * declared by `zephyr/cxx-compat/new`. Cyclone DDS sources use
 * `new (std::nothrow) T{...}` heavily for ddsi/ddsc state
 * objects; Zephyr's `lib/cpp/minimal/include/new` declares the
 * tag type but not the matching operators.
 *
 * Implementation forwards to `k_malloc` / `k_free` to match the
 * project's standard CONFIG_HEAP_MEM_POOL_SIZE-backed allocation
 * path (same allocator nros's existing `operator new` overrides
 * use; see `nros-c/src/lib.rs :: zephyr_alloc`).
 */
#include <stdlib.h>
#include <new>

const std::nothrow_t std::nothrow{};

void* operator new(size_t size, const std::nothrow_t&) noexcept {
    return malloc(size);
}

void* operator new[](size_t size, const std::nothrow_t&) noexcept {
    return malloc(size);
}

void operator delete(void* p, const std::nothrow_t&) noexcept {
    free(p);
}

void operator delete[](void* p, const std::nothrow_t&) noexcept {
    free(p);
}
