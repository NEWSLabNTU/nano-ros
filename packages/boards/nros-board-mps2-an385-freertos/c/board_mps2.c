/*
 * board_mps2.c — everything about this board that is not shared.
 *
 * phase-337 W5 — this is the whole per-board C surface, and BOTH lanes (cargo
 * and CMake) compile it. It contains, and should only ever contain:
 *   - the Cortex-M3 vector table (`isr_vector`)
 *   - `Reset_Handler` (data copy + bss zero + jump to `main`)
 *   - `Default_Handler` (infinite loop for unhandled IRQs)
 *   - the LAN9118 netif registration + poll (the strong overrides for
 *     `network_glue.c`'s weak `nros_board_*` hooks)
 *
 * W5.c removed `nros_freertos_diag_network` — ~180 lines of raw LAN9118 CSR
 * pokes and a hand-assembled ARP frame, duplicated into the retired
 * `startup.c` as well, and called from no path in either lane. The technique
 * it demonstrated is written up in
 * `docs/guides/freertos-lan9118-debugging.md`, which is where a debugging aid
 * belongs.
 */

#include <stdint.h>
#include <string.h>

#include "FreeRTOS.h"
#include "task.h"

#include "lwip/netif.h"
#include "lwip/netifapi.h"
#include "lwip/ip4_addr.h"
#include "lwip/tcpip.h"

#include "lan9118_lwip.h"

/* ---- Linker symbols ---- */
extern uint32_t _etext;
extern uint32_t _sdata;
extern uint32_t _edata;
extern uint32_t _sbss;
extern uint32_t _ebss;
extern uint32_t _estack;

/* ---- Forward declarations ---- */
void Reset_Handler(void);
void Default_Handler(void);
void SysTick_Handler(void);  /* defined in freertos_hooks.c */

/* FreeRTOS port handlers — installed directly in the vector table.
 * FreeRTOS asserts that these exact function pointers appear in the
 * vector table, so wrapper functions are not allowed. */
extern void xPortPendSVHandler(void);
extern void vPortSVCHandler(void);

/* Firmware entry point.
 *
 * phase-337 W5.b — ONE symbol for both lanes: on the Rust lane `main` is the
 * `#[unsafe(no_mangle)] pub extern "C" fn main() -> i32` the Entry pkg emits;
 * on the C/C++ lane it is `nros-board-freertos/c/freertos_c_entry.c::main`.
 * The retired `startup.c` had its own `Reset_Handler` calling its own
 * `_start`, which is exactly how the shadow copy stayed alive.
 *
 * Phase 212.M-F.15 — the firmware binary's entry point is the standard
 * `#[unsafe(no_mangle)] pub extern "C" fn main() -> i32` symbol emitted
 * by the Phase 212.N Entry pkg shape (`<Board as BoardEntry>::run(...)`
 * → see `examples/qemu-arm-freertos/rust/*_entry/src/main.rs`). The
 * legacy `_start` shape used by the pre-N.7 M.5.a baker fixture was
 * retired together with the `freertos-qemu-mps2-an385-bsp` crate
 * (commit `d99386173`); calling `_start` from `Reset_Handler` left a
 * `rust-lld: error: undefined symbol: _start` regression that this
 * Phase 212.M-F.15 fix closes.
 */
extern int main(void);

/* ---- LAN9118 netif globals (152.1.B.2 lift) ---- *
 * Phase 152.1.B.2 — these lived in `network_glue.c` until 152.1.B.1;
 * 152.1.B.2 moved them into the board-specific TU together with
 * the strong `nros_board_register_netif` + `nros_board_poll_netif`
 * implementations the generic glue invokes through its weak hooks. */
struct netif lan9118_netif;
struct lan9118_config lan9118_cfg;

/* ---- Strong overrides for the generic network_glue.c hooks ---- */

int nros_board_register_netif(
    const uint8_t mac[6],
    const uint8_t ip[4],
    const uint8_t netmask[4],
    const uint8_t gw[4])
{
    ip4_addr_t ipaddr, mask, gateway;

    IP4_ADDR(&ipaddr,  ip[0], ip[1], ip[2], ip[3]);
    IP4_ADDR(&mask,    netmask[0], netmask[1], netmask[2], netmask[3]);
    IP4_ADDR(&gateway, gw[0], gw[1], gw[2], gw[3]);

    lan9118_cfg.base_addr = LAN9118_BASE_DEFAULT;
    memcpy(lan9118_cfg.mac_addr, mac, 6);

    /* Register netif via netifapi (thread-safe: executes in
     * tcpip_thread). netif_add() does NOT set netif_default even
     * with LWIP_SINGLE_NETIF; call netif_set_default() explicitly. */
    if (netifapi_netif_add(&lan9118_netif, &ipaddr, &mask, &gateway,
                           &lan9118_cfg, lan9118_lwip_init, tcpip_input) != ERR_OK) {
        return -1;
    }

    netifapi_netif_set_default(&lan9118_netif);
    netifapi_netif_set_up(&lan9118_netif);
    netifapi_netif_set_link_up(&lan9118_netif);
    return 0;
}

void nros_board_poll_netif(void) {
    lan9118_lwip_poll(&lan9118_netif);
}

/* ---- Interrupt vector table ---- */
typedef void (*vector_fn)(void);

__attribute__((section(".isr_vector"), used))
const vector_fn isr_vector[] = {
    (vector_fn)(uintptr_t)&_estack,  /* Initial MSP */
    Reset_Handler,
    Default_Handler,  /* NMI */
    Default_Handler,  /* HardFault */
    Default_Handler,  /* MemManage */
    Default_Handler,  /* BusFault */
    Default_Handler,  /* UsageFault */
    0, 0, 0, 0,      /* Reserved */
    vPortSVCHandler,
    Default_Handler,  /* DebugMon */
    0,                /* Reserved */
    xPortPendSVHandler,
    SysTick_Handler,
};

/* ---- Reset handler ---- */
void Reset_Handler(void) {
    /* Copy .data from flash to RAM */
    uint32_t *src = &_etext;
    uint32_t *dst = &_sdata;
    while (dst < &_edata) {
        *dst++ = *src++;
    }
    /* Zero .bss */
    dst = &_sbss;
    while (dst < &_ebss) {
        *dst++ = 0;
    }
    /* Jump to the firmware entry. `main` returns `i32`; ignore the value here
     * — `BoardEntry::run` is divergent in practice (FreeRTOS scheduler
     * never returns under normal operation; `exit_success`/`failure`
     * trigger semihosting exit). The trailing `for(;;)` keeps the
     * Cortex-M3 from executing garbage instructions if we ever do
     * fall through. */
    (void)main();
    for (;;) {}
}

/* ---- Default handler (infinite loop) ---- */
void Default_Handler(void) {
    for (;;) {}
}
