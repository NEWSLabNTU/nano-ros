/*
 * Phase 88.12 — C shim for `rclc_log_emit_fmt`.
 *
 * Rust's `c_variadic` feature is still unstable on stable, so the
 * printf-style entry point is implemented in C: it `vsnprintf`s into
 * a 256-byte stack buffer and forwards to the Rust-side
 * `rclc_log_emit` (defined in `src/log.rs`).
 *
 * Compiled by nros-c's `build.rs` via cc-rs; linked into the
 * nros-c staticlib alongside the Rust object files.
 */

#include <rclc/log.h>

#include <stdarg.h>
#include <stddef.h>
#include <stdint.h>
#include <stdio.h>

/* Render `fmt` into `buf`, marking truncation with "..." in place, and
 * return the length to emit (0 = nothing to emit). Shared by both entry
 * points below so they truncate identically. */
static size_t render(char* buf, size_t cap, const char* fmt, va_list args) {
    int written = vsnprintf(buf, cap, fmt, args);
    if (written <= 0) {
        return 0;
    }
    size_t n = (size_t)written;
    if (n >= cap) {
        /* `vsnprintf` returns the would-have-written length on overflow but
         * writes at most `cap - 1` bytes. Append "..." inside the buffer. */
        n = cap - 1;
        if (n >= 3) {
            buf[n - 3] = '.';
            buf[n - 2] = '.';
            buf[n - 1] = '.';
        }
    }
    return n;
}

/* phase-417 W4.d — the located entry point every `RCLC_LOG_*` macro expands
 * to. It was `static inline` in <rclc/log.h> until phase-492 moved it here so
 * that header needs no `<stdio.h>`. */
void rclc_log_emit_fmt_at(rclc_logger_t logger, rcl_log_severity_t severity, const char* file,
                          uint32_t line, const char* fmt, ...) {
    if (logger == NULL || fmt == NULL) {
        return;
    }
    char buf[NROS_LOG_FMT_BUFFER_SIZE];
    va_list args;
    va_start(args, fmt);
    size_t n = render(buf, sizeof(buf), fmt, args);
    va_end(args);
    if (n > 0) {
        rclc_log_emit_at(logger, severity, buf, n, file, line);
    }
}

void rclc_log_emit_fmt(rclc_logger_t logger,
                       rcl_log_severity_t severity,
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
    size_t n = render(buf, sizeof(buf), fmt, args);
    va_end(args);
    if (n == 0) {
        return;
    }
    rclc_log_emit(logger, severity, buf, n);
}
