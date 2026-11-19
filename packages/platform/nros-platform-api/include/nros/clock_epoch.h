/* The ONE wall-clock epoch store (RFC-0141 D3, phase-582 W1a).
 *
 * A wall-clock source that works by calibration — an SNTP exchange, or the
 * zenoh HLC timestamps on received samples — produces an OFFSET: UNIX time
 * minus the monotonic clock at the instant of the reading. Every later wall
 * reading is `offset + nros_platform_clock_ns()`, so the stamp path never
 * waits on a network, and readings are monotonic between installs.
 *
 * Each port used to keep that offset itself (Zephyr's was a spinlock-guarded
 * static in its platform.c). Three copies of one invariant drift: Zephyr's
 * needed fixing when a second writer arrived (issue 0758, 2026-09-29: a hi/lo
 * pair read while a writer straddles a 2^32 us boundary is off by 71
 * minutes). This header is the one copy, included by every port that
 * calibrates:
 *
 *   - ONE storage object, defined weak here so every including translation
 *     unit names the same object and the linker keeps one (GNU ld and TI's
 *     tiarmlnk both merge weak data definitions);
 *   - ONE setter, `nros_clock_set_epoch_offset_us`, and one reader;
 *   - both under the platform's critical section
 *     (`nros_platform_critical_section_acquire`, which every port defines),
 *     so a 64-bit value cannot tear on a 32-bit target, and an SMP image is
 *     covered (Zephyr's `irq_lock` takes the global lock there).
 *
 * The unit is microseconds, as the stores it replaces kept it. Zero means
 * "not acquired": a reader then answers "no wall clock", which `platform.h`
 * defines as 0 from `nros_platform_time_now_ns`. A real offset is never 0,
 * since it is UNIX time minus a boot-relative clock.
 *
 * Header-only on purpose: every port's sources are compiled by a different
 * road (a per-board CMake list, a cargo build script, Zephyr's module), so a
 * shared `.c` would be a file to add on each of them, and a road that missed
 * it would be a link error found late. A header every port's platform.c
 * already reaches needs no build change. */
#ifndef NROS_CLOCK_EPOCH_H
#define NROS_CLOCK_EPOCH_H

#include <stdint.h>

#include <nros/platform.h>

#ifdef __cplusplus
extern "C" {
#endif

/* The store. Weak: each including TU emits this definition, and the link
 * keeps exactly one. Never read or written except through the two functions
 * below. */
__attribute__((weak)) uint64_t nros_clock_epoch_offset_us_store;

/* The one setter. `offset_us` is UNIX time in microseconds minus
 * `nros_platform_clock_ns() / 1000` at the instant of the reading. */
static inline void nros_clock_set_epoch_offset_us(uint64_t offset_us) {
    uint32_t token = nros_platform_critical_section_acquire();
    nros_clock_epoch_offset_us_store = offset_us;
    nros_platform_critical_section_release(token);
}

/* The one reader: the installed offset, or 0 when none has been. */
static inline uint64_t nros_clock_epoch_offset_us(void) {
    uint32_t token = nros_platform_critical_section_acquire();
    uint64_t offset_us = nros_clock_epoch_offset_us_store;
    nros_platform_critical_section_release(token);
    return offset_us;
}

/* UNIX time in microseconds from the installed offset, or 0 ("no wall
 * clock") before the first install. */
static inline uint64_t nros_clock_epoch_us(void) {
    uint64_t offset_us = nros_clock_epoch_offset_us();
    if (offset_us == 0ULL) {
        return 0ULL;
    }
    return offset_us + nros_platform_clock_ns() / 1000ULL;
}

#ifdef __cplusplus
}
#endif

#endif /* NROS_CLOCK_EPOCH_H */
