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
#include <nros/app_config.h>

/* APP_INTERFACE remains a per-example compile define — bridge name is
 * test-harness-specific and not present in the typed config.toml. */
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

    nros_threadx_set_config(
        NROS_APP_CONFIG.network.ip,
        NROS_APP_CONFIG.network.netmask,
        NROS_APP_CONFIG.network.gateway,
        NROS_APP_CONFIG.network.mac,
        APP_INTERFACE);

    /* Enter ThreadX scheduler — never returns.
     * tx_application_define() is called from within tx_kernel_enter(). */
    tx_kernel_enter();

    return 0;
}
