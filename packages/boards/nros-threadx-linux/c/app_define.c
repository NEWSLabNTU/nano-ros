/*
 * app_define.c — ThreadX application_define() for nros Linux simulation
 *
 * Called by tx_kernel_enter() after kernel init. Creates the byte pool
 * and spawns the application thread that calls back into Rust.
 *
 * Networking goes through nsos-netx (NetX BSD shim over host POSIX
 * sockets) — no NetX Duo TCP/IP stack, no IP instance, no packet pool,
 * no ARP, no veth/TAP driver. The application's `nx_bsd_*` calls are
 * forwarded directly to the host kernel.
 */

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "tx_api.h"

/* ---- Platform byte pool + RNG registration ---- */
extern void nros_platform_threadx_set_byte_pool(TX_BYTE_POOL *pool);
extern void nros_platform_threadx_seed_rng(uint32_t value);
/* Legacy: zpico-sys C system.c reads this global (CMake path only) */
TX_BYTE_POOL *zpico_threadx_byte_pool;

/* ---- Sizing constants ---- */
#define BYTE_POOL_SIZE          (512 * 1024)    /* 512 KB: ThreadX infra + zenoh-pico */
#define APP_THREAD_STACK_SIZE   (64 * 1024)     /* 64 KB for Executor + zenoh-pico */
#define APP_THREAD_PRIORITY     4

/* ---- Static objects ---- */
static TX_BYTE_POOL     byte_pool;
static UCHAR            byte_pool_storage[BYTE_POOL_SIZE];
static TX_THREAD        app_thread;

/* ---- Configuration (set from Rust before tx_kernel_enter) ---- */
/* IP/MAC/interface fields are accepted but ignored — NSOS uses the
 * host kernel's networking, so no per-instance IP setup is needed. */
static uint8_t  cfg_ip[4]      = {127, 0, 0, 1};
static uint8_t  cfg_mac[6]     = {0x02, 0x00, 0x00, 0x00, 0x00, 0x00};

/* ---- Rust callback (set from Rust before tx_kernel_enter) ---- */
static void (*rust_app_entry)(void *) = NULL;
static void *rust_app_arg = NULL;

/* ---- C/C++ entry point (linked from user code) ---- */
extern void app_main(void) __attribute__((weak));

/* ---- FFI: called from Rust to set config ---- *
 * Signature kept for compatibility with existing Rust glue. The IP/
 * netmask/gateway/interface_name parameters are ignored under NSOS. */
void nros_threadx_set_config(
    const uint8_t *ip,
    const uint8_t *netmask,
    const uint8_t *gateway,
    const uint8_t *mac,
    const char *interface_name)
{
    (void)netmask;
    (void)gateway;
    (void)interface_name;
    if (ip != NULL)  { memcpy(cfg_ip,  ip,  4); }
    if (mac != NULL) { memcpy(cfg_mac, mac, 6); }
}

/* ---- FFI: called from Rust to set the app callback ---- */
void nros_threadx_set_app_callback(void (*entry)(void *), void *arg)
{
    rust_app_entry = entry;
    rust_app_arg = arg;
}

/* ---- App thread entry: invokes Rust callback or C/C++ app_main ---- */
static void app_thread_entry(ULONG input)
{
    (void)input;

    if (rust_app_entry) {
        rust_app_entry(rust_app_arg);
    } else if (app_main) {
        app_main();
    } else {
        printf("ERROR: no app entry point (set rust callback or define app_main)\n");
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

    /* Register byte pool with both C global and Rust platform */
    zpico_threadx_byte_pool = &byte_pool;
    nros_platform_threadx_set_byte_pool(&byte_pool);

    /* Seed RNG (C srand + Rust platform). Used by zenoh-pico session
     * IDs — must vary per-instance so two simulations don't collide. */
    {
        uint32_t seed = ((uint32_t)cfg_ip[0] << 24) | ((uint32_t)cfg_ip[1] << 16)
                      | ((uint32_t)cfg_ip[2] << 8)  | (uint32_t)cfg_ip[3];
        seed = seed * 2654435761u;  /* Knuth multiplicative hash */
        seed ^= ((uint32_t)cfg_mac[4] << 8) | (uint32_t)cfg_mac[5];
        if (seed == 0) seed = 1;
        srand(seed);
        nros_platform_threadx_seed_rng(seed);
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
