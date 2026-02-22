/**
 * zenoh-pico Bare-Metal Platform Type Definitions
 *
 * This header defines the types required by zenoh-pico's platform abstraction
 * layer for bare-metal systems (QEMU ARM, ESP32-C3, STM32F4, etc.).
 *
 * Included via ZENOH_GENERIC define pointing to this file.
 */

#ifndef ZENOH_PICO_SYSTEM_BARE_METAL_TYPES_H
#define ZENOH_PICO_SYSTEM_BARE_METAL_TYPES_H

#include <stdint.h>
#include <stdbool.h>
#include <stddef.h>

#include "zenoh-pico/config.h"

#ifdef __cplusplus
extern "C" {
#endif

// ============================================================================
// Threading Types (stubs when Z_FEATURE_MULTI_THREAD == 0)
// ============================================================================

#if Z_FEATURE_MULTI_THREAD == 1
// Bare-metal platform is single-threaded, but provide types if needed for compilation
typedef void *_z_task_t;
typedef void *z_task_attr_t;
typedef void *_z_mutex_t;
typedef void *_z_mutex_rec_t;
typedef void *_z_condvar_t;
#endif  // Z_FEATURE_MULTI_THREAD == 1

// ============================================================================
// Clock and Time Types
// ============================================================================

/**
 * Monotonic clock type.
 * Stores milliseconds since system start (from DWT or RTIC monotonic).
 */
typedef uint64_t z_clock_t;

/**
 * System time type.
 * For embedded systems without RTC, this is the same as z_clock_t.
 * Stores milliseconds since some epoch.
 */
typedef uint64_t z_time_t;

// ============================================================================
// Network Types
// ============================================================================

/**
 * Socket handle.
 * Generic handle index plus connection state.
 *
 * Note: zenoh-pico's link layer code references `_fd` (POSIX field name).
 * We alias `_fd` to `_handle` for compatibility with link/unicast/tls.c.
 */
typedef struct {
    union {
        int8_t _handle;     // Socket handle (-1 = invalid)
        int8_t _fd;         // POSIX-compatible alias for link layer code
    };
    bool _connected;    // Connection state
#if Z_FEATURE_LINK_TLS == 1
    void *_tls_sock;    // Pointer to _z_tls_socket_t (back-pointer from TCP socket to TLS context)
#endif
} _z_sys_net_socket_t;

/**
 * Network endpoint (address + port).
 * Stores IPv4 address and port for TCP connections.
 */
typedef struct {
    uint8_t _ip[4];     // IPv4 address bytes
    uint16_t _port;     // Port number
} _z_sys_net_endpoint_t;

#ifdef __cplusplus
}
#endif

#endif /* ZENOH_PICO_SYSTEM_BARE_METAL_TYPES_H */
