/**
 * nros bare-metal platform implementation
 *
 * Platform support for bare-metal systems without an OS.
 * Users must provide time and sleep implementations externally.
 *
 * Required user implementations:
 *   uint64_t nros_platform_time_ns(void);
 *   void nros_platform_sleep_ns(uint64_t ns);
 *
 * Example implementation for STM32 with HAL:
 *
 *   uint64_t nros_platform_time_ns(void) {
 *       return (uint64_t)HAL_GetTick() * 1000000ULL;  // ms to ns
 *   }
 *
 *   void nros_platform_sleep_ns(uint64_t ns) {
 *       uint64_t start = nros_platform_time_ns();
 *       while ((nros_platform_time_ns() - start) < ns) {
 *           __WFI();  // Wait for interrupt
 *       }
 *   }
 *
 * Copyright 2024 nros contributors
 * Licensed under Apache-2.0
 */

#ifndef NROS_PLATFORM_BAREMETAL_H
#define NROS_PLATFORM_BAREMETAL_H

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
#define NROS_PLATFORM_HAS_ATOMICS

// No dynamic memory by default on bare-metal
#ifndef NROS_NO_DYNAMIC_MEMORY
#define NROS_NO_DYNAMIC_MEMORY
#endif

// No threading on bare-metal
#ifdef NROS_FEATURE_THREADS
#error                                                                                             \
    "NROS_FEATURE_THREADS not supported on bare-metal. Use NROS_PLATFORM_CUSTOM for RTOS support."
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
extern uint64_t nros_platform_time_ns(void);

/**
 * Sleep for the specified duration in nanoseconds.
 *
 * User must implement this function. Common implementations:
 *   - Busy-wait loop using nros_platform_time_ns()
 *   - WFI instruction for power saving
 *   - Hardware timer interrupt
 *
 * @param ns Duration to sleep in nanoseconds
 */
extern void nros_platform_sleep_ns(uint64_t ns);

// ============================================================================
// Atomic Operations
// ============================================================================

/**
 * Memory barrier for single-core bare-metal.
 *
 * Prevents compiler reordering. For Cortex-M, also includes DMB.
 */
#if defined(__ARM_ARCH)
#define NROS_MEMORY_BARRIER() __asm__ volatile("dmb" ::: "memory")
#elif defined(__GNUC__)
#define NROS_MEMORY_BARRIER() __asm__ volatile("" ::: "memory")
#else
#define NROS_MEMORY_BARRIER()                                                                      \
    do {                                                                                           \
    } while (0)
#endif

/**
 * Atomically store a boolean value with release semantics.
 *
 * For single-core bare-metal, volatile write with barrier is sufficient.
 */
static inline void nros_platform_atomic_store_bool(volatile bool* ptr, bool value) {
    NROS_MEMORY_BARRIER();
    *ptr = value;
    NROS_MEMORY_BARRIER();
}

/**
 * Atomically load a boolean value with acquire semantics.
 */
static inline bool nros_platform_atomic_load_bool(volatile bool* ptr) {
    NROS_MEMORY_BARRIER();
    bool value = *ptr;
    NROS_MEMORY_BARRIER();
    return value;
}

// ============================================================================
// Helper Macros
// ============================================================================

/**
 * Disable interrupts. ARM has two distinct mechanisms:
 *
 *   - **Cortex-M (ARMv6-M/v7-M/v8-M)**: dedicated `PRIMASK` register,
 *     accessed via `mrs Rd, primask` / `msr primask, Rs`. The token
 *     returned is the prior PRIMASK value.
 *   - **Cortex-R / Cortex-A (ARMv7-R/v7-A/v8-R)**: no PRIMASK; the
 *     IRQ mask lives in `CPSR` bit 7 (the I-bit). Read via
 *     `mrs Rd, cpsr`, write via `msr cpsr_c, Rs`. The token returned
 *     is the prior CPSR (mode + flag + IRQ-mask bits).
 *
 * The `mrs Rd, primask` form is **not assemblable** on R/A profiles —
 * gating only on `__ARM_ARCH` (which is defined for every ARM profile)
 * would break Cortex-R5 builds (Orin SPE) at the assembler stage.
 *
 * `__ARM_ARCH_PROFILE` is the ARM ACLE-defined character constant for
 * the profile: `'M'`, `'R'`, or `'A'`. GCC and Clang both define it.
 * User may override either path for boards with a custom IRQ scheme.
 */
#if defined(__ARM_ARCH_PROFILE) && (__ARM_ARCH_PROFILE == 'M')
static inline uint32_t nros_platform_disable_irq(void) {
    uint32_t primask;
    __asm__ volatile("mrs %0, primask\n\t"
                     "cpsid i"
                     : "=r"(primask)::"memory");
    return primask;
}

static inline void nros_platform_restore_irq(uint32_t primask) {
    __asm__ volatile("msr primask, %0" ::"r"(primask) : "memory");
}
#elif defined(__ARM_ARCH_PROFILE) && ((__ARM_ARCH_PROFILE == 'R') || (__ARM_ARCH_PROFILE == 'A'))
static inline uint32_t nros_platform_disable_irq(void) {
    uint32_t cpsr;
    __asm__ volatile("mrs %0, cpsr\n\t"
                     "cpsid i"
                     : "=r"(cpsr)::"memory");
    return cpsr;
}

static inline void nros_platform_restore_irq(uint32_t cpsr) {
    /* Restore only the control byte (mode + IRQ/FIQ masks). Touching
     * the flag byte would clobber NZCV unrelated to interrupt state. */
    __asm__ volatile("msr cpsr_c, %0" ::"r"(cpsr) : "memory");
}
#endif

#ifdef __cplusplus
}
#endif

#endif // NROS_PLATFORM_BAREMETAL_H
