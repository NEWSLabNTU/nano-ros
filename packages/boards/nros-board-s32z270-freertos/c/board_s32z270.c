/*
 * board_s32z270.c — everything about this board that is not shared
 * (phase-372 W2, mirroring board_mps2.c's contract).
 *
 * Cortex-R52 is ARMv8-R AArch32: a classic 8-entry ARM vector table (no
 * NVIC), boot in ARM state, GIC + a timer the KERNEL PORT owns. This TU
 * carries:
 *   - the exception vector table + Reset_Handler (stack init, data copy,
 *     bss zero, .init_array walk lives in the shared layout, jump to main)
 *   - Default_Handler (spin) for unhandled exceptions
 *   - WEAK netif hooks (`nros_board_register_netif` / `nros_board_poll_netif`)
 *     that FAIL LOUD — the real implementation is the consumer's NXP-RTD
 *     NETC glue (NXP Confidential; cannot live in this repo). Mirrors the
 *     LAN9118 strong-override shape on MPS2 (phase-372 W3).
 *   - WEAK tick hooks for the in-tree GCC/ARM_CRx_No_GIC port
 *     (`configSETUP_TICK_INTERRUPT` / `configCLEAR_TICK_INTERRUPT` map here).
 *     The NXP GCC/ARM_CR52_GIC port brings its own tick and ignores these.
 *
 * BOOT-COMPLETENESS CAVEAT (deliberate, phase-372 W5): this startup is the
 * LINK-COMPLETE skeleton. EL2->EL1 drop, MPU regions (including the
 * non-cacheable NETC window), cache enable and FPU access (cp15) are the
 * consumer's early-init today — ASI's Apache-licensed `cp15_arm.S` +
 * `board_init.c` are the proven seed and migrate here when W5 brings
 * hardware to the lane.
 */

#include <stdint.h>
#include <string.h>

#include "FreeRTOS.h"
#include "task.h"

/* ---- Linker symbols (shared section layout) ---- */
extern uint32_t _estack;

/* ---- Forward declarations ---- */
void Reset_Handler(void);
void Default_Handler(void);

extern int main(void);

/* FreeRTOS ARM_CRx_No_GIC port handlers. */
extern void FreeRTOS_IRQ_Handler(void);
extern void FreeRTOS_SVC_Handler(void);

/* ---- Vector table ----
 * Classic ARM layout: eight instruction slots. LDR PC-relative jumps keep
 * the table position-independent; VBAR must point here (consumer early
 * init; the reset path works from any base because QEMU/ROM enters at
 * Reset_Handler via the ELF entry).
 */
__attribute__((section(".isr_vector"), naked, used)) void vector_table(void) {
    __asm volatile("ldr pc, =Reset_Handler        \n" /* reset */
                   "ldr pc, =Default_Handler      \n" /* undefined instruction */
                   "ldr pc, =FreeRTOS_SVC_Handler \n" /* SVC */
                   "ldr pc, =Default_Handler      \n" /* prefetch abort */
                   "ldr pc, =Default_Handler      \n" /* data abort */
                   "ldr pc, =Default_Handler      \n" /* reserved */
                   "ldr pc, =FreeRTOS_IRQ_Handler \n" /* IRQ */
                   "ldr pc, =Default_Handler      \n" /* FIQ */
    );
}

void Default_Handler(void) {
    for (;;) {
    }
}

/* ---- Reset ----
 * Minimal A32 boot: set SP for SVC mode (FreeRTOS tasks run in SYS/USR;
 * the port switches modes itself), copy .data, zero .bss, call main.
 * The .init_array walk runs from the shared boot lanes (issue 0733
 * pattern), not here.
 */
__attribute__((naked, used)) void Reset_Handler(void) {
    __asm volatile(
        /* SVC-mode stack = top of RAM (the shared layout's _estack). */
        "ldr sp, =_estack              \n"
        /* IRQ-mode stack: carve 4 KiB below the SVC stack. */
        "cps #0x12                     \n" /* IRQ mode */
        "ldr sp, =_estack              \n"
        "sub sp, sp, #0x1000           \n"
        "cps #0x13                     \n" /* back to SVC */
        "bl  nros_board_c_startup      \n"
        "b   .                         \n");
}

__attribute__((used)) void nros_board_c_startup(void) {
    extern uint32_t _sdata, _edata, _etext, _sbss, _ebss;
    memcpy(&_sdata, &_etext, (size_t)((uintptr_t)&_edata - (uintptr_t)&_sdata));
    memset(&_sbss, 0, (size_t)((uintptr_t)&_ebss - (uintptr_t)&_sbss));
    (void)main();
    for (;;) {
    }
}

/* ---- Weak netif hooks (phase-372 W3 seam) ----
 * The generic `nros-board-freertos/c/network_glue.c` invokes these through
 * its weak-hook protocol; MPS2 overrides them with LAN9118 strong symbols.
 * On S32Z270 the ethernet is NXP NETC (RTD, NXP Confidential): the CONSUMER
 * provides the strong overrides (ASI: `ethif_shim.c`). These defaults fail
 * loud so a bundle-only image says WHY it has no network instead of
 * timing out silently (RFC-0052 fail-loud).
 */
/* The parameter list MATCHES `nros-board-freertos/c/network_glue.c` — issue
 * 0769. This was `(void)` while the sole caller
 * (`network_glue.c:nros_board_network_init`) passes four pointers, and the two
 * are not merely different prototypes of different functions: they are two WEAK
 * definitions of the SAME symbol, in crates that compose. This crate depends on
 * `nros-board-freertos`, whose `build.rs` compiles `network_glue.c` into
 * `libfreertos_glue.a`, so both land in an S32Z270 image and the LINKER picks
 * one by archive order — the hazard issue 0050 exists to catch.
 *
 * Aligning the signature removes the ABI half. It does NOT remove the tie: two
 * weak defaults still coexist, both return -1, and only THIS one prints. The
 * fail-loud promise (RFC-0052) therefore still rides on winning a tie it does
 * not control — see issue 0769 for the elimination. */
__attribute__((weak)) int nros_board_register_netif(
    const uint8_t mac[6],
    const uint8_t ip[4],
    const uint8_t netmask[4],
    const uint8_t gw[4])
{
    (void)mac; (void)ip; (void)netmask; (void)gw;
    extern int printf(const char*, ...);
    printf("nros-board-s32z270-freertos: no netif — the NXP NETC glue is "
           "consumer-provided (strong nros_board_register_netif override); "
           "see phase-372 W3\n");
    return -1;
}

__attribute__((weak)) void nros_board_poll_netif(void) {}

/* ---- Weak tick hooks (GCC/ARM_CRx_No_GIC port only) ----
 * FreeRTOSConfig.h maps configSETUP_TICK_INTERRUPT/configCLEAR_TICK_INTERRUPT
 * to these. A hardware consumer implements them against a GPT / the generic
 * timer — or uses the NXP GCC/ARM_CR52_GIC port, which owns its own tick.
 */
__attribute__((weak)) void nros_board_setup_tick_interrupt(void) {
    extern int printf(const char*, ...);
    printf("nros-board-s32z270-freertos: no tick source — implement "
           "nros_board_setup_tick_interrupt() (GPT/generic timer) or use the "
           "NXP GCC/ARM_CR52_GIC port; see phase-372 W2\n");
}

__attribute__((weak)) void nros_board_clear_tick_interrupt(void) {}
