#include "internal.h"

#include "nros/platform_net.h"

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>
#include <stdlib.h>

#include <uxr/client/profile/transport/custom/custom_transport.h>

#if defined(UCLIENT_PLATFORM_ZEPHYR)

typedef struct {
    void *sock;
    void *endpoint;
} xrce_zephyr_udp_bridge;

static bool zephyr_udp_open(struct uxrCustomTransport *t) {
    (void)t;
    return true;
}

static bool zephyr_udp_close(struct uxrCustomTransport *t) {
    if (t == NULL) return true;
    xrce_zephyr_udp_bridge *b = (xrce_zephyr_udp_bridge *)t->args;
    if (b == NULL) return true;
    if (b->sock != NULL) {
        nros_platform_udp_close(b->sock);
    }
    if (b->endpoint != NULL) {
        nros_platform_udp_free_endpoint(b->endpoint);
    }
    free(b->sock);
    free(b->endpoint);
    b->sock = NULL;
    b->endpoint = NULL;
    return true;
}

static void zephyr_udp_bridge_cleanup(xrce_zephyr_udp_bridge *b) {
    if (b == NULL) return;
    if (b->sock != NULL) {
        nros_platform_udp_close(b->sock);
    }
    if (b->endpoint != NULL) {
        nros_platform_udp_free_endpoint(b->endpoint);
    }
    free(b->sock);
    free(b->endpoint);
    b->sock = NULL;
    b->endpoint = NULL;
}

static size_t zephyr_udp_write(struct uxrCustomTransport *t,
                               const uint8_t *buf, size_t len,
                               uint8_t *err) {
    (void)err;
    if (t == NULL) return 0;
    xrce_zephyr_udp_bridge *b = (xrce_zephyr_udp_bridge *)t->args;
    if (b == NULL || b->sock == NULL || b->endpoint == NULL) return 0;
    size_t n = nros_platform_udp_send(b->sock, buf, len, b->endpoint);
    return n == NROS_PLATFORM_NET_SOCKET_ERROR ? 0u : n;
}

static size_t zephyr_udp_read(struct uxrCustomTransport *t,
                              uint8_t *buf, size_t len,
                              int timeout, uint8_t *err) {
    (void)err;
    if (t == NULL) return 0;
    xrce_zephyr_udp_bridge *b = (xrce_zephyr_udp_bridge *)t->args;
    if (b == NULL || b->sock == NULL) return 0;
    nros_platform_udp_set_recv_timeout(b->sock, timeout < 0 ? 0u : (uint32_t)timeout);
    size_t n = nros_platform_udp_read(b->sock, buf, len);
    return n == NROS_PLATFORM_NET_SOCKET_ERROR ? 0u : n;
}

nros_rmw_ret_t xrce_zephyr_udp_init(xrce_session_state_t *st,
                                    const char *host, const char *port) {
    if (st == NULL || host == NULL || port == NULL) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }

    xrce_zephyr_udp_bridge *bridge = (xrce_zephyr_udp_bridge *)&st->udp_bridge;
    bridge->sock = calloc(1, sizeof(int));
    bridge->endpoint = calloc(1, sizeof(void *));
    if (bridge->sock == NULL || bridge->endpoint == NULL) {
        zephyr_udp_bridge_cleanup(bridge);
        return NROS_RMW_RET_ERROR;
    }

    if (nros_platform_udp_create_endpoint(bridge->endpoint,
                                          (const uint8_t *)host,
                                          (const uint8_t *)port) != 0) {
        zephyr_udp_bridge_cleanup(bridge);
        return NROS_RMW_RET_ERROR;
    }
    if (nros_platform_udp_open(bridge->sock, bridge->endpoint, 100) != 0) {
        zephyr_udp_bridge_cleanup(bridge);
        return NROS_RMW_RET_ERROR;
    }

    uxr_set_custom_transport_callbacks(
        &st->custom, /*framing=*/false,
        zephyr_udp_open,
        zephyr_udp_close,
        zephyr_udp_write,
        zephyr_udp_read);

    if (!uxr_init_custom_transport(&st->custom, bridge)) {
        zephyr_udp_bridge_cleanup(bridge);
        return NROS_RMW_RET_ERROR;
    }
    return NROS_RMW_RET_OK;
}

#endif
