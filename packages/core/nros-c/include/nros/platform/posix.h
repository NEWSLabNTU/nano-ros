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
#ifndef __cplusplus
// C11 `<stdatomic.h>` is unused now that the atomic helpers below use the
// `__atomic_*` builtins (see `nros_platform_atomic_load_bool`). It is also
// broken under g++ (`atomic_flag does not name a type`), and this header is
// included from C++ TUs (the nros-cpp umbrella), so keep it C-only.
#include <stdatomic.h>
#endif
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
 *
 * Uses the `__atomic_*` compiler builtins (GCC/Clang) rather than C11
 * `<stdatomic.h>` `atomic_*_explicit`: this header is included from both C
 * and C++ translation units (the nros-cpp umbrella header), and the C11
 * `_Atomic` cast + `atomic_load_explicit` macro do not compile cleanly under
 * g++. The builtins have identical acquire/release semantics in both languages.
 */
static inline void nros_platform_atomic_store_bool(volatile bool* ptr, bool value) {
    __atomic_store_n(ptr, value, __ATOMIC_RELEASE);
}

/**
 * Atomically load a boolean value with acquire semantics.
 */
static inline bool nros_platform_atomic_load_bool(volatile bool* ptr) {
    return __atomic_load_n(ptr, __ATOMIC_ACQUIRE);
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
