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
#include "zenoh_shim.h"

#include <zephyr/kernel.h>
#include <zephyr/logging/log.h>
#include <zephyr/net/net_if.h>

LOG_MODULE_REGISTER(zpico_zephyr, LOG_LEVEL_INF);

int32_t zpico_zephyr_wait_network(int timeout_ms)
{
    struct net_if *iface = net_if_get_default();
    int elapsed = 0;

    while (!net_if_is_up(iface) && elapsed < timeout_ms) {
        k_sleep(K_MSEC(50));
        elapsed += 50;
    }

    if (!net_if_is_up(iface)) {
        LOG_ERR("Network interface not ready after %d ms", timeout_ms);
        return -1;
    }

    LOG_INF("Network interface up (waited %d ms)", elapsed);
    return 0;
}

int32_t zpico_zephyr_init_session(const char *locator)
{
    if (locator == NULL) {
        LOG_ERR("Locator is NULL");
        return -1;
    }

    LOG_INF("Initializing zenoh session");
    LOG_INF("  Locator: %s", locator);

    int32_t ret = zenoh_shim_init(locator);
    if (ret != ZPICO_OK) {
        LOG_ERR("Failed to initialize zenoh: %d", ret);
        return ret;
    }

    LOG_INF("  Zenoh initialized");

    ret = zenoh_shim_open();
    if (ret != ZPICO_OK) {
        LOG_ERR("Failed to open zenoh session: %d", ret);
        return ret;
    }

    LOG_INF("  Session opened");
    return 0;
}

void zpico_zephyr_shutdown(void)
{
    zenoh_shim_close();
    LOG_INF("Zenoh session closed");
}
