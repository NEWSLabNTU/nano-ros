/*
 * app_define.c — ThreadX application_define() for nros Linux simulation
 *
 * Called by tx_kernel_enter() after kernel init. Creates the byte pool,
 * packet pool, IP instance, enables protocols, initializes BSD sockets,
 * and spawns the application thread that calls back into Rust.
 */

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "tx_api.h"
#include "nx_api.h"
#include "nxd_bsd.h"

/* ---- Linux network driver (from threadx-learn-samples) ---- */
extern VOID _nx_linux_network_driver(NX_IP_DRIVER *driver_req_ptr);
extern VOID nx_linux_set_interface_name(const CHAR *interface_name);

/* ---- zpico-sys expects this global for ThreadX memory allocation ---- */
TX_BYTE_POOL *zpico_threadx_byte_pool;

/* ---- Sizing constants ---- */
#define BYTE_POOL_SIZE          (256 * 1024)    /* 256 KB for all allocations */
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
static uint8_t  cfg_mac[6]     = {0x02, 0x00, 0x00, 0x00, 0x00, 0x00};
static const char *cfg_interface_name = "tap-qemu0";

/* ---- Rust callback (set from Rust before tx_kernel_enter) ---- */
static void (*rust_app_entry)(void *) = NULL;
static void *rust_app_arg = NULL;

/* ---- FFI: called from Rust to set config ---- */
void nros_threadx_set_config(
    const uint8_t *ip,
    const uint8_t *netmask,
    const uint8_t *gateway,
    const uint8_t *mac,
    const char *interface_name)
{
    memcpy(cfg_ip, ip, 4);
    memcpy(cfg_netmask, netmask, 4);
    memcpy(cfg_gateway, gateway, 4);
    memcpy(cfg_mac, mac, 6);
    cfg_interface_name = interface_name;
}

/* ---- FFI: called from Rust to set the app callback ---- */
void nros_threadx_set_app_callback(void (*entry)(void *), void *arg)
{
    rust_app_entry = entry;
    rust_app_arg = arg;
}

/* ---- App thread entry: invokes the Rust closure ---- */
static void app_thread_entry(ULONG input)
{
    (void)input;

    if (rust_app_entry) {
        rust_app_entry(rust_app_arg);
    } else {
        printf("ERROR: no Rust app callback set\n");
    }
}

/* ---- ThreadX application_define (called by tx_kernel_enter) ---- */
void tx_application_define(void *first_unused_memory)
{
    UINT status;
    UCHAR *pointer;

    (void)first_unused_memory;

    /* Create byte pool for all dynamic allocations */
    status = tx_byte_pool_create(&byte_pool, "nros_byte_pool",
                                  byte_pool_storage, BYTE_POOL_SIZE);
    if (status != TX_SUCCESS) {
        printf("ERROR: byte pool create failed (%u)\n", status);
        return;
    }

    /* Export byte pool for zpico-sys ThreadX memory allocator */
    zpico_threadx_byte_pool = &byte_pool;

    /* Seed RNG with IP-based value (same pattern as FreeRTOS board crate) */
    {
        uint32_t seed = ((uint32_t)cfg_ip[0] << 24) | ((uint32_t)cfg_ip[1] << 16)
                      | ((uint32_t)cfg_ip[2] << 8)  | (uint32_t)cfg_ip[3];
        seed = seed * 2654435761u;  /* Knuth multiplicative hash */
        seed ^= ((uint32_t)cfg_mac[4] << 8) | (uint32_t)cfg_mac[5];
        if (seed == 0) seed = 1;
        srand(seed);
    }

    /* Initialize the NetX system */
    nx_system_initialize();

    /* Set the Linux TAP interface name before creating IP instance */
    nx_linux_set_interface_name(cfg_interface_name);

    /* Allocate packet pool memory from byte pool */
    status = tx_byte_allocate(&byte_pool, (VOID **)&pointer,
                               PACKET_POOL_SIZE, TX_NO_WAIT);
    if (status != TX_SUCCESS) {
        printf("ERROR: packet pool memory alloc failed (%u)\n", status);
        return;
    }

    /* Create packet pool */
    status = nx_packet_pool_create(&packet_pool, "nros_packet_pool",
                                    PACKET_SIZE, pointer, PACKET_POOL_SIZE);
    if (status != NX_SUCCESS) {
        printf("ERROR: packet pool create failed (0x%x)\n", status);
        return;
    }

    /* Allocate IP stack memory */
    status = tx_byte_allocate(&byte_pool, (VOID **)&pointer,
                               IP_STACK_SIZE, TX_NO_WAIT);
    if (status != TX_SUCCESS) {
        printf("ERROR: IP stack memory alloc failed (%u)\n", status);
        return;
    }

    /* Create IP instance with Linux network driver */
    ULONG ip_addr = ((ULONG)cfg_ip[0] << 24) | ((ULONG)cfg_ip[1] << 16)
                  | ((ULONG)cfg_ip[2] << 8)  | (ULONG)cfg_ip[3];
    ULONG netmask = ((ULONG)cfg_netmask[0] << 24) | ((ULONG)cfg_netmask[1] << 16)
                  | ((ULONG)cfg_netmask[2] << 8)  | (ULONG)cfg_netmask[3];

    status = nx_ip_create(&ip_instance, "nros_ip", ip_addr, netmask,
                           &packet_pool, _nx_linux_network_driver,
                           pointer, IP_STACK_SIZE, IP_THREAD_PRIORITY);
    if (status != NX_SUCCESS) {
        printf("ERROR: IP create failed (0x%x)\n", status);
        return;
    }

    /* Set default gateway */
    ULONG gw_addr = ((ULONG)cfg_gateway[0] << 24) | ((ULONG)cfg_gateway[1] << 16)
                  | ((ULONG)cfg_gateway[2] << 8)  | (ULONG)cfg_gateway[3];
    nx_ip_gateway_address_set(&ip_instance, gw_addr);

    /* Enable ARP */
    status = tx_byte_allocate(&byte_pool, (VOID **)&pointer,
                               ARP_POOL_SIZE, TX_NO_WAIT);
    if (status != TX_SUCCESS) {
        printf("ERROR: ARP pool memory alloc failed (%u)\n", status);
        return;
    }
    status = nx_arp_enable(&ip_instance, pointer, ARP_POOL_SIZE);
    if (status != NX_SUCCESS) {
        printf("ERROR: ARP enable failed (0x%x)\n", status);
        return;
    }

    /* Enable TCP, UDP, ICMP */
    status = nx_tcp_enable(&ip_instance);
    if (status != NX_SUCCESS) {
        printf("ERROR: TCP enable failed (0x%x)\n", status);
        return;
    }

    status = nx_udp_enable(&ip_instance);
    if (status != NX_SUCCESS) {
        printf("ERROR: UDP enable failed (0x%x)\n", status);
        return;
    }

    status = nx_icmp_enable(&ip_instance);
    if (status != NX_SUCCESS) {
        printf("ERROR: ICMP enable failed (0x%x)\n", status);
        return;
    }

    /* Initialize BSD socket layer */
    status = tx_byte_allocate(&byte_pool, (VOID **)&pointer,
                               BSD_STACK_SIZE, TX_NO_WAIT);
    if (status != TX_SUCCESS) {
        printf("ERROR: BSD stack memory alloc failed (%u)\n", status);
        return;
    }
    status = nx_bsd_initialize(&ip_instance, &packet_pool,
                                (CHAR *)pointer, BSD_STACK_SIZE,
                                APP_THREAD_PRIORITY + 1);
    if (status != NX_SUCCESS) {
        printf("ERROR: BSD initialize failed (0x%x)\n", status);
        return;
    }

    /* Create application thread */
    status = tx_byte_allocate(&byte_pool, (VOID **)&pointer,
                               APP_THREAD_STACK_SIZE, TX_NO_WAIT);
    if (status != TX_SUCCESS) {
        printf("ERROR: app thread stack alloc failed (%u)\n", status);
        return;
    }

    status = tx_thread_create(&app_thread, "nros_app",
                               app_thread_entry, 0,
                               pointer, APP_THREAD_STACK_SIZE,
                               APP_THREAD_PRIORITY, APP_THREAD_PRIORITY,
                               TX_NO_TIME_SLICE, TX_AUTO_START);
    if (status != TX_SUCCESS) {
        printf("ERROR: app thread create failed (%u)\n", status);
        return;
    }
}
