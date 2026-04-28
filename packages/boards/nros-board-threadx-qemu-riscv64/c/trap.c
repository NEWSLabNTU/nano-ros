/**
 * trap.c — RISC-V trap handler with diagnostic output
 *
 * Replacement for ThreadX's example trap.c that actually prints
 * mcause, mepc, and mtval for debugging exceptions.
 */

#include "csr.h"
#include <stdint.h>
#include "uart.h"
#include "hwtimer.h"
#include "plic.h"
#include <tx_port.h>
#include <tx_api.h>

#define OS_IS_INTERUPT(mcause)     (mcause & 0x8000000000000000ull)
#define OS_IS_TICK_INT(mcause)     (mcause == 0x8000000000000007ull)
#define OS_IS_SOFT_INT(mcause)     (mcause == 0x8000000000000003ull)
#define OS_IS_EXT_INT(mcause)      (mcause == 0x800000000000000bull)

extern void _tx_timer_interrupt(void);
/* uart_puts is declared in uart.h as int uart_puts(const char *) */

static void print_hex(uintptr_t val)
{
    static const char hex[] = "0123456789abcdef";
    char buf[17];
    for (int i = 15; i >= 0; i--) {
        buf[15 - i] = hex[(val >> (i * 4)) & 0xF];
    }
    buf[16] = '\0';
    uart_puts(buf);
}

void trap_handler(uintptr_t mcause, uintptr_t mepc, uintptr_t mtval)
{
    if (OS_IS_INTERUPT(mcause)) {
        if (OS_IS_TICK_INT(mcause)) {
            hwtimer_handler();
            _tx_timer_interrupt();
        } else if (OS_IS_EXT_INT(mcause)) {
            int ret = plic_irq_intr();
            if (ret) {
                uart_puts("[INTERRUPT]: handler irq error!\n");
                while (1) ;
            }
        } else {
            uart_puts("[INTERRUPT]: unhandled interrupt, mcause=0x");
            print_hex(mcause);
            uart_puts("\n");
            while (1) ;
        }
    } else {
        uart_puts("\n[EXCEPTION] mcause=0x");
        print_hex(mcause);
        uart_puts(" mepc=0x");
        print_hex(mepc);
        uart_puts(" mtval=0x");
        print_hex(mtval);
        uart_puts("\n");

        /* Decode common causes */
        uintptr_t code = mcause & 0xFFFF;
        switch (code) {
        case 0:  uart_puts("  Instruction address misaligned\n"); break;
        case 1:  uart_puts("  Instruction access fault\n"); break;
        case 2:  uart_puts("  Illegal instruction\n"); break;
        case 3:  uart_puts("  Breakpoint\n"); break;
        case 4:  uart_puts("  Load address misaligned\n"); break;
        case 5:  uart_puts("  Load access fault\n"); break;
        case 6:  uart_puts("  Store/AMO address misaligned\n"); break;
        case 7:  uart_puts("  Store/AMO access fault\n"); break;
        case 8:  uart_puts("  Environment call from U-mode\n"); break;
        case 9:  uart_puts("  Environment call from S-mode\n"); break;
        case 11: uart_puts("  Environment call from M-mode\n"); break;
        case 12: uart_puts("  Instruction page fault\n"); break;
        case 13: uart_puts("  Load page fault\n"); break;
        case 15: uart_puts("  Store/AMO page fault\n"); break;
        default: uart_puts("  Unknown exception code\n"); break;
        }

        /* Dump register state for debugging */
        uint64_t sp_val, ra_val;
        __asm__ volatile("mv %0, sp" : "=r"(sp_val));
        __asm__ volatile("mv %0, ra" : "=r"(ra_val));
        uart_puts("  ra=0x"); print_hex(ra_val);
        uart_puts(" sp=0x"); print_hex(sp_val);
        uart_puts("\n");

        while (1) ;
    }
}
