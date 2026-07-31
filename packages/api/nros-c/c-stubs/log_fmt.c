/*
 * Phase 88.12 — C shim for `nros_log_emit_fmt`.
 *
 * Rust's `c_variadic` feature is still unstable on stable, so the
 * printf-style entry point is implemented in C: it `vsnprintf`s into
 * a 256-byte stack buffer and forwards to the Rust-side
 * `nros_log_emit` (defined in `src/log.rs`).
 *
 * Compiled by nros-c's `build.rs` via cc-rs; linked into the
 * nros-c staticlib alongside the Rust object files.
 */

#include <nros/log.h>

#include <stdarg.h>
#include <stddef.h>
#include <stdio.h>

void nros_log_emit_fmt(nros_logger_t logger,
                       nros_log_severity_t severity,
                       const char *fmt,
                       ...) {
    if (logger == NULL || fmt == NULL) {
        return;
    }
    /* 256-byte default matches nros-log's `buffer-size-256` facade
     * default. Overflow truncates + appends "..." in-place. */
    char buf[256];
    va_list args;
    va_start(args, fmt);
    int written = vsnprintf(buf, sizeof(buf), fmt, args);
    va_end(args);
    if (written <= 0) {
        return;
    }
    size_t n = (size_t) written;
    if (n >= sizeof(buf)) {
        /* `vsnprintf` returns the would-have-written length on
         * overflow but writes at most `sizeof(buf) - 1` bytes.
         * Append "..." inside the buffer to mark truncation. */
        n = sizeof(buf) - 1;
        if (n >= 3) {
            buf[n - 3] = '.';
            buf[n - 2] = '.';
            buf[n - 1] = '.';
        }
    }
    nros_log_emit(logger, severity, buf, n);
}
