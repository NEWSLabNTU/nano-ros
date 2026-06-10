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

#include <stdbool.h>
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
 *  Memory-only mode (RFC-0034 / phase-230 1c). On an RTOS that vendors its
 *  own sleep / random / threading / net primitives (FreeRTOS:
 *  `system/freertos/system.c` + `lwip/network.c`), emit ONLY the three
 *  memory forwarders above — the scalar heap is the single service we
 *  funnel first, while sleep/random/mutex/condvar/task/net stay vendored
 *  (emitting alias copies would duplicate the vendor's strong symbols).
 *  `NROS_ZP_ALIAS_MEMORY_ONLY` (set by `nros-zpico-build` for FreeRTOS)
 *  drops everything below; the vendored `z_malloc`/`z_realloc`/`z_free` are
 *  meanwhile guarded out by `Z_FEATURE_NROS_PLATFORM_ALLOC`, so only these
 *  three forwarders remain on the link. Wave 2 extends the funnel to
 *  sleep/random behind the same pattern.
 * ----------------------------------------------------------------------- */
#ifndef NROS_ZP_ALIAS_MEMORY_ONLY

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
 *  smoltcp bridge — used by `nros-smoltcp/src/bridge.rs` to read
 *  millisecond wall-clock from any platform (bare-metal smoltcp
 *  driver) without taking a Rust trait dep on `PlatformClock`.
 *  Boards that ship their own bare-metal `smoltcp_clock_now_ms`
 *  emitter (e.g. `nros-board-mps2-an385`) must NOT enable
 *  `platform-aliases` on `zpico-sys` to avoid double-define.
 * ----------------------------------------------------------------------- */

uint64_t smoltcp_clock_now_ms(void) {
    return nros_platform_time_now_ms();
}

/* -------------------------------------------------------------------------
 *  Threading: tasks. zenoh-pico passes a `_z_task_t *` whose layout is
 *  opaque caller storage. nros_platform_task_init takes a `void *` for
 *  the same purpose — direct pass-through.
 *
 *  Phase 146.1 — ThreadX must provide its own `_z_task_*` symbols
 *  (`c/platform/threadx/task.c`) because the `_z_task_t` layout
 *  embeds a `TX_THREAD` + stack + entry/arg fields that the trampoline
 *  recovers via `tx_thread_identify()`. Skip the generic alias-TU
 *  versions under `NROS_PLATFORM_ALIASES_SKIP_TASK` so both TUs can
 *  coexist in the same `zpico_sys` rlib without a duplicate-symbol
 *  link error.
 * ----------------------------------------------------------------------- */

#ifndef NROS_PLATFORM_ALIASES_SKIP_TASK

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

#endif  /* NROS_PLATFORM_ALIASES_SKIP_TASK */

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
 *
 *  Phase 160.L.1 — `z_clock_*` is the *monotonic* clock in zenoh-pico's
 *  contract (`unix/system.c:247` uses `CLOCK_MONOTONIC`; `z_time_now`
 *  is the wall-clock variant). The alias TU previously routed
 *  `z_clock_now` through `nros_platform_time_now_ms` (wall-clock /
 *  CLOCK_REALTIME on POSIX). That meant
 *  `zpico_spin_once`'s cv-deadline (`z_clock_now() + 100 ms`) was a
 *  REALTIME-epoch number (~1.78e15 ms in 2026), but
 *  `nros_platform_condvar_wait_until` interpreted the deadline against
 *  `nros_platform_clock_ms` (CLOCK_MONOTONIC, ~uptime ms). Subtracting
 *  the two yielded `rel_ms ≈ 55 YEARS`; pthread_cond_timedwait then
 *  blocked forever and the executor's spin_period stalled before
 *  the first timer callback could fire — root cause of the C/C++
 *  native talker regression (Phase 160.L cluster).
 *
 *  Fix: route the monotonic clock variants through
 *  `nros_platform_clock_ms` so the value matches what
 *  `nros_platform_condvar_wait_until` expects. `z_time_*` (wall
 *  clock) — if/when added to the alias TU — should keep
 *  `nros_platform_time_now_ms`.
 * ----------------------------------------------------------------------- */

uint64_t z_clock_now(void) {
    return nros_platform_clock_ms();
}

unsigned long z_clock_elapsed_us(const uint64_t *clock) {
    if (clock == NULL) return 0;
    uint64_t now = nros_platform_clock_ms();
    return (unsigned long) ((now - *clock) * 1000ULL);
}

unsigned long z_clock_elapsed_ms(const uint64_t *clock) {
    if (clock == NULL) return 0;
    uint64_t now = nros_platform_clock_ms();
    return (unsigned long) (now - *clock);
}

unsigned long z_clock_elapsed_s(const uint64_t *clock) {
    if (clock == NULL) return 0;
    uint64_t now = nros_platform_clock_ms();
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

/* Phase 160 — alias TU's networking struct shapes MUST match the
 * vendor's view of `_z_sys_net_*_t` for the platform being built.
 * Two distinct shapes exist:
 *
 *  - Bare-metal (gated `NROS_ZP_ALIAS_BARE_METAL_NET`): vendor compiles
 *    against `c/platform/bare-metal/platform.h`'s 6-byte endpoint
 *    + 2-byte socket. RV32 / Cortex-M3 pass-by-value ABI puts these
 *    inline in arg registers; if the alias TU declared a 16-byte
 *    opaque the call site would treat register slot as a pointer →
 *    fault. Used on ESP32-C3, qemu-arm-baremetal, stm32f4.
 *
 *  - Opaque 16/32-byte (gated `NROS_ZP_ALIAS_OPAQUE_NET`): vendor
 *    compiles against `nros_zenoh_generic_platform.h` (because
 *    `NROS_PLATFORM_ALIASES` is defined). Both sides see the same
 *    opaque storage cap so by-value pass uses hidden-pointer ABI
 *    consistently. Used on ThreadX (vendor doesn't ship its own
 *    network.c in extra_sources; alias TU is the sole provider).
 *
 *  POSIX, NuttX, Zephyr, FreeRTOS use neither gate — each ships its
 *  vendor `system/<rtos>/network.c` and the alias TU's network
 *  section is `#ifdef`-elided. */
#if defined(NROS_ZP_ALIAS_BARE_METAL_NET)
typedef struct {
    union {
        int8_t _handle;
        int8_t _fd;
    };
    bool _connected;
} nros_zp_alias_socket_t;
typedef struct {
    uint8_t _ip[4];
    uint16_t _port;
} nros_zp_alias_endpoint_t;
#elif defined(NROS_ZP_ALIAS_OPAQUE_NET)
typedef struct {
    uint8_t _opaque[NROS_ZP_NET_SOCKET_STORAGE_BYTES];
} nros_zp_alias_socket_t;
typedef struct {
    uint8_t _opaque[NROS_ZP_NET_ENDPOINT_STORAGE_BYTES];
} nros_zp_alias_endpoint_t;
#endif

/* Phase 160 — compile-time drift guard. `build.rs` extracts the
 * vendor sizes from `size_probe.c` (built against bare-metal/
 * platform.h) and passes them as `-D` defines. If vendor flips
 * `_z_sys_net_socket_t` to embed a TLS pointer or extends the
 * endpoint to carry an interface tag, this assert trips at
 * compile time instead of crashing at runtime with `lhu a2, 2(a1)`
 * on a register that holds an inline-packed IP value. The
 * defines are only present on the bare-metal alias build (other
 * platforms gate the whole network section off via
 * `NROS_ZP_ALIAS_BARE_METAL_NET`). */
#if defined(NROS_ZP_VENDOR_NET_SOCKET_SIZE) \
    && defined(NROS_ZP_VENDOR_NET_ENDPOINT_SIZE)
_Static_assert(sizeof(nros_zp_alias_socket_t)
                   == NROS_ZP_VENDOR_NET_SOCKET_SIZE,
               "alias TU's nros_zp_alias_socket_t drifted from vendor "
               "_z_sys_net_socket_t — pass-by-value ABI will corrupt");
_Static_assert(sizeof(nros_zp_alias_endpoint_t)
                   == NROS_ZP_VENDOR_NET_ENDPOINT_SIZE,
               "alias TU's nros_zp_alias_endpoint_t drifted from vendor "
               "_z_sys_net_endpoint_t — pass-by-value ABI will corrupt");
#endif

/* Phase 156 (option B) — POSIX consumers use zenoh-pico's
 * upstream `src/system/unix/network.c` for the network impls
 * so the `_z_sys_net_socket_t = { int _fd; }` (4 bytes, from
 * `unix.h`) layout matches the by-value socket arg the call
 * sites in `tx.c` push (4-byte int in one register). Without
 * the skip, this alias TU defines the same symbols with a
 * 32-byte opaque struct (from `nros_zenoh_generic_platform.h`),
 * `--allow-multiple-definition` picks the wrong copy at
 * link time, and `_z_send_tcp` reads garbage off the stack
 * (`fd=0 (stdin)`, nonsense `len`, EAGAIN/ENOTSOCK on send).
 * Pointer-shaped aliases (threading/mutex/condvar/clock)
 * stay active on POSIX since pointer ABI is uniform across
 * struct sizes. */
#if defined(NROS_ZP_ALIAS_BARE_METAL_NET) || defined(NROS_ZP_ALIAS_OPAQUE_NET)

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

/* UDP multicast (Phase 134.7).
 *
 * Bridges the `_z_*_udp_multicast` symbols (called from upstream
 * zenoh-pico's `link/multicast/udp.c`) to the canonical
 * `nros_platform_udp_mcast_*` ABI declared in <nros/platform_net.h>.
 * Same opaque-storage idiom as the unicast aliases above. `addr` is
 * declared `void *` because `_z_slice_t` is a zenoh-pico-internal
 * type the alias TU cannot pull in without a circular include — the
 * canonical platform fn signature also takes `void *` for this
 * parameter, so the cast is byte-for-byte.
 *
 * Pre-134 the CMake POSIX path deleted `src/system/unix/network.c`
 * from the build copy and relied on `nros-platform-cffi` providing
 * the impls via these aliases. The multicast bridge was never wired
 * — `libnros_rmw_zenoh.a` shipped the `_z_f_link_*_udp_multicast`
 * wrappers compiled against the canonical header's
 * `Z_FEATURE_LINK_UDP_MULTICAST=1` but the `_z_*_udp_multicast`
 * impls were undefined, breaking every C / C++ native link.
 */
int8_t _z_open_udp_multicast(void *sock,
                             nros_zp_alias_endpoint_t rep,
                             void *lep,
                             uint32_t tout,
                             const char *iface) {
    return nros_platform_udp_mcast_open(sock, &rep, lep, tout,
                                        (const uint8_t *)iface);
}
int8_t _z_listen_udp_multicast(void *sock,
                               nros_zp_alias_endpoint_t rep,
                               uint32_t tout,
                               const char *iface,
                               const char *join) {
    return nros_platform_udp_mcast_listen(sock, &rep, tout,
                                          (const uint8_t *)iface,
                                          (const uint8_t *)join);
}
void _z_close_udp_multicast(void *sockrecv,
                            void *socksend,
                            nros_zp_alias_endpoint_t rep,
                            nros_zp_alias_endpoint_t lep) {
    nros_platform_udp_mcast_close(sockrecv, socksend, &rep, &lep);
}
size_t _z_read_udp_multicast(nros_zp_alias_socket_t sock,
                             uint8_t *ptr, size_t len,
                             nros_zp_alias_endpoint_t lep,
                             void *addr) {
    return nros_platform_udp_mcast_read(&sock, ptr, len, &lep, addr);
}
size_t _z_read_exact_udp_multicast(nros_zp_alias_socket_t sock,
                                   uint8_t *ptr, size_t len,
                                   nros_zp_alias_endpoint_t lep,
                                   void *addr) {
    return nros_platform_udp_mcast_read_exact(&sock, ptr, len, &lep, addr);
}
size_t _z_send_udp_multicast(nros_zp_alias_socket_t sock,
                             const uint8_t *ptr, size_t len,
                             nros_zp_alias_endpoint_t rep) {
    return nros_platform_udp_mcast_send(&sock, ptr, len, &rep);
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

#endif /* NROS_ZP_ALIAS_BARE_METAL_NET || NROS_ZP_ALIAS_OPAQUE_NET — Phase 160 */

/* Serial transport.
 *
 * Phase 152 — minimum stubs so `libnros_rmw_zenoh_staticlib.a` (which
 * keeps `Z_FEATURE_LINK_SERIAL=1` to satisfy zenoh-pico's link-wrapper
 * layer in `src/link/unicast/serial.c` + `src/system/common/serial.c`)
 * links cleanly against platform_aliases.c. Stubs return `-1` / `0`
 * so the serial transport is reachable but non-functional at runtime;
 * POSIX consumers use TCP / UDP scouting which is rmw_zenoh's default.
 *
 * Functional POSIX serial via termios is straightforward (~150 LOC)
 * but no consumer has requested it. RTOS / bare-metal targets that
 * actually use serial supply their own impls via per-board shims
 * (e.g. `nros-platform-{freertos,nuttx,threadx}` board crates).
 *
 * Mirrors Phase 134's UDP-multicast stub pattern. */
__attribute__((weak)) int8_t _z_open_serial_from_pins(void *sock, uint32_t txpin, uint32_t rxpin,
                                uint32_t baudrate) {
    (void)sock;
    (void)txpin;
    (void)rxpin;
    (void)baudrate;
    return -1;
}
__attribute__((weak)) int8_t _z_open_serial_from_dev(void *sock, char *dev, uint32_t baudrate) {
    (void)sock;
    (void)dev;
    (void)baudrate;
    return -1;
}
__attribute__((weak)) int8_t _z_listen_serial_from_pins(void *sock, uint32_t txpin, uint32_t rxpin,
                                  uint32_t baudrate) {
    (void)sock;
    (void)txpin;
    (void)rxpin;
    (void)baudrate;
    return -1;
}
__attribute__((weak)) int8_t _z_listen_serial_from_dev(void *sock, char *dev, uint32_t baudrate) {
    (void)sock;
    (void)dev;
    (void)baudrate;
    return -1;
}
__attribute__((weak)) void _z_close_serial(void *sock) {
    (void)sock;
}
__attribute__((weak)) size_t _z_send_serial_internal(void *sock, const uint8_t *buf, size_t len) {
    (void)sock;
    (void)buf;
    (void)len;
    return 0;
}
__attribute__((weak)) size_t _z_read_serial_internal(void *sock, uint8_t *buf, size_t len) {
    (void)sock;
    (void)buf;
    (void)len;
    return 0;
}

/* Weak stubs for legacy smoltcp_init / smoltcp_cleanup hooks. zpico.c
 * still calls these inside `#ifdef ZPICO_SMOLTCP` blocks (added pre-Phase
 * 80 when the smoltcp glue lived inside zpico-sys's own `platform_smoltcp.rs`).
 * After Phase 80 carved the smoltcp ↔ zenoh-pico bridge into the standalone
 * `nros-smoltcp` crate the symbols moved to `nros_smoltcp_init` and the
 * board's `define_network_state!` macro handles init/teardown directly,
 * so these zpico.c calls are no-ops. Weak stubs let bare-metal builds
 * link; a board crate may override by defining its own non-weak
 * `smoltcp_init` / `smoltcp_cleanup` if needed. */
__attribute__((weak)) int32_t smoltcp_init(void) {
    return 0;
}
__attribute__((weak)) void smoltcp_cleanup(void) {}

#endif /* NROS_PLATFORM_ALIASES */

#endif /* !NROS_ZP_ALIAS_MEMORY_ONLY — phase-230 1c memory-only funnel */
