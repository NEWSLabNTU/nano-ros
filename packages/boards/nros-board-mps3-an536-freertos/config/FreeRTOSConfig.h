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

/* Heap. The family's non-zenoh default is 640 KiB, derived from THIS board's
 * own demo cell (issue 1624: `workspace-cpp-mps3-an536-freertos` peaks at
 * 447,944 bytes). This one has 3 GiB of DDR and exists to host REAL
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

/* Every task gets an FPU context (issue 0997).
 *
 * The ARM_CRx ports default to 1: a task is created WITHOUT a floating-point
 * context and the port saves VFP/NEON registers across a context switch only
 * for a task that called vPortTaskUsesFPU(). Nothing here calls it, and it
 * could not be called reliably anyway: this board compiles every object
 * -mfloat-abi=hard -mfpu=neon-fp-armv8, so GCC uses the d-registers in code
 * with no floating point in it -- a 64-bit constant, a struct copy, a NEON
 * memcpy. Measured: Cyclone's event loop loads DDS_NEVER into d8 once and
 * stores it with `vstr d8` for every event it runs; another task's VFP use
 * replaced d8, events were stamped tsched = 0 instead of NEVER, and
 * resched_xevent_if_earlier then refused every reschedule (no time is earlier
 * than 0). The periodic SPDP and PMD events dropped out on their first
 * firing, the participant fell silent ~100 ms after discovery, and every peer
 * expired its lease; the same stale stamp sent delete_xevent down the
 * decrease-key path for an event no longer on the heap, which surfaced later
 * as a heap_4 double free.
 *
 * All 32 d-registers must be saved too -- neon-fp-armv8 is a D32 unit and the
 * image uses d16-d31 in about a thousand instructions -- but that switch,
 * configFPU_D32, is NOT set here: portASM.S does the save and never includes
 * this header, so a definition here would reach port.c (which sizes the
 * frame) and not the assembly (which fills it), and the first switch into a
 * new task would restore a misaligned frame. It is a compile definition on
 * `freertos_config` instead, which both TUs see
 * (cmake/board/nano-ros-board-mps3-an536-freertos.cmake). */
#define configUSE_TASK_FPU_SUPPORT 2

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
