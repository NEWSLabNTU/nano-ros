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

#include <zephyr/logging/log.h>

LOG_MODULE_REGISTER(zpico_zephyr, LOG_LEVEL_INF);

/* Network-wait moved to nros-platform-zephyr (`net_wait.c`,
 * `nros_platform_zephyr_wait_network`) in Phase 200.1 — it's an
 * RMW-independent platform primitive that was historically mis-filed here
 * only because zenoh was the first Zephyr backend. */

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
