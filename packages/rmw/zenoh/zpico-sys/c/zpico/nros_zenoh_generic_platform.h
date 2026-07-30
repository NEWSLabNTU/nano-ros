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
 * `uint8_t[N]` storage. Sizes match a worst-case across every
 * supported platform with a 2× safety margin:
 *
 *   - `_z_task_t`: 256 B   (POSIX pthread_t ≤ 8;
 *                           FreeRTOS TCB pointer + attrs ≈ 32;
 *                           ThreadX TX_THREAD ≈ 232)
 *   - `_z_mutex_t`: 64 B   (POSIX pthread_mutex_t = 40;
 *                           Zephyr k_mutex ≈ 32;
 *                           FreeRTOS xSemaphoreHandle ≈ 8)
 *   - `_z_condvar_t`: 64 B (POSIX pthread_cond_t = 48;
 *                           Zephyr k_condvar ≈ 32;
 *                           FreeRTOS event group ≈ 32)
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

#ifdef __cplusplus
extern "C" {
#endif

/* -------------------------------------------------------------------------
 *  Threading handles — opaque worst-case storage.
 * ----------------------------------------------------------------------- */

#define NROS_ZP_TASK_STORAGE_BYTES    256
/* Phase 154 — bumped from 64 → 256 to cover ThreadX's
 * `TX_MUTEX` (≈ 120 B with ownership / inheritance / suspension-
 * list fields) and `TX_SEMAPHORE` (≈ 60 B). The smaller bound
 * silently corrupted the next field when vendor `mutex.c`
 * (which sees the alias-flavoured 64 B storage with
 * NROS_PLATFORM_ALIASES) handed buffer to
 * `nros_platform_mutex_init` (which casts to `TX_MUTEX *` and
 * writes the full struct). Manifests as a hang in
 * `Executor::open` after the zenoh handshake completes — every
 * mutex op on the in-band executor corrupts a neighbouring
 * field. Same logic for `_z_condvar_t = { TX_MUTEX + TX_SEMAPHORE
 * + UINT }` ≈ 184 B. */
#define NROS_ZP_MUTEX_STORAGE_BYTES   256
#define NROS_ZP_CONDVAR_STORAGE_BYTES 256

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
