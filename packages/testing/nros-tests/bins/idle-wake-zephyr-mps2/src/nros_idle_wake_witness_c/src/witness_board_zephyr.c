/// @file witness_board_zephyr.c
/// @brief The idle-wake witness's measurement on Zephyr (mps2/an385).
///
/// Counted from Zephyr's user tracing hooks (`CONFIG_TRACING_USER`): every
/// switch-in of the executor's thread is one wake, and the last LAN9118
/// receive interrupt (IPSR naming its IRQ) is stamped on the 25 MHz cycle
/// counter. Built only inside a Zephyr build (see CMakeLists.txt).

#include <zephyr/devicetree.h>
#include <zephyr/kernel.h>
#include <cmsis_core.h>

#include "witness_board.h"

/* The LAN9118's IRQ line; the exception number is 16 above it. */
#define ETH_IRQ DT_IRQN(DT_NODELABEL(eth0))

/* The executor's thread, learned when the window opens, and the switch-in
 * count of that thread while the window is open. Written from the
 * scheduler's switch-in hook, so plain volatiles: the hook runs with
 * interrupts locked on a single-core Cortex-M3. */
static struct k_thread* volatile exec_thread;
static volatile uint32_t switch_ins;
static volatile bool window_open;

void sys_trace_thread_switched_in_user(void) {
    if (window_open && k_current_get() == exec_thread) {
        switch_ins++;
    }
}

/* The cycle count at the last LAN9118 receive interrupt. */
static volatile uint32_t last_eth_cyc;
static volatile bool eth_seen;

void sys_trace_isr_enter_user(void) {
    if ((__get_IPSR() & 0x1FFu) == 16u + (uint32_t)ETH_IRQ) {
        last_eth_cyc = k_cycle_get_32();
        eth_seen = true;
    }
}

int64_t witness_board_now_us(void) {
    return (int64_t)k_ticks_to_us_floor64(k_uptime_ticks());
}

void witness_board_open_window(void) {
    exec_thread = k_current_get();
    switch_ins = 0;
    window_open = true;
}

uint32_t witness_board_close_window(void) {
    window_open = false;
    return switch_ins;
}

bool witness_board_arrival_us(uint32_t* us) {
    uint32_t now = k_cycle_get_32();
    if (!eth_seen) {
        return false;
    }
    *us = k_cyc_to_us_floor32(now - last_eth_cyc);
    return true;
}
