/* fixture-r5f-freertos: the FreeRTOS family's arch/cc.h, copied (this package
 * cannot reach nano-ros's by a relative path; see FreeRTOSConfig.h). */
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
/* `struct timeval` (lwipopts.h sets LWIP_TIMEVAL_PRIVATE 0). sys/select.h,
 * not sys/time.h: newlib and TI Arm Clang's C library both define it there,
 * and TI's has no sys/time.h (phase-575 W3). */
#include <sys/select.h>

/* The socket errno values lwIP's socket layer and zenoh-pico's lwIP link set
 * and test. newlib's errno.h defines every one of them, so on GCC nothing here
 * applies. TI Arm Clang's C library has no socket errnos at all (phase-575 W3),
 * so the image's lwIP states its own. Each is 1000 + its Linux value: never one
 * of a C library's own small numbers, so no socket error can read as, say,
 * TI's EMSGSIZE (97, which is Linux's EAFNOSUPPORT). Only lwIP and zenoh-pico
 * set or read these, and both see this header, so the numbers agree. */
#ifndef EADDRINUSE
#define EADDRINUSE 1098
#endif
#ifndef EADDRNOTAVAIL
#define EADDRNOTAVAIL 1099
#endif
#ifndef EAFNOSUPPORT
#define EAFNOSUPPORT 1097
#endif
#ifndef EALREADY
#define EALREADY 1114
#endif
#ifndef ECOMM
#define ECOMM 1070
#endif
#ifndef ECONNABORTED
#define ECONNABORTED 1103
#endif
#ifndef ECONNREFUSED
#define ECONNREFUSED 1111
#endif
#ifndef ECONNRESET
#define ECONNRESET 1104
#endif
#ifndef EHOSTUNREACH
#define EHOSTUNREACH 1113
#endif
#ifndef EISCONN
#define EISCONN 1106
#endif
#ifndef ENETUNREACH
#define ENETUNREACH 1101
#endif
#ifndef ENOBUFS
#define ENOBUFS 1105
#endif
#ifndef ENOMSG
#define ENOMSG 1042
#endif
#ifndef ENOPROTOOPT
#define ENOPROTOOPT 1092
#endif
#ifndef ENOTCONN
#define ENOTCONN 1107
#endif
#ifndef ENOTSOCK
#define ENOTSOCK 1088
#endif
#ifndef EOPNOTSUPP
#define EOPNOTSUPP 1095
#endif
/* POSIX lets it equal EAGAIN, which every C library here defines. */
#ifndef EWOULDBLOCK
#define EWOULDBLOCK EAGAIN
#endif

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
