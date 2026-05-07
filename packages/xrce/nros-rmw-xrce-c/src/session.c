/* Phase 115.K.2.1 — session lifecycle implementation.
 *
 * Mirrors the Rust `nros-rmw-xrce::XrceRmw::open` shape but in pure
 * C against `uxr_*`. v1 supports UDP transport only; serial / custom
 * lands in 115.K.2.4 alongside the Phase 115.E custom-transport
 * bridge port.
 *
 * Allocation: a single `struct xrce_session_state` per session
 * lives on the heap (`malloc`). The pointer is parked in
 * `nros_rmw_session_t::backend_data`. The runtime owns the entity-
 * shell `nros_rmw_session_t` struct itself.
 */

#include "internal.h"

#include "nros/rmw_ret.h"

#include <uxr/client/client.h>
#include <uxr/client/profile/transport/ip/udp/udp_transport.h>
#include <uxr/client/profile/transport/ip/udp/udp_transport_posix.h>

#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

/* Default agent UDP port, matches Micro-XRCE-DDS-Agent's default. */
#define XRCE_DEFAULT_AGENT_PORT 2018

/* Session-creation retry budget. Mirrors `SESSION_CREATION_RETRIES`
 * in the Rust impl. */
#define XRCE_SESSION_CREATION_RETRIES 3

/* Output / input reliable stream config. Mirrors the Rust impl's
 * `STREAM_BUFFER_SIZE` + `STREAM_HISTORY`. */
#define XRCE_STREAM_BUFFER_SIZE 1024
#define XRCE_STREAM_HISTORY 8

struct xrce_session_state {
    uxrUDPTransport     udp;
    uxrSession          session;

    /* Reliable stream buffers. The streams themselves are referred
     * to by id (`uxrStreamId`); the underlying memory must outlive
     * the session. */
    uint8_t             output_reliable_buf[XRCE_STREAM_BUFFER_SIZE * XRCE_STREAM_HISTORY];
    uint8_t             input_reliable_buf [XRCE_STREAM_BUFFER_SIZE * XRCE_STREAM_HISTORY];
    uxrStreamId         output_reliable;
    uxrStreamId         input_reliable;
};

/* Hash a node-name string into a 32-bit XRCE session key. The Rust
 * impl uses FNV-1a; same here so two implementations can interoperate
 * against the same agent without surprise. */
static uint32_t hash_session_key(const char *s) {
    uint32_t h = 0x811c9dc5u;
    if (s == NULL) {
        return h;
    }
    for (const unsigned char *p = (const unsigned char *)s; *p; ++p) {
        h ^= (uint32_t)*p;
        h *= 0x01000193u;
    }
    /* Avoid the reserved 0 / 0xffff_ffff session keys. */
    if (h == 0u || h == 0xffffffffu) {
        h ^= 0xdeadbeefu;
    }
    return h;
}

/* Parse `host:port`. On failure, returns 0 and leaves outputs
 * unchanged. The caller-provided `host_buf` receives a NUL-terminated
 * copy of the host substring up to its capacity. */
static int parse_host_port(const char *locator, char *host_buf,
                           size_t host_buf_len, uint16_t *port_out) {
    if (locator == NULL || host_buf == NULL || port_out == NULL) {
        return 0;
    }
    const char *colon = strrchr(locator, ':');
    if (colon == NULL) {
        return 0;
    }
    size_t host_len = (size_t)(colon - locator);
    if (host_len == 0 || host_len + 1 > host_buf_len) {
        return 0;
    }
    memcpy(host_buf, locator, host_len);
    host_buf[host_len] = '\0';
    long port = strtol(colon + 1, NULL, 10);
    if (port <= 0 || port > 0xffff) {
        return 0;
    }
    *port_out = (uint16_t)port;
    return 1;
}

nros_rmw_ret_t xrce_session_open(const char *locator, uint8_t mode,
                                 uint32_t domain_id, const char *node_name,
                                 nros_rmw_session_t *out) {
    (void)mode;
    (void)domain_id; /* domain_id consumed at participant-create time
                        — Phase 115.K.2.2. */
    if (out == NULL || node_name == NULL) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    if (out->backend_data != NULL) {
        return NROS_RMW_RET_ERROR;
    }

    /* `udp/host:port` or `udp4://host:port` strip the scheme prefix;
     * bare `host:port` is also accepted (matches the Rust shim). */
    const char *addr_locator = locator != NULL ? locator : "127.0.0.1";
    const char *prefixes[] = { "udp/", "udp4://", "udp://" };
    for (size_t i = 0; i < sizeof(prefixes) / sizeof(prefixes[0]); ++i) {
        size_t plen = strlen(prefixes[i]);
        if (strncmp(addr_locator, prefixes[i], plen) == 0) {
            addr_locator = addr_locator + plen;
            break;
        }
    }

    char host[64];
    uint16_t port = XRCE_DEFAULT_AGENT_PORT;
    if (parse_host_port(addr_locator, host, sizeof(host), &port) == 0) {
        /* Treat the whole locator as a host with default port. */
        size_t hlen = strlen(addr_locator);
        if (hlen == 0 || hlen + 1 > sizeof(host)) {
            return NROS_RMW_RET_INVALID_ARGUMENT;
        }
        memcpy(host, addr_locator, hlen + 1);
    }

    struct xrce_session_state *st = (struct xrce_session_state *)
        calloc(1, sizeof(struct xrce_session_state));
    if (st == NULL) {
        return NROS_RMW_RET_BAD_ALLOC;
    }

    if (!uxr_init_udp_transport(&st->udp, UXR_IPv4, host, "2018")) {
        free(st);
        return NROS_RMW_RET_ERROR;
    }
    /* uxr_init_udp_transport ignores its port arg in some platform
     * builds; re-set explicitly via the platform comm if needed.
     * Documented as a 115.K.2.1 follow-up. */
    char port_str[8];
    snprintf(port_str, sizeof(port_str), "%u", (unsigned)port);
    /* Re-init with the parsed port to make sure we don't end up
     * pointing at 2018 when the locator said otherwise. */
    if (port != 2018) {
        uxr_close_udp_transport(&st->udp);
        if (!uxr_init_udp_transport(&st->udp, UXR_IPv4, host, port_str)) {
            free(st);
            return NROS_RMW_RET_ERROR;
        }
    }

    uxr_init_session(&st->session, &st->udp.comm,
                     hash_session_key(node_name));

    if (!uxr_create_session_retries(&st->session,
                                    XRCE_SESSION_CREATION_RETRIES)) {
        uxr_close_udp_transport(&st->udp);
        free(st);
        return NROS_RMW_RET_ERROR;
    }

    st->output_reliable = uxr_create_output_reliable_stream(
        &st->session, st->output_reliable_buf,
        sizeof(st->output_reliable_buf), XRCE_STREAM_HISTORY);
    st->input_reliable = uxr_create_input_reliable_stream(
        &st->session, st->input_reliable_buf,
        sizeof(st->input_reliable_buf), XRCE_STREAM_HISTORY);

    out->backend_data = st;
    return NROS_RMW_RET_OK;
}

nros_rmw_ret_t xrce_session_close(nros_rmw_session_t *session) {
    if (session == NULL) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    struct xrce_session_state *st = (struct xrce_session_state *)session->backend_data;
    if (st == NULL) {
        return NROS_RMW_RET_ERROR;
    }
    (void)uxr_delete_session(&st->session);
    uxr_close_udp_transport(&st->udp);
    free(st);
    session->backend_data = NULL;
    return NROS_RMW_RET_OK;
}

nros_rmw_ret_t xrce_session_drive_io(nros_rmw_session_t *session,
                                     int32_t timeout_ms) {
    if (session == NULL) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    struct xrce_session_state *st = (struct xrce_session_state *)session->backend_data;
    if (st == NULL) {
        return NROS_RMW_RET_ERROR;
    }
    int t = timeout_ms < 0 ? 0 : (int)timeout_ms;
    (void)uxr_run_session_time(&st->session, t);
    return NROS_RMW_RET_OK;
}
