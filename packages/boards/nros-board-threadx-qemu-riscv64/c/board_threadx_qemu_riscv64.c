/*
 * board_threadx_qemu_riscv64.c — board-specific glue for nros ThreadX
 * on the QEMU RISC-V 64-bit virt machine.
 *
 * Phase 152.2.B.1 — the shared `tx_application_define` + byte-pool +
 * app-thread plumbing lives in `nros-board-common`'s
 * `c/threadx_hooks.c`. This file fills in:
 *
 *   - `nros_threadx_set_config(...)` — RISC-V flavour (no
 *     `interface_name` parameter).
 *   - `nros_board_log` → `uart_puts` (16550 UART at 0x10000000).
 *   - `nros_board_init_eth` → full NetX-Duo + virtio-net + BSD
 *     socket bring-up.
 *   - `nros_board_compute_rng_seed` → IP/MAC-derived.
 *   - Strong-override `nros_board_app_stack_size` (512 KB) and
 *     `nros_board_app_priority` (15, below zenoh-pico read/lease
 *     priority 14 per the 120.3 keep-alive fix).
 *   - Global `errno` (bare-metal has no TLS).
 */

#include <stdint.h>
#include <stdlib.h>
#include <string.h>

#include "tx_api.h"
#include "nx_api.h"
#include "nxd_bsd.h"
#include "virtio_net_nx.h"

/* ---- Global errno for bare-metal (no TLS) ---- */
/* nxd_bsd.h defines `errno` as a per-thread macro; undef before the
 * declaration so the symbol is a plain global on this TU. The macro
 * still applies to every other TU that includes nxd_bsd.h. */
#undef errno
int errno;

/* ---- UART output ---- */
extern void uart_puts(const char *s);

/* ---- Overlay-tunable parameters (strong override of the weak
 * getters in threadx_hooks.c) ---- */
/* Phase 247 W3.2 (#50) — strong override functions for the weak
 * `nros_board_app_stack_size`/`_priority` getters. Was a strong
 * *data* override; the weak-data shape needed the 155.A `drop const`
 * workaround (gcc folded the weak 64 KB at the call site, dropping
 * this override → Rust closure stack overflowed silently). Functions
 * can't be const-folded across the TU boundary, so the override wins
 * deterministically and the workaround is no longer load-bearing. */
uint32_t nros_board_app_stack_size(void) { return 512 * 1024; }
/* zenoh-pico's read/lease tasks default to ThreadX priority 14
 * (`Z_TASK_PRIORITY` in `zenoh-pico/src/system/threadx/.../platform.h`).
 * App must run at strictly lower priority (= higher numeric value)
 * so zenoh-pico keep-alive gets CPU during the action server's
 * spin loop. Pre-120.3 this was 4 → preempted keep-alive →
 * 10 s lease expiry → router unregistered all queryables before
 * the client's first z_get even arrived. */
uint32_t nros_board_app_priority(void) { return 15; }

/* ---- Sizing constants for the local NetX bring-up ---- */
#define PACKET_SIZE             1536
#define PACKET_COUNT            30
#define PACKET_POOL_SIZE        ((PACKET_SIZE + sizeof(NX_PACKET)) * PACKET_COUNT)
#define IP_STACK_SIZE           4096
#define IP_THREAD_PRIORITY      1
#define ARP_POOL_SIZE           1024
/* Phase 120.3: bumped from 2 KB to 8 KB. The BSD thread runs
 * nx_bsd_thread_entry's periodic socket scan; on rv64 with LP64D
 * each frame is ~120 B and recursion in NetX BSD's poll loops
 * overflows 2 KB silently, corrupting adjacent .bss state. */
#define BSD_STACK_SIZE          8192
/* Phase 97.4.threadx-riscv64 — NetX Duo BSD-Support docs note
 * "this thread should be the highest priority task defined in
 * the program". With BSD = APP+1, the cooperative DDS poll
 * loop in the app thread starves the BSD thread → deferred packet
 * processing never fires → `bind`/`getaddrinfo` hang inside
 * `create_subscription`. Priority 2 (one below the IP helper,
 * two above the app) lets BSD run when the app sleeps. */
#define BSD_THREAD_PRIORITY     2

/* ---- Static NetX objects ---- */
static NX_PACKET_POOL packet_pool;
static NX_IP          ip_instance;

/* ---- Configuration (set from Rust before tx_kernel_enter) ---- */
static uint8_t cfg_ip[4]      = {192, 0, 3, 10};
static uint8_t cfg_netmask[4] = {255, 255, 255, 0};
static uint8_t cfg_gateway[4] = {192, 0, 3, 1};
static uint8_t cfg_mac[6]     = {0x52, 0x54, 0x00, 0x12, 0x34, 0x56};

/* VirtIO MMIO config: slot 0 = 0x10001000, IRQ 1 */
static uint64_t cfg_mmio_base = 0x10001000;
static int      cfg_irq_num   = 1;

/* FFI: called from Rust to set config. Signature matches the
 * unified 5-arg form (Phase 152.2.B.4). The `interface_name`
 * parameter is unused here — bare-metal RISC-V QEMU has no
 * host network interface to bind to.
 *
 * Return type is `void` by contract — Phase 214.A.1: this impl
 * is pure memcpy into static storage, has no meaningful failure
 * modes, and is called from board startup before networking
 * begins. Callers (e.g. `startup.c`) do not capture a return
 * code. If a future revision adds I/O or validation that can
 * fail, this contract changes; bump to `int` + propagate. */
void nros_threadx_set_config(
    const uint8_t *ip,
    const uint8_t *netmask,
    const uint8_t *gateway,
    const uint8_t *mac,
    const char *interface_name)
{
    (void)interface_name;
    memcpy(cfg_ip,      ip,      4);
    memcpy(cfg_netmask, netmask, 4);
    memcpy(cfg_gateway, gateway, 4);
    memcpy(cfg_mac,     mac,     6);
}

/* ---- Weak-hook impls ---- */

void nros_board_log(const char *s)
{
    if (s) { uart_puts(s); }
}

void nros_board_compute_rng_seed(uint32_t *out)
{
    if (!out) { return; }
    uint32_t seed = ((uint32_t)cfg_ip[0] << 24) | ((uint32_t)cfg_ip[1] << 16)
                  | ((uint32_t)cfg_ip[2] << 8)  | (uint32_t)cfg_ip[3];
    seed = seed * 2654435761u;  /* Knuth multiplicative hash */
    seed ^= ((uint32_t)cfg_mac[4] << 8) | (uint32_t)cfg_mac[5];
    *out = seed;
}

int ddsrt_threadx_get_primary_ipv4(
    uint32_t *addr,
    uint32_t *netmask,
    uint32_t *broadcast,
    char *name,
    size_t name_size)
{
    uint32_t ip = ((uint32_t)cfg_ip[0] << 24) | ((uint32_t)cfg_ip[1] << 16)
                | ((uint32_t)cfg_ip[2] << 8)  | (uint32_t)cfg_ip[3];
    uint32_t mask = ((uint32_t)cfg_netmask[0] << 24) | ((uint32_t)cfg_netmask[1] << 16)
                  | ((uint32_t)cfg_netmask[2] << 8)  | (uint32_t)cfg_netmask[3];
    if (addr) { *addr = ip; }
    if (netmask) { *netmask = mask; }
    if (broadcast) { *broadcast = ip | ~mask; }
    if (name && name_size > 0) {
        strncpy(name, "nx0", name_size - 1);
        name[name_size - 1] = '\0';
    }
    return 0;
}

/* nros_board_init_eth — called from the generic
 * `tx_application_define` after byte-pool create + RNG seed. Owns
 * the entire NetX-Duo + virtio-net + BSD bring-up. We re-fetch the
 * shared byte pool through the platform getter rather than
 * duplicating storage. */
extern TX_BYTE_POOL *zpico_threadx_byte_pool;

static void log_hex_status(UINT status)
{
    static const char hex[] = "0123456789abcdef";
    char buf[10];
    buf[0] = '0';
    buf[1] = 'x';
    for (int i = 7; i >= 0; i--) {
        buf[9 - i - 2] = hex[(status >> (i * 4)) & 0xF];
    }
    /* buf[8]/[9] = last two nibbles already written via the loop;
     * close with NUL */
    buf[9 - 0 - 2 + 1] = '\0';
    uart_puts(buf);
}

int nros_board_init_eth(void)
{
    UINT   status;
    UCHAR *pointer;
    TX_BYTE_POOL *pool = zpico_threadx_byte_pool;

    if (!pool) {
        uart_puts("ERROR: nros_board_init_eth called before byte pool\n");
        return -1;
    }

    uart_puts("[board] Initializing NetX system...\n");
    nx_system_initialize();

    uart_puts("[board] Configuring virtio-net...\n");
    {
        struct virtio_net_nx_config vcfg;
        vcfg.mmio_base = cfg_mmio_base;
        vcfg.irq_num   = cfg_irq_num;
        virtio_net_nx_configure(&vcfg);
    }

    /* Allocate packet pool memory */
    status = tx_byte_allocate(pool, (VOID **)&pointer, PACKET_POOL_SIZE, TX_NO_WAIT);
    if (status != TX_SUCCESS) {
        uart_puts("ERROR: packet pool memory alloc failed\n");
        return -1;
    }
    status = nx_packet_pool_create(&packet_pool, "nros_packet_pool",
                                    PACKET_SIZE, pointer, PACKET_POOL_SIZE);
    if (status != NX_SUCCESS) {
        uart_puts("ERROR: packet pool create failed\n");
        return -1;
    }

    /* IP stack memory */
    status = tx_byte_allocate(pool, (VOID **)&pointer, IP_STACK_SIZE, TX_NO_WAIT);
    if (status != TX_SUCCESS) {
        uart_puts("ERROR: IP stack memory alloc failed\n");
        return -1;
    }

    uart_puts("[board] Creating IP instance...\n");
    ULONG ip_addr = ((ULONG)cfg_ip[0] << 24) | ((ULONG)cfg_ip[1] << 16)
                  | ((ULONG)cfg_ip[2] << 8)  | (ULONG)cfg_ip[3];
    ULONG netmask = ((ULONG)cfg_netmask[0] << 24) | ((ULONG)cfg_netmask[1] << 16)
                  | ((ULONG)cfg_netmask[2] << 8)  | (ULONG)cfg_netmask[3];

    status = nx_ip_create(&ip_instance, "nros_ip", ip_addr, netmask,
                           &packet_pool, virtio_net_nx_driver,
                           pointer, IP_STACK_SIZE, IP_THREAD_PRIORITY);
    if (status != NX_SUCCESS) {
        uart_puts("ERROR: IP create failed\n");
        return -1;
    }

    ULONG gw_addr = ((ULONG)cfg_gateway[0] << 24) | ((ULONG)cfg_gateway[1] << 16)
                  | ((ULONG)cfg_gateway[2] << 8)  | (ULONG)cfg_gateway[3];
    nx_ip_gateway_address_set(&ip_instance, gw_addr);

    uart_puts("[board] Enabling ARP...\n");
    status = tx_byte_allocate(pool, (VOID **)&pointer, ARP_POOL_SIZE, TX_NO_WAIT);
    if (status != TX_SUCCESS) {
        uart_puts("ERROR: ARP pool memory alloc failed\n");
        return -1;
    }
    status = nx_arp_enable(&ip_instance, pointer, ARP_POOL_SIZE);
    if (status != NX_SUCCESS) {
        uart_puts("ERROR: ARP enable failed\n");
        return -1;
    }

    uart_puts("[board] Enabling TCP/UDP/ICMP/IGMP...\n");
    /* Phase 127.B.5 — IGMP needed for both RX (IP_ADD_MEMBERSHIP
     * setsockopt) and TX (class-D multicast send path consults
     * IGMP join list to derive L2 multicast MAC). */
    nx_tcp_enable(&ip_instance);
    nx_udp_enable(&ip_instance);
    nx_icmp_enable(&ip_instance);
    nx_igmp_enable(&ip_instance);

    uart_puts("[board] Initializing BSD sockets...\n");
    status = tx_byte_allocate(pool, (VOID **)&pointer, BSD_STACK_SIZE, TX_NO_WAIT);
    if (status != TX_SUCCESS) {
        uart_puts("ERROR: BSD stack memory alloc failed\n");
        return -1;
    }
    status = nx_bsd_initialize(&ip_instance, &packet_pool,
                                (CHAR *)pointer, BSD_STACK_SIZE,
                                BSD_THREAD_PRIORITY);
    if (status != NX_SUCCESS) {
        uart_puts("ERROR: BSD initialize failed, status=");
        log_hex_status(status);
        uart_puts("\n");
        return -1;
    }
    uart_puts("[board] BSD sockets initialized\n");
    return 0;
}
