/*
 * hwtimer.h — Timer configuration override for RISC-V 64-bit QEMU virt.
 *
 * Shadows the upstream hwtimer.h to fix the timer frequency.
 * Upstream uses TICKNUM_PER_SECOND/10 = 10 Hz (100ms per tick), but
 * ThreadX expects TX_TIMER_TICKS_PER_SECOND = 100 (10ms per tick).
 *
 * QEMU virt's CLINT timer runs at 10 MHz (10,000,000 ticks/sec).
 * For 100 Hz ThreadX ticks: 10,000,000 / 100 = 100,000 timer ticks per interrupt.
 */

#ifndef RISCV_HWTIMER_H
#define RISCV_HWTIMER_H

#include <stdint.h>

#define TICKNUM_PER_SECOND  10000000
#define TICKNUM_PER_TIMER   (TICKNUM_PER_SECOND / 100)

int hwtimer_init(void);

int hwtimer_handler(void);

#endif
