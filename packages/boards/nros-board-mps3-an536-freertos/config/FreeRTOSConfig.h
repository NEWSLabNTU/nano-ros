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

/* Heap. The family default is 3 MiB, sized for the MPS2-AN385 demo cells on a
 * board with 16 MiB of SRAM. This one has 3 GiB of DDR and exists to host REAL
 * consumer images: the ASI controller (MPC + PID, a 256-slot parameter store,
 * 16 KiB subscription buffers and CycloneDDS) exhausts 3 MiB during node
 * construction and dies with `*** MALLOC FAILED ***` right after `Network
 * ready` — a failure that reads as a network problem rather than a heap one.
 *
 * 32 MiB is chosen to be uninteresting rather than tuned: it is far past what
 * any current cell needs and still a fraction of both the DDR and this board's
 * 48 MiB RAM window (the heap is a static array in .bss, so it must fit there).
 * A consumer that wants a different number sets NROS_FREERTOS_HEAP_KB. */
#ifndef NROS_FREERTOS_HEAP_KB
#define NROS_FREERTOS_HEAP_KB 32768
#endif

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
