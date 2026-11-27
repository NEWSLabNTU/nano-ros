/* A wall-clock source asked again on an interval, on a task of its own.
 *
 * A source that answers once at boot (`board`, `sntp`) can be asked again so
 * the image follows a clock that drifts or is later corrected. Each answer is
 * installed through the one setter (`nros/clock_epoch.h`), so the image's
 * `clock.upgrade` policy decides what a re-answer may change, exactly as it
 * does at boot.
 *
 * ONE helper for every such source and port: a re-ask is a loop of "sleep the
 * interval, ask", and it must run on its own task because asking can block
 * (a network exchange, or a board hook of unknown depth) and the task that
 * reads the clock must never wait on it. The stack is the source's own knob,
 * stated in bytes, and it is a FLOOR: it goes to the platform ABI's
 * `nros_platform_task_attr_t::stack_bytes`, which every port raises to its
 * own minimum and never refuses for being small. A fixed stack size compiled
 * into the loop is the defect this replaces.
 *
 * The interval here is the image's monotonic time. A source that must pace
 * on another clock (SNTP paces on the server's) supplies its own `due`.
 *
 * Header-only, like `nros/clock_resolve.h`: every port reaches it. */
#ifndef NROS_CLOCK_RESYNC_H
#define NROS_CLOCK_RESYNC_H

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#include <nros/platform.h>

#ifdef __cplusplus
extern "C" {
#endif

/* One re-ask of the source: true when it answered (and installed). */
typedef bool (*nros_clock_resync_ask_fn)(void* ctx);
/* Whether the next ask is due, `since_ms` of monotonic time after the last
 * one. NULL: due once `since_ms` reaches the interval. */
typedef bool (*nros_clock_resync_due_fn)(void* ctx, uint64_t since_ms, uint32_t interval_ms);
/* Called when an ask's outcome differs from the one before it (first call:
 * after the first ask), so a failing source is reported once, not per ask. */
typedef void (*nros_clock_resync_changed_fn)(void* ctx, bool answered);

typedef struct {
    /* Stated by the caller before start. */
    const char* name;
    uint32_t interval_ms;
    size_t stack_bytes;
    nros_clock_resync_ask_fn ask;
    nros_clock_resync_due_fn due;         /* may be NULL */
    nros_clock_resync_changed_fn changed; /* may be NULL */
    void* ctx;
    /* Owned by the helper. */
    uint64_t last_ask_ms;
    uint8_t outcome; /* 0 = no ask yet, 1 = answered, 2 = not */
    /* The task's opaque storage, aligned for any port's task type (a union,
     * because `_Alignas` is C11 only and C++ includes this too). */
    union {
        uint8_t bytes[NROS_PLATFORM_TASK_STORAGE_SIZE];
        uint64_t u64;
        long double ld;
        void* ptr;
    } task;
} nros_clock_resync_t;

/* The tick a due check runs on: a sixteenth of the interval, 10 ms to 1 s. */
static inline uint32_t nros_clock_resync_tick_ms(uint32_t interval_ms) {
    uint32_t t = interval_ms / 16u;
    return t < 10u ? 10u : (t > 1000u ? 1000u : t);
}

/* One pass of the loop at monotonic time `now_ms`: asks when due, reports a
 * change of outcome. Returns whether it asked. Pure apart from the callbacks,
 * so a test drives it with its own clock. */
static inline bool nros_clock_resync_step(nros_clock_resync_t* r, uint64_t now_ms) {
    uint64_t since = now_ms - r->last_ask_ms;
    bool due =
        r->due != NULL ? r->due(r->ctx, since, r->interval_ms) : since >= (uint64_t)r->interval_ms;
    if (!due) {
        return false;
    }
    r->last_ask_ms = now_ms;
    bool ok = r->ask(r->ctx);
    uint8_t outcome = ok ? 1u : 2u;
    if (outcome != r->outcome && r->changed != NULL) {
        r->changed(r->ctx, ok);
    }
    r->outcome = outcome;
    return true;
}

static inline void* nros_clock_resync_entry(void* arg) {
    nros_clock_resync_t* r = (nros_clock_resync_t*)arg;
    const uint32_t tick = nros_clock_resync_tick_ms(r->interval_ms);
    for (;;) {
        nros_platform_sleep_ms(tick);
        (void)nros_clock_resync_step(r, nros_platform_clock_ns() / 1000000ULL);
    }
    return NULL;
}

/* Start the re-ask task. `r` must outlive the image (a static). The first ask
 * comes one interval after this call. Returns the platform ABI's code
 * (`NROS_PLATFORM_RET_OK` on success); an interval of 0 starts nothing and
 * returns OK. The task keeps its creator's priority: a portable "least
 * urgent" does not exist (the POSIX port maps any band value to SCHED_FIFO,
 * which an unprivileged process may not create a thread under). */
static inline int8_t nros_clock_resync_start(nros_clock_resync_t* r) {
    if (r->interval_ms == 0u || r->ask == NULL) {
        return 0;
    }
    r->last_ask_ms = nros_platform_clock_ns() / 1000000ULL;
    r->outcome = 0u;
    nros_platform_task_attr_t attr;
    nros_platform_task_attr_init(&attr);
    attr.name = r->name;
    attr.stack_bytes = r->stack_bytes;
    attr.priority = NROS_PLATFORM_PRIORITY_INHERIT;
    attr.flags = NROS_PLATFORM_TASK_DETACHED;
    return nros_platform_task_init(r->task.bytes, &attr, nros_clock_resync_entry, r);
}

#ifdef __cplusplus
}
#endif

#endif /* NROS_CLOCK_RESYNC_H */
