/* The ONE wall-clock epoch store.
 *
 * A wall-clock source that works by calibration — an SNTP exchange, or the
 * zenoh HLC timestamps on received samples — produces an OFFSET: UNIX time
 * minus the monotonic clock at the instant of the reading. Every later wall
 * reading is `offset + nros_platform_clock_ns()`, so the stamp path never
 * waits on a network, and readings are monotonic between installs.
 *
 * Each port used to keep that offset itself (Zephyr's was a spinlock-guarded
 * static in its platform.c). Copies of one invariant drift: Zephyr's needed
 * fixing when a second writer arrived (a hi/lo pair read while a writer
 * straddles a 2^32 us boundary is off by 71 minutes). This header is the one
 * copy, included by every port that calibrates:
 *
 *   - ONE storage object, defined weak here so every including translation
 *     unit names the same object and the linker keeps one (GNU ld and TI's
 *     tiarmlnk both merge weak data definitions);
 *   - ONE setter, `nros_clock_set_epoch_offset_us`, one reader, and the
 *     state the port reports (`nros_clock_epoch_state`);
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
 * keeps exactly one. Never read or written except through the functions
 * below. Beside the offset it keeps WHICH source installed it and how many
 * times that source has changed, the facts `nros_platform_wall_clock_state`
 * reports. */
typedef struct {
    uint64_t offset_us; /* 0: not acquired */
    uint8_t source;     /* NROS_WALL_CLOCK_SOURCE_* of the last install */
    uint32_t steps;     /* installs whose source differed from the one before */
} nros_clock_epoch_store_t;
__attribute__((weak)) nros_clock_epoch_store_t nros_clock_epoch_store;

/* The one setter. `offset_us` is UNIX time in microseconds minus
 * `nros_platform_clock_ns() / 1000` at the instant of the reading, and
 * `source` (`NROS_WALL_CLOCK_SOURCE_*`) is who read it. A re-install by the
 * same source (an HLC window, an SNTP re-sync) is not a step; an install by a
 * DIFFERENT source after one has answered is, and is counted. */
static inline void nros_clock_set_epoch_offset_us(uint64_t offset_us, uint8_t source) {
    uint32_t token = nros_platform_critical_section_acquire();
    if (nros_clock_epoch_store.offset_us != 0ULL && nros_clock_epoch_store.source != source) {
        nros_clock_epoch_store.steps++;
    }
    nros_clock_epoch_store.offset_us = offset_us;
    nros_clock_epoch_store.source = source;
    nros_platform_critical_section_release(token);
}

/* The one reader: the installed offset, or 0 when none has been. */
static inline uint64_t nros_clock_epoch_offset_us(void) {
    uint32_t token = nros_platform_critical_section_acquire();
    uint64_t offset_us = nros_clock_epoch_store.offset_us;
    nros_platform_critical_section_release(token);
    return offset_us;
}

/* The state a calibrating port answers `nros_platform_wall_clock_state`
 * with, read in one critical section so the three fields agree. Before the
 * first install it is unsynced and names `configured`, the source the port
 * is waiting on (`NROS_WALL_CLOCK_SOURCE_*`). */
static inline nros_wall_clock_state_t nros_clock_epoch_state(uint8_t configured) {
    nros_wall_clock_state_t st;
    uint32_t token = nros_platform_critical_section_acquire();
    st.synced = nros_clock_epoch_store.offset_us != 0ULL;
    st.source = st.synced ? nros_clock_epoch_store.source : configured;
    st.steps = nros_clock_epoch_store.steps;
    nros_platform_critical_section_release(token);
    return st;
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
