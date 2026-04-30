/**
 * @file zpico_zephyr.c
 * @brief Zenoh-pico platform support for Zephyr RTOS
 *
 * Provides network initialization, zenoh session management, and platform
 * helpers for running zenoh-pico on Zephyr. This is the platform layer only;
 * the nros API is provided by nros-c (C) or the nros crate (Rust).
 *
 * @copyright Copyright (c) 2024 nros contributors
 * @license MIT OR Apache-2.0
 */

#include "zpico_zephyr.h"
#include "zpico.h"

#include <zephyr/kernel.h>
#include <zephyr/logging/log.h>
#include <zephyr/net/net_if.h>
#include <zephyr/net/net_mgmt.h>
#include <zephyr/net/conn_mgr_monitor.h>

LOG_MODULE_REGISTER(zpico_zephyr, LOG_LEVEL_INF);

/* ── L4 connectivity semaphore ───────────────────────────────────────────── */

static K_SEM_DEFINE(net_l4_connected, 0, 1);

static struct net_mgmt_event_callback l4_cb;

static void l4_event_handler(struct net_mgmt_event_callback* cb, uint32_t event,
                             struct net_if* iface) {
    if (event == NET_EVENT_L4_CONNECTED) {
        k_sem_give(&net_l4_connected);
    }
}

/* Register the L4 callback at boot, before any application code runs.
 * This ensures we don't miss the event if the interface comes up fast. */
static int register_l4_callback(void) {
    net_mgmt_init_event_callback(&l4_cb, l4_event_handler,
                                 NET_EVENT_L4_CONNECTED | NET_EVENT_L4_DISCONNECTED);
    net_mgmt_add_event_callback(&l4_cb);
    return 0;
}

SYS_INIT(register_l4_callback, APPLICATION, CONFIG_APPLICATION_INIT_PRIORITY);

/* ── Public API ���─────────────────────────────────────────────────────────── */

int32_t zpico_zephyr_wait_network(int timeout_ms) {
#ifdef CONFIG_NET_NATIVE_OFFLOADED_SOCKETS
    /* NSOS (Native Sim Offloaded Sockets) uses host kernel networking
     * directly — always ready, no L4 event needed. */
    LOG_INF("Network ready (NSOS — host kernel sockets)");
    return 0;
#else
    /* Native Zephyr net stack: wait for the iface to come up + acquire
     * an IPv4 address. The previous strict `NET_EVENT_L4_CONNECTED`
     * handshake never fires on `qemu_cortex_a9` with the GEM driver in
     * promiscuous mode (no DHCP, no PHY-managed link state) — the
     * conn_mgr never promotes "iface up + IP set" to L4_CONNECTED, so
     * the listener was hanging the full timeout while talker continued
     * publishing into the void. Instead, poll for the post-condition
     * that DDS / zenoh-pico actually need: iface admin-up + carrier ok
     * + at least one IPv4 address bound to the interface. Falls back to
     * the L4 sem if the conn_mgr does fire it (NSOS / Zephyr-native /
     * any future board with a managed PHY). */
    struct net_if* iface = net_if_get_default();
    if (iface == NULL) {
        LOG_ERR("No default net_if");
        return -1;
    }

    LOG_INF("Waiting for network readiness (timeout %d ms)...", timeout_ms);

    const int poll_ms = 50;
    int waited = 0;
    while (timeout_ms < 0 || waited < timeout_ms) {
        if (net_if_is_up(iface) && net_if_is_carrier_ok(iface)) {
            /* Has at least one configured IPv4 address? */
            struct net_if_ipv4* ipv4 = iface->config.ip.ipv4;
            if (ipv4 != NULL) {
                for (int i = 0; i < NET_IF_MAX_IPV4_ADDR; i++) {
                    if (ipv4->unicast[i].ipv4.is_used &&
                        ipv4->unicast[i].ipv4.addr_state == NET_ADDR_PREFERRED) {
                        LOG_INF("Network ready (iface up + IPv4 bound)");
                        return 0;
                    }
                }
            }
        }
        /* Also accept the L4 sem as a positive signal — keeps the
         * fast path for boards that do drive conn_mgr correctly. */
        if (k_sem_take(&net_l4_connected, K_NO_WAIT) == 0) {
            LOG_INF("Network L4 connected");
            return 0;
        }
        k_sleep(K_MSEC(poll_ms));
        waited += poll_ms;
    }

    LOG_ERR("Network not ready after %d ms", timeout_ms);
    return -1;
#endif
}

int32_t zpico_zephyr_init_session(const char* locator) {
    if (locator == NULL) {
        LOG_ERR("Locator is NULL");
        return -1;
    }

    LOG_INF("Initializing zenoh session");
    LOG_INF("  Locator: %s", locator);

    int32_t ret = zpico_init(locator);
    if (ret != ZPICO_OK) {
        LOG_ERR("Failed to initialize zenoh: %d", ret);
        return ret;
    }

    LOG_INF("  Zenoh initialized");

    ret = zpico_open();
    if (ret != ZPICO_OK) {
        LOG_ERR("Failed to open zenoh session: %d", ret);
        return ret;
    }

    LOG_INF("  Session opened");
    return 0;
}

void zpico_zephyr_shutdown(void) {
    zpico_close();
    LOG_INF("Zenoh session closed");
}
