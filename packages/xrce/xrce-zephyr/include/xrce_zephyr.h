/**
 * @file xrce_zephyr.h
 * @brief XRCE-DDS platform support for Zephyr RTOS
 *
 * Platform-level functions for initializing XRCE-DDS custom transport
 * over Zephyr BSD sockets. Handles network readiness, UDP socket setup,
 * and custom transport callback registration.
 *
 * For the full nros API, use nros-c headers (nros/init.h, nros/node.h, etc.)
 * or the nros Rust crate.
 *
 * @copyright Copyright (c) 2024 nros contributors
 * @license MIT OR Apache-2.0
 */

#ifndef XRCE_ZEPHYR_H
#define XRCE_ZEPHYR_H

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/**
 * @brief Wait for the Zephyr network interface to come up.
 *
 * Blocks until the default network interface reports "up" or the
 * timeout expires.
 *
 * @param timeout_ms Maximum time to wait in milliseconds
 * @return 0 on success, -1 if the interface did not come up
 */
int32_t xrce_zephyr_wait_network(int timeout_ms);

/**
 * @brief Initialize XRCE custom transport over UDP.
 *
 * Creates a UDP socket, connects to the XRCE-DDS Agent at the specified
 * address, and registers the custom transport callbacks with xrce-sys.
 * Also provides uxr_millis()/uxr_nanos() clock symbols.
 *
 * The network interface must be up before calling this function.
 *
 * @param agent_addr Agent IP address (e.g., "192.0.2.2")
 * @param agent_port Agent UDP port (e.g., 2018)
 * @return 0 on success, negative error code on failure
 */
int32_t xrce_zephyr_init(const char *agent_addr, int agent_port);

#ifdef __cplusplus
}
#endif

#endif /* XRCE_ZEPHYR_H */
