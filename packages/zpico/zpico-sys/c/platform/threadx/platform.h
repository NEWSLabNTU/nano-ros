/**
 * zenoh-pico ThreadX Platform Type Definitions
 *
 * Defines types required by zenoh-pico's platform abstraction for
 * Eclipse ThreadX RTOS. Works with both the Linux simulation port
 * and embedded targets (RISC-V, ARM).
 *
 * Included via ZENOH_GENERIC + ZENOH_THREADX defines pointing through
 * zenoh_generic_platform.h to this file.
 */

#ifndef ZENOH_PICO_SYSTEM_THREADX_TYPES_H
#define ZENOH_PICO_SYSTEM_THREADX_TYPES_H

#include <stdint.h>
#include <stdbool.h>
#include <stddef.h>

#include "zenoh-pico/config.h"
#include "tx_api.h"

#ifdef __cplusplus
extern "C" {
#endif

// ============================================================================
// Threading Types
// ============================================================================

#if Z_FEATURE_MULTI_THREAD == 1

#ifndef Z_TASK_STACK_SIZE
#define Z_TASK_STACK_SIZE 8192
#endif

#ifndef Z_TASK_PRIORITY
#define Z_TASK_PRIORITY 14
#endif

#ifndef Z_TASK_PREEMPT_THRESHOLD
#define Z_TASK_PREEMPT_THRESHOLD 14
#endif

#ifndef Z_TASK_TIME_SLICE
#define Z_TASK_TIME_SLICE 1
#endif

typedef struct {
    TX_THREAD threadx_thread;
    uint8_t threadx_stack[Z_TASK_STACK_SIZE];
    void *(*_fun)(void *);   /* Real entry function (full pointer width) */
    void  *_arg;             /* Real argument (full pointer width) */
} _z_task_t;

typedef void *z_task_attr_t;  // Not used

typedef TX_MUTEX _z_mutex_rec_t;
typedef TX_MUTEX _z_mutex_t;

typedef struct {
    TX_MUTEX mutex;
    TX_SEMAPHORE sem;
    UINT waiters;
} _z_condvar_t;

#endif  // Z_FEATURE_MULTI_THREAD == 1

// ============================================================================
// Clock and Time Types
// ============================================================================

/**
 * Monotonic clock type.
 * Uses a timespec-compatible struct backed by tx_time_get().
 */
typedef struct {
    long tv_sec;
    long tv_nsec;
} z_clock_t;

/**
 * System time type.
 * ThreadX tick count.
 */
typedef ULONG z_time_t;

// ============================================================================
// Network Types (BSD sockets via NetX Duo nxd_bsd.h)
// ============================================================================

/**
 * Socket handle for NetX Duo BSD socket layer.
 * Uses standard BSD file descriptor returned by socket().
 */
typedef struct {
    int _fd;  // BSD socket file descriptor (-1 = invalid)
} _z_sys_net_socket_t;

/**
 * Network endpoint.
 * Stores IPv4 address and port for TCP/UDP connections.
 * Converted to sockaddr_in in the network transport layer.
 */
typedef struct {
    uint32_t _addr;   // IPv4 address in network byte order
    uint16_t _port;   // Port in network byte order
} _z_sys_net_endpoint_t;

#ifdef __cplusplus
}
#endif

#endif /* ZENOH_PICO_SYSTEM_THREADX_TYPES_H */
