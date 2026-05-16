/*
 * Phase 128.D.3 — folded subset of zpico-platform-shim.
 *
 * Maps the zenoh-pico-named symbols (`z_malloc`, `z_sleep_ms`,
 * `z_random_*`, `z_time_now`, …) to the canonical `nros_platform_*`
 * ABI declared in <nros/platform.h>. Lets a consumer that links
 * nros-platform-cffi (or any provider that exposes the canonical
 * symbols) drop the `zpico-platform-shim` rlib for the small
 * stateless helpers below.
 *
 * Out of scope here (still lives in `zpico-platform-shim`):
 *
 *   - Threading primitives (`_z_task_*`, `_z_mutex_*`, `_z_condvar_*`)
 *     because zenoh-pico declares them with opaque `[u8; N]` storage
 *     that does not match `nros_platform_task_t` shapes 1:1.
 *   - smoltcp / serial / IVC bridge hooks (`smoltcp_clock_now_ms`,
 *     `_z_open_serial_from_dev`, IVC helpers) — per-board / per-rtos.
 *
 * Linkage discipline:
 *
 *   - This TU is opt-in via `CARGO_FEATURE_PLATFORM_ALIASES` (set by
 *     the `platform-aliases` feature on `zpico-sys`). When the
 *     consumer enables both this and `zpico-platform-shim`, the
 *     `--allow-multiple-definition` flag the nros-c link layer
 *     already passes silently wins for the shim copy (no behavioural
 *     change). Disabling `zpico-platform-shim`'s `active` feature
 *     leaves only the aliases.
 */

#include <stddef.h>
#include <stdint.h>

#include "nros/platform.h"

/* -------------------------------------------------------------------------
 *  Memory — direct alias (signatures match).
 * ----------------------------------------------------------------------- */

void *z_malloc(size_t size) {
    return nros_platform_alloc(size);
}

void *z_realloc(void *ptr, size_t size) {
    return nros_platform_realloc(ptr, size);
}

void z_free(void *ptr) {
    nros_platform_dealloc(ptr);
}

/* -------------------------------------------------------------------------
 *  Sleep — wrapper (z_sleep_* returns int8_t, nros_platform_sleep_*
 *  returns void).
 * ----------------------------------------------------------------------- */

int8_t z_sleep_us(size_t time) {
    nros_platform_sleep_us(time);
    return 0;
}

int8_t z_sleep_ms(size_t time) {
    nros_platform_sleep_ms(time);
    return 0;
}

int8_t z_sleep_s(size_t time) {
    nros_platform_sleep_s(time);
    return 0;
}

/* -------------------------------------------------------------------------
 *  Random — direct alias (signatures match).
 * ----------------------------------------------------------------------- */

uint8_t z_random_u8(void) {
    return nros_platform_random_u8();
}

uint16_t z_random_u16(void) {
    return nros_platform_random_u16();
}

uint32_t z_random_u32(void) {
    return nros_platform_random_u32();
}

uint64_t z_random_u64(void) {
    return nros_platform_random_u64();
}

void z_random_fill(void *buf, size_t len) {
    nros_platform_random_fill(buf, len);
}

/* -------------------------------------------------------------------------
 *  Wall-clock time — wrapper. zenoh-pico's `z_time_now()` returns a
 *  64-bit ms count compatible with `nros_platform_time_now_ms()`.
 *  `_z_get_time_since_epoch` writes a struct, matched here.
 * ----------------------------------------------------------------------- */

uint64_t z_time_now(void) {
    return nros_platform_time_now_ms();
}

uint64_t z_time_elapsed_us(const uint64_t *time) {
    uint64_t now = nros_platform_time_now_ms();
    return (now - *time) * 1000ULL;
}

uint64_t z_time_elapsed_ms(const uint64_t *time) {
    uint64_t now = nros_platform_time_now_ms();
    return now - *time;
}

uint64_t z_time_elapsed_s(const uint64_t *time) {
    uint64_t now = nros_platform_time_now_ms();
    return (now - *time) / 1000ULL;
}

struct nros_z_time_since_epoch {
    uint32_t secs;
    uint32_t nanos;
};

int8_t _z_get_time_since_epoch(struct nros_z_time_since_epoch *t) {
    if (t == NULL) {
        return -1;
    }
    t->secs = nros_platform_time_since_epoch_secs();
    t->nanos = nros_platform_time_since_epoch_nanos();
    return 0;
}
