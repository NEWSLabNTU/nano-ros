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

#include <stdbool.h>
#include <stdint.h>

#include <nros/platform.h>

#ifdef __cplusplus
extern "C" {
#endif

/* No source: the store's "none latched" value for `latched`. */
#define NROS_CLOCK_SOURCE_NONE 0xFFu

/* `upgrade` (RFC-0141 D1 rule 2): whether a source earlier in the declared
 * list may take over after boot. */
#define NROS_CLOCK_UPGRADE_NEVER 0u
#define NROS_CLOCK_UPGRADE_MONOTONIC 1u

/* How many sources a declared list may hold: one per
 * `NROS_WALL_CLOCK_SOURCE_*`, each at most once. */
#define NROS_CLOCK_MAX_SOURCES 4u

/* The store. Weak: each including TU emits this definition, and the link
 * keeps exactly one. Never read or written except through the functions
 * below. Beside the offset it keeps WHICH source installed it and how many
 * times that source has changed, the facts `nros_platform_wall_clock_state`
 * reports, and the policy boot resolution latched (`nros/clock_resolve.h`).
 *
 * Zero-initialised it is the UNARMED store: no policy, every install
 * admitted. That is the state of an image that never runs a resolver. */
typedef struct {
    uint64_t offset_us; /* 0: not acquired */
    uint8_t source;     /* NROS_WALL_CLOCK_SOURCE_* of the last install */
    uint32_t steps;     /* installs whose source differed from the one before */
    /* The policy, set by `nros_clock_epoch_arm` (boot resolution). */
    uint8_t armed;      /* 0: unarmed, every install admitted */
    uint8_t resolving;  /* 1 while boot resolution tries `admit` */
    uint8_t admit;      /* while resolving: the one source being tried */
    uint8_t latched;    /* after resolution: the image's source, or NONE */
    uint8_t upgrade;    /* NROS_CLOCK_UPGRADE_* */
    uint8_t n_order;
    uint8_t order[NROS_CLOCK_MAX_SOURCES]; /* the declared list, first = best */
    uint64_t last_us;   /* upgrade = monotonic: the last reading returned */
} nros_clock_epoch_store_t;
__attribute__((weak)) nros_clock_epoch_store_t nros_clock_epoch_store;

/* A source's place in the declared list (0 = first), or
 * NROS_CLOCK_MAX_SOURCES when the list does not name it. Caller holds the
 * critical section. */
static inline uint8_t nros_clock_epoch_rank_locked(uint8_t source) {
    for (uint8_t i = 0; i < nros_clock_epoch_store.n_order; i++) {
        if (nros_clock_epoch_store.order[i] == source) {
            return i;
        }
    }
    return (uint8_t)NROS_CLOCK_MAX_SOURCES;
}

/* Arm the store with the image's declared list and upgrade policy, at the
 * start of boot resolution. Until then every install is admitted. */
static inline void nros_clock_epoch_arm(const uint8_t* order, uint8_t n, uint8_t upgrade) {
    uint32_t token = nros_platform_critical_section_acquire();
    if (n > NROS_CLOCK_MAX_SOURCES) {
        n = (uint8_t)NROS_CLOCK_MAX_SOURCES;
    }
    nros_clock_epoch_store.n_order = n;
    for (uint8_t i = 0; i < n; i++) {
        nros_clock_epoch_store.order[i] = order[i];
    }
    nros_clock_epoch_store.upgrade = upgrade;
    nros_clock_epoch_store.armed = 1u;
    nros_clock_epoch_store.resolving = 1u;
    nros_clock_epoch_store.admit = (uint8_t)NROS_CLOCK_SOURCE_NONE;
    nros_clock_epoch_store.latched = (uint8_t)NROS_CLOCK_SOURCE_NONE;
    nros_platform_critical_section_release(token);
}

/* During resolution: the one source whose install is admitted now. Every
 * other source's install is refused, so the source that answers is the one
 * being tried, whichever thread happens to deliver a reading first. */
static inline void nros_clock_epoch_admit(uint8_t source) {
    uint32_t token = nros_platform_critical_section_acquire();
    nros_clock_epoch_store.admit = source;
    nros_platform_critical_section_release(token);
}

/* End of resolution: `source` is the image's for the rest of the run, or
 * NROS_CLOCK_SOURCE_NONE when no source answered. */
static inline void nros_clock_epoch_latch(uint8_t source) {
    uint32_t token = nros_platform_critical_section_acquire();
    nros_clock_epoch_store.latched = source;
    nros_clock_epoch_store.resolving = 0u;
    nros_clock_epoch_store.admit = (uint8_t)NROS_CLOCK_SOURCE_NONE;
    nros_platform_critical_section_release(token);
}

/* Whether an install by `source` is admitted under the armed policy. Caller
 * holds the critical section. Returns 2 for "admitted, and it moves the image
 * to a new source" (a counted step), 1 for "admitted", 0 for "refused".
 *
 *   - unarmed: every install (an image with no resolver);
 *   - resolving: only the source being tried;
 *   - latched, upgrade = never: only the latched source, so no stamp in the
 *     run comes from two clocks;
 *   - latched, upgrade = monotonic: the latched source, or one EARLIER in
 *     the list (a move up, never down);
 *   - nothing latched (no source answered at boot): any listed source; the
 *     first install is a forward step from "unset" and latches it. */
static inline int nros_clock_epoch_admits_locked(uint8_t source) {
    nros_clock_epoch_store_t* st = &nros_clock_epoch_store;
    if (!st->armed) {
        return 1;
    }
    if (st->resolving) {
        return source == st->admit ? 1 : 0;
    }
    uint8_t rank = nros_clock_epoch_rank_locked(source);
    if (rank >= st->n_order) {
        return 0;
    }
    if (st->latched == NROS_CLOCK_SOURCE_NONE) {
        return 2;
    }
    if (source == st->latched) {
        return 1;
    }
    if (st->upgrade == NROS_CLOCK_UPGRADE_MONOTONIC &&
        rank < nros_clock_epoch_rank_locked(st->latched)) {
        return 2;
    }
    return 0;
}

/* The one setter. `offset_us` is UNIX time in microseconds minus
 * `nros_platform_clock_ns() / 1000` at the instant of the reading, and
 * `source` (`NROS_WALL_CLOCK_SOURCE_*`) is who read it. Returns whether the
 * install was ADMITTED (see `nros_clock_epoch_admits_locked`); a refused
 * install leaves the store untouched.
 *
 * A re-install by the same source (an HLC window, an SNTP re-sync) is not a
 * step; an install by a DIFFERENT source after one has answered is, and is
 * counted, as is the first install after a boot at which no source answered. */
static inline bool nros_clock_set_epoch_offset_us(uint64_t offset_us, uint8_t source) {
    uint32_t token = nros_platform_critical_section_acquire();
    nros_clock_epoch_store_t* st = &nros_clock_epoch_store;
    int verdict = nros_clock_epoch_admits_locked(source);
    if (verdict == 0) {
        nros_platform_critical_section_release(token);
        return false;
    }
    if ((st->offset_us != 0ULL && st->source != source) || verdict == 2) {
        st->steps++;
    }
    if (verdict == 2) {
        st->latched = source;
    }
    st->offset_us = offset_us;
    st->source = source;
    nros_platform_critical_section_release(token);
    return true;
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
 * first install it is unsynced and names the source the image waits on: the
 * one boot resolution latched, else `configured`
 * (`NROS_WALL_CLOCK_SOURCE_*`). */
static inline nros_wall_clock_state_t nros_clock_epoch_state(uint8_t configured) {
    nros_wall_clock_state_t st;
    uint32_t token = nros_platform_critical_section_acquire();
    st.synced = nros_clock_epoch_store.offset_us != 0ULL;
    if (st.synced) {
        st.source = nros_clock_epoch_store.source;
    } else if (nros_clock_epoch_store.latched != NROS_CLOCK_SOURCE_NONE &&
               nros_clock_epoch_store.armed) {
        st.source = nros_clock_epoch_store.latched;
    } else {
        st.source = configured;
    }
    st.steps = nros_clock_epoch_store.steps;
    nros_platform_critical_section_release(token);
    return st;
}

/* UNIX time in microseconds from the installed offset, or 0 ("no wall
 * clock") before the first install.
 *
 * With upgrade = monotonic the reading is FLOORED at the last one returned:
 * a source that takes over while behind the old one holds the clock still
 * until it catches up, instead of stepping a stamp back (RFC-0141 D1 rule 2).
 * That floor is what keeps a consumer from seeing a stamp from an earlier
 * epoch after a later one when the source changes mid-run. */
static inline uint64_t nros_clock_epoch_us(void) {
    uint64_t mono_us = nros_platform_clock_ns() / 1000ULL;
    uint32_t token = nros_platform_critical_section_acquire();
    nros_clock_epoch_store_t* st = &nros_clock_epoch_store;
    uint64_t us = 0ULL;
    if (st->offset_us != 0ULL) {
        us = st->offset_us + mono_us;
        if (st->armed && st->upgrade == NROS_CLOCK_UPGRADE_MONOTONIC) {
            if (us < st->last_us) {
                us = st->last_us;
            }
            st->last_us = us;
        }
    }
    nros_platform_critical_section_release(token);
    return us;
}

#ifdef __cplusplus
}
#endif

#endif /* NROS_CLOCK_EPOCH_H */
