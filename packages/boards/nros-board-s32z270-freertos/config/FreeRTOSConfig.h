/* FreeRTOS kernel configuration — NXP S32Z270 RTU (Cortex-R52).
 * phase-372 W2: the board supplies its facts; the rest is shared.
 * Seeded from the ASI consumer's hardware-proven RTU0 configuration.
 *
 * NOTE the port split: the in-tree GCC/ARM_CRx_No_GIC port (the
 * link-complete default) needs the tick macros below; the NXP
 * GCC/ARM_CR52_GIC port a hardware consumer provisions brings its own
 * tick + GIC handling and typically its own FreeRTOSConfig additions —
 * override via FREERTOS_CONFIG_DIR in that case. */
#define NROS_BOARD_CPU_CLOCK_HZ 800000000 /* RTU0 R52 lock-step, 800 MHz */
#define NROS_BOARD_PRIO_BITS    5         /* GIC priority bits (unused by CRx_No_GIC) */
#include "../../nros-board-freertos/config/FreeRTOSConfig.h"

/* GCC/ARM_CRx_No_GIC tick seam — board hooks, weak-stubbed in
 * c/board_s32z270.c, strong-overridden by the consumer. */
#ifndef configSETUP_TICK_INTERRUPT
void nros_board_setup_tick_interrupt(void);
void nros_board_clear_tick_interrupt(void);
#define configSETUP_TICK_INTERRUPT() nros_board_setup_tick_interrupt()
#define configCLEAR_TICK_INTERRUPT() nros_board_clear_tick_interrupt()
#endif

/* GCC/ARM_CRx_No_GIC end-of-interrupt register: the port writes the
 * acknowledged interrupt ID here on exit. S32Z270 GICv3 uses the system
 * register interface (ICC_EOIR1) rather than a memory-mapped GICC, so no
 * MMIO EOI address exists; point the port at the GICD base as a
 * link-complete placeholder. A hardware consumer runs the NXP
 * GCC/ARM_CR52_GIC port (its own EOI handling) — this value is never
 * reached there. GICD base 0x4780_0000 per the public S32Z2 map. */
#ifndef configEOI_ADDRESS
#define configEOI_ADDRESS 0x47800000UL
#endif
