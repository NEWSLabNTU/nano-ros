/*
 * startup.c — ThreadX Linux simulation entry point for C/C++ examples
 *
 * Provides main() which sets up network config from APP_* macros
 * and calls tx_kernel_enter() to start the ThreadX scheduler.
 * The scheduler invokes tx_application_define() (from app_define.c)
 * which creates the IP instance and spawns the app thread calling app_main().
 */

#include <stdio.h>
#include <string.h>
#include "tx_api.h"

/* ---- Configuration from CMake (via config.toml) ---- */
#ifndef APP_IP
#define APP_IP {192, 0, 3, 10}
#endif
#ifndef APP_MAC
#define APP_MAC {0x02, 0x00, 0x00, 0x00, 0x00, 0x00}
#endif
#ifndef APP_GATEWAY
#define APP_GATEWAY {192, 0, 3, 1}
#endif
#ifndef APP_NETMASK
#define APP_NETMASK {255, 255, 255, 0}
#endif
#ifndef APP_INTERFACE
#define APP_INTERFACE "veth-tx0"
#endif

/* ---- FFI: set config in app_define.c ---- */
extern void nros_threadx_set_config(
    const uint8_t *ip,
    const uint8_t *netmask,
    const uint8_t *gateway,
    const uint8_t *mac,
    const char *interface_name);

int main(void)
{
    /* Line-buffer stdout so printf output is visible to test harnesses
     * that pipe stdout (otherwise it would be fully buffered and only
     * flushed at exit, losing all output on timeout/kill). */
    setvbuf(stdout, NULL, _IOLBF, 0);

    uint8_t ip[]      = APP_IP;
    uint8_t netmask[] = APP_NETMASK;
    uint8_t gateway[] = APP_GATEWAY;
    uint8_t mac[]     = APP_MAC;

    nros_threadx_set_config(ip, netmask, gateway, mac, APP_INTERFACE);

    /* Enter ThreadX scheduler — never returns.
     * tx_application_define() is called from within tx_kernel_enter(). */
    tx_kernel_enter();

    return 0;
}
