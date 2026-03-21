/*
 * startup.c — ThreadX RISC-V QEMU virt entry point for C/C++ examples
 *
 * Provides the C-level setup that calls nros_threadx_set_config() with
 * compile-time APP_* macros, then enters the ThreadX kernel.
 * tx_kernel_enter() calls tx_application_define() from app_define.c.
 */

#include <stdint.h>
#include <string.h>
#include "tx_api.h"

/* ---- Configuration from CMake (via config.toml) ---- */
#ifndef APP_IP
#define APP_IP {10, 0, 2, 40}
#endif
#ifndef APP_MAC
#define APP_MAC {0x52, 0x54, 0x00, 0x12, 0x34, 0x56}
#endif
#ifndef APP_GATEWAY
#define APP_GATEWAY {10, 0, 2, 2}
#endif
#ifndef APP_NETMASK
#define APP_NETMASK {255, 255, 255, 0}
#endif

/* ---- FFI: set config in app_define.c ---- */
extern void nros_threadx_set_config(
    const uint8_t *ip,
    const uint8_t *netmask,
    const uint8_t *gateway,
    const uint8_t *mac,
    const char *interface_name);

/*
 * nros_threadx_startup() — called from the board crate's Rust main or
 * from the C entry path. Sets network config and enters ThreadX.
 *
 * On RISC-V QEMU virt, there is no separate main() — entry.s jumps
 * directly into the ThreadX low-level init which calls
 * tx_application_define(). The startup config is set via a global
 * constructor or by being linked before tx_kernel_enter.
 */

/* entry.s calls main() after BSS init and stack setup.
 * We set network config, then enter the ThreadX kernel. */
int main(void) {
    uint8_t ip[]      = APP_IP;
    uint8_t netmask[] = APP_NETMASK;
    uint8_t gateway[] = APP_GATEWAY;
    uint8_t mac[]     = APP_MAC;

    nros_threadx_set_config(ip, netmask, gateway, mac, "");

    /* Enter ThreadX scheduler — never returns.
     * tx_application_define() is called from within tx_kernel_enter(). */
    tx_kernel_enter();

    return 0;
}
