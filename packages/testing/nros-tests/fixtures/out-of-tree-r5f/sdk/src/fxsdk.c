/*
 * fxsdk.c: the vendor SDK stand-in's drivers (see ../include/fixture_r5f_sdk.h).
 * Register offsets are the public ZynqMP ones (UG1087).
 */
#include "fixture_r5f_sdk.h"

#define FXSDK_REG(base, off) (*(volatile uint32_t *)((uintptr_t)(base) + (off)))

/* Cadence UART, ZynqMP UART0. */
#define UART0_BASE      0xFF000000u
#define UART_CR         0x00u
#define UART_SR         0x2Cu
#define UART_FIFO       0x30u
#define UART_CR_TX_EN   (1u << 4)
#define UART_CR_RX_EN   (1u << 2)
#define UART_SR_TXFULL  (1u << 4)

void fxsdk_uart_init(void) {
    FXSDK_REG(UART0_BASE, UART_CR) = UART_CR_TX_EN | UART_CR_RX_EN;
}

void fxsdk_uart_putc(char c) {
    while (FXSDK_REG(UART0_BASE, UART_SR) & UART_SR_TXFULL) {
    }
    FXSDK_REG(UART0_BASE, UART_FIFO) = (uint32_t)(unsigned char)c;
}

/* Triple-timer counter 0, counter 1. */
#define TTC0_BASE        0xFF110000u
#define TTC_CLK_CTRL     0x00u
#define TTC_CNT_CTRL     0x0Cu
#define TTC_INTERVAL     0x24u
#define TTC_ISR          0x54u
#define TTC_IER          0x60u
#define TTC_CNT_DISABLE  (1u << 0)
#define TTC_CNT_INTERVAL (1u << 1)
#define TTC_CNT_RESET    (1u << 4)
#define TTC_INT_INTERVAL (1u << 0)

void fxsdk_tick_start(uint32_t cpu_hz, uint32_t hz) {
    FXSDK_REG(TTC0_BASE, TTC_CNT_CTRL) = TTC_CNT_DISABLE;
    FXSDK_REG(TTC0_BASE, TTC_CLK_CTRL) = 0; /* no prescale */
    FXSDK_REG(TTC0_BASE, TTC_INTERVAL) = cpu_hz / hz;
    FXSDK_REG(TTC0_BASE, TTC_IER) = TTC_INT_INTERVAL;
    FXSDK_REG(TTC0_BASE, TTC_CNT_CTRL) = TTC_CNT_INTERVAL | TTC_CNT_RESET;
}

void fxsdk_tick_clear(void) {
    (void)FXSDK_REG(TTC0_BASE, TTC_ISR);
}

volatile unsigned long fxsdk_eoi_word;
