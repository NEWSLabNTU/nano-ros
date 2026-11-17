/*
 * board_fixture_r5f.c: what fixture-r5f-freertos supplies over its vendor SDK.
 *
 * ARMv7-R: a classic 8-entry ARM vector table, boot in ARM state, a timer the
 * kernel port drives through two hooks. This TU carries:
 *   - the vector table and Reset_Handler (stacks, FPU enable, .data copy, .bss
 *     zero, then main);
 *   - the tick hooks for the GCC/ARM_CRx_No_GIC port, over the SDK's timer;
 *   - weak netif hooks that fail loud: this fixture's network driver is out of
 *     its scope, as a vendor's would be in a real board package.
 */

#include <stdint.h>
#include <string.h>

#include "FreeRTOS.h"
#include "fixture_r5f_sdk.h"
#include "task.h"

extern uint32_t _estack;
void Reset_Handler(void);
void Default_Handler(void);
extern int main(void);
extern void FreeRTOS_IRQ_Handler(void);
extern void FreeRTOS_SVC_Handler(void);

__attribute__((section(".isr_vector"), naked, used)) void vector_table(void) {
    __asm volatile("ldr pc, =Reset_Handler        \n"
                   "ldr pc, =Default_Handler      \n"
                   "ldr pc, =FreeRTOS_SVC_Handler \n"
                   "ldr pc, =Default_Handler      \n"
                   "ldr pc, =Default_Handler      \n"
                   "ldr pc, =Default_Handler      \n"
                   "ldr pc, =FreeRTOS_IRQ_Handler \n"
                   "ldr pc, =Default_Handler      \n");
}

void Default_Handler(void) {
    for (;;) {
    }
}

__attribute__((naked, used)) void Reset_Handler(void) {
    __asm volatile(
        /* Enable the VFP before any hard-float code runs: CPACR grants
         * cp10/cp11, then FPEXC.EN. */
        "mrc p15, 0, r0, c1, c0, 2     \n"
        "orr r0, r0, #(0xF << 20)      \n"
        "mcr p15, 0, r0, c1, c0, 2     \n"
        "isb                           \n"
        "mov r0, #0x40000000           \n"
        "vmsr fpexc, r0                \n"
        "ldr sp, =_estack              \n"
        "cps #0x12                     \n" /* IRQ mode */
        "ldr sp, =_estack              \n"
        "sub sp, sp, #0x1000           \n"
        "cps #0x13                     \n" /* SVC */
        "bl  nros_board_c_startup      \n"
        "b   .                         \n");
}

__attribute__((used)) void nros_board_c_startup(void) {
    extern uint32_t _sdata, _edata, _etext, _sbss, _ebss;
    memcpy(&_sdata, &_etext, (size_t)((uintptr_t)&_edata - (uintptr_t)&_sdata));
    memset(&_sbss, 0, (size_t)((uintptr_t)&_ebss - (uintptr_t)&_sbss));
    fxsdk_uart_init();
    (void)main();
    for (;;) {
    }
}

void nros_board_setup_tick_interrupt(void) {
    fxsdk_tick_start(configCPU_CLOCK_HZ, configTICK_RATE_HZ);
}

void nros_board_clear_tick_interrupt(void) {
    fxsdk_tick_clear();
}

/* Parameter list matches nros-board-freertos/c/network_glue.c. */
__attribute__((weak)) int nros_board_register_netif(const uint8_t mac[6], const uint8_t ip[4],
                                                    const uint8_t netmask[4],
                                                    const uint8_t gw[4]) {
    (void)mac;
    (void)ip;
    (void)netmask;
    (void)gw;
    static const char msg[] = "fixture-r5f-freertos: no netif driver in this fixture\n";
    for (const char *p = msg; *p; ++p) {
        fxsdk_uart_putc(*p);
    }
    return -1;
}

__attribute__((weak)) void nros_board_poll_netif(void) {}
