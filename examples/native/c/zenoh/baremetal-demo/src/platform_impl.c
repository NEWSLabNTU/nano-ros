/**
 * @file platform_impl.c
 * @brief Example bare-metal platform implementation for nros-c
 *
 * This file demonstrates how users would implement the platform abstraction
 * layer for their specific embedded platform. In a real embedded system,
 * these functions would use hardware timers, SysTick, or other peripherals.
 *
 * This example uses POSIX functions to simulate a bare-metal environment,
 * allowing the code to compile and run on desktop for testing.
 *
 * On a real embedded platform (e.g., STM32), you would:
 * - Use HAL_GetTick() or DWT cycle counter for time
 * - Use __WFI() or busy-wait for sleep
 * - Use __DMB() or __DSB() for memory barriers
 */

#include <stdint.h>
#include <stdbool.h>
#include <time.h>  // For simulation only - not available on bare-metal

// ============================================================================
// Platform Implementation
// ============================================================================

/**
 * Get current monotonic time in nanoseconds.
 *
 * REAL EMBEDDED IMPLEMENTATION EXAMPLE (STM32 with HAL):
 *
 *   // Using HAL_GetTick() which gives 1ms resolution
 *   uint64_t nano_ros_platform_time_ns(void) {
 *       return (uint64_t)HAL_GetTick() * 1000000ULL;
 *   }
 *
 *   // Using DWT cycle counter for higher resolution (Cortex-M3/M4/M7)
 *   uint64_t nano_ros_platform_time_ns(void) {
 *       static uint64_t high_bits = 0;
 *       static uint32_t last_count = 0;
 *       uint32_t count = DWT->CYCCNT;
 *       if (count < last_count) {
 *           high_bits += (1ULL << 32);  // Overflow occurred
 *       }
 *       last_count = count;
 *       uint64_t cycles = high_bits | count;
 *       return cycles * (1000000000ULL / SystemCoreClock);
 *   }
 */
uint64_t nano_ros_platform_time_ns(void) {
    // Simulation using POSIX clock_gettime
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return (uint64_t)ts.tv_sec * 1000000000ULL + (uint64_t)ts.tv_nsec;
}

/**
 * Sleep for the specified duration in nanoseconds.
 *
 * REAL EMBEDDED IMPLEMENTATION EXAMPLE (STM32 with HAL):
 *
 *   // Simple busy-wait implementation
 *   void nano_ros_platform_sleep_ns(uint64_t ns) {
 *       uint64_t start = nano_ros_platform_time_ns();
 *       while ((nano_ros_platform_time_ns() - start) < ns) {
 *           // Optionally use WFI for power saving
 *           // __WFI();
 *       }
 *   }
 *
 *   // Using HAL_Delay for millisecond resolution
 *   void nano_ros_platform_sleep_ns(uint64_t ns) {
 *       uint32_t ms = (uint32_t)(ns / 1000000);
 *       if (ms > 0) {
 *           HAL_Delay(ms);
 *       } else if (ns > 0) {
 *           // Sub-millisecond: busy wait
 *           uint64_t start = nano_ros_platform_time_ns();
 *           while ((nano_ros_platform_time_ns() - start) < ns) {}
 *       }
 *   }
 */
void nano_ros_platform_sleep_ns(uint64_t ns) {
    // Simulation using POSIX nanosleep
    struct timespec ts = {
        .tv_sec = (time_t)(ns / 1000000000ULL),
        .tv_nsec = (long)(ns % 1000000000ULL)
    };
    nanosleep(&ts, NULL);
}

/**
 * Atomically store a boolean value with release semantics.
 *
 * REAL EMBEDDED IMPLEMENTATION EXAMPLE (Cortex-M):
 *
 *   void nano_ros_platform_atomic_store_bool(volatile bool *ptr, bool value) {
 *       __DMB();  // Data memory barrier
 *       *ptr = value;
 *       __DMB();
 *   }
 *
 * For single-core bare-metal without interrupts accessing the variable,
 * a simple volatile write is sufficient:
 *
 *   void nano_ros_platform_atomic_store_bool(volatile bool *ptr, bool value) {
 *       *ptr = value;
 *   }
 */
void nano_ros_platform_atomic_store_bool(volatile bool *ptr, bool value) {
    // Simulation using GCC atomic builtin
    __atomic_store_n(ptr, value, __ATOMIC_RELEASE);
}

/**
 * Atomically load a boolean value with acquire semantics.
 *
 * REAL EMBEDDED IMPLEMENTATION EXAMPLE (Cortex-M):
 *
 *   bool nano_ros_platform_atomic_load_bool(volatile bool *ptr) {
 *       __DMB();
 *       bool value = *ptr;
 *       __DMB();
 *       return value;
 *   }
 */
bool nano_ros_platform_atomic_load_bool(volatile bool *ptr) {
    // Simulation using GCC atomic builtin
    return __atomic_load_n(ptr, __ATOMIC_ACQUIRE);
}
