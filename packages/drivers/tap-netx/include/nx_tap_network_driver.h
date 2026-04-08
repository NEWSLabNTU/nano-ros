/**
 * nx_tap_network_driver.h — TAP-based network driver for NetX Duo on Linux
 *
 * Replaces the AF_PACKET raw socket driver from threadx-learn-samples.
 * Uses /dev/net/tun with IFF_TAP | IFF_NO_PI for clean ethernet frame I/O.
 *
 * Advantages over AF_PACKET:
 *   - No CAP_NET_RAW or root required (just /dev/net/tun access)
 *   - No sockaddr_ll initialization issues
 *   - No PACKET_OUTGOING loopback (TAP only delivers incoming frames)
 *   - No checksum offload problems
 *
 * Usage:
 *   nx_tap_set_interface_name("tap-tx0");
 *   nx_ip_create(&ip, "ip", addr, mask, &pool, nx_tap_network_driver, ...);
 */

#ifndef NX_TAP_NETWORK_DRIVER_H
#define NX_TAP_NETWORK_DRIVER_H

#include "nx_api.h"

#ifdef __cplusplus
extern "C" {
#endif

/**
 * Set the TAP interface name before calling nx_ip_create().
 * The interface must already exist (created by setup script).
 */
void nx_tap_set_interface_name(const char *name);

/**
 * NetX Duo network driver entry point.
 * Pass this to nx_ip_create() as the driver function.
 */
void nx_tap_network_driver(NX_IP_DRIVER *driver_req_ptr);

#ifdef __cplusplus
}
#endif

#endif /* NX_TAP_NETWORK_DRIVER_H */
