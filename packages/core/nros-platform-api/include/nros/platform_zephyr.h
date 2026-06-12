#ifndef NROS_PLATFORM_ZEPHYR_H
#define NROS_PLATFORM_ZEPHYR_H

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/**
 * @file platform_zephyr.h
 * @brief Zephyr-specific platform primitives (RMW-independent).
 *
 * These belong to the platform layer, not any RMW. Phase 200.1 relocated the
 * network-wait helper here from the zenoh-pico support crate (`zpico-zephyr`,
 * where it was historically mis-filed as `zpico_zephyr_wait_network` only
 * because zenoh was the first Zephyr backend) — so every RMW build (zenoh /
 * XRCE / CycloneDDS) gets one canonical copy.
 */

/**
 * @brief Block until the default Zephyr network interface is operational.
 *
 * Polls for the post-condition DDS / zenoh-pico actually need: iface admin-up +
 * carrier ok + at least one preferred IPv4 address bound (also accepts the
 * `NET_EVENT_L4_CONNECTED` sem for boards with a managed PHY). Under NSOS
 * (native_sim offloaded sockets) returns immediately — host kernel networking.
 *
 * @param timeout_ms Max wait in ms (negative = wait forever).
 * @return 0 when the interface is ready, -1 on timeout.
 */
int32_t nros_platform_zephyr_wait_network(int timeout_ms);

#ifdef __cplusplus
}
#endif

#endif /* NROS_PLATFORM_ZEPHYR_H */
