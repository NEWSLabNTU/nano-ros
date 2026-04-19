/**
 * @file xrce_zephyr.c
 * @brief XRCE-DDS network readiness for Zephyr RTOS
 *
 * Provides L4 connectivity detection via Zephyr Connection Manager.
 * Transport callbacks and clock symbols are now handled by the Rust
 * nros-rmw-xrce platform_udp module and xrce-platform-shim respectively.
 *
 * @copyright Copyright (c) 2024 nros contributors
 * @license MIT OR Apache-2.0
 */

#include <zephyr/kernel.h>
#include <zephyr/logging/log.h>
#include <zephyr/net/net_if.h>
#include <zephyr/net/net_mgmt.h>
#include <zephyr/net/conn_mgr_monitor.h>

LOG_MODULE_REGISTER(xrce_zephyr, LOG_LEVEL_INF);

/* ============================================================================
 * L4 connectivity semaphore
 * ============================================================================ */

static K_SEM_DEFINE(net_l4_connected, 0, 1);
static struct net_mgmt_event_callback l4_cb;

static void l4_event_handler(struct net_mgmt_event_callback *cb,
                             uint32_t event, struct net_if *iface)
{
    if (event == NET_EVENT_L4_CONNECTED) {
        k_sem_give(&net_l4_connected);
    }
}

static int register_l4_callback(void)
{
    net_mgmt_init_event_callback(&l4_cb, l4_event_handler,
                                 NET_EVENT_L4_CONNECTED |
                                 NET_EVENT_L4_DISCONNECTED);
    net_mgmt_add_event_callback(&l4_cb);
    return 0;
}

SYS_INIT(register_l4_callback, APPLICATION, CONFIG_APPLICATION_INIT_PRIORITY);

/* ============================================================================
 * Network wait
 * ============================================================================ */

int32_t xrce_zephyr_wait_network(int timeout_ms)
{
#ifdef CONFIG_NET_NATIVE_OFFLOADED_SOCKETS
    /* NSOS (Native Sim Offloaded Sockets) uses host kernel networking
     * directly — always ready, no L4 event needed. */
    LOG_INF("Network ready (NSOS — host kernel sockets)");
    return 0;
#else
    bool already_up = false;

    if (k_sem_take(&net_l4_connected, K_NO_WAIT) == 0) {
        LOG_INF("Network L4 connected (already up)");
        already_up = true;
    }

    if (!already_up) {
        LOG_INF("Waiting for network L4 connectivity (timeout %d ms)...", timeout_ms);
        int ret = k_sem_take(&net_l4_connected,
                             timeout_ms < 0 ? K_FOREVER : K_MSEC(timeout_ms));
        if (ret != 0) {
            LOG_ERR("Network L4 not connected after %d ms", timeout_ms);
            return -1;
        }
        LOG_INF("Network L4 connected");
    }

#ifdef CONFIG_BOARD_NATIVE_SIM
    /* TAP bridge stabilization */
    k_sleep(K_MSEC(2000));
#endif

    return 0;
#endif /* CONFIG_NET_NATIVE_OFFLOADED_SOCKETS */
}

/* ============================================================================
 * Clock symbols for Micro-XRCE-DDS-Client
 * ============================================================================ */

int64_t uxr_millis(void)
{
    return k_uptime_get();
}

int64_t uxr_nanos(void)
{
    return k_uptime_get() * 1000000LL;
}
