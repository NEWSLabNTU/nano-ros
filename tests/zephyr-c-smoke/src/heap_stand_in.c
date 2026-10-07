/*
 * Issue 1720 -- the Rust heap half this C-only smoke does not link.
 *
 * phase-391 W3 moved `nros_platform_alloc`'s backing out of `k_malloc` and into
 * the rlsf arena in `nros-platform/src/zephyr_heap.rs`, which every real image
 * links through its Rust staticlib. This smoke builds `platform.c` with NO Rust
 * half, so from that wave on it stopped LINKING (`undefined reference to
 * nros_zephyr_heap_alloc`) -- and nothing noticed, because no lane ran it.
 *
 * These stand-ins supply the same `nros_zephyr_heap_*` symbols over a Zephyr
 * `k_heap`, so the smoke exercises what it exists for: the PORT's `platform.c`
 * funnel (size checks, exhaustion report, dealloc) over the real kernel. The
 * rlsf arena itself is tested on the Rust side (`zpico-alloc`), not here.
 */

#include <zephyr/kernel.h>

#include <stddef.h>

#define NROS_SMOKE_HEAP_BYTES 65536

K_HEAP_DEFINE(nros_smoke_heap, NROS_SMOKE_HEAP_BYTES);

void *nros_zephyr_heap_alloc(size_t size) {
    return k_heap_alloc(&nros_smoke_heap, size, K_NO_WAIT);
}

void *nros_zephyr_heap_realloc(void *ptr, size_t size) {
    return k_heap_realloc(&nros_smoke_heap, ptr, size, K_NO_WAIT);
}

void nros_zephyr_heap_free(void *ptr) {
    k_heap_free(&nros_smoke_heap, ptr);
}

size_t nros_zephyr_heap_capacity(void) {
    return NROS_SMOKE_HEAP_BYTES;
}

/* Not tracked by the stand-in; 0 is the platform ABI's "unknown". */
size_t nros_zephyr_heap_used(void) {
    return 0;
}

size_t nros_zephyr_heap_peak(void) {
    return 0;
}

/* No shape to report: 0 = "not known to be fragmentation". */
int nros_zephyr_heap_free_shape(size_t size, size_t *largest, size_t *total) {
    (void) size;
    if (largest != NULL) {
        *largest = 0;
    }
    if (total != NULL) {
        *total = 0;
    }
    return 0;
}
