/**
 * @file xrce_zephyr.c
 * @brief XRCE-DDS platform support for Zephyr RTOS
 *
 * Provides network initialization, XRCE custom transport callbacks over
 * Zephyr BSD sockets, and clock symbols for Micro-XRCE-DDS-Client.
 *
 * The transport callbacks are registered by the Rust nros-rmw-xrce crate
 * via extern "C" function pointers. This file only provides the C
 * implementations — it does NOT call uxr_set_custom_transport_callbacks()
 * itself.
 *
 * @copyright Copyright (c) 2024 nros contributors
 * @license MIT OR Apache-2.0
 */

#include "xrce_zephyr.h"

#include <zephyr/kernel.h>
#include <zephyr/logging/log.h>
#include <zephyr/net/net_if.h>
#include <zephyr/net/socket.h>

#include <errno.h>

LOG_MODULE_REGISTER(xrce_zephyr, LOG_LEVEL_INF);

/* ============================================================================
 * Static state
 * ============================================================================ */

static int udp_sock = -1;

/* ============================================================================
 * Network wait
 * ============================================================================ */

int32_t xrce_zephyr_wait_network(int timeout_ms)
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

/* ============================================================================
 * Transport init: create UDP socket and connect to agent
 * ============================================================================ */

int32_t xrce_zephyr_init(const char *agent_addr, int agent_port)
{
    if (agent_addr == NULL) {
        LOG_ERR("Agent address is NULL");
        return -1;
    }

    LOG_INF("Initializing XRCE transport");
    LOG_INF("  Agent: %s:%d", agent_addr, agent_port);

    /* Create UDP socket */
    udp_sock = zsock_socket(AF_INET, SOCK_DGRAM, IPPROTO_UDP);
    if (udp_sock < 0) {
        LOG_ERR("Failed to create UDP socket: %d", errno);
        return -1;
    }

    /* Connect to agent */
    struct sockaddr_in agent = {
        .sin_family = AF_INET,
        .sin_port = htons((uint16_t)agent_port),
    };

    if (zsock_inet_pton(AF_INET, agent_addr, &agent.sin_addr) != 1) {
        LOG_ERR("Invalid agent address: %s", agent_addr);
        zsock_close(udp_sock);
        udp_sock = -1;
        return -1;
    }

    if (zsock_connect(udp_sock, (struct sockaddr *)&agent, sizeof(agent)) < 0) {
        LOG_ERR("Failed to connect to agent: %d", errno);
        zsock_close(udp_sock);
        udp_sock = -1;
        return -1;
    }

    LOG_INF("  UDP socket connected (fd=%d)", udp_sock);
    return 0;
}

/* ============================================================================
 * XRCE custom transport callbacks
 *
 * These are called from the Rust side via function pointers registered
 * through nros_rmw_xrce::zephyr::init_zephyr_transport().
 * ============================================================================ */

bool xrce_zephyr_transport_open(void *transport)
{
    (void)transport;
    return (udp_sock >= 0);
}

bool xrce_zephyr_transport_close(void *transport)
{
    (void)transport;
    if (udp_sock >= 0) {
        zsock_close(udp_sock);
        udp_sock = -1;
    }
    return true;
}

size_t xrce_zephyr_transport_write(void *transport,
                                   const uint8_t *buffer,
                                   size_t length,
                                   uint8_t *error_code)
{
    (void)transport;

    if (udp_sock < 0) {
        *error_code = 1;
        return 0;
    }

    ssize_t ret = zsock_send(udp_sock, buffer, length, 0);
    if (ret < 0) {
        *error_code = 1;
        return 0;
    }

    return (size_t)ret;
}

size_t xrce_zephyr_transport_read(void *transport,
                                  uint8_t *buffer,
                                  size_t length,
                                  int timeout,
                                  uint8_t *error_code)
{
    (void)transport;

    if (udp_sock < 0) {
        *error_code = 1;
        return 0;
    }

    /* Poll for data with timeout */
    struct zsock_pollfd fds = {
        .fd = udp_sock,
        .events = ZSOCK_POLLIN,
    };

    int poll_ret = zsock_poll(&fds, 1, timeout);
    if (poll_ret <= 0) {
        /* Timeout or error — not an error for XRCE */
        return 0;
    }

    ssize_t ret = zsock_recvfrom(udp_sock, buffer, length, 0, NULL, NULL);
    if (ret < 0) {
        *error_code = 1;
        return 0;
    }

    return (size_t)ret;
}

/* ============================================================================
 * Clock symbols for Micro-XRCE-DDS-Client
 *
 * When xrce-sys is built with the `zephyr` feature, time.c is skipped.
 * These provide the uxr_millis()/uxr_nanos() symbols that the XRCE
 * library needs, using Zephyr's k_uptime_get().
 * ============================================================================ */

int64_t uxr_millis(void)
{
    return k_uptime_get();
}

int64_t uxr_nanos(void)
{
    return k_uptime_get() * 1000000LL;
}
