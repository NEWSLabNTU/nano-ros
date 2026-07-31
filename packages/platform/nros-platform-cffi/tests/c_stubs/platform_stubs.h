/*
 * Phase 121.4.a — C-stub harness counter accessors.
 *
 * `tests/c_stubs/platform_stubs.c` defines every `nros_platform_*`
 * symbol declared in `<nros/platform.h>` and bumps a per-category
 * counter on each call. The Rust integration test calls every
 * extern through the `nros_platform_cffi` wrappers and verifies
 * each counter advanced.
 */

#ifndef NROS_PLATFORM_STUBS_H
#define NROS_PLATFORM_STUBS_H

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef enum {
    NROS_STUB_TOTAL    = 0,
    NROS_STUB_CLOCK    = 1,
    NROS_STUB_ALLOC    = 2,
    NROS_STUB_SLEEP    = 3,
    NROS_STUB_YIELD    = 4,
    NROS_STUB_RANDOM   = 5,
    NROS_STUB_TIME     = 6,
    NROS_STUB_TASK     = 7,
    NROS_STUB_MUTEX    = 8,
    NROS_STUB_CONDVAR  = 9,
    NROS_STUB_CATEGORY_COUNT = 10,
} nros_platform_stub_category_t;

uint32_t nros_platform_stub_counter(nros_platform_stub_category_t category);
void     nros_platform_stub_reset_counters(void);

#ifdef __cplusplus
}
#endif

#endif /* NROS_PLATFORM_STUBS_H */
