/*
 * Phase 129.A.3.a — generic platform header for zenoh-pico.
 *
 * Selected by zenoh-pico's `system/common/platform.h` when
 * `ZENOH_GENERIC` is defined (see vendor source, line ~55).
 * `zpico-sys/build.rs` defines `ZENOH_GENERIC` when the
 * `platform-aliases` feature is on, and adds this file's
 * directory to the cc include path.
 *
 * The generic adapter types every zenoh-pico platform handle
 * (`_z_task_t`, `_z_mutex_t`, `_z_condvar_t`, …) as opaque
 * `uint8_t[N]` storage. phase-364 W2: the sizes come from
 * `<nros/platform.h>`'s `NROS_PLATFORM_*_STORAGE_SIZE` bounds, which each
 * port checks against its own type with a `_Static_assert`. They used to be
 * a table of `≈` estimates maintained in this file; see the defines below
 * for what that cost.
 *
 * `nros_platform_task_init` (phase 121 ABI) takes a `void *`
 * pointer to caller storage — an `N`-sized array satisfies
 * the contract. Platform impl reads / writes its own native
 * type out of that buffer.
 *
 * Clock and wall-clock time collapse to `uint64_t` milliseconds,
 * matching `nros_platform_time_now_ms` and the
 * `_z_condvar_wait_until` deadline argument.
 *
 * Network sockets stay per-platform-provider — this header
 * declares only the threading + time surface. The vendor's
 * `network.c` selection still applies (smoltcp / lwIP / POSIX).
 */

#ifndef NROS_ZENOH_GENERIC_PLATFORM_H
#define NROS_ZENOH_GENERIC_PLATFORM_H

#include <stddef.h>
#include <stdint.h>

/* phase-364 W2 — the storage bounds come from the ABI header now, not from a
 * table maintained here. See below. */
#include <nros/platform.h>

#ifdef __cplusplus
extern "C" {
#endif

/* -------------------------------------------------------------------------
 *  Threading handles — opaque worst-case storage.
 * ----------------------------------------------------------------------- */

/* phase-364 W2 (RFC-0076 D1) — these were three hand-maintained numbers here,
 * derived from a table of OTHER platforms' struct sizes recorded as `≈` values
 * with a stated "2× safety margin", checked by nobody.
 *
 * Phase 154 is what that costs. The bound was 64 B, ThreadX's `TX_MUTEX` is
 * ~120 B with its ownership / inheritance / suspension-list fields, and
 * `nros_platform_mutex_init` casts the buffer to `TX_MUTEX *` and writes the
 * whole struct — so it silently corrupted the neighbouring field, presenting as
 * a HANG in `Executor::open` after the zenoh handshake completed, because every
 * mutex op on the in-band executor trampled a neighbour.
 *
 * The bounds now come from `<nros/platform.h>`, where each port asserts its own
 * type fits (`_Static_assert` in its `platform.c`). A bound that stops being
 * true is a compile error in the port that broke it, rather than a corrupted
 * neighbour in this consumer. */
#define NROS_ZP_TASK_STORAGE_BYTES    NROS_PLATFORM_TASK_STORAGE_SIZE
#define NROS_ZP_MUTEX_STORAGE_BYTES   NROS_PLATFORM_MUTEX_STORAGE_SIZE
#define NROS_ZP_CONDVAR_STORAGE_BYTES NROS_PLATFORM_CONDVAR_STORAGE_SIZE

typedef uint8_t _z_task_t[NROS_ZP_TASK_STORAGE_BYTES];
typedef uint8_t _z_mutex_t[NROS_ZP_MUTEX_STORAGE_BYTES];
typedef uint8_t _z_mutex_rec_t[NROS_ZP_MUTEX_STORAGE_BYTES];
typedef uint8_t _z_condvar_t[NROS_ZP_CONDVAR_STORAGE_BYTES];
typedef void *z_task_attr_t;

/* -------------------------------------------------------------------------
 *  Clock + wall-clock time — both are millisecond `uint64_t`.
 *  This matches `nros_platform_time_now_ms` and the
 *  `nros_platform_condvar_wait_until` deadline arg.
 * ----------------------------------------------------------------------- */

typedef uint64_t z_clock_t;
typedef uint64_t z_time_t;

/* -------------------------------------------------------------------------
 *  Sockets — opaque storage. Per-platform `network.c` (POSIX BSD,
 *  smoltcp, lwIP, NetX) provides the implementation. Storage sized
 *  to hold either an `int _fd` (POSIX, smoltcp handle, lwIP) or a
 *  pointer + small state. 32 B covers every supported provider with
 *  a 2× margin. Endpoint is a pointer-to-resolved-address (addrinfo
 *  on POSIX, a smoltcp `IpEndpoint` heap-box on bare-metal); 16 B
 *  covers pointer + flags.
 * ----------------------------------------------------------------------- */

#define NROS_ZP_NET_SOCKET_STORAGE_BYTES   32
#define NROS_ZP_NET_ENDPOINT_STORAGE_BYTES 16

typedef struct {
    uint8_t _opaque[NROS_ZP_NET_SOCKET_STORAGE_BYTES];
} _z_sys_net_socket_t;

typedef struct {
    uint8_t _opaque[NROS_ZP_NET_ENDPOINT_STORAGE_BYTES];
} _z_sys_net_endpoint_t;

#ifdef __cplusplus
} /* extern "C" */
#endif

#endif /* NROS_ZENOH_GENERIC_PLATFORM_H */
