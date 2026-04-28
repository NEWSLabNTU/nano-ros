/**
 * @file tband_config.h
 * @brief Tonbandgeraet configuration for MPS2-AN385 FreeRTOS (QEMU)
 *
 * Snapshot backend: records to a 16 KB RAM ring buffer.
 * After the test, the board crate triggers a snapshot and dumps it to
 * a file via semihosting for offline Perfetto visualization.
 *
 * Timestamp: xTaskGetTickCount() gives 1 ms resolution (1 kHz tick),
 * sufficient for task scheduling analysis. Higher resolution would need
 * DWT CYCCNT (not available on all Cortex-M3) or a hardware timer.
 */
#ifndef TBAND_CONFIG_H_
#define TBAND_CONFIG_H_

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

/* ---- Feature enables ---- */
#define tband_configENABLE                 1
#define tband_configFREERTOS_TRACE_ENABLE  1
#define tband_configISR_TRACE_ENABLE       0  /* ISR tracing adds overhead; enable if needed */
#define tband_configMARKER_TRACE_ENABLE    1

/* ---- Periodic drop counter (detect buffer overflows) ---- */
#define tband_configTRACE_DROP_CNT_EVERY   100

/* ---- Timestamp ---- */
/* Use SysTick reload counter for cycle-level timestamps.
 * SysTick counts DOWN from configCPU_CLOCK_HZ/configTICK_RATE_HZ - 1.
 * Combined with the tick count, this gives sub-ms resolution.
 * We read the SysTick registers directly to avoid circular FreeRTOS includes
 * (tband_config.h is included from FreeRTOSConfig.h before types are defined).
 */
#define SYSTICK_CVR  (*(volatile uint32_t *)0xE000E018)  /* Current Value Register */
#define SYSTICK_RVR  (*(volatile uint32_t *)0xE000E014)  /* Reload Value Register */

/* Declared in trace_dump.c, incremented by a tick hook or read from kernel */
extern volatile uint32_t nros_trace_tick_count;

static inline uint64_t tband_port_timestamp(void) {
    /* Combine tick count (ms) with SysTick down-counter for sub-ms resolution.
     * Each tick = configCPU_CLOCK_HZ / configTICK_RATE_HZ = 25000 cycles.
     * Resolution: 1 cycle = 40 ns at 25 MHz. */
    uint32_t ticks = nros_trace_tick_count;
    uint32_t cycles_remaining = SYSTICK_CVR;
    uint32_t reload = SYSTICK_RVR;
    uint32_t cycles_in_tick = reload - cycles_remaining;
    return (uint64_t)ticks * (reload + 1) + cycles_in_tick;
}

#define tband_portTIMESTAMP()              tband_port_timestamp()
#define tband_portTIMESTAMP_RESOLUTION_NS  40  /* 25 MHz = 40 ns/cycle */

/* ---- Backend: snapshot ---- */
#define tband_configUSE_BACKEND_SNAPSHOT   1
#define tband_configBACKEND_SNAPSHOT_BUF_SIZE  (16 * 1024)  /* 16 KB trace buffer */
#define tband_configMETADATA_BUF_SIZE          256

/* Callback when snapshot buffer is full (optional diagnostic) */
extern volatile bool nros_trace_snapshot_full;
static inline void tband_port_snapshot_full(void) { nros_trace_snapshot_full = true; }
#define tband_portBACKEND_SNAPSHOT_BUF_FULL_CALLBACK() tband_port_snapshot_full()

#endif /* TBAND_CONFIG_H_ */
