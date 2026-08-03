/*
 * lwIP arch/cc.h — compiler abstraction for GCC on ARM Cortex-M
 *
 * Minimal definitions required by lwIP.  Most defaults in lwip/arch.h
 * are fine for GCC + 32-bit ARM.
 */

#ifndef LWIP_ARCH_CC_H
#define LWIP_ARCH_CC_H

#include <stdint.h>
#include <stdlib.h>
#include <errno.h>
#include <sys/time.h>

/* Diagnostics — use semihosting for QEMU */
extern void semihosting_write0(const char *s);

#define LWIP_PLATFORM_DIAG(x) do { (void)0; } while (0)
#define LWIP_PLATFORM_ASSERT(x) do { semihosting_write0("lwIP ASSERT: " x "\n"); for (;;) {} } while (0)

/* Use GCC's packed attribute */
#define PACK_STRUCT_FIELD(x) x
#define PACK_STRUCT_STRUCT __attribute__((packed))
#define PACK_STRUCT_BEGIN
#define PACK_STRUCT_END

/* Byte order — ARM Cortex-M is little-endian */
#ifndef BYTE_ORDER
#define BYTE_ORDER LITTLE_ENDIAN
#endif

/* Random number generator (Phase 155.C — route through the
 * board's seeded platform RNG instead of libc `rand()`.
 *
 * `rand()` without a matching `srand(seed)` defaults to seed 1
 * across every process — two QEMU instances on the same binary
 * see identical sequences. Vendor zenoh-pico's
 * `system/freertos/system.c::z_random_u32` calls `LWIP_RAND()`
 * for its session-ZID generator; with `rand()` underneath, both
 * the server and the client QEMUs produce the SAME ZID, and
 * zenohd rejects the second connection's `OpenSyn` (duplicate
 * peer ID, `max_links=1`). Manifests as the FreeRTOS C++
 * service E2E: server connects + declares queryable cleanly,
 * client's TCP gets `Close` from zenohd immediately after
 * OpenSyn → no queries → `Future::wait` times out.
 *
 * `nros_platform_random_u32` reads `s_rng_state` which
 * `nros_platform_freertos_seed_rng(seed_from_ip_mac)` writes
 * during board init. IP + MAC differ between the two example
 * configs, so the resulting ZIDs differ too. */
extern uint32_t nros_platform_random_u32(void);
#define LWIP_RAND() (nros_platform_random_u32())

#endif /* LWIP_ARCH_CC_H */
