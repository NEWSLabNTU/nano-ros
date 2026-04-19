/**
 * @file xrce_zephyr.h
 * @brief XRCE-DDS network readiness for Zephyr RTOS
 *
 * Provides network readiness detection for XRCE-DDS on Zephyr.
 * Transport and clock symbols are handled by nros-rmw-xrce and
 * xrce-platform-shim respectively.
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
 * Blocks until L4 connectivity is reported or the timeout expires.
 * On NSOS (native_sim offloaded sockets), returns immediately.
 *
 * @param timeout_ms Maximum time to wait in milliseconds
 * @return 0 on success, -1 if the interface did not come up
 */
int32_t xrce_zephyr_wait_network(int timeout_ms);

#ifdef __cplusplus
}
#endif

#endif /* XRCE_ZEPHYR_H */
