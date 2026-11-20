/* Boot-time resolution of the image's wall-clock source (RFC-0141 D1, D4).
 *
 * An image declares one source, or an ordered fallback, in `[knobs.clock]`:
 *
 *     source = ["sntp", "platform"]   # tried in this order
 *     fallback_settle_ms = 10000      # how long each source gets
 *     upgrade = "never"               # or "monotonic"
 *     epoch_failure = "unstamped"     # or "halt"
 *
 * `nros_clock_resolve` runs once, during boot, after the image's entities are
 * registered and BEFORE the executor first runs, so nothing has been stamped
 * yet. It tries each source in order, giving each up to `settle_ms`, and the
 * FIRST that answers is latched as the image's source for the rest of the
 * run (`nros/clock_epoch.h` then admits installs from it alone, or, with
 * `upgrade = monotonic`, also from a source earlier in the list, under a
 * floor that never lets a reading go backwards).
 *
 * If no source answers, `epoch_failure` decides: `unstamped` (the default)
 * says so once and runs on, every stamp reading 0, ROS's "unset" time;
 * `halt` stops the image through `nros_platform_panic`, naming the sources.
 *
 * The port supplies how each source is tried, as a callback, because only the
 * port knows how: an SNTP exchange on its IP stack, a wait for the first zenoh
 * HLC timestamp, the port's own clock. The policy here is the same on every
 * port, which is why it is one header and not one copy per port.
 *
 * Header-only, for the reason `nros/clock_epoch.h` gives: every port is
 * compiled by a different road, and a header its platform code already
 * reaches needs no build change. */
#ifndef NROS_CLOCK_RESOLVE_H
#define NROS_CLOCK_RESOLVE_H

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#include <nros/clock_epoch.h>
#include <nros/platform.h>

#ifdef __cplusplus
extern "C" {
#endif

/* `epoch_failure`: what the image does when no source answered at boot. */
#define NROS_CLOCK_EPOCH_FAILURE_UNSTAMPED 0u
#define NROS_CLOCK_EPOCH_FAILURE_HALT 1u

/* Try one source. Return true when it ANSWERED within `settle_ms`: a
 * calibrating source has installed its offset through
 * `nros_clock_set_epoch_offset_us` (the store admits only the source being
 * tried), and `platform` has accepted the port's own clock. `ctx` is the
 * caller's. */
typedef bool (*nros_clock_try_fn)(uint8_t source, uint32_t settle_ms, void* ctx);

/* Where the resolver's one line goes (a console, a log). `msg` is `len`
 * bytes, no NUL required. NULL drops it. */
typedef void (*nros_clock_report_fn)(const char* msg, size_t len, void* ctx);

typedef struct {
    const uint8_t* sources; /* NROS_WALL_CLOCK_SOURCE_*, first = best */
    uint8_t n_sources;      /* 0 reads as ["platform"] */
    uint32_t settle_ms;     /* per source */
    uint8_t upgrade;        /* NROS_CLOCK_UPGRADE_* */
    uint8_t epoch_failure;  /* NROS_CLOCK_EPOCH_FAILURE_* */
} nros_clock_policy_t;

/* A source's name as the knob spells it. */
static inline const char* nros_clock_source_name(uint8_t source) {
    switch (source) {
    case NROS_WALL_CLOCK_SOURCE_PLATFORM:
        return "platform";
    case NROS_WALL_CLOCK_SOURCE_SNTP:
        return "sntp";
    case NROS_WALL_CLOCK_SOURCE_ZENOH_HLC:
        return "zenoh-hlc";
    case NROS_WALL_CLOCK_SOURCE_BOARD:
        return "board";
    default:
        return "?";
    }
}

/* Whether `word` (`len` bytes) spells `name`. */
static inline bool nros_clock_word_is(const char* word, size_t len, const char* name) {
    size_t i = 0;
    for (; i < len; i++) {
        if (name[i] == '\0' || name[i] != word[i]) {
            return false;
        }
    }
    return name[i] == '\0';
}

/* Parse the knobs' one spelling, `clock.source` comma-joined
 * (`"sntp,platform"`), into `out`. Returns how many sources it holds. The
 * build already refused a value outside the set, so an unknown word here is
 * skipped rather than reported. */
static inline uint8_t nros_clock_parse_sources(const char* csv, uint8_t out[NROS_CLOCK_MAX_SOURCES]) {
    static const uint8_t ids[4] = {NROS_WALL_CLOCK_SOURCE_PLATFORM, NROS_WALL_CLOCK_SOURCE_SNTP,
                                   NROS_WALL_CLOCK_SOURCE_ZENOH_HLC, NROS_WALL_CLOCK_SOURCE_BOARD};
    uint8_t n = 0;
    const char* p = csv;
    while (p != NULL && *p != '\0' && n < NROS_CLOCK_MAX_SOURCES) {
        const char* end = p;
        while (*end != '\0' && *end != ',') {
            end++;
        }
        while (p < end && *p == ' ') {
            p++;
        }
        size_t len = (size_t)(end - p);
        while (len > 0u && p[len - 1u] == ' ') {
            len--;
        }
        for (uint8_t k = 0; k < 4u; k++) {
            if (nros_clock_word_is(p, len, nros_clock_source_name(ids[k]))) {
                out[n++] = ids[k];
                break;
            }
        }
        p = *end == ',' ? end + 1 : end;
    }
    return n;
}

/* Whether a parsed list names `source`. */
static inline bool nros_clock_sources_name(const uint8_t* sources, uint8_t n, uint8_t source) {
    for (uint8_t i = 0; i < n; i++) {
        if (sources[i] == source) {
            return true;
        }
    }
    return false;
}

/* `clock.upgrade` and `clock.epoch_failure`, from their knob spelling. */
static inline uint8_t nros_clock_parse_upgrade(const char* s) {
    return nros_clock_word_is(s, __builtin_strlen(s), "monotonic") ? NROS_CLOCK_UPGRADE_MONOTONIC
                                                                   : NROS_CLOCK_UPGRADE_NEVER;
}
static inline uint8_t nros_clock_parse_epoch_failure(const char* s) {
    return nros_clock_word_is(s, __builtin_strlen(s), "halt") ? NROS_CLOCK_EPOCH_FAILURE_HALT
                                                              : NROS_CLOCK_EPOCH_FAILURE_UNSTAMPED;
}

/* Append `src` to `buf` at `*at`, never past `cap`. No libc: this header is
 * reached from freestanding images. */
static inline void nros_clock_msg_put(char* buf, size_t cap, size_t* at, const char* src) {
    while (*src != '\0' && *at < cap) {
        buf[(*at)++] = *src++;
    }
}

/* Wait up to `settle_ms` for the store to hold an offset, sleeping in short
 * steps. The `try` of a source whose reading arrives on another thread (the
 * zenoh HLC, delivered by the session's read task) is this. */
static inline bool nros_clock_wait_synced(uint32_t settle_ms) {
    const uint64_t start_ns = nros_platform_clock_ns();
    const uint64_t budget_ns = (uint64_t)settle_ms * 1000000ULL;
    for (;;) {
        if (nros_clock_epoch_offset_us() != 0ULL) {
            return true;
        }
        if (nros_platform_clock_ns() - start_ns >= budget_ns) {
            return false;
        }
        nros_platform_sleep_ms(10);
    }
}

/* Resolve the image's wall-clock source. Returns the latched
 * `NROS_WALL_CLOCK_SOURCE_*`, or NROS_CLOCK_SOURCE_NONE when no source
 * answered and the policy is `unstamped`. With `halt` and no answer it does
 * not return. */
static inline uint8_t nros_clock_resolve(const nros_clock_policy_t* policy,
                                         nros_clock_try_fn try_source,
                                         nros_clock_report_fn report,
                                         void* ctx) {
    static const uint8_t platform_only[1] = {NROS_WALL_CLOCK_SOURCE_PLATFORM};
    const uint8_t* sources = policy->sources;
    uint8_t n = policy->n_sources;
    if (n == 0u || sources == NULL) {
        sources = platform_only;
        n = 1u;
    }
    nros_clock_epoch_arm(sources, n, policy->upgrade);

    char msg[160];
    size_t at = 0;
    for (uint8_t i = 0; i < n; i++) {
        nros_clock_epoch_admit(sources[i]);
        if (try_source(sources[i], policy->settle_ms, ctx)) {
            nros_clock_epoch_latch(sources[i]);
            nros_clock_msg_put(msg, sizeof msg, &at, "nros: wall clock from ");
            nros_clock_msg_put(msg, sizeof msg, &at, nros_clock_source_name(sources[i]));
            if (report != NULL) {
                report(msg, at, ctx);
            }
            return sources[i];
        }
    }
    nros_clock_epoch_latch((uint8_t)NROS_CLOCK_SOURCE_NONE);

    nros_clock_msg_put(msg, sizeof msg, &at, "nros: no wall-clock source answered (");
    for (uint8_t i = 0; i < n; i++) {
        if (i != 0u) {
            nros_clock_msg_put(msg, sizeof msg, &at, ", ");
        }
        nros_clock_msg_put(msg, sizeof msg, &at, nros_clock_source_name(sources[i]));
    }
    if (policy->epoch_failure == NROS_CLOCK_EPOCH_FAILURE_HALT) {
        nros_clock_msg_put(msg, sizeof msg, &at, "); halting, as clock.epoch_failure = halt asks");
        if (report != NULL) {
            report(msg, at, ctx);
        }
        nros_platform_panic(msg, at);
    }
    nros_clock_msg_put(msg, sizeof msg, &at, "); stamps are unset (header.stamp reads 0)");
    if (report != NULL) {
        report(msg, at, ctx);
    }
    return (uint8_t)NROS_CLOCK_SOURCE_NONE;
}

#ifdef __cplusplus
}
#endif

#endif /* NROS_CLOCK_RESOLVE_H */
