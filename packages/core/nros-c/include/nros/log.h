/**
 * @file log.h
 * @ingroup grp_log
 * @brief Phase 88.12 — ROS 2 style leveled logging surface for the C API.
 *
 * Mirrors the Rust [`nros_log`] facade. Each `nros_node_t` exposes a
 * `&'static nros_log::Logger` via `nros_node_get_logger(node)`; the
 * `NROS_LOG_*` macros render the message, attach severity + line
 * info, and dispatch through the same per-platform sink chain as
 * Rust call sites (see Phase 88.5 onwards).
 *
 * Usage:
 * @code
 * nros_logger_t logger = nros_node_get_logger(&node);
 * NROS_LOG_INFO(logger, "started; domain=%u", domain_id);
 * NROS_LOG_WARN(logger, "queue depth %u exceeds soft limit", depth);
 * @endcode
 *
 * Per-platform delivery (POSIX stderr / Zephyr LOG / ESP-IDF
 * esp_log_write / NuttX syslog / FreeRTOS+ThreadX+bare-metal board
 * fn-ptr) is invisible at the C API layer — the dispatcher routes
 * through `nros_platform_log_write` (declared in
 * `<nros/platform.h>`).
 */

#ifndef NROS_LOG_H
#define NROS_LOG_H

#include <stddef.h>
#include <stdint.h>
#include <stdio.h>

#ifdef __cplusplus
extern "C" {
#endif

/**
 * Severity levels — match `nros_log::Severity::as_u8()` discriminant.
 * Lower value = more verbose.
 */
typedef enum nros_log_severity_t {
    NROS_LOG_SEVERITY_TRACE = 0,
    NROS_LOG_SEVERITY_DEBUG = 1,
    NROS_LOG_SEVERITY_INFO  = 2,
    NROS_LOG_SEVERITY_WARN  = 3,
    NROS_LOG_SEVERITY_ERROR = 4,
    NROS_LOG_SEVERITY_FATAL = 5,
} nros_log_severity_t;

/**
 * Opaque handle to a `&'static nros_log::Logger`. Obtain via
 * `nros_node_get_logger(...)`. NEVER free.
 */
typedef const void *nros_logger_t;

/**
 * Low-level emit. Renders `message` (already-formatted UTF-8 text;
 * NOT null-terminated by contract — pass an explicit length) at
 * `severity` through the dispatcher.
 *
 * Most users should use the `NROS_LOG_*` printf-style macros below
 * rather than calling this directly.
 *
 * @param logger    Logger handle from `nros_node_get_logger`. NULL =
 *                  silently drops (kept total to simplify call sites).
 * @param severity  One of `nros_log_severity_t`.
 * @param message   UTF-8 text; not required to be null-terminated.
 * @param message_len  Length of `message` in bytes.
 */
void nros_log_emit(
    nros_logger_t        logger,
    nros_log_severity_t  severity,
    const char          *message,
    size_t               message_len);

/**
 * Internal helper used by the macros. Formats `fmt + args` into a
 * stack buffer, then calls `nros_log_emit`. Buffer size = 256 bytes;
 * overflow is truncated + appended `...`.
 */
void nros_log_emit_fmt(
    nros_logger_t        logger,
    nros_log_severity_t  severity,
    const char          *fmt,
    ...) __attribute__((format(printf, 3, 4)));

/* ---- Convenience macros ---- */
/* The macros stage the printf args into the heapless stack buffer
 * inside `nros_log_emit_fmt`. Below-ceiling filtering happens on the
 * Rust side via the per-logger threshold; no compile-time gating on
 * the C surface (use `if (...)` guards if you need it). */

#define NROS_LOG_TRACE(logger, ...) nros_log_emit_fmt((logger), NROS_LOG_SEVERITY_TRACE, __VA_ARGS__)
#define NROS_LOG_DEBUG(logger, ...) nros_log_emit_fmt((logger), NROS_LOG_SEVERITY_DEBUG, __VA_ARGS__)
#define NROS_LOG_INFO(logger, ...)  nros_log_emit_fmt((logger), NROS_LOG_SEVERITY_INFO,  __VA_ARGS__)
#define NROS_LOG_WARN(logger, ...)  nros_log_emit_fmt((logger), NROS_LOG_SEVERITY_WARN,  __VA_ARGS__)
#define NROS_LOG_ERROR(logger, ...) nros_log_emit_fmt((logger), NROS_LOG_SEVERITY_ERROR, __VA_ARGS__)
#define NROS_LOG_FATAL(logger, ...) nros_log_emit_fmt((logger), NROS_LOG_SEVERITY_FATAL, __VA_ARGS__)

/**
 * Phase 88.16.H — explicit installation of the default sink list.
 *
 * Cross-language `no_std` builds (FreeRTOS / NuttX / ThreadX C and
 * C++ examples) must call this once after the board crate's
 * platform-log writer is registered (typically right after
 * `nros_node_init` returns). Pins `nros_log::init` as a linker root
 * so `--gc-sections` keeps it; without this, the lazy guard inside
 * `nros_log_emit` can be unreachable and every record silently
 * drops.
 *
 * Idempotent. Hosted POSIX consumers can skip this — the
 * `.init_array` ctors already wire the dispatcher.
 */
void nros_log_init(void);

/**
 * Phase 88.16.H — opaque handle to the catch-all `nros` logger.
 *
 * Lets C callers emit through `NROS_LOG_*` without standing up a
 * full `nros_node_t` (useful for boot diagnostics, panic hooks,
 * smoke fixtures). The returned handle is `'static`. Never free.
 *
 * @return `nros_logger_t` for the default ("nros") logger.
 */
nros_logger_t nros_log_default_logger(void);

#ifdef __cplusplus
}  /* extern "C" */
#endif

#endif /* NROS_LOG_H */
