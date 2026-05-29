/**
 * @file xrce_zephyr.c
 * @brief XRCE-DDS clock symbols for Zephyr RTOS
 *
 * Network readiness moved to nros-platform-zephyr
 * (`nros_platform_zephyr_wait_network`, `net_wait.c`) in Phase 200.1 — it's an
 * RMW-independent platform primitive. The XRCE-specific native_sim
 * stabilization grace folded into that shared helper. Transport callbacks and
 * clock symbols are handled by the Rust nros-rmw-xrce platform_udp module and
 * here, respectively.
 *
 * @copyright Copyright (c) 2024 nros contributors
 * @license MIT OR Apache-2.0
 */

#include <zephyr/kernel.h>
#include <zephyr/logging/log.h>

LOG_MODULE_REGISTER(xrce_zephyr, LOG_LEVEL_INF);

/* ============================================================================
 * Clock symbols for Micro-XRCE-DDS-Client
 * ============================================================================ */

int64_t uxr_millis(void) {
    return k_uptime_get();
}

int64_t uxr_nanos(void) {
    return k_uptime_get() * 1000000LL;
}
