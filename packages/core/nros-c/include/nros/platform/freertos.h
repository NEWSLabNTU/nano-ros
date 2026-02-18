/**
 * nros FreeRTOS platform implementation
 *
 * Platform support for FreeRTOS.
 * Uses xTaskGetTickCount() for time, vTaskDelay() for delays.
 *
 * Copyright 2024 nros contributors
 * Licensed under Apache-2.0
 */

#ifndef NROS_PLATFORM_FREERTOS_H
#define NROS_PLATFORM_FREERTOS_H

#include "FreeRTOS.h"
#include "task.h"
#include "semphr.h"
#include <stdbool.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

// ============================================================================
// Platform Capability Flags
// ============================================================================

#define NROS_PLATFORM_HAS_ATOMICS

#if configSUPPORT_DYNAMIC_ALLOCATION == 1
#define NROS_PLATFORM_HAS_MALLOC
#endif

#define NROS_PLATFORM_HAS_MUTEX
typedef SemaphoreHandle_t nano_ros_mutex_t;

// ============================================================================
// Time Functions
// ============================================================================

/**
 * Get current monotonic time in nanoseconds.
 *
 * Note: FreeRTOS tick resolution depends on configTICK_RATE_HZ.
 * For typical 1000 Hz tick rate, resolution is 1ms.
 */
static inline uint64_t nano_ros_platform_time_ns(void) {
    TickType_t ticks = xTaskGetTickCount();
    // Convert ticks to nanoseconds
    // Each tick is (1000 / configTICK_RATE_HZ) ms = (1e9 / configTICK_RATE_HZ) ns
    return (uint64_t)ticks * (1000000000ULL / configTICK_RATE_HZ);
}

/**
 * Sleep for the specified duration in nanoseconds.
 */
static inline void nano_ros_platform_sleep_ns(uint64_t ns) {
    // Convert nanoseconds to ticks
    // ticks = ns / (1e9 / configTICK_RATE_HZ) = ns * configTICK_RATE_HZ / 1e9
    TickType_t ticks = (TickType_t)((ns * configTICK_RATE_HZ) / 1000000000ULL);
    if (ticks == 0 && ns > 0) {
        ticks = 1;  // Minimum 1 tick delay
    }
    vTaskDelay(ticks);
}

// ============================================================================
// Atomic Operations
// ============================================================================

/**
 * Atomically store a boolean value with release semantics.
 *
 * Uses compiler barrier and volatile for basic atomicity.
 * For Cortex-M, single-byte writes are atomic.
 */
static inline void nano_ros_platform_atomic_store_bool(volatile bool *ptr, bool value) {
    taskENTER_CRITICAL();
    *ptr = value;
    taskEXIT_CRITICAL();
}

/**
 * Atomically load a boolean value with acquire semantics.
 */
static inline bool nano_ros_platform_atomic_load_bool(volatile bool *ptr) {
    bool value;
    taskENTER_CRITICAL();
    value = *ptr;
    taskEXIT_CRITICAL();
    return value;
}

// ============================================================================
// Memory Functions
// ============================================================================

#ifdef NROS_PLATFORM_HAS_MALLOC

/**
 * Allocate memory from FreeRTOS heap.
 */
static inline void *nano_ros_platform_malloc(size_t size) {
    return pvPortMalloc(size);
}

/**
 * Free previously allocated memory.
 */
static inline void nano_ros_platform_free(void *ptr) {
    vPortFree(ptr);
}

#endif // NROS_PLATFORM_HAS_MALLOC

// ============================================================================
// Threading Functions
// ============================================================================

/**
 * Initialize a mutex.
 */
static inline int nano_ros_platform_mutex_init(nano_ros_mutex_t *mutex) {
    *mutex = xSemaphoreCreateMutex();
    return (*mutex != NULL) ? 0 : -1;
}

/**
 * Lock a mutex (blocking).
 */
static inline int nano_ros_platform_mutex_lock(nano_ros_mutex_t *mutex) {
    return (xSemaphoreTake(*mutex, portMAX_DELAY) == pdTRUE) ? 0 : -1;
}

/**
 * Unlock a mutex.
 */
static inline int nano_ros_platform_mutex_unlock(nano_ros_mutex_t *mutex) {
    return (xSemaphoreGive(*mutex) == pdTRUE) ? 0 : -1;
}

/**
 * Destroy a mutex.
 */
static inline int nano_ros_platform_mutex_destroy(nano_ros_mutex_t *mutex) {
    if (*mutex != NULL) {
        vSemaphoreDelete(*mutex);
        *mutex = NULL;
    }
    return 0;
}

#ifdef __cplusplus
}
#endif

#endif // NROS_PLATFORM_FREERTOS_H
