/* FreeRTOS kernel configuration: fixture-r5f-freertos (Cortex-R5F, hard float).
 *
 * A board package outside nano-ros states its facts and includes the FreeRTOS
 * family's configuration by name: the family directory is on the include path
 * of every FreeRTOS C build, on both roads (issue 2687). This file used to be
 * a 250-line copy of the family's, which had already fallen behind it.
 */
#ifndef FIXTURE_R5F_FREERTOS_CONFIG_H
#define FIXTURE_R5F_FREERTOS_CONFIG_H

#define NROS_BOARD_CPU_CLOCK_HZ 500000000 /* RPU at 500 MHz */
#define NROS_BOARD_PRIO_BITS    5

/* Every task gets an FPU context: hard-float objects use d0-d15 freely. */
#define configUSE_TASK_FPU_SUPPORT 2

#include "nros_freertos_family_config.h"

/* ---- Board hooks, after the family's configuration ---- */

/* GCC/ARM_CRx_No_GIC tick seam: board hooks over the vendor SDK's timer. */
#ifndef configSETUP_TICK_INTERRUPT
void nros_board_setup_tick_interrupt(void);
void nros_board_clear_tick_interrupt(void);
#define configSETUP_TICK_INTERRUPT() nros_board_setup_tick_interrupt()
#define configCLEAR_TICK_INTERRUPT() nros_board_clear_tick_interrupt()
#endif

/* The port writes the acknowledged interrupt here on exit: the SDK's word. */
#ifndef configEOI_ADDRESS
extern volatile unsigned long fxsdk_eoi_word;
#define configEOI_ADDRESS ((unsigned long)&fxsdk_eoi_word)
#endif

#endif /* FIXTURE_R5F_FREERTOS_CONFIG_H */
