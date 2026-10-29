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
 *     that does not match ~~`nros_platform_task_t`~~ shapes 1:1.
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
 *  Scalar services — memory (above) + sleep + random. These are pure
 *  scalars (no host-type in the ABI), so they are ALWAYS emitted, including
 *  in the FreeRTOS `NROS_ZP_ALIAS_SCALAR_ONLY` mode. The vendored FreeRTOS
 *  copies are guarded out by `Z_FEATURE_NROS_PLATFORM_{ALLOC,SLEEP,RANDOM}`
 *  (RFC-0034 / phase-230 1c + Wave 2), so exactly these forwarders remain on
 *  the link. The non-scalar services below (wall-clock z_time, threading,
 *  net) carry per-platform layouts and stay vendored on FreeRTOS — they are
 *  dropped by `NROS_ZP_ALIAS_SCALAR_ONLY` (see the guard after `z_random`).
 * ----------------------------------------------------------------------- */

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
 *  Non-scalar services below (wall-clock z_time, threading, net). On an RTOS
 *  that vendors these (FreeRTOS: `system/freertos/system.c` clock/threading +
 *  `lwip/network.c`), `NROS_ZP_ALIAS_SCALAR_ONLY` (set by `nros-zpico-build`
 *  for FreeRTOS) drops them so they do not duplicate the vendor's strong
 *  symbols. `z_time`/`z_clock` stay vendored because `z_clock_t` is the
 *  FreeRTOS `TickType_t`, not a portable scalar.
 * ----------------------------------------------------------------------- */
#ifndef NROS_ZP_ALIAS_SCALAR_ONLY

/* -------------------------------------------------------------------------
 *  Wall-clock time — wrapper. zenoh-pico's `z_time_now()` returns a
 *  64-bit ms count; the platform ABI is nanoseconds since issue 0532 item 5,
 *  so these divide. `_z_get_time_since_epoch` writes a struct, matched here.
 *
 *  This is the shape the old `PlatformTime` doc used to justify keeping THREE
 *  symbols ("the shim forwards each directly to a `_z_time_*` symbol"). That
 *  argument was against returning a STRUCT, and it still holds — but one u64
 *  is not a struct: the pair below comes from a single read plus a divide and
 *  a remainder, which is cheaper than two FFI calls AND cannot pair an old
 *  second with a new sub-second remainder the way the split could.
 * ----------------------------------------------------------------------- */

static uint64_t nros_zp_now_ms(void) {
    return nros_platform_time_now_ns() / 1000000ULL;
}

/* issue 1636 — `NROS_PLATFORM_ALIASES_SKIP_SYSTEM`: the port compiles its OWN
 * C system ABI (Zephyr: `zephyr/nros_zenoh_zephyr_system.c`, POSIX-shaped
 * `pthread_*` / `struct timespec` types), so the generic time, clock, task,
 * mutex and condvar wrappers here would be a SECOND definition of every one of
 * those 33 symbols. Measured on native_sim with `--allow-multiple-definition`
 * removed: all 33 collided, and only link order picked the Zephyr copy. */
#ifndef NROS_PLATFORM_ALIASES_SKIP_SYSTEM
uint64_t z_time_now(void) {
    return nros_zp_now_ms();
}

uint64_t z_time_elapsed_us(const uint64_t *time) {
    uint64_t now = nros_zp_now_ms();
    return (now - *time) * 1000ULL;
}

uint64_t z_time_elapsed_ms(const uint64_t *time) {
    uint64_t now = nros_zp_now_ms();
    return now - *time;
}

uint64_t z_time_elapsed_s(const uint64_t *time) {
    uint64_t now = nros_zp_now_ms();
    return (now - *time) / 1000ULL;
}
#endif /* !NROS_PLATFORM_ALIASES_SKIP_SYSTEM */

struct nros_z_time_since_epoch {
    uint32_t secs;
    uint32_t nanos;
};

int8_t _z_get_time_since_epoch(struct nros_z_time_since_epoch *t) {
    if (t == NULL) {
        return -1;
    }
    const uint64_t ns = nros_platform_time_now_ns();
    t->secs = (uint32_t) (ns / 1000000000ULL);
    t->nanos = (uint32_t) (ns % 1000000000ULL);
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
    return nros_zp_now_ms();
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

#if !defined(NROS_PLATFORM_ALIASES_SKIP_TASK) && !defined(NROS_PLATFORM_ALIASES_SKIP_SYSTEM)

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
 *  Threading: task identity — zenoh-pico 1.9's executor (RFC-0110).
 *
 *  The background executor asks "is the calling thread the executor
 *  thread?" before it takes its own lock, so the system ABI gained
 *  `_z_task_get_id` / `_z_task_current_id` / `_z_task_id_equal`. Their
 *  TYPE is fixed by the vendor platform header the build selects, not by
 *  this TU: on POSIX (and NuttX) that is `unix.h`, where `_z_task_t` and
 *  `_z_task_id_t` are both `pthread_t` — and `nros_platform_task_init`
 *  on those ports stores exactly a `pthread_t` in the task storage. So the
 *  identity is pthread's own, spelled here under a define the build sets
 *  for POSIX only (`NROS_ZP_ALIAS_PTHREAD_TASK_ID`) — NuttX compiles this TU
 *  without its own headers, so its trio is in `nuttx_task_id.c`. A port
 *  with its own `_z_task_t` layout (ThreadX `task.c`, Zephyr's system TU,
 *  FreeRTOS's vendor `system.c`) defines these beside its `_z_task_init`.
 * ----------------------------------------------------------------------- */

#if defined(NROS_ZP_ALIAS_PTHREAD_TASK_ID)
#include <pthread.h>

pthread_t _z_task_get_id(const pthread_t *task) {
    return *task;
}

pthread_t _z_task_current_id(void) {
    return pthread_self();
}

bool _z_task_id_equal(const pthread_t *l, const pthread_t *r) {
    return pthread_equal(*l, *r) != 0;
}
#endif /* NROS_ZP_ALIAS_PTHREAD_TASK_ID */

#ifndef NROS_PLATFORM_ALIASES_SKIP_SYSTEM

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

/* Issue 1196 / phase-436 D2 — zenoh-pico's `_z_condvar_wait` is unbounded by
 * CONTRACT, and the platform ABI no longer has an unbounded wait to map it
 * onto. It is bridged through a deadline THIS SHIM owns: each platform wait is
 * bounded by `NROS_ZP_CONDVAR_SLICE_MS`, and a slice that expires returns
 * 0 — a spurious wakeup.
 *
 * That is not a change to zenoh-pico's semantics. A condition variable may
 * wake spuriously (POSIX says so, and zenoh-pico's own POSIX port is
 * `pthread_cond_wait`), so every caller re-tests its predicate in a loop —
 * checked for all five upstream sites (`scheduler.c`, `ring_mt.c`,
 * `fifo_mt.c` x2, `sync_group.c`) and for `zpico.c`'s `while (!ctx.done)`.
 * The unbounded part of the wait is now the CALLER's loop, where it was
 * always decided, and no single platform wait can block forever.
 *
 * Every port returns 0 when signalled, 1 on timeout and -1 on error
 * (`nros/platform.h`, issue 1906), so an expired slice is told apart from an
 * error by the code alone. */
#include "nros_zp_condvar_slice.h"

int8_t _z_condvar_wait(void *cv, void *m) {
    if (cv == NULL || m == NULL) {
        return -1;
    }
    uint64_t deadline_ms = nros_platform_clock_ns() / 1000000u + NROS_ZP_CONDVAR_SLICE_MS;
    int8_t rc = nros_platform_condvar_wait_until(cv, m, deadline_ms);
    return rc < 0 ? -1 : 0; /* 1 (slice expired) is a spurious wakeup */
}
#endif /* !NROS_PLATFORM_ALIASES_SKIP_SYSTEM */

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

#ifndef NROS_PLATFORM_ALIASES_SKIP_SYSTEM
#include "zenoh-pico/utils/result.h"

/* Issue 2094 -- everything from here to the end of the clock section takes or
 * returns a `z_clock_t`, spelt `uint64_t` ms because this TU is compiled
 * against the GENERIC header. That is only the caller's type when the port's
 * zenoh-pico TUs see the generic header too. NuttX's do not: `ZENOH_NUTTX`
 * selects `unix.h`, where `z_clock_t` is a 16-byte `struct timespec` returned
 * through a hidden pointer, so these definitions read and wrote the wrong
 * shape and every executor wake-up was scheduled at stack garbage. A port in
 * that position provides the clock in its own TU (`nuttx_clock.c`) and skips
 * this one. */
#ifndef NROS_PLATFORM_ALIASES_SKIP_CLOCK

/* Issue 1906 -- translate the platform's codes into zenoh-pico's.
 *
 * The platform returns 0 when signalled, 1 on timeout and -1 on error
 * (`nros/platform.h`). zenoh-pico's contract is `_Z_RES_OK` /
 * `Z_ETIMEDOUT` / a negative error, and every upstream system layer maps
 * it. This shim passed the platform's value through unchanged, so a
 * timeout arrived as `1`. `_z_sync_group_wait_deadline` tests
 * `ret == Z_ETIMEDOUT`, misses it, takes its `ret != _Z_RES_OK` branch and
 * returns while still holding `counter_mutex`, which blocks every later
 * notifier drop and close for good.
 *
 * A NULL deadline is an error, not an unbounded wait. No zenoh-pico caller
 * passes one (each passes `&deadline`), and upstream's own implementations
 * dereference it unconditionally. Falling back to ~~`condvar_wait`~~ here was
 * a contract this shim invented, and it was the second unbounded wait the
 * issue-1196 gate had to exempt. */
int8_t _z_condvar_wait_until(void *cv, void *m, const uint64_t *abstime_ms) {
    if (abstime_ms == NULL) {
        return _Z_ERR_GENERIC;
    }
    int8_t rc = nros_platform_condvar_wait_until(cv, m, *abstime_ms);
    if (rc == 0) {
        return _Z_RES_OK;
    }
    return rc > 0 ? Z_ETIMEDOUT : _Z_ERR_GENERIC;
}

/* -------------------------------------------------------------------------
 *  Clock / monotonic-time variants. Vendor `<system/common/platform.h>`
 *  declares them with per-platform `z_clock_t` / `z_time_t`. Same
 *  generic-header gate as `_z_condvar_wait_until`.
 *
 *  Phase 160.L.1 — `z_clock_*` is the *monotonic* clock in zenoh-pico's
 *  contract (`unix/system.c:247` uses `CLOCK_MONOTONIC`; `z_time_now`
 *  is the wall-clock variant). The alias TU previously routed
 *  `z_clock_now` through the wall clock (`nros_platform_time_now_*` /
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
 *  clock) — if/when added to the alias TU — should keep the wall clock
 *  (`nros_platform_time_now_ns`, issue 0532 item 5).
 * ----------------------------------------------------------------------- */

uint64_t z_clock_now(void) {
    return (nros_platform_clock_ns() / 1000000ULL);
}

unsigned long z_clock_elapsed_us(const uint64_t *clock) {
    if (clock == NULL) return 0;
    uint64_t now = (nros_platform_clock_ns() / 1000000ULL);
    return (unsigned long) ((now - *clock) * 1000ULL);
}

unsigned long z_clock_elapsed_ms(const uint64_t *clock) {
    if (clock == NULL) return 0;
    uint64_t now = (nros_platform_clock_ns() / 1000000ULL);
    return (unsigned long) (now - *clock);
}

unsigned long z_clock_elapsed_s(const uint64_t *clock) {
    if (clock == NULL) return 0;
    uint64_t now = (nros_platform_clock_ns() / 1000000ULL);
    return (unsigned long) ((now - *clock) / 1000ULL);
}

/* phase-508 — zenoh-pico 1.9's executor keeps every wake-up as an instant
 * relative to its own epoch, so the clock ABI gained "elapsed from `epoch` to
 * `instant`" beside "elapsed until now". Saturating at 0 for an instant before
 * the epoch, as upstream's own ports do. */
unsigned long zp_clock_elapsed_us_since(const uint64_t *instant, const uint64_t *epoch) {
    if (instant == NULL || epoch == NULL || *instant <= *epoch) return 0;
    return (unsigned long) ((*instant - *epoch) * 1000ULL);
}

unsigned long zp_clock_elapsed_ms_since(const uint64_t *instant, const uint64_t *epoch) {
    if (instant == NULL || epoch == NULL || *instant <= *epoch) return 0;
    return (unsigned long) (*instant - *epoch);
}

unsigned long zp_clock_elapsed_s_since(const uint64_t *instant, const uint64_t *epoch) {
    if (instant == NULL || epoch == NULL || *instant <= *epoch) return 0;
    return (unsigned long) ((*instant - *epoch) / 1000ULL);
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
#endif /* !NROS_PLATFORM_ALIASES_SKIP_CLOCK */
#endif /* !NROS_PLATFORM_ALIASES_SKIP_SYSTEM */

/* -------------------------------------------------------------------------
 *  Networking. Wraps zenoh-pico's `_z_tcp_*` / `_z_udp_unicast_*` /
 *  `_z_udp_multicast_*` link ABI on top of `nros_platform_{tcp,udp,udp_mcast,
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
 *    fault. Used on ESP32-C3, mps2-an385-baremetal, stm32f4.
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

/* phase-508 — zenoh-pico 1.9 renamed the link ABI (`zenoh-pico/link/transport/
 * {tcp,udp_unicast,udp_multicast}.h`): `_z_open_tcp` became `_z_tcp_open`,
 * `_z_send_*` became `_z_*_write`, endpoints are made from C strings, and UDP
 * unicast open takes an interface name. The socket half (`_z_socket_close`,
 * `_z_socket_set_blocking`, `_z_socket_wait_readable`) needs zenoh-pico's own
 * iterator type, which this TU cannot include, so it lives in
 * `c/zpico/platform_socket.c` (compiled beside this TU on the same platforms). */

/* TCP. */
int8_t _z_tcp_endpoint_init(void *ep, const char *address, const char *port) {
    return nros_platform_tcp_create_endpoint(ep, (const uint8_t *)address, (const uint8_t *)port);
}
void _z_tcp_endpoint_clear(void *ep) {
    nros_platform_tcp_free_endpoint(ep);
}
int8_t _z_tcp_open(void *sock, nros_zp_alias_endpoint_t rep, uint32_t tout) {
    return nros_platform_tcp_open(sock, &rep, tout);
}
int8_t _z_tcp_listen(void *sock, nros_zp_alias_endpoint_t rep) {
    return nros_platform_tcp_listen(sock, &rep);
}
int8_t _z_tcp_accept(const void *sock_in, void *sock_out) {
    return nros_platform_socket_accept(sock_in, sock_out);
}
void _z_tcp_close(void *sock) {
    nros_platform_tcp_close(sock);
}
size_t _z_tcp_read(nros_zp_alias_socket_t sock, uint8_t *buf, size_t len) {
    return nros_platform_tcp_read(&sock, buf, len);
}
size_t _z_tcp_read_exact(nros_zp_alias_socket_t sock, uint8_t *buf, size_t len) {
    return nros_platform_tcp_read_exact(&sock, buf, len);
}
size_t _z_tcp_write(nros_zp_alias_socket_t sock, const uint8_t *buf, size_t len) {
    return nros_platform_tcp_send(&sock, buf, len);
}

/* UDP unicast. `iface` (bind to a named interface) has no counterpart in the
 * platform ABI; these platforms have one interface, so a NULL name is the only
 * one that can be honoured and any other is refused rather than ignored. */
int8_t _z_udp_unicast_endpoint_init(void *ep, const char *address, const char *port) {
    return nros_platform_udp_create_endpoint(ep, (const uint8_t *)address, (const uint8_t *)port);
}
void _z_udp_unicast_endpoint_clear(void *ep) {
    nros_platform_udp_free_endpoint(ep);
}
int8_t _z_udp_unicast_open(void *sock, nros_zp_alias_endpoint_t rep, uint32_t tout, const char *iface) {
    if (iface != NULL) {
        return -1;
    }
    return nros_platform_udp_open(sock, &rep, tout);
}
int8_t _z_udp_unicast_listen(void *sock, nros_zp_alias_endpoint_t rep, uint32_t tout) {
    return nros_platform_udp_listen(sock, &rep, tout);
}
void _z_udp_unicast_close(void *sock) {
    nros_platform_udp_close(sock);
}
size_t _z_udp_unicast_read(nros_zp_alias_socket_t sock, uint8_t *buf, size_t len) {
    return nros_platform_udp_read(&sock, buf, len);
}
size_t _z_udp_unicast_read_exact(nros_zp_alias_socket_t sock, uint8_t *buf, size_t len) {
    return nros_platform_udp_read_exact(&sock, buf, len);
}
size_t _z_udp_unicast_write(nros_zp_alias_socket_t sock, const uint8_t *buf, size_t len,
                            nros_zp_alias_endpoint_t rep) {
    return nros_platform_udp_send(&sock, buf, len, &rep);
}

/* UDP multicast (Phase 134.7).
 *
 * Bridges the `_z_udp_multicast_*` symbols (called from upstream zenoh-pico's
 * `link/multicast/udp.c`) to the canonical `nros_platform_udp_mcast_*` ABI
 * declared in <nros/platform_net.h>. Same opaque-storage idiom as the unicast
 * aliases above. `addr` is declared `void *` because `_z_slice_t` is a
 * zenoh-pico-internal type the alias TU cannot pull in without a circular
 * include — the canonical platform fn signature also takes `void *` for this
 * parameter, so the cast is byte-for-byte.
 *
 * The endpoint is built the way upstream's own multicast links build it, from
 * the unicast parser (`udp_multicast.h`'s default); `address` is a
 * `const _z_string_t *`, passed through untouched. */
int8_t _z_udp_unicast_endpoint_init_from_address(void *ep, const void *address);
int8_t _z_udp_multicast_endpoint_init_from_address(void *ep, const void *address) {
    return _z_udp_unicast_endpoint_init_from_address(ep, address);
}
void _z_udp_multicast_endpoint_clear(void *ep) {
    nros_platform_udp_free_endpoint(ep);
}
int8_t _z_udp_multicast_open(void *sock,
                             nros_zp_alias_endpoint_t rep,
                             void *lep,
                             uint32_t tout,
                             const char *iface) {
    return nros_platform_udp_mcast_open(sock, &rep, lep, tout,
                                        (const uint8_t *)iface);
}
int8_t _z_udp_multicast_listen(void *sock,
                               nros_zp_alias_endpoint_t rep,
                               uint32_t tout,
                               const char *iface,
                               const char *join) {
    return nros_platform_udp_mcast_listen(sock, &rep, tout,
                                          (const uint8_t *)iface,
                                          (const uint8_t *)join);
}
void _z_udp_multicast_close(void *sockrecv,
                            void *socksend,
                            nros_zp_alias_endpoint_t rep,
                            nros_zp_alias_endpoint_t lep) {
    nros_platform_udp_mcast_close(sockrecv, socksend, &rep, &lep);
}
size_t _z_udp_multicast_read(nros_zp_alias_socket_t sock,
                             uint8_t *ptr, size_t len,
                             nros_zp_alias_endpoint_t lep,
                             void *addr) {
    return nros_platform_udp_mcast_read(&sock, ptr, len, &lep, addr);
}
size_t _z_udp_multicast_read_exact(nros_zp_alias_socket_t sock,
                                   uint8_t *ptr, size_t len,
                                   nros_zp_alias_endpoint_t lep,
                                   void *addr) {
    return nros_platform_udp_mcast_read_exact(&sock, ptr, len, &lep, addr);
}
size_t _z_udp_multicast_write(nros_zp_alias_socket_t sock,
                              const uint8_t *ptr, size_t len,
                              nros_zp_alias_endpoint_t rep) {
    return nros_platform_udp_mcast_send(&sock, ptr, len, &rep);
}

#endif /* NROS_ZP_ALIAS_BARE_METAL_NET || NROS_ZP_ALIAS_OPAQUE_NET — Phase 160 */

/* Serial transport.
 *
 * Phase 152 — minimum stubs so `libnros_rmw_zenoh_staticlib.a` (which keeps
 * `Z_FEATURE_LINK_SERIAL=1` to satisfy zenoh-pico's link-wrapper layer in
 * `src/link/unicast/serial.c` + `src/link/transport/upper/serial_protocol.c`)
 * links cleanly against platform_aliases.c. Open/listen return `-1`, so the
 * serial transport is reachable but refuses to open on a platform with no
 * serial backend.
 *
 * A platform that HAS one overrides these: POSIX and NuttX with upstream's
 * `tty_posix.c`, Zephyr with its own backend, bare metal with `zpico-serial`.
 * phase-508 — zenoh-pico 1.9 names the ABI `_z_serial_{open,listen}_from_*`,
 * `_z_serial_close` and the byte-level `_z_serial_read` / `_z_serial_write`;
 * a platform defining `ZP_PLATFORM_SERIAL_FRAMING` (Zephyr, bare metal)
 * replaces the byte level with whole frames, `_z_read_serial_internal` /
 * `_z_send_serial_internal`, and the protocol layer runs the link handshake
 * itself after the open. All six are stubbed: the protocol layer references
 * the pair its framing mode uses, and a stub nobody references costs nothing
 * under `--gc-sections`. */
__attribute__((weak)) int8_t _z_serial_open_from_pins(void *sock, uint32_t txpin, uint32_t rxpin,
                                                      uint32_t baudrate) {
    (void)sock;
    (void)txpin;
    (void)rxpin;
    (void)baudrate;
    return -1;
}
__attribute__((weak)) int8_t _z_serial_open_from_dev(void *sock, const char *dev, uint32_t baudrate) {
    (void)sock;
    (void)dev;
    (void)baudrate;
    return -1;
}
__attribute__((weak)) int8_t _z_serial_listen_from_pins(void *sock, uint32_t txpin, uint32_t rxpin,
                                                        uint32_t baudrate) {
    (void)sock;
    (void)txpin;
    (void)rxpin;
    (void)baudrate;
    return -1;
}
__attribute__((weak)) int8_t _z_serial_listen_from_dev(void *sock, const char *dev, uint32_t baudrate) {
    (void)sock;
    (void)dev;
    (void)baudrate;
    return -1;
}
__attribute__((weak)) void _z_serial_close(void *sock) {
    (void)sock;
}
/* phase-386 W2 — SIZE_MAX, not 0.
 *
 * These returned 0, and 0 is not an error in this API: zenoh-pico's own
 * callers test `ret == SIZE_MAX` (`serial_protocol.c`, both `_z_connect_serial`
 * and the read loop), so 0 reads as "succeeded, moved zero bytes" — a normal
 * state. A send that silently discarded every byte and a read that reported
 * "nothing yet" forever both looked healthy.
 *
 * Unreachable while the open/listen stubs above return -1, so this is not a
 * live bug. It becomes one under a PARTIAL override: a board shim that supplies
 * the open and forgets send/read links cleanly, opens the port, and then loses
 * all traffic with no diagnostic. That is precisely the shape this set should
 * refuse to have.
 *
 * Printing is available here — unlike the ThreadX libc stubs, these are not on
 * the console path, so a diagnostic cannot recurse. It is still not used: the
 * sentinel already reaches the caller, which has real error handling
 * (`_Z_ERR_TRANSPORT_RX_FAILED`), and an unconditional print inside a transport
 * poll loop would be its own hazard.
 *
 * The socket is passed BY VALUE in this ABI, and this TU cannot see the vendor
 * header's layout for it. On ThreadX and bare metal it is the network
 * section's opaque storage; elsewhere a stub is only ever the losing weak
 * definition beside the platform's real backend, and none of these stubs reads
 * any argument, so the parameter's shape cannot matter. */
#if defined(NROS_ZP_ALIAS_BARE_METAL_NET) || defined(NROS_ZP_ALIAS_OPAQUE_NET)
typedef nros_zp_alias_socket_t nros_zp_alias_serial_socket_t;
#else
typedef struct {
    uint8_t _opaque[NROS_ZP_NET_SOCKET_STORAGE_BYTES];
} nros_zp_alias_serial_socket_t;
#endif
__attribute__((weak)) size_t _z_serial_read(nros_zp_alias_serial_socket_t sock, uint8_t *buf, size_t len) {
    (void)sock;
    (void)buf;
    (void)len;
    return SIZE_MAX;
}
__attribute__((weak)) size_t _z_serial_write(nros_zp_alias_serial_socket_t sock, const uint8_t *buf,
                                             size_t len) {
    (void)sock;
    (void)buf;
    (void)len;
    return SIZE_MAX;
}
__attribute__((weak)) size_t _z_send_serial_internal(nros_zp_alias_serial_socket_t sock, uint8_t header,
                                                     const uint8_t *buf, size_t len) {
    (void)sock;
    (void)header;
    (void)buf;
    (void)len;
    return SIZE_MAX;
}
__attribute__((weak)) size_t _z_read_serial_internal(nros_zp_alias_serial_socket_t sock, uint8_t *header,
                                                     uint8_t *buf, size_t len) {
    (void)sock;
    (void)header;
    (void)buf;
    (void)len;
    return SIZE_MAX;
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

#endif /* !NROS_ZP_ALIAS_SCALAR_ONLY — phase-230 1c/Wave2 scalar funnel */
