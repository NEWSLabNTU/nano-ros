/**
 * @file timer.h
 * @ingroup grp_executor
 * @brief Periodic timer API.
 *
 * Create timers with rclc_timer_init_default() (or rcl_timer_init() for a
 * timer on a clock) and register them with an executor via
 * rclc_executor_add_timer().
 */

#ifndef NROS_TIMER_H
#define NROS_TIMER_H

/* Type and function definitions live in <rclc/detail/nros_generated.h>.
 * This per-module header is kept as a thin shim so existing code that
 * does `#include <rclc/timer.h>` continues to compile. */
#include "rclc/types.h"

#include <stddef.h>

/**
 * @brief Recover the struct that embeds a member, from a pointer to that member.
 *
 * rcl's timer callback, `void (*)(rcl_timer_t *timer, int64_t last_call_time)`,
 * carries no context pointer (phase-515 D2). A callback reaches its state the
 * standard C way: embed the `rcl_timer_t` in the state struct and step back
 * from the timer to the struct. The executor always hands the callback the
 * caller's own timer, never a copy, so this is exact.
 *
 * ```c
 * typedef struct { rcl_publisher_t pub; rcl_timer_t timer; int32_t count; } talker_t;
 *
 * static void on_tick(rcl_timer_t *timer, int64_t last_call_time) {
 *     talker_t *self = RCLC_CONTAINER_OF(timer, talker_t, timer);
 *     ...
 * }
 * ```
 *
 * Ours, not rclc's: rclc programs write the `offsetof` arithmetic by hand,
 * which this expands to, so a ported callback needs no change.
 */
#define RCLC_CONTAINER_OF(ptr, type, member) ((type*)(void*)((char*)(ptr)-offsetof(type, member)))

/**
 * @brief Compute the duration between two time points: `finish - start`.
 *
 * rcl's `rcl_difference_times(const rcl_time_point_t *start,
 * const rcl_time_point_t *finish, rcl_duration_t *delta)` — **Time - Time ->
 * Duration**. Ledger row `c:difference_times` (phase-379 W5, group B) records
 * why this was a gap rather than a rename: `rclc_time_sub` is Time - Duration
 * -> Time, a different operation with different operands and a different
 * return type, so there was never a rename to make. C++ ships the operation as
 * `Time::operator-(const Time &) -> Duration` and Rust as
 * `impl Sub<Time> for Time`; only C had no way to get a duration out of two
 * timestamps.
 *
 * A `static inline` rather than an exported symbol because there is no state
 * behind it: this is arithmetic on two values the caller already holds.
 * Nothing is computed here that is not already an entry point — the
 * decode is `rclc_time_to_nanoseconds` and the encode is
 * `rclc_duration_from_nanoseconds`, which is the ONE place the
 * `builtin_interfaces` `{sec, nanosec}` split (floor division, non-negative
 * remainder, issue 0799) is implemented. Open-coding the split here is what
 * the C++ `Duration::to_msg` used to do, and it got negatives wrong.
 *
 * ### Overflow, and how this differs from rcl
 *
 * The nanosecond subtraction **cannot** overflow. `rclc_time_t` is
 * `{int32_t sec, uint32_t nanosec}`, so a decoded time point is bounded by
 * roughly ±2.15e18 ns and any difference of two of them by ±4.3e18 ns —
 * inside `int64_t`'s ±9.22e18. rcl has no such bound: `rcl_time_point_t`
 * carries a bare `int64_t` nanosecond count, and `rcl_difference_times`
 * subtracts two of them with plain signed arithmetic (`rcl/src/rcl/time.c`,
 * `rcl_difference_times`), which is undefined behaviour at the extremes of
 * the type. Our narrower time encoding removes the case rather than checking
 * for it.
 *
 * What *can* exceed its type is the RESULT once re-encoded: a duration of
 * ±4.3e9 seconds does not fit `rcl_duration_t`'s `int32_t sec`.
 * `rclc_duration_from_nanoseconds` SATURATES there — it clamps to the
 * nearest representable duration rather than wrapping, which keeps the
 * encoding monotone (see its own note on `split_nanoseconds`). It is the only
 * behaviour available to a function with no error channel, and it is the same
 * choice every other `_time_` entry point already makes.
 *
 * ### Divergence from rcl, deliberate
 *
 * rcl returns `RCL_RET_ERROR` when the two points carry different
 * `clock_type`s. Ours cannot: ledger row `c:time_point_t` records that
 * `rclc_time_t` is the value alone and the clock kind is a property of the
 * clock it was read from, not of every timestamp — which halves a timestamp
 * on the wire and in a message. Differencing two times read from different
 * clocks is therefore the caller's obligation here, and the header says so
 * rather than the function pretending to check.
 *
 * @param[in]  start  The time point for the start of the duration.
 * @param[in]  finish The time point for the end of the duration.
 * @param[out] delta  `finish - start`. Negative when `finish` precedes
 *                    `start`, encoded the way `builtin_interfaces/msg/Duration`
 *                    requires (`{sec: -2, nanosec: 3e8}` is -1.7 s).
 * @return `RCL_RET_OK`, or `RCL_RET_INVALID_ARGUMENT` if any argument is
 *         NULL.
 */
static inline rcl_ret_t rcl_difference_times(const struct rclc_time_t* start,
                                             const struct rclc_time_t* finish,
                                             struct rcl_duration_t* delta) {
    if (start == NULL || finish == NULL || delta == NULL) {
        return RCL_RET_INVALID_ARGUMENT;
    }
    /* Both decodes go through the exported entry point; see the note above on
     * why neither the decode nor the encode is spelled out here. */
    *delta = rclc_duration_from_nanoseconds(rclc_time_to_nanoseconds(finish) -
                                            rclc_time_to_nanoseconds(start));
    return RCL_RET_OK;
}

#endif /* NROS_TIMER_H */

/* Every upstream-path header carries the whole C API, as `<rclc/rclc.h>` does
 * (phase-492): a ported file that includes only `<rclc/timer.h>` still finds
 * `RCLC_CHECK_RET`, `RCLC_LOG_*` and the rcl spellings. Outside the guard, so
 * this header's own declarations are complete before the umbrella reads them. */
#include "rclc/rclc.h"
