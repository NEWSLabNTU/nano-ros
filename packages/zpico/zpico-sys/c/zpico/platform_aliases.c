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

/* -------------------------------------------------------------------------
 *  Yield — direct alias.
 * ----------------------------------------------------------------------- */

void z_yield(void) {
    nros_platform_yield_now();
}

/* -------------------------------------------------------------------------
 *  Threading: tasks. zenoh-pico passes a `_z_task_t *` whose layout is
 *  opaque caller storage. nros_platform_task_init takes a `void *` for
 *  the same purpose — direct pass-through.
 * ----------------------------------------------------------------------- */

int8_t _z_task_init(void *task, void *attr, void *(*entry)(void *), void *arg) {
    return nros_platform_task_init(task, attr, entry, arg);
}

int8_t _z_task_join(void *task) {
    return nros_platform_task_join(task);
}

int8_t _z_task_detach(void *task) {
    return nros_platform_task_detach(task);
}

int8_t _z_task_cancel(void *task) {
    return nros_platform_task_cancel(task);
}

void _z_task_exit(void) {
    nros_platform_task_exit();
}

void _z_task_free(void **task) {
    nros_platform_task_free(task);
}

/* -------------------------------------------------------------------------
 *  Threading: non-recursive mutex.
 * ----------------------------------------------------------------------- */

int8_t _z_mutex_init(void *m) {
    return nros_platform_mutex_init(m);
}

int8_t _z_mutex_drop(void *m) {
    return nros_platform_mutex_drop(m);
}

int8_t _z_mutex_lock(void *m) {
    return nros_platform_mutex_lock(m);
}

int8_t _z_mutex_try_lock(void *m) {
    return nros_platform_mutex_try_lock(m);
}

int8_t _z_mutex_unlock(void *m) {
    return nros_platform_mutex_unlock(m);
}

/* -------------------------------------------------------------------------
 *  Threading: recursive mutex.
 * ----------------------------------------------------------------------- */

int8_t _z_mutex_rec_init(void *m) {
    return nros_platform_mutex_rec_init(m);
}

int8_t _z_mutex_rec_drop(void *m) {
    return nros_platform_mutex_rec_drop(m);
}

int8_t _z_mutex_rec_lock(void *m) {
    return nros_platform_mutex_rec_lock(m);
}

int8_t _z_mutex_rec_try_lock(void *m) {
    return nros_platform_mutex_rec_try_lock(m);
}

int8_t _z_mutex_rec_unlock(void *m) {
    return nros_platform_mutex_rec_unlock(m);
}

/* -------------------------------------------------------------------------
 *  Threading: condition variables. `wait_until` takes an
 *  `(secs, nanos)` deadline in zenoh-pico's API — collapse to the
 *  monotonic-millisecond deadline the platform ABI uses.
 * ----------------------------------------------------------------------- */

int8_t _z_condvar_init(void *cv) {
    return nros_platform_condvar_init(cv);
}

int8_t _z_condvar_drop(void *cv) {
    return nros_platform_condvar_drop(cv);
}

int8_t _z_condvar_signal(void *cv) {
    return nros_platform_condvar_signal(cv);
}

int8_t _z_condvar_signal_all(void *cv) {
    return nros_platform_condvar_signal_all(cv);
}

int8_t _z_condvar_wait(void *cv, void *m) {
    return nros_platform_condvar_wait(cv, m);
}

/* -------------------------------------------------------------------------
 *  `_z_condvar_wait_until` — only safe to emit when the build defined
 *  `NROS_PLATFORM_ALIASES`, which forces zenoh-pico to use the
 *  generic platform header (`nros_zenoh_generic_platform.h`) that
 *  types `z_clock_t = uint64_t` ms. Without the generic header,
 *  per-platform `z_clock_t` layouts (`struct timespec` on POSIX,
 *  `TickType_t` on FreeRTOS orin-spe) make a generic wrapper
 *  unsafe. Vendor `system/<rtos>/system.c` provides the symbol in
 *  that mode.
 * ----------------------------------------------------------------------- */

#ifdef NROS_PLATFORM_ALIASES

int8_t _z_condvar_wait_until(void *cv, void *m, const uint64_t *abstime_ms) {
    if (abstime_ms == NULL) {
        return nros_platform_condvar_wait(cv, m);
    }
    return nros_platform_condvar_wait_until(cv, m, *abstime_ms);
}

/* -------------------------------------------------------------------------
 *  Clock / monotonic-time variants. Vendor `<system/common/platform.h>`
 *  declares them with per-platform `z_clock_t` / `z_time_t`. Same
 *  generic-header gate as `_z_condvar_wait_until`.
 * ----------------------------------------------------------------------- */

uint64_t z_clock_now(void) {
    return nros_platform_time_now_ms();
}

unsigned long z_clock_elapsed_us(const uint64_t *clock) {
    if (clock == NULL) return 0;
    uint64_t now = nros_platform_time_now_ms();
    return (unsigned long) ((now - *clock) * 1000ULL);
}

unsigned long z_clock_elapsed_ms(const uint64_t *clock) {
    if (clock == NULL) return 0;
    uint64_t now = nros_platform_time_now_ms();
    return (unsigned long) (now - *clock);
}

unsigned long z_clock_elapsed_s(const uint64_t *clock) {
    if (clock == NULL) return 0;
    uint64_t now = nros_platform_time_now_ms();
    return (unsigned long) ((now - *clock) / 1000ULL);
}

void z_clock_advance_us(uint64_t *clock, unsigned long duration) {
    if (clock == NULL) return;
    *clock += (uint64_t) duration / 1000ULL;
}

void z_clock_advance_ms(uint64_t *clock, unsigned long duration) {
    if (clock == NULL) return;
    *clock += (uint64_t) duration;
}

void z_clock_advance_s(uint64_t *clock, unsigned long duration) {
    if (clock == NULL) return;
    *clock += (uint64_t) duration * 1000ULL;
}

/* -------------------------------------------------------------------------
 *  Networking. Wraps zenoh-pico's `_z_open_tcp` / `_z_open_udp_*` /
 *  socket helpers on top of `nros_platform_{tcp,udp,udp_mcast,
 *  socket}_*`. The opaque storage typedefs come from the generic
 *  platform header; the platform impl knows the real layout.
 *
 *  Endpoints are passed BY VALUE in zenoh-pico's per-link
 *  signatures, but the `nros_platform_*` ABI takes a pointer.
 *  Stack-allocate then `&` to satisfy the contract.
 * ----------------------------------------------------------------------- */

#include "nros/platform_net.h"
#include "nros_zenoh_generic_platform.h"

/* The vendor `_z_sys_net_socket_t` / `_z_sys_net_endpoint_t`
 * typedefs live in the generic platform header. The alias TU
 * uses fixed-size opaque storage `uint8_t[N]` matching
 * `nros_zenoh_generic_platform.h` — the linker doesn't care
 * about the typedef name, only the ABI size + by-value vs.
 * pointer convention. Inlining keeps the alias TU
 * header-independent. */
typedef struct {
    uint8_t _opaque[NROS_ZP_NET_SOCKET_STORAGE_BYTES];
} nros_zp_alias_socket_t;
typedef struct {
    uint8_t _opaque[NROS_ZP_NET_ENDPOINT_STORAGE_BYTES];
} nros_zp_alias_endpoint_t;

/* TCP. */
int8_t _z_create_endpoint_tcp(void *ep, const uint8_t *address, const uint8_t *port) {
    return nros_platform_tcp_create_endpoint(ep, address, port);
}
void _z_free_endpoint_tcp(void *ep) {
    nros_platform_tcp_free_endpoint(ep);
}
int8_t _z_open_tcp(void *sock, nros_zp_alias_endpoint_t rep, uint32_t tout) {
    return nros_platform_tcp_open(sock, &rep, tout);
}
int8_t _z_listen_tcp(void *sock, nros_zp_alias_endpoint_t rep) {
    return nros_platform_tcp_listen(sock, &rep);
}
void _z_close_tcp(void *sock) {
    nros_platform_tcp_close(sock);
}
size_t _z_read_tcp(nros_zp_alias_socket_t sock, uint8_t *buf, size_t len) {
    return nros_platform_tcp_read(&sock, buf, len);
}
size_t _z_read_exact_tcp(nros_zp_alias_socket_t sock, uint8_t *buf, size_t len) {
    return nros_platform_tcp_read_exact(&sock, buf, len);
}
size_t _z_send_tcp(nros_zp_alias_socket_t sock, const uint8_t *buf, size_t len) {
    return nros_platform_tcp_send(&sock, buf, len);
}

/* UDP unicast. */
int8_t _z_create_endpoint_udp(void *ep, const uint8_t *address, const uint8_t *port) {
    return nros_platform_udp_create_endpoint(ep, address, port);
}
void _z_free_endpoint_udp(void *ep) {
    nros_platform_udp_free_endpoint(ep);
}
int8_t _z_open_udp_unicast(void *sock, nros_zp_alias_endpoint_t rep, uint32_t tout) {
    return nros_platform_udp_open(sock, &rep, tout);
}
int8_t _z_listen_udp_unicast(void *sock, nros_zp_alias_endpoint_t rep, uint32_t tout) {
    return nros_platform_udp_listen(sock, &rep, tout);
}
void _z_close_udp_unicast(void *sock) {
    nros_platform_udp_close(sock);
}
size_t _z_read_udp_unicast(nros_zp_alias_socket_t sock, uint8_t *buf, size_t len) {
    return nros_platform_udp_read(&sock, buf, len);
}
size_t _z_read_exact_udp_unicast(nros_zp_alias_socket_t sock, uint8_t *buf, size_t len) {
    return nros_platform_udp_read_exact(&sock, buf, len);
}
size_t _z_send_udp_unicast(nros_zp_alias_socket_t sock, const uint8_t *buf, size_t len,
                           nros_zp_alias_endpoint_t rep) {
    return nros_platform_udp_send(&sock, buf, len, &rep);
}

/* Socket helpers. */
int8_t _z_socket_set_non_blocking(const void *sock) {
    return nros_platform_socket_set_non_blocking(sock);
}
int8_t _z_socket_accept(const void *in_sock, void *out_sock) {
    return nros_platform_socket_accept(in_sock, out_sock);
}
void _z_socket_close(void *sock) {
    nros_platform_socket_close(sock);
}
int8_t _z_socket_wait_event(void *peers, void *mutex) {
    return nros_platform_socket_wait_event(peers, mutex);
}

#endif /* NROS_PLATFORM_ALIASES */
