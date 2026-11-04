/**
 * @file platform_foreign_wake.h
 * @brief Forward a wake requested by a thread the RTOS port does not own.
 *
 * Issue 2217 (RFC-0113 D4). On a HOST-SIMULATOR port (threadx-linux,
 * freertos-posix) an RTOS thread is a host pthread the port manages, and a
 * host library linked into the image runs threads of its own: Cyclone's
 * receive thread is one. Such a FOREIGN thread may not enter a kernel service
 * (issues 1237, 1750): on FreeRTOS's POSIX port `vPortYield` asserts, and on
 * ThreadX's Linux port the kernel lock leaks until the kernel stops. So it
 * cannot signal an executor's wake object, which is the only way an arrival
 * on that thread can end a park.
 *
 * This is the bridge. A kernel thread binds a callback to a slot; a foreign
 * thread POSTS the slot, which marks it pending and writes a host eventfd
 * without entering the kernel; a port-owned forwarder, a kernel thread at the
 * lowest priority blocked in a host `poll()` on that eventfd, calls the
 * callback in kernel context. Event-driven: nothing runs while nothing is
 * posted.
 *
 * Only the host-simulator ports implement it, and only their board files turn
 * on a consumer (`NROS_RMW_CYCLONEDDS_FOREIGN_WAKE_BRIDGE`). Every other
 * port's threads are the platform's own, so a callback runs in a context that
 * may signal directly and this header is not needed there.
 *
 * Latency: a posted slot is forwarded when the forwarder runs, which is at
 * once while every higher-priority thread is blocked, and after them
 * otherwise. The same bound as a host socket's receive notification on
 * threadx-linux (phase-521 W6f), which shares its forwarder.
 */
#ifndef NROS_PLATFORM_FOREIGN_WAKE_H
#define NROS_PLATFORM_FOREIGN_WAKE_H

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/** Slots the bridge holds; one per Cyclone session in practice. */
#ifndef NROS_PLATFORM_FOREIGN_WAKE_SLOTS
#define NROS_PLATFORM_FOREIGN_WAKE_SLOTS 8
#endif

/**
 * Bind @p fn(@p ctx) to a free slot and return the slot (>= 0), or a negative
 * value when no slot is free or the forwarder could not start. Call from a
 * kernel thread. @p fn runs on the forwarder, in kernel context: it may
 * signal a wake object, and it must not block.
 */
int32_t nros_platform_foreign_wake_bind(void (*fn)(void *ctx), void *ctx);

/**
 * Release @p slot. Call from a kernel thread. Once this returns, @p slot's
 * callback is never called again, and a post to it is ignored.
 */
void nros_platform_foreign_wake_unbind(int32_t slot);

/**
 * Request a call of @p slot's callback. Safe from ANY thread, including one
 * the port does not own: it stores a flag and writes a host eventfd, and
 * enters no kernel service. Posts that arrive before the forwarder runs
 * coalesce into one call, which suits a wake (a level, not a count).
 */
void nros_platform_foreign_wake_post(int32_t slot);

#ifdef __cplusplus
}
#endif

#endif /* NROS_PLATFORM_FOREIGN_WAKE_H */
