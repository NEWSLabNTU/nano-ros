/**
 * nros Zephyr RTOS platform implementation
 *
 * Platform support for Zephyr RTOS.
 * Uses k_uptime_ticks() for time, k_sleep() for delays, and Zephyr atomics.
 *
 * Copyright 2024 nros contributors
 * Licensed under Apache-2.0
 */

#ifndef NANO_ROS_PLATFORM_ZEPHYR_H
#define NANO_ROS_PLATFORM_ZEPHYR_H

#include <zephyr/kernel.h>
#include <zephyr/sys/atomic.h>
#include <stdbool.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

// ============================================================================
// Platform Capability Flags
// ============================================================================

#define NANO_ROS_PLATFORM_HAS_ATOMICS

#ifdef CONFIG_HEAP_MEM_POOL_SIZE
#if CONFIG_HEAP_MEM_POOL_SIZE > 0
#define NANO_ROS_PLATFORM_HAS_MALLOC
#endif
#endif

#ifdef CONFIG_MULTITHREADING
#define NANO_ROS_PLATFORM_HAS_MUTEX
typedef struct k_mutex nano_ros_mutex_t;
#endif

// ============================================================================
// Time Functions
// ============================================================================

/**
 * Get current monotonic time in nanoseconds.
 */
static inline uint64_t nano_ros_platform_time_ns(void) {
    int64_t ticks = k_uptime_ticks();
    // Convert ticks to nanoseconds
    // ticks * (1e9 / ticks_per_sec) = ticks * 1e9 / CONFIG_SYS_CLOCK_TICKS_PER_SEC
    return (uint64_t)ticks * (1000000000ULL / CONFIG_SYS_CLOCK_TICKS_PER_SEC);
}

/**
 * Sleep for the specified duration in nanoseconds.
 */
static inline void nano_ros_platform_sleep_ns(uint64_t ns) {
    // K_NSEC converts nanoseconds to Zephyr timeout
    // For very short sleeps, k_busy_wait might be more appropriate
    if (ns < 1000000) {
        // Less than 1ms: use busy wait (microseconds)
        k_busy_wait((uint32_t)(ns / 1000));
    } else {
        k_sleep(K_NSEC(ns));
    }
}

// ============================================================================
// Atomic Operations
// ============================================================================

/**
 * Atomically store a boolean value with release semantics.
 *
 * Zephyr's atomic_set provides the necessary memory ordering.
 */
static inline void nano_ros_platform_atomic_store_bool(volatile bool *ptr, bool value) {
    atomic_set((atomic_t *)ptr, value ? 1 : 0);
}

/**
 * Atomically load a boolean value with acquire semantics.
 */
static inline bool nano_ros_platform_atomic_load_bool(volatile bool *ptr) {
    return atomic_get((atomic_t *)ptr) != 0;
}

// ============================================================================
// Memory Functions
// ============================================================================

#ifdef NANO_ROS_PLATFORM_HAS_MALLOC

/**
 * Allocate memory from Zephyr heap.
 */
static inline void *nano_ros_platform_malloc(size_t size) {
    return k_malloc(size);
}

/**
 * Free previously allocated memory.
 */
static inline void nano_ros_platform_free(void *ptr) {
    k_free(ptr);
}

#endif // NANO_ROS_PLATFORM_HAS_MALLOC

// ============================================================================
// Threading Functions
// ============================================================================

#ifdef NANO_ROS_PLATFORM_HAS_MUTEX

/**
 * Initialize a mutex.
 */
static inline int nano_ros_platform_mutex_init(nano_ros_mutex_t *mutex) {
    return k_mutex_init(mutex);
}

/**
 * Lock a mutex (blocking).
 */
static inline int nano_ros_platform_mutex_lock(nano_ros_mutex_t *mutex) {
    return k_mutex_lock(mutex, K_FOREVER);
}

/**
 * Unlock a mutex.
 */
static inline int nano_ros_platform_mutex_unlock(nano_ros_mutex_t *mutex) {
    return k_mutex_unlock(mutex);
}

/**
 * Destroy a mutex.
 *
 * Zephyr mutexes don't require explicit destruction, but we reset the state.
 */
static inline int nano_ros_platform_mutex_destroy(nano_ros_mutex_t *mutex) {
    (void)mutex;
    return 0;
}

#endif // NANO_ROS_PLATFORM_HAS_MUTEX

#ifdef __cplusplus
}
#endif

#endif // NANO_ROS_PLATFORM_ZEPHYR_H
