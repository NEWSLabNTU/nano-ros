/**
 * nano-ros bare-metal platform implementation
 *
 * Platform support for bare-metal systems without an OS.
 * Users must provide time and sleep implementations externally.
 *
 * Required user implementations:
 *   uint64_t nano_ros_platform_time_ns(void);
 *   void nano_ros_platform_sleep_ns(uint64_t ns);
 *
 * Example implementation for STM32 with HAL:
 *
 *   uint64_t nano_ros_platform_time_ns(void) {
 *       return (uint64_t)HAL_GetTick() * 1000000ULL;  // ms to ns
 *   }
 *
 *   void nano_ros_platform_sleep_ns(uint64_t ns) {
 *       uint64_t start = nano_ros_platform_time_ns();
 *       while ((nano_ros_platform_time_ns() - start) < ns) {
 *           __WFI();  // Wait for interrupt
 *       }
 *   }
 *
 * Copyright 2024 nano-ros contributors
 * Licensed under Apache-2.0
 */

#ifndef NANO_ROS_PLATFORM_BAREMETAL_H
#define NANO_ROS_PLATFORM_BAREMETAL_H

#include <stdbool.h>
#include <stdint.h>
#include <stddef.h>

#ifdef __cplusplus
extern "C" {
#endif

// ============================================================================
// Platform Capability Flags
// ============================================================================

// Bare-metal provides simple atomic operations via volatile
#define NANO_ROS_PLATFORM_HAS_ATOMICS

// No dynamic memory by default on bare-metal
#ifndef NANO_ROS_NO_DYNAMIC_MEMORY
#define NANO_ROS_NO_DYNAMIC_MEMORY
#endif

// No threading on bare-metal
#ifdef NANO_ROS_FEATURE_THREADS
#error "NANO_ROS_FEATURE_THREADS not supported on bare-metal. Use NANO_ROS_PLATFORM_CUSTOM for RTOS support."
#endif

// ============================================================================
// Time Functions (User must provide)
// ============================================================================

/**
 * Get current monotonic time in nanoseconds.
 *
 * User must implement this function using platform-specific timer.
 * Common implementations:
 *   - SysTick counter
 *   - Hardware timer peripheral
 *   - DWT cycle counter (Cortex-M)
 *
 * @return Current time in nanoseconds
 */
extern uint64_t nano_ros_platform_time_ns(void);

/**
 * Sleep for the specified duration in nanoseconds.
 *
 * User must implement this function. Common implementations:
 *   - Busy-wait loop using nano_ros_platform_time_ns()
 *   - WFI instruction for power saving
 *   - Hardware timer interrupt
 *
 * @param ns Duration to sleep in nanoseconds
 */
extern void nano_ros_platform_sleep_ns(uint64_t ns);

// ============================================================================
// Atomic Operations
// ============================================================================

/**
 * Memory barrier for single-core bare-metal.
 *
 * Prevents compiler reordering. For Cortex-M, also includes DMB.
 */
#if defined(__ARM_ARCH)
    #define NANO_ROS_MEMORY_BARRIER() __asm__ volatile ("dmb" ::: "memory")
#elif defined(__GNUC__)
    #define NANO_ROS_MEMORY_BARRIER() __asm__ volatile ("" ::: "memory")
#else
    #define NANO_ROS_MEMORY_BARRIER() do {} while(0)
#endif

/**
 * Atomically store a boolean value with release semantics.
 *
 * For single-core bare-metal, volatile write with barrier is sufficient.
 */
static inline void nano_ros_platform_atomic_store_bool(volatile bool *ptr, bool value) {
    NANO_ROS_MEMORY_BARRIER();
    *ptr = value;
    NANO_ROS_MEMORY_BARRIER();
}

/**
 * Atomically load a boolean value with acquire semantics.
 */
static inline bool nano_ros_platform_atomic_load_bool(volatile bool *ptr) {
    NANO_ROS_MEMORY_BARRIER();
    bool value = *ptr;
    NANO_ROS_MEMORY_BARRIER();
    return value;
}

// ============================================================================
// Helper Macros
// ============================================================================

/**
 * Disable interrupts (Cortex-M specific).
 * User may need to override for other architectures.
 */
#if defined(__ARM_ARCH)
static inline uint32_t nano_ros_platform_disable_irq(void) {
    uint32_t primask;
    __asm__ volatile ("mrs %0, primask\n\t"
                      "cpsid i"
                      : "=r" (primask) :: "memory");
    return primask;
}

static inline void nano_ros_platform_restore_irq(uint32_t primask) {
    __asm__ volatile ("msr primask, %0" :: "r" (primask) : "memory");
}
#endif

#ifdef __cplusplus
}
#endif

#endif // NANO_ROS_PLATFORM_BAREMETAL_H
