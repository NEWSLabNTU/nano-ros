/* Boot-time resolution of the image's wall-clock source.
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

/* `clock.fallback_settle_ms`'s registry default
 * (`packages/tooling/nros-zephyr-build/knobs.toml`). A test in
 * `nros_zephyr_build::clock_selection` holds the two equal. */
#define NROS_CLOCK_FALLBACK_SETTLE_MS_DEFAULT 10000u

/* A port that resolves from the BUILD's defines rather than from Kconfig
 * (POSIX, FreeRTOS). The build defines NROS_CLOCK_SOURCE_CSV (and, when
 * stated, NROS_CLOCK_UPGRADE, NROS_CLOCK_EPOCH_FAILURE,
 * NROS_CLOCK_FALLBACK_SETTLE_MS) only for an image that states a list other
 * than the default (`nros_zephyr_build::clock_selection`); an unstated knob
 * takes its registry default here. */
#ifdef NROS_CLOCK_SOURCE_CSV
#ifndef NROS_CLOCK_UPGRADE
#define NROS_CLOCK_UPGRADE "never"
#endif
#ifndef NROS_CLOCK_EPOCH_FAILURE
#define NROS_CLOCK_EPOCH_FAILURE "unstamped"
#endif
#ifndef NROS_CLOCK_FALLBACK_SETTLE_MS
#define NROS_CLOCK_FALLBACK_SETTLE_MS NROS_CLOCK_FALLBACK_SETTLE_MS_DEFAULT
#endif

/* The policy the build stated. `sources` is the caller's, and must outlive
 * the resolution. */
static inline void nros_clock_policy_from_build(nros_clock_policy_t* policy,
                                                uint8_t sources[NROS_CLOCK_MAX_SOURCES]) {
    policy->sources = sources;
    policy->n_sources = nros_clock_parse_sources(NROS_CLOCK_SOURCE_CSV, sources);
    policy->settle_ms = (uint32_t)(NROS_CLOCK_FALLBACK_SETTLE_MS);
    policy->upgrade = nros_clock_parse_upgrade(NROS_CLOCK_UPGRADE);
    policy->epoch_failure = nros_clock_parse_epoch_failure(NROS_CLOCK_EPOCH_FAILURE);
}

/* The first source the build listed: what the state names before, or
 * without, an answer. */
static inline uint8_t nros_clock_first_source_from_build(void) {
    uint8_t sources[NROS_CLOCK_MAX_SOURCES];
    return nros_clock_parse_sources(NROS_CLOCK_SOURCE_CSV, sources) != 0u
               ? sources[0]
               : (uint8_t)NROS_WALL_CLOCK_SOURCE_PLATFORM;
}
#endif /* NROS_CLOCK_SOURCE_CSV */

/* ---- SNTP re-sync pacing ------------------------------------------------
 *
 * Ruling 2026-11-24: a re-sync is paced on the clock the re-sync maintains,
 * the server's, not on the image's monotonic time, so the stated interval is
 * real time even where guest time races (an Arm FVP runs an idle guest 10x to
 * 100x fast). Between answers this port's wall clock is monotonic + offset and
 * runs at guest rate too, so the server's clock is followed by its RATE: the
 * server time two answers advanced over the monotonic time between them. The
 * next query is due once `elapsed x rate` reaches the interval, where
 * `elapsed` is monotonic time since the last ATTEMPT (so a failing server is
 * asked at the same paced spacing, never in a loop).
 *
 * The rate is not trusted blindly: a server step (an operator set, a leap), a
 * monotonic delta of zero, or an answer that went missing can make it zero,
 * negative or absurd. Outside [RATE_MIN, RATE_MAX] the gap falls back to
 * monotonic pacing, as before two answers exist, and the caller says so.
 *
 * Two margins keep the paced gap from coming in under the interval (both
 * measured, see their defines): a guard band of 1 % on every paced
 * deadline, and twice the deadline while the rate's window is shorter than
 * one interval of server time.
 *
 * Pure: the caller supplies every time; nothing here sleeps or reads a clock.
 * The first re-sync after boot has one answer and so no rate: it is paced on
 * monotonic time. */

/* Server ms per monotonic ms, in parts per million. 0.001 to 100: an FVP
 * racing 100x reads 0.01, a guest slowed 10x reads 10. */
#define NROS_SNTP_PACE_RATE_MIN_PPM 1000ULL
#define NROS_SNTP_PACE_RATE_MAX_PPM 100000000ULL

/* The paced deadline's guard band, in parts per thousand of the interval. The
 * rate is the last gap's, so it lags a change of guest speed: measured on an
 * Arm FVP at a 3.6x guest/host ratio, 4 of 19 paced gaps fell 12 to 57 ms
 * short of a 10 s interval. 1 % is 100 ms there. */
#define NROS_SNTP_PACE_GUARD_PERMILLE 10ULL
/* A rate measured over less than one interval of server time (the boot
 * exchange to the first re-sync, on a racing guest) is a short sample of a
 * guest whose speed is still settling: measured, the boot window read 0.45
 * where the steady state was 0.27, and the next gap came 6.1 s after a 10 s
 * interval. Until the window spans an interval, the paced deadline is this
 * many times the interval. */
#define NROS_SNTP_PACE_SHORT_WINDOW_FACTOR 2ULL

typedef struct {
    uint64_t prev_mono_ms; /* the answer before the last: monotonic ... */
    uint64_t prev_wall_ms; /* ... and the wall clock right after it installed */
    uint64_t last_mono_ms;
    uint64_t last_wall_ms;
    uint32_t answers; /* how many answers were noted, saturating */
} nros_sntp_pace_t;

typedef enum {
    NROS_SNTP_PACE_RATE_UNKNOWN = 0, /* fewer than two answers */
    NROS_SNTP_PACE_RATE_OK = 1,
    NROS_SNTP_PACE_RATE_OUT_OF_RANGE = 2, /* zero, negative or outside the bounds */
} nros_sntp_pace_rate_t;

/* Note a successful exchange: the monotonic time and the wall clock read
 * right after the offset it installed. */
static inline void nros_sntp_pace_answer(nros_sntp_pace_t* p, uint64_t mono_ms, uint64_t wall_ms) {
    p->prev_mono_ms = p->last_mono_ms;
    p->prev_wall_ms = p->last_wall_ms;
    p->last_mono_ms = mono_ms;
    p->last_wall_ms = wall_ms;
    if (p->answers < 2u) {
        p->answers++;
    }
}

/* The rate between the last two answers, in ppm, and whether it is usable. */
static inline nros_sntp_pace_rate_t nros_sntp_pace_rate(const nros_sntp_pace_t* p,
                                                        uint64_t* rate_ppm) {
    *rate_ppm = 0u;
    if (p->answers < 2u) {
        return NROS_SNTP_PACE_RATE_UNKNOWN;
    }
    if (p->last_mono_ms <= p->prev_mono_ms || p->last_wall_ms <= p->prev_wall_ms) {
        return NROS_SNTP_PACE_RATE_OUT_OF_RANGE;
    }
    uint64_t d_mono = p->last_mono_ms - p->prev_mono_ms;
    uint64_t d_wall = p->last_wall_ms - p->prev_wall_ms;
    /* d_wall above ~584 years in ms would overflow the product: out of range. */
    if (d_wall > UINT64_MAX / 1000000ULL) {
        return NROS_SNTP_PACE_RATE_OUT_OF_RANGE;
    }
    uint64_t r = d_wall * 1000000ULL / d_mono;
    if (r < NROS_SNTP_PACE_RATE_MIN_PPM || r > NROS_SNTP_PACE_RATE_MAX_PPM) {
        return NROS_SNTP_PACE_RATE_OUT_OF_RANGE;
    }
    *rate_ppm = r;
    return NROS_SNTP_PACE_RATE_OK;
}

/* Whether the next query is due, `since_attempt_ms` of monotonic time after
 * the last attempt. With a usable rate: once that time, scaled to the
 * server's, reaches `interval_ms`; otherwise once it reaches `interval_ms`
 * itself. */
/* Whether the rate's window (server time between the last two answers) spans
 * at least `interval_ms`. Meaningful only with a usable rate. */
static inline bool nros_sntp_pace_window_full(const nros_sntp_pace_t* p, uint32_t interval_ms) {
    return p->answers >= 2u && p->last_wall_ms > p->prev_wall_ms &&
           p->last_wall_ms - p->prev_wall_ms >= (uint64_t)interval_ms;
}

static inline bool nros_sntp_pace_due(const nros_sntp_pace_t* p, uint64_t since_attempt_ms,
                                      uint32_t interval_ms) {
    uint64_t rate = 0u;
    if (nros_sntp_pace_rate(p, &rate) != NROS_SNTP_PACE_RATE_OK) {
        return since_attempt_ms >= (uint64_t)interval_ms;
    }
    /* The deadline in server ms x 1e6: the interval plus the guard band,
     * times the short-window factor while the rate's window is short. */
    uint64_t need = (uint64_t)interval_ms * (1000ULL + NROS_SNTP_PACE_GUARD_PERMILLE) * 1000ULL;
    if (!nros_sntp_pace_window_full(p, interval_ms)) {
        need *= NROS_SNTP_PACE_SHORT_WINDOW_FACTOR;
    }
    /* since x rate >= need, without the division. `since` above ~584 years
     * of ms cannot occur; saturate rather than wrap. */
    if (since_attempt_ms > UINT64_MAX / rate) {
        return true;
    }
    return since_attempt_ms * rate >= need;
}

#ifdef NROS_CLOCK_HAS_BOARD
/* The `board` source's try, in `nros_clock_board__set_by__knobs_clock_source.c`
 * (compiled only with it): ask the board hook until it answers or `settle_ms`
 * runs out, polling every 10 ms, and install its offset through the one
 * setter. Shared by every port that resolves `board`. */
bool nros_clock_board_try__set_by__knobs_clock_source(uint32_t settle_ms);
#endif

#ifdef __cplusplus
}
#endif

#endif /* NROS_CLOCK_RESOLVE_H */
