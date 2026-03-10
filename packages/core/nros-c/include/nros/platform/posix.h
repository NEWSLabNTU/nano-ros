/**
 * nros POSIX platform implementation
 *
 * Platform support for Linux, macOS, and other POSIX-compliant systems.
 * Uses clock_gettime() for time, nanosleep() for delays, and C11 atomics.
 *
 * Copyright 2024 nros contributors
 * Licensed under Apache-2.0
 */

#ifndef NROS_PLATFORM_POSIX_H
#define NROS_PLATFORM_POSIX_H

#include <time.h>
#include <stdlib.h>
#include <stdatomic.h>
#include <stdbool.h>
#include <stdint.h>

#ifdef NROS_FEATURE_THREADS
#include <pthread.h>
#endif

#ifdef __cplusplus
extern "C" {
#endif

// ============================================================================
// Platform Capability Flags
// ============================================================================

#define NROS_PLATFORM_HAS_ATOMICS
#define NROS_PLATFORM_HAS_MALLOC

#ifdef NROS_FEATURE_THREADS
#define NROS_PLATFORM_HAS_MUTEX
typedef pthread_mutex_t nros_mutex_t;
#endif

// ============================================================================
// Time Functions
// ============================================================================

/**
 * Get current monotonic time in nanoseconds.
 */
static inline uint64_t nros_platform_time_ns(void) {
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return (uint64_t)ts.tv_sec * 1000000000ULL + (uint64_t)ts.tv_nsec;
}

/**
 * Sleep for the specified duration in nanoseconds.
 */
static inline void nros_platform_sleep_ns(uint64_t ns) {
    struct timespec ts = {.tv_sec = (time_t)(ns / 1000000000ULL),
                          .tv_nsec = (long)(ns % 1000000000ULL)};
    nanosleep(&ts, NULL);
}

// ============================================================================
// Atomic Operations
// ============================================================================

/**
 * Atomically store a boolean value with release semantics.
 */
static inline void nros_platform_atomic_store_bool(volatile bool* ptr, bool value) {
    atomic_store_explicit((_Atomic bool*)ptr, value, memory_order_release);
}

/**
 * Atomically load a boolean value with acquire semantics.
 */
static inline bool nros_platform_atomic_load_bool(volatile bool* ptr) {
    return atomic_load_explicit((_Atomic bool*)ptr, memory_order_acquire);
}

// ============================================================================
// Memory Functions
// ============================================================================

#ifndef NROS_NO_DYNAMIC_MEMORY

/**
 * Allocate memory.
 */
static inline void* nros_platform_malloc(size_t size) {
    return malloc(size);
}

/**
 * Free previously allocated memory.
 */
static inline void nros_platform_free(void* ptr) {
    free(ptr);
}

#endif // !NROS_NO_DYNAMIC_MEMORY

// ============================================================================
// Threading Functions
// ============================================================================

#ifdef NROS_FEATURE_THREADS

/**
 * Initialize a mutex.
 */
static inline int nros_platform_mutex_init(nros_mutex_t* mutex) {
    return pthread_mutex_init(mutex, NULL);
}

/**
 * Lock a mutex.
 */
static inline int nros_platform_mutex_lock(nros_mutex_t* mutex) {
    return pthread_mutex_lock(mutex);
}

/**
 * Unlock a mutex.
 */
static inline int nros_platform_mutex_unlock(nros_mutex_t* mutex) {
    return pthread_mutex_unlock(mutex);
}

/**
 * Destroy a mutex.
 */
static inline int nros_platform_mutex_destroy(nros_mutex_t* mutex) {
    return pthread_mutex_destroy(mutex);
}

#endif // NROS_FEATURE_THREADS

#ifdef __cplusplus
}
#endif

#endif // NROS_PLATFORM_POSIX_H
