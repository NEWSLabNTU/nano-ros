#ifndef NROS_PLATFORM_FREERTOS_H
#define NROS_PLATFORM_FREERTOS_H

#ifdef __cplusplus
extern "C" {
#endif

/**
 * @file platform_freertos.h
 * @brief FreeRTOS-specific platform primitives the board entries call.
 *
 * Defined by `nros-platform-freertos` only; consumed by the FreeRTOS board
 * entries (C and Rust), never by portable code. Like `platform_zephyr.h` it is
 * excluded from the bindgen platform surface.
 */

/**
 * @brief Resolve the image's wall-clock source, once, at this boot point.
 *
 * Runs the `[knobs.clock] source` list exactly once: each listed source in
 * turn (`board` gets `clock.fallback_settle_ms` to answer, asked every
 * 10 ms), then `clock.epoch_failure` if none answered. Latches the result and
 * reports the verdict (`nros: wall clock from <source>`) through the log
 * writer registered with `nros_platform_register_log_writer`.
 *
 * Until this is called, `nros_platform_time_now_ns` reads 0 and
 * `nros_platform_wall_clock_state` reports "not synced": a read resolves
 * nothing and starts no window. So a board entry calls it from the app task
 * AFTER the network is up (a board clock backed by an SNTP exchange needs it)
 * and AFTER the log writer is registered (so the verdict is seen), and before
 * the application runs.
 *
 * Must be called from a running task: the window sleeps. Called before the
 * scheduler runs it does nothing. A second call (from any task) returns once
 * the first has resolved, and resolves nothing. An image that states no
 * source list has nothing to resolve, and this returns at once.
 */
void nros_platform_freertos_clock_resolve_point(void);

#ifdef __cplusplus
}
#endif

#endif /* NROS_PLATFORM_FREERTOS_H */
