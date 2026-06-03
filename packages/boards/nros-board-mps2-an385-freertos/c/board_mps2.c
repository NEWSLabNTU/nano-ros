/*
 * board_mps2.c — MPS2-AN385 board-specific startup + LAN9118 diag
 *
 * Phase 152.1.B.1 — extracted from build.rs's `STARTUP_C` const.
 * Contains:
 *   - Cortex-M3 vector table (`isr_vector`)
 *   - `Reset_Handler` (data copy + bss zero + jump to Rust `main`)
 *   - `Default_Handler` (infinite loop for unhandled IRQs)
 *   - Low-level LAN9118 register-poking diagnostic
 *     (`nros_freertos_diag_network`)
 *
 * Stays in the per-board overlay even after the generic
 * `nros-board-freertos` crate lifts the FreeRTOS + lwIP plumbing —
 * MPS2-AN385's vector table + LAN9118 register map are
 * board-specific.
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

/* Rust entry point.
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

/* Semihosting helper exported by freertos_hooks.c */
extern void semihosting_write0(const char *s);

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
    /* Jump to Rust entry. `main` returns `i32`; ignore the value here
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

/* ---- LAN9118 register-level diagnostic ---- */
static void semihosting_write_int(int val) {
    char buf[12];
    char *p = buf + sizeof(buf) - 1;
    *p = '\0';
    if (val < 0) { semihosting_write0("-"); val = -val; }
    if (val == 0) { semihosting_write0("0"); return; }
    while (val > 0) { *--p = '0' + (val % 10); val /= 10; }
    semihosting_write0(p);
}

/*
 * Low-level network diagnostic: send a raw ARP request via LAN9118
 * and check if packets flow through the TX/RX path.
 */
void nros_freertos_diag_network(void) {
    uint32_t base = LAN9118_BASE_DEFAULT;

    /* Read MAC_CR via indirect MAC CSR */
    {
        /* Wait for MAC not busy */
        while (*(volatile uint32_t *)(uintptr_t)(base + 0xA4) & (1u << 31)) {}
        /* Issue read of MAC_CR (index 1) */
        *(volatile uint32_t *)(uintptr_t)(base + 0xA4) = (1u << 31) | (1u << 30) | 1;
        while (*(volatile uint32_t *)(uintptr_t)(base + 0xA4) & (1u << 31)) {}
        uint32_t mac_cr = *(volatile uint32_t *)(uintptr_t)(base + 0xA8);
        semihosting_write0("  [diag] MAC_CR: 0x");
        {
            static const char hex[] = "0123456789abcdef";
            char buf[9];
            for (int i = 7; i >= 0; i--) {
                buf[7 - i] = hex[(mac_cr >> (i * 4)) & 0xF];
            }
            buf[8] = '\0';
            semihosting_write0(buf);
        }
        semihosting_write0(" (RXEN=");
        semihosting_write0((mac_cr & (1u << 2)) ? "YES" : "NO");
        semihosting_write0(", TXEN=");
        semihosting_write0((mac_cr & (1u << 3)) ? "YES" : "NO");
        semihosting_write0(")\n");
    }

    /* Check TX FIFO free space */
    uint32_t tx_inf = *(volatile uint32_t *)(uintptr_t)(base + 0x80);
    uint32_t tx_free = tx_inf & 0xFFFF;
    semihosting_write0("  [diag] TX FIFO free: ");
    semihosting_write_int((int)tx_free);
    semihosting_write0(" bytes\n");

    /* Check RX FIFO status */
    uint32_t rx_inf = *(volatile uint32_t *)(uintptr_t)(base + 0x7C);
    uint32_t rx_used = (rx_inf >> 16) & 0xFF;
    semihosting_write0("  [diag] RX status entries: ");
    semihosting_write_int((int)rx_used);
    semihosting_write0("\n");

    /* Check INT_STS for TX/RX activity */
    uint32_t int_sts = *(volatile uint32_t *)(uintptr_t)(base + 0x58);
    semihosting_write0("  [diag] INT_STS: 0x");
    {
        static const char hex[] = "0123456789abcdef";
        char buf[9];
        for (int i = 7; i >= 0; i--) {
            buf[7 - i] = hex[(int_sts >> (i * 4)) & 0xF];
        }
        buf[8] = '\0';
        semihosting_write0(buf);
    }
    semihosting_write0("\n");

    /* Send a raw ARP request to test TX path */
    /* ARP request: who has 192.0.3.1? tell 192.0.3.10 */
    uint8_t *our_mac = lan9118_netif.hwaddr;
    semihosting_write0("  [diag] netif MAC: ");
    {
        static const char hex[] = "0123456789abcdef";
        for (int i = 0; i < 6; i++) {
            char buf[4];
            buf[0] = hex[our_mac[i] >> 4];
            buf[1] = hex[our_mac[i] & 0xF];
            buf[2] = (i < 5) ? ':' : '\n';
            buf[3] = '\0';
            semihosting_write0(buf);
        }
    }

    uint8_t arp_frame[42] = {
        /* Ethernet header */
        0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,  /* Destination: broadcast */
        our_mac[0], our_mac[1], our_mac[2],
        our_mac[3], our_mac[4], our_mac[5],  /* Source: our MAC */
        0x08, 0x06,                            /* EtherType: ARP */
        /* ARP payload */
        0x00, 0x01,                            /* Hardware type: Ethernet */
        0x08, 0x00,                            /* Protocol type: IPv4 */
        0x06,                                  /* Hardware size: 6 */
        0x04,                                  /* Protocol size: 4 */
        0x00, 0x01,                            /* Opcode: request */
        our_mac[0], our_mac[1], our_mac[2],
        our_mac[3], our_mac[4], our_mac[5],  /* Sender MAC */
        0xC0, 0x00, 0x03, 0x0A,              /* Sender IP: 192.0.3.10 */
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00,  /* Target MAC: unknown */
        0xC0, 0x00, 0x03, 0x01               /* Target IP: 192.0.3.1 */
    };

    /* Write TX Command A */
    uint32_t cmd_a = (1u << 13) | (1u << 12) | 42;  /* FIRST_SEG | LAST_SEG | len */
    *(volatile uint32_t *)(uintptr_t)(base + 0x20) = cmd_a;
    /* Write TX Command B */
    uint32_t cmd_b = (42u << 16) | 42;
    *(volatile uint32_t *)(uintptr_t)(base + 0x20) = cmd_b;
    /* Write frame data (DWORD-aligned) */
    uint32_t nwords = (42 + 3) / 4;
    for (uint32_t i = 0; i < nwords; i++) {
        uint32_t word = 0;
        for (int b = 0; b < 4 && (i * 4 + b) < 42; b++) {
            word |= (uint32_t)arp_frame[i * 4 + b] << (b * 8);
        }
        *(volatile uint32_t *)(uintptr_t)(base + 0x20) = word;
    }
    semihosting_write0("  [diag] ARP request sent (42 bytes) for 192.0.3.1\n");

    /* Check RX immediately after TX (synchronous delivery check) */
    rx_inf = *(volatile uint32_t *)(uintptr_t)(base + 0x7C);
    rx_used = (rx_inf >> 16) & 0xFF;
    semihosting_write0("  [diag] RX immediately after TX: ");
    semihosting_write_int((int)rx_used);
    semihosting_write0("\n");

    /* Also send ARP for 10.0.2.2 (QEMU user-mode gateway) */
    arp_frame[38] = 10; arp_frame[39] = 0; arp_frame[40] = 2; arp_frame[41] = 2;
    cmd_a = (1u << 13) | (1u << 12) | 42;
    *(volatile uint32_t *)(uintptr_t)(base + 0x20) = cmd_a;
    cmd_b = (42u << 16) | 42;
    *(volatile uint32_t *)(uintptr_t)(base + 0x20) = cmd_b;
    for (uint32_t i = 0; i < nwords; i++) {
        uint32_t word = 0;
        for (int b = 0; b < 4 && (i * 4 + b) < 42; b++) {
            word |= (uint32_t)arp_frame[i * 4 + b] << (b * 8);
        }
        *(volatile uint32_t *)(uintptr_t)(base + 0x20) = word;
    }
    semihosting_write0("  [diag] ARP request sent (42 bytes) for 10.0.2.2\n");

    /* Check RX immediately after second TX (synchronous delivery check) */
    rx_inf = *(volatile uint32_t *)(uintptr_t)(base + 0x7C);
    rx_used = (rx_inf >> 16) & 0xFF;
    semihosting_write0("  [diag] RX immediately after 2nd TX: ");
    semihosting_write_int((int)rx_used);
    semihosting_write0("\n");

    /* Now wait for async delivery (WFI triggers QEMU main loop) */
    /* Use busy-wait with WFI instead of vTaskDelay to avoid poll task consuming frames */
    for (int i = 0; i < 200; i++) {
        __asm__ volatile("wfi");
        rx_inf = *(volatile uint32_t *)(uintptr_t)(base + 0x7C);
        rx_used = (rx_inf >> 16) & 0xFF;
        if (rx_used > 0) {
            semihosting_write0("  [diag] RX arrived after ");
            semihosting_write_int(i);
            semihosting_write0(" WFI iterations: ");
            semihosting_write_int((int)rx_used);
            semihosting_write0(" entries\n");
            break;
        }
    }

    /* Final check */
    rx_inf = *(volatile uint32_t *)(uintptr_t)(base + 0x7C);
    rx_used = (rx_inf >> 16) & 0xFF;
    semihosting_write0("  [diag] RX final: ");
    semihosting_write_int((int)rx_used);
    semihosting_write0(" entries\n");

    /* TX status check */
    int_sts = *(volatile uint32_t *)(uintptr_t)(base + 0x58);
    semihosting_write0("  [diag] INT_STS final: 0x");
    {
        static const char hex[] = "0123456789abcdef";
        char buf[9];
        for (int i = 7; i >= 0; i--) {
            buf[7 - i] = hex[(int_sts >> (i * 4)) & 0xF];
        }
        buf[8] = '\0';
        semihosting_write0(buf);
    }
    semihosting_write0("\n");
}
