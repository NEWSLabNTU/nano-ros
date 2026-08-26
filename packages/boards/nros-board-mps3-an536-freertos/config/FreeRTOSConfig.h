/* FreeRTOS kernel configuration — QEMU MPS3-AN536 (dual Cortex-R52).
 * phase-385 W1: the board supplies its facts; the rest is shared.
 *
 * Unlike the S32Z270 sibling, this board IMPLEMENTS the tick seam below
 * (c/board_an536.c): QEMU models a GICv3 and the ARM generic timer, so the
 * in-tree GCC/ARM_CRx_No_GIC port runs here with no licensed port and no
 * consumer glue. That is the whole point of the board.
 */
/* The generic timer's CNTFRQ, not the CPU clock: the tick is programmed
 * against CNTP (see nros_board_setup_tick_interrupt). QEMU's virtual counter
 * runs at 62.5 MHz on this machine; the board reads CNTFRQ at runtime rather
 * than trusting this number, so it is only the FreeRTOS-visible constant. */
#define NROS_BOARD_CPU_CLOCK_HZ 62500000
#define NROS_BOARD_PRIO_BITS    5 /* GICv3 supports 32 priority levels here */
#include "../../nros-board-freertos/config/FreeRTOSConfig.h"

/* GCC/ARM_CRx_No_GIC tick seam — implemented in c/board_an536.c against the
 * ARM generic timer (PPI 30) routed through the GICv3 redistributor. */
#ifndef configSETUP_TICK_INTERRUPT
void nros_board_setup_tick_interrupt(void);
void nros_board_clear_tick_interrupt(void);
#define configSETUP_TICK_INTERRUPT() nros_board_setup_tick_interrupt()
#define configCLEAR_TICK_INTERRUPT() nros_board_clear_tick_interrupt()
#endif

/* GCC/ARM_CRx_No_GIC end-of-interrupt address.
 *
 * The port ends every IRQ with a store to this address. On a GICv2 that is
 * the memory-mapped ICCEOIR; this machine has NO memory-mapped CPU interface
 * (`info mtree` shows only gicv3_dist and gicv3_redist_region), so there is no
 * such address to name.
 *
 * That does not require patching the kernel. The port never ACKNOWLEDGES an
 * interrupt either — it calls `vApplicationIRQHandler()` with no argument, so
 * reading ICC_IAR1 is already the board's job, and doing the matching
 * ICC_EOIR1 write there too keeps the pair in one place. The port's trailing
 * store is therefore pointed at a scratch word the board owns
 * (`nros_board_eoi_scratch`), where it is harmless.
 *
 * Deliberately NOT a device address: pointing it at the distributor would
 * write a register nobody meant to write. */
#ifndef configEOI_ADDRESS
extern unsigned long nros_board_eoi_scratch;
#define configEOI_ADDRESS ((unsigned long)&nros_board_eoi_scratch)
#endif
