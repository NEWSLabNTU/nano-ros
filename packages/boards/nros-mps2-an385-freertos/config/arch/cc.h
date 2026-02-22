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

/* Random number generator (simple, sufficient for lwIP port IDs) */
#define LWIP_RAND() ((uint32_t)rand())

#endif /* LWIP_ARCH_CC_H */
