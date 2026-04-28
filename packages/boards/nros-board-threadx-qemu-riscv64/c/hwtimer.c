/*
 * hwtimer.c — CLINT timer for QEMU RISC-V virt at 100 Hz
 *
 * Based on ThreadX QEMU virt hwtimer.c but with TICKNUM_PER_TIMER
 * set for 100 Hz to match TX_TIMER_TICKS_PER_SECOND = 100.
 *
 * QEMU virt CLINT frequency: 10 MHz
 * Timer period: 10000000 / 100 = 100000 cycles = 100 Hz
 */

#include "csr.h"
#include <stdint.h>

#define CLINT                  (0x02000000L)
#define CLINT_TIME             (CLINT + 0xBFF8)
#define CLINT_TIMECMP(hart_id) (CLINT + 0x4000 + 8 * (hart_id))

#define TICKNUM_PER_SECOND  10000000
#define TICKNUM_PER_TIMER   (TICKNUM_PER_SECOND / 100)  /* 100 Hz */

int hwtimer_init(void)
{
    int hart = riscv_get_core();
    uint64_t time = *((volatile uint64_t *)CLINT_TIME);
    *((volatile uint64_t *)CLINT_TIMECMP(hart)) = time + TICKNUM_PER_TIMER;
    return 0;
}

int hwtimer_handler(void)
{
    int hart = riscv_get_core();
    uint64_t time = *((volatile uint64_t *)CLINT_TIME);
    *((volatile uint64_t *)CLINT_TIMECMP(hart)) = time + TICKNUM_PER_TIMER;
    return 0;
}
