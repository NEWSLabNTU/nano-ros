/**
 * nros platform abstraction layer
 *
 * This header provides compile-time platform selection for nros.
 * Users must define one of the platform macros before including nros headers.
 *
 * Supported platforms:
 *   NROS_PLATFORM_POSIX     - Linux, macOS, other POSIX systems
 *   NROS_PLATFORM_ZEPHYR    - Zephyr RTOS
 *   NROS_PLATFORM_FREERTOS  - FreeRTOS
 *   NROS_PLATFORM_BAREMETAL - Bare-metal (user provides time/sleep)
 *   NROS_PLATFORM_CUSTOM    - User provides all platform functions
 *
 * Example usage:
 *   #define NROS_PLATFORM_ZEPHYR
 *   #include <nros/init.h>
 *
 * Or via compiler flag:
 *   gcc -DNROS_PLATFORM_POSIX -c main.c
 *
 * Copyright 2024 nros contributors
 * Licensed under Apache-2.0
 */

#ifndef NROS_PLATFORM_H
#define NROS_PLATFORM_H

#include <stdint.h>
#include <stdbool.h>
#include <stddef.h>

#ifdef __cplusplus
extern "C" {
#endif

// ============================================================================
// Platform Selection
// ============================================================================

#if defined(NROS_PLATFORM_POSIX)
#include "nros/platform/posix.h"
#elif defined(NROS_PLATFORM_ZEPHYR)
#include "nros/platform/zephyr.h"
#elif defined(NROS_PLATFORM_FREERTOS)
#include "nros/platform/freertos.h"
#elif defined(NROS_PLATFORM_BAREMETAL)
#include "nros/platform/baremetal.h"
#elif defined(NROS_PLATFORM_CUSTOM)
// User must implement all platform functions externally
#else
// Default to POSIX for backward compatibility
#ifndef NROS_PLATFORM_POSIX
#define NROS_PLATFORM_POSIX
#endif
#include "nros/platform/posix.h"
#endif

// ============================================================================
// Platform Function Declarations
// ============================================================================
// These functions must be provided by the platform implementation.
// For built-in platforms, they are defined as static inline in the headers.
// For NROS_PLATFORM_CUSTOM, user must provide implementations.

#if defined(NROS_PLATFORM_CUSTOM) || defined(NROS_PLATFORM_BAREMETAL)

/**
 * Get current monotonic time in nanoseconds.
 *
 * This function must return a monotonically increasing value suitable for
 * measuring elapsed time. It should not be affected by system time changes.
 *
 * @return Current time in nanoseconds
 */
uint64_t nros_platform_time_ns(void);

/**
 * Sleep for the specified duration in nanoseconds.
 *
 * This function may busy-wait or use platform-specific sleep mechanisms.
 * The actual sleep duration may be longer than requested due to system
 * scheduling, but should not be significantly shorter.
 *
 * @param ns Duration to sleep in nanoseconds
 */
void nros_platform_sleep_ns(uint64_t ns);

#endif // NROS_PLATFORM_CUSTOM || NROS_PLATFORM_BAREMETAL

// ============================================================================
// Atomic Operations
// ============================================================================
// These are required for guard conditions and thread-safe signaling.
// For single-threaded bare-metal systems, simple volatile access is sufficient.

#ifndef NROS_PLATFORM_HAS_ATOMICS

/**
 * Atomically store a boolean value with release semantics.
 *
 * For multi-threaded platforms, this ensures that all prior writes
 * are visible to other threads before the store.
 *
 * @param ptr Pointer to the boolean variable
 * @param value Value to store
 */
void nros_platform_atomic_store_bool(volatile bool* ptr, bool value);

/**
 * Atomically load a boolean value with acquire semantics.
 *
 * For multi-threaded platforms, this ensures that all subsequent reads
 * see writes that happened before any release store to the same location.
 *
 * @param ptr Pointer to the boolean variable
 * @return The current value
 */
bool nros_platform_atomic_load_bool(volatile bool* ptr);

#endif // !NROS_PLATFORM_HAS_ATOMICS

// ============================================================================
// Memory Functions (Optional)
// ============================================================================
// These are only required if dynamic memory allocation is used.
// Define NROS_NO_DYNAMIC_MEMORY to disable dynamic memory.

#ifndef NROS_NO_DYNAMIC_MEMORY

#ifndef NROS_PLATFORM_HAS_MALLOC

/**
 * Allocate memory.
 *
 * @param size Number of bytes to allocate
 * @return Pointer to allocated memory, or NULL on failure
 */
void* nros_platform_malloc(size_t size);

/**
 * Free previously allocated memory.
 *
 * @param ptr Pointer to memory to free (may be NULL)
 */
void nros_platform_free(void* ptr);

#endif // !NROS_PLATFORM_HAS_MALLOC

#endif // !NROS_NO_DYNAMIC_MEMORY

// ============================================================================
// Threading Functions (Optional)
// ============================================================================
// These are only required if threading support is enabled.
// Define NROS_FEATURE_THREADS to enable threading.

#ifdef NROS_FEATURE_THREADS

#ifndef NROS_PLATFORM_HAS_MUTEX

/**
 * Initialize a mutex.
 *
 * @param mutex Pointer to mutex to initialize
 * @return 0 on success, non-zero on failure
 */
int nros_platform_mutex_init(nros_mutex_t* mutex);

/**
 * Lock a mutex.
 *
 * This function blocks until the mutex is acquired.
 *
 * @param mutex Pointer to mutex to lock
 * @return 0 on success, non-zero on failure
 */
int nros_platform_mutex_lock(nros_mutex_t* mutex);

/**
 * Unlock a mutex.
 *
 * @param mutex Pointer to mutex to unlock
 * @return 0 on success, non-zero on failure
 */
int nros_platform_mutex_unlock(nros_mutex_t* mutex);

/**
 * Destroy a mutex.
 *
 * @param mutex Pointer to mutex to destroy
 * @return 0 on success, non-zero on failure
 */
int nros_platform_mutex_destroy(nros_mutex_t* mutex);

#endif // !NROS_PLATFORM_HAS_MUTEX

#endif // NROS_FEATURE_THREADS

#ifdef __cplusplus
}
#endif

#endif // NROS_PLATFORM_H
