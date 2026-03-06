/*
 * app_define.c — ThreadX tx_application_define() for QEMU RISC-V virt
 *
 * Called by tx_kernel_enter() after kernel init. Creates the byte pool,
 * packet pool, IP instance (with virtio-net driver), enables protocols,
 * initializes BSD sockets, and spawns the application thread that calls
 * back into Rust.
 */

#include <stdlib.h>
#include <string.h>

#include "tx_api.h"
#include "nx_api.h"
#include "nxd_bsd.h"
#include "virtio_net_nx.h"

/* ---- Global errno for bare-metal (no TLS) ---- */
int errno;

/* ---- Board init (from ThreadX QEMU virt port) ---- */
extern int board_init(void);

/* ---- UART output for diagnostics ---- */
extern void uart_puts(const char *s);

/* ---- zpico-sys expects this global for ThreadX memory allocation ---- */
TX_BYTE_POOL *zpico_threadx_byte_pool;

/* ---- Sizing constants ---- */
#define BYTE_POOL_SIZE          (512 * 1024)    /* 512 KB for all allocations */
#define PACKET_SIZE             1536
#define PACKET_COUNT            30
#define PACKET_POOL_SIZE        ((PACKET_SIZE + sizeof(NX_PACKET)) * PACKET_COUNT)
#define IP_STACK_SIZE           4096
#define IP_THREAD_PRIORITY      1
#define ARP_POOL_SIZE           1024
#define BSD_STACK_SIZE          2048
#define APP_THREAD_STACK_SIZE   (64 * 1024)     /* 64 KB for Executor + zenoh-pico */
#define APP_THREAD_PRIORITY     4

/* ---- Static objects ---- */
static TX_BYTE_POOL     byte_pool;
static UCHAR            byte_pool_storage[BYTE_POOL_SIZE];

static NX_PACKET_POOL   packet_pool;
static NX_IP            ip_instance;
static TX_THREAD        app_thread;

/* ---- Configuration (set from Rust before tx_kernel_enter) ---- */
static uint8_t  cfg_ip[4]      = {192, 0, 3, 10};
static uint8_t  cfg_netmask[4] = {255, 255, 255, 0};
static uint8_t  cfg_gateway[4] = {192, 0, 3, 1};
static uint8_t  cfg_mac[6]     = {0x52, 0x54, 0x00, 0x12, 0x34, 0x56};

/* VirtIO MMIO config: slot 0 = 0x10001000, IRQ 1 */
static uint64_t cfg_mmio_base  = 0x10001000;
static int      cfg_irq_num    = 1;

/* ---- Rust callback (set from Rust before tx_kernel_enter) ---- */
static void (*rust_app_entry)(void *) = 0;
static void *rust_app_arg = 0;

/* ---- FFI: called from Rust to set config ---- */
void nros_threadx_set_config(
    const uint8_t *ip,
    const uint8_t *netmask,
    const uint8_t *gateway,
    const uint8_t *mac)
{
    memcpy(cfg_ip, ip, 4);
    memcpy(cfg_netmask, netmask, 4);
    memcpy(cfg_gateway, gateway, 4);
    memcpy(cfg_mac, mac, 6);
}

/* ---- FFI: called from Rust to set the app callback ---- */
void nros_threadx_set_app_callback(void (*entry)(void *), void *arg)
{
    rust_app_entry = entry;
    rust_app_arg = arg;
}

/* ---- Hex print helpers for diagnostics ---- */
static void diag_hex32(ULONG v)
{
    static const char hx[] = "0123456789abcdef";
    char buf[9];
    for (int i = 7; i >= 0; i--)
        buf[7 - i] = hx[(v >> (i * 4)) & 0xF];
    buf[8] = '\0';
    uart_puts(buf);
}

static void diag_hex64(uint64_t v)
{
    diag_hex32((ULONG)(v >> 32));
    diag_hex32((ULONG)(v & 0xFFFFFFFF));
}

/* ---- App thread entry: invokes the Rust closure ---- */
static void app_thread_entry(ULONG input)
{
    (void)input;

    uart_puts("[app_thread] Started\n");

    /* DIAGNOSTIC: Gateway ping to verify IPv4 round-trip */
    {
        ULONG gw = ((ULONG)cfg_gateway[0] << 24) | ((ULONG)cfg_gateway[1] << 16)
                  | ((ULONG)cfg_gateway[2] << 8)  | (ULONG)cfg_gateway[3];
        NX_PACKET *resp = NULL;
        UINT ping_status;

        uart_puts("[app_thread] Pinging gateway ");
        diag_hex32(gw);
        uart_puts("...\n");

        ping_status = nx_icmp_ping(&ip_instance, gw, "PING", 4, &resp, 500);
        uart_puts("[app_thread] Ping status=");
        diag_hex32(ping_status);
        uart_puts("\n");

        if (ping_status == NX_SUCCESS && resp) {
            uart_puts("[app_thread] Ping OK! IPv4 round-trip works\n");
            nx_packet_release(resp);
        } else {
            uart_puts("[app_thread] Ping FAILED\n");
        }
    }

    if (rust_app_entry) {
        uart_puts("[app_thread] Calling Rust entry...\n");
        rust_app_entry(rust_app_arg);
        uart_puts("[app_thread] Rust entry returned\n");
    } else {
        uart_puts("ERROR: no Rust app callback set\n");
    }
}

/* ---- ThreadX tx_application_define (called by tx_kernel_enter) ---- */
void tx_application_define(void *first_unused_memory)
{
    UINT status;
    UCHAR *pointer;

    (void)first_unused_memory;

    uart_puts("[app_define] Creating byte pool...\n");
    /* Create byte pool for all dynamic allocations */
    status = tx_byte_pool_create(&byte_pool, "nros_byte_pool",
                                  byte_pool_storage, BYTE_POOL_SIZE);
    if (status != TX_SUCCESS) {
        uart_puts("ERROR: byte pool create failed\n");
        return;
    }

    /* Export byte pool for zpico-sys ThreadX memory allocator */
    zpico_threadx_byte_pool = &byte_pool;

    /* Seed the C stdlib RNG with a value unique to this node.
     * Without this, rand() starts from seed 1 on every boot, causing
     * all QEMU instances to generate identical zenoh-pico session IDs
     * (16 bytes from z_random_fill → rand()). zenohd rejects duplicate
     * session IDs, so the second QEMU's z_open() always fails.
     */
    {
        uint32_t seed = ((uint32_t)cfg_ip[0] << 24) | ((uint32_t)cfg_ip[1] << 16)
                      | ((uint32_t)cfg_ip[2] << 8)  | (uint32_t)cfg_ip[3];
        seed = seed * 2654435761u;  /* Knuth multiplicative hash */
        seed ^= ((uint32_t)cfg_mac[4] << 8) | (uint32_t)cfg_mac[5];
        if (seed == 0) seed = 1;
        srand(seed);
    }

    /* Initialize the NetX system */
    uart_puts("[app_define] Initializing NetX system...\n");
    nx_system_initialize();

    uart_puts("[app_define] Configuring virtio-net...\n");
    /* Configure virtio-net driver before creating IP instance */
    {
        struct virtio_net_nx_config vcfg;
        vcfg.mmio_base = cfg_mmio_base;
        vcfg.irq_num   = cfg_irq_num;
        virtio_net_nx_configure(&vcfg);
    }

    /* Allocate packet pool memory from byte pool */
    status = tx_byte_allocate(&byte_pool, (VOID **)&pointer,
                               PACKET_POOL_SIZE, TX_NO_WAIT);
    if (status != TX_SUCCESS) {
        uart_puts("ERROR: packet pool memory alloc failed");
        return;
    }

    /* Create packet pool */
    status = nx_packet_pool_create(&packet_pool, "nros_packet_pool",
                                    PACKET_SIZE, pointer, PACKET_POOL_SIZE);
    if (status != NX_SUCCESS) {
        uart_puts("ERROR: packet pool create failed");
        return;
    }

    /* Allocate IP stack memory */
    status = tx_byte_allocate(&byte_pool, (VOID **)&pointer,
                               IP_STACK_SIZE, TX_NO_WAIT);
    if (status != TX_SUCCESS) {
        uart_puts("ERROR: IP stack memory alloc failed");
        return;
    }

    uart_puts("[app_define] Creating IP instance...\n");
    /* Create IP instance with virtio-net driver */
    ULONG ip_addr = ((ULONG)cfg_ip[0] << 24) | ((ULONG)cfg_ip[1] << 16)
                  | ((ULONG)cfg_ip[2] << 8)  | (ULONG)cfg_ip[3];
    ULONG netmask = ((ULONG)cfg_netmask[0] << 24) | ((ULONG)cfg_netmask[1] << 16)
                  | ((ULONG)cfg_netmask[2] << 8)  | (ULONG)cfg_netmask[3];

    status = nx_ip_create(&ip_instance, "nros_ip", ip_addr, netmask,
                           &packet_pool, virtio_net_nx_driver,
                           pointer, IP_STACK_SIZE, IP_THREAD_PRIORITY);
    if (status != NX_SUCCESS) {
        uart_puts("ERROR: IP create failed");
        return;
    }

    /* Set default gateway */
    ULONG gw_addr = ((ULONG)cfg_gateway[0] << 24) | ((ULONG)cfg_gateway[1] << 16)
                  | ((ULONG)cfg_gateway[2] << 8)  | (ULONG)cfg_gateway[3];
    nx_ip_gateway_address_set(&ip_instance, gw_addr);

    uart_puts("[app_define] Enabling ARP...\n");
    /* Enable ARP */
    status = tx_byte_allocate(&byte_pool, (VOID **)&pointer,
                               ARP_POOL_SIZE, TX_NO_WAIT);
    if (status != TX_SUCCESS) {
        uart_puts("ERROR: ARP pool memory alloc failed");
        return;
    }
    status = nx_arp_enable(&ip_instance, pointer, ARP_POOL_SIZE);
    if (status != NX_SUCCESS) {
        uart_puts("ERROR: ARP enable failed");
        return;
    }

    uart_puts("[app_define] Enabling TCP/UDP/ICMP...\n");
    /* Enable TCP, UDP, ICMP */
    nx_tcp_enable(&ip_instance);
    nx_udp_enable(&ip_instance);
    nx_icmp_enable(&ip_instance);

    uart_puts("[app_define] Initializing BSD sockets...\n");
    /* Initialize BSD socket layer */
    status = tx_byte_allocate(&byte_pool, (VOID **)&pointer,
                               BSD_STACK_SIZE, TX_NO_WAIT);
    if (status != TX_SUCCESS) {
        uart_puts("ERROR: BSD stack memory alloc failed");
        return;
    }
    status = nx_bsd_initialize(&ip_instance, &packet_pool,
                             (CHAR *)pointer, BSD_STACK_SIZE,
                             APP_THREAD_PRIORITY + 1);
    if (status != NX_SUCCESS) {
        uart_puts("ERROR: BSD initialize failed, status=0x");
        {
            static const char hex[] = "0123456789abcdef";
            char buf[9];
            for (int i = 7; i >= 0; i--) {
                buf[7 - i] = hex[(status >> (i * 4)) & 0xF];
            }
            buf[8] = '\0';
            uart_puts(buf);
        }
        uart_puts("\n");
        return;
    }
    uart_puts("[app_define] BSD sockets initialized\n");

    /* Diagnostic: verify pointer sizes and key addresses */
    uart_puts("[diag] sizeof(ULONG)=");
    diag_hex32((ULONG)sizeof(ULONG));
    uart_puts(" sizeof(void*)=");
    diag_hex32((ULONG)sizeof(void*));
    uart_puts(" sizeof(ALIGN_TYPE)=");
    diag_hex32((ULONG)sizeof(ALIGN_TYPE));
    uart_puts("\n");

    uart_puts("[diag] &ip_instance=0x");
    diag_hex64((uint64_t)(uintptr_t)&ip_instance);
    uart_puts("\n");

    extern TX_MUTEX *nx_bsd_protection_ptr;
    uart_puts("[diag] nx_bsd_protection_ptr=0x");
    diag_hex64((uint64_t)(uintptr_t)nx_bsd_protection_ptr);
    uart_puts("\n");

    uart_puts("[diag] &ip_instance.nx_ip_protection=0x");
    diag_hex64((uint64_t)(uintptr_t)&ip_instance.nx_ip_protection);
    uart_puts("\n");

    /* Verify mutex ID is valid */
    uart_puts("[diag] mutex_id=0x");
    diag_hex32(ip_instance.nx_ip_protection.tx_mutex_id);
    uart_puts(" (expect 0x4d555445)\n");

    uart_puts("[app_define] Creating app thread...\n");
    /* Create application thread */
    status = tx_byte_allocate(&byte_pool, (VOID **)&pointer,
                               APP_THREAD_STACK_SIZE, TX_NO_WAIT);
    if (status != TX_SUCCESS) {
        uart_puts("ERROR: app thread stack alloc failed");
        return;
    }

    status = tx_thread_create(&app_thread, "nros_app",
                               app_thread_entry, 0,
                               pointer, APP_THREAD_STACK_SIZE,
                               APP_THREAD_PRIORITY, APP_THREAD_PRIORITY,
                               TX_NO_TIME_SLICE, TX_AUTO_START);
    if (status != TX_SUCCESS) {
        uart_puts("ERROR: app thread create failed\n");
        return;
    }
    uart_puts("[app_define] App thread created, returning to kernel...\n");
}
