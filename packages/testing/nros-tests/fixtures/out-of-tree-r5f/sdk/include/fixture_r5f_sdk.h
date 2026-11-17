/*
 * fixture_r5f_sdk.h: the vendor SDK stand-in for the out-of-tree board
 * fixture `fixture-r5f-freertos` (phase-575 W2, phase-577 W8).
 *
 * A real board package wraps a BSP its vendor ships under its own licence
 * (TI's Processor SDK RTOS for a J784S4 R5F, say), which nano-ros never
 * fetches. This directory plays that SDK in tests: a "released SDK root" with
 * a version file, headers and sources, written from public register maps
 * (the Xilinx ZynqMP RPU's Cadence UART and triple-timer counter), so CI
 * needs no vendor download. The board package names it through its own
 * variable, FIXTURE_R5F_SDK_ROOT.
 */
#ifndef FIXTURE_R5F_SDK_H
#define FIXTURE_R5F_SDK_H

#include <stdint.h>

#define FXSDK_VERSION "1.0.0"

/* Debug UART: a Cadence UART (ZynqMP UART0). */
void fxsdk_uart_init(void);
void fxsdk_uart_putc(char c);

/* Tick: a triple-timer counter (ZynqMP TTC0), interval mode at `hz`. */
void fxsdk_tick_start(uint32_t cpu_hz, uint32_t hz);
/* Acknowledge the tick interrupt (reading the TTC interrupt register clears it). */
void fxsdk_tick_clear(void);

/* End-of-interrupt word an RTOS port writes the acknowledged interrupt to. */
extern volatile unsigned long fxsdk_eoi_word;

#endif /* FIXTURE_R5F_SDK_H */
