/**
 * @file zpico_zephyr.h
 * @brief Zenoh-pico platform support for Zephyr RTOS
 *
 * Platform-level functions for initializing zenoh-pico on Zephyr.
 * Handles network readiness and zenoh session lifecycle.
 *
 * For the full nros API, use nros-c headers (nros/init.h, nros/node.h, etc.)
 * or the nros Rust crate.
 *
 * @copyright Copyright (c) 2024 nros contributors
 * @license MIT OR Apache-2.0
 */

#ifndef ZPICO_ZEPHYR_H
#define ZPICO_ZEPHYR_H

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/* Network-wait relocated to <nros/platform_zephyr.h>
 * (`nros_platform_zephyr_wait_network`) in Phase 200.1 — RMW-independent. */

/**
 * @brief Initialize and open a zenoh-pico session.
 *
 * Calls zpico_init() followed by zpico_open(). The network
 * interface must be up before calling this function.
 *
 * @param locator Zenoh router locator (e.g., "tcp/127.0.0.1:7456")
 * @return 0 on success, negative error code on failure
 */
int32_t zpico_zephyr_init_session(const char* locator);

/**
 * @brief Shut down the zenoh-pico session.
 *
 * Closes the active zenoh session. Safe to call even if no session
 * is open.
 */
void zpico_zephyr_shutdown(void);

#ifdef __cplusplus
}
#endif

#endif /* ZPICO_ZEPHYR_H */
