/* Phase 115.K.2 — session lifecycle implementation.
 *
 * Mirrors the Rust `nros-rmw-xrce::XrceRmw::open` shape but in pure
 * C against `uxr_*`. Phase 115.K.2.1 supported UDP only; 115.K.2.4
 * adds a `custom://` locator scheme that routes through the runtime
 * transport vtable bridge in `transport_custom.c`.
 *
 * Allocation: a single `struct xrce_session_state` per session lives
 * on the heap (`malloc`). The pointer is parked in
 * `nros_rmw_session_t::backend_data`. The runtime owns the entity-
 * shell `nros_rmw_session_t` struct itself.
 */

#include "internal.h"

#include "nros/rmw_ret.h"

#include <uxr/client/client.h>
#if defined(UCLIENT_PROFILE_UDP)
#  include <uxr/client/profile/transport/ip/udp/udp_transport.h>
#endif
#if defined(UCLIENT_PROFILE_UDP) && defined(UCLIENT_PLATFORM_POSIX)
#  include <uxr/client/profile/transport/ip/udp/udp_transport_posix.h>
#endif
#include <uxr/client/profile/transport/custom/custom_transport.h>
#include <uxr/client/core/session/object_id.h>
#include <uxr/client/util/ping.h>

#include <stdint.h>
#include <time.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

/* ---- Helpers ------------------------------------------------------- */

uxrObjectId xrce_alloc_entity_id(xrce_session_state_t *st, uint8_t type) {
    uint16_t id = st->next_entity_id++;
    return uxr_object_id(id, type);
}

nros_rmw_ret_t xrce_confirm_entities(xrce_session_state_t *st,
                                     const uint16_t *requests,
                                     uint8_t        *statuses,
                                     size_t          count) {
    bool ok = uxr_run_session_until_all_status(
        &st->session, XRCE_ENTITY_CREATION_TIMEOUT_MS,
        requests, statuses, count);
    if (!ok) {
        return NROS_RMW_RET_ERROR;
    }
    for (size_t i = 0; i < count; ++i) {
        if (statuses[i] != UXR_STATUS_OK && statuses[i] != UXR_STATUS_OK_MATCHED) {
            return NROS_RMW_RET_ERROR;
        }
    }
    return NROS_RMW_RET_OK;
}

/* Naming helpers — pure-C ports of `naming.rs`. */
static void copy_truncating(char *out, size_t out_cap, const char *src) {
    if (out == NULL || out_cap == 0) {
        return;
    }
    size_t len = strlen(src);
    if (len + 1 > out_cap) {
        len = out_cap - 1;
    }
    memcpy(out, src, len);
    out[len] = '\0';
}

static void append_truncating(char *out, size_t out_cap, const char *src) {
    if (out == NULL || out_cap == 0) {
        return;
    }
    size_t cur = strlen(out);
    if (cur + 1 >= out_cap) {
        return;
    }
    size_t avail = out_cap - cur - 1;
    size_t add = strlen(src);
    if (add > avail) {
        add = avail;
    }
    memcpy(out + cur, src, add);
    out[cur + add] = '\0';
}

void xrce_dds_topic_name(const char *topic_name, int avoid_ros_prefix,
                         char *out, size_t out_cap) {
    if (out_cap == 0) return;
    out[0] = '\0';
    const char *src = topic_name;
    if (src && src[0] == '/') {
        src += 1;
    }
    if (!avoid_ros_prefix) {
        copy_truncating(out, out_cap, "rt/");
        append_truncating(out, out_cap, src ? src : "");
    } else {
        copy_truncating(out, out_cap, src ? src : "");
    }
}

void xrce_dds_request_topic(const char *service_name, char *out, size_t out_cap) {
    if (out_cap == 0) return;
    out[0] = '\0';
    const char *src = service_name;
    if (src && src[0] == '/') src += 1;
    copy_truncating(out, out_cap, "rq/");
    append_truncating(out, out_cap, src ? src : "");
    append_truncating(out, out_cap, "Request");
}

void xrce_dds_reply_topic(const char *service_name, char *out, size_t out_cap) {
    if (out_cap == 0) return;
    out[0] = '\0';
    const char *src = service_name;
    if (src && src[0] == '/') src += 1;
    copy_truncating(out, out_cap, "rr/");
    append_truncating(out, out_cap, src ? src : "");
    append_truncating(out, out_cap, "Reply");
}

/* Insert "Request_" / "Reply_" before a trailing '_' (matches Rust
 * impl: `example_interfaces::srv::dds_::AddTwoInts_` →
 * `example_interfaces::srv::dds_::AddTwoInts_Request_`). */
static void insert_before_trailing_underscore(const char *type_name,
                                              const char *insert,
                                              char *out, size_t out_cap) {
    if (out_cap == 0) return;
    out[0] = '\0';
    if (type_name == NULL) {
        copy_truncating(out, out_cap, insert);
        append_truncating(out, out_cap, "_");
        return;
    }
    size_t len = strlen(type_name);
    if (len > 0 && type_name[len - 1] == '_') {
        /* prefix = type_name without the trailing '_' */
        size_t prefix_len = len - 1;
        if (prefix_len + 1 > out_cap) {
            prefix_len = out_cap - 1;
        }
        memcpy(out, type_name, prefix_len);
        out[prefix_len] = '\0';
        append_truncating(out, out_cap, "_");
        append_truncating(out, out_cap, insert);
        append_truncating(out, out_cap, "_");
    } else {
        copy_truncating(out, out_cap, type_name);
        append_truncating(out, out_cap, "_");
        append_truncating(out, out_cap, insert);
        append_truncating(out, out_cap, "_");
    }
}

void xrce_dds_request_type(const char *type_name, char *out, size_t out_cap) {
    insert_before_trailing_underscore(type_name, "Request", out, out_cap);
}

void xrce_dds_reply_type(const char *type_name, char *out, size_t out_cap) {
    /* ROS 2 service reply type is `<Service>_Response_` (the topic keeps the
     * `Reply` suffix, but the type uses `Response`). */
    insert_before_trailing_underscore(type_name, "Response", out, out_cap);
}

uxrQoS_t xrce_map_qos(const nros_rmw_qos_t *qos) {
    uxrQoS_t out;
    if (qos == NULL) {
        out.durability  = UXR_DURABILITY_VOLATILE;
        out.reliability = UXR_RELIABILITY_RELIABLE;
        out.history     = UXR_HISTORY_KEEP_LAST;
        out.depth       = 10;
        return out;
    }
    out.durability = (qos->durability == NROS_RMW_DURABILITY_TRANSIENT_LOCAL)
                     ? UXR_DURABILITY_TRANSIENT_LOCAL
                     : UXR_DURABILITY_VOLATILE;
    out.reliability = (qos->reliability == NROS_RMW_RELIABILITY_BEST_EFFORT)
                      ? UXR_RELIABILITY_BEST_EFFORT
                      : UXR_RELIABILITY_RELIABLE;
    out.history = (qos->history == NROS_RMW_HISTORY_KEEP_ALL)
                  ? UXR_HISTORY_KEEP_ALL
                  : UXR_HISTORY_KEEP_LAST;
    out.depth = qos->depth;
    return out;
}

/* ---- Session-key hashing ------------------------------------------- */

/* djb2 — matches the Rust impl's `hash_session_key`
 * (DJB2_INIT=5381, multiplier=33). Two implementations connecting
 * to the same agent under the same node name now agree on the
 * session key. */
static uint32_t hash_session_key(const char *s) {
    uint32_t h = 5381u;
    if (s == NULL) {
        return h;
    }
    for (const unsigned char *p = (const unsigned char *)s; *p; ++p) {
        h = h * 33u + (uint32_t)*p;
    }
    /* Ensure non-zero (XRCE-DDS may treat 0 specially). */
    return h == 0u ? 1u : h;
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

/* ---- Session open / close / drive_io ------------------------------- */

/* `udp/host:port` or `udp4://host:port` strip the scheme prefix; bare
 * `host:port` is also accepted. `custom://...` selects the runtime
 * transport vtable bridge. */
static int locator_strip_udp_prefix(const char **locator) {
    static const char *const prefixes[] = { "udp/", "udp4://", "udp://" };
    for (size_t i = 0; i < sizeof(prefixes) / sizeof(prefixes[0]); ++i) {
        size_t plen = strlen(prefixes[i]);
        if (strncmp(*locator, prefixes[i], plen) == 0) {
            *locator = *locator + plen;
            return 1;
        }
    }
    return 0;
}

static int locator_is_custom(const char *locator) {
    if (locator == NULL) return 0;
    return strncmp(locator, "custom://", 9) == 0
        || strcmp(locator, "custom") == 0;
}

/* Phase 115.K.2.5.1.5-serial — recognise locator forms that name a
 * serial / pty device:
 *   - `serial://<path>`
 *   - `serial:/<path>`        (some callers omit the second slash)
 *   - `/dev/...`              (bare absolute device path)
 *
 * Returns the substring pointing at the device path on match (caller
 * passes that to `xrce_posix_serial_init`), or NULL on no-match. */
static const char *locator_serial_path(const char *locator) {
    if (locator == NULL) return NULL;
    static const char *const sch_two = "serial://";
    static const char *const sch_one = "serial:/";
    if (strncmp(locator, sch_two, 9) == 0) {
        return locator + 9;
    }
    /* Match `serial:/` *only* if `serial://` did not match — checked
     * via prefix length above (9 vs 8). */
    if (strncmp(locator, sch_one, 8) == 0 && locator[8] != '/') {
        return locator + 8;
    }
    if (strncmp(locator, "/dev/", 5) == 0) {
        return locator;
    }
    return NULL;
}

nros_rmw_ret_t xrce_session_open(const char *locator, uint8_t mode,
                                 uint32_t domain_id, const char *node_name,
                                 nros_rmw_session_t *out) {
    (void)mode;
    if (out == NULL || node_name == NULL) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    if (out->backend_data != NULL) {
        return NROS_RMW_RET_ERROR;
    }

    xrce_session_state_t *st = (xrce_session_state_t *)
        calloc(1, sizeof(xrce_session_state_t));
    if (st == NULL) {
        return NROS_RMW_RET_BAD_ALLOC;
    }
    st->next_entity_id = 2; /* id 1 reserved for the participant */

    /* Phase 115.K.2.4 — `custom://...` routes through
     * `xrce_custom_transport_install`. UDP path mirrors K.2.1.
     * Phase 115.K.2.5.1.5-serial — `serial://...` / `/dev/...`
     * routes through `xrce_posix_serial_init` (POSIX hosts only).
     * Phase 127.C.4 — bare host:port locator now uses the
     * platform-blind `xrce_nros_udp_init` path on every target;
     * consumer must link a `nros_platform_udp_*` provider (POSIX
     * net.c auto-linked by xrce-cffi build.rs on libc hosts; Zephyr /
     * bare-metal pull `nros-platform-<rtos>` via cmake glue). */
#if defined(UCLIENT_PLATFORM_POSIX)
    const char *serial_path = locator_serial_path(locator);
#else
    const char *serial_path = NULL;
#endif
    if (locator_is_custom(locator)) {
        st->use_custom_transport = true;
        nros_rmw_ret_t ret = xrce_custom_transport_install(st, /*framing=*/false);
        if (ret != NROS_RMW_RET_OK) {
            free(st);
            return ret;
        }
        uxr_init_session(&st->session, &st->custom.comm,
                         hash_session_key(node_name));
#if defined(UCLIENT_PLATFORM_POSIX)
    } else if (serial_path != NULL) {
        st->use_custom_transport = true;
        nros_rmw_ret_t sret = xrce_posix_serial_init(st, serial_path);
        if (sret != NROS_RMW_RET_OK) {
            free(st);
            return sret;
        }
        uxr_init_session(&st->session, &st->custom.comm,
                         hash_session_key(node_name));
#endif
    } else {
        const char *addr_locator = locator != NULL ? locator : "127.0.0.1";
        (void)locator_strip_udp_prefix(&addr_locator);

        char host[64];
        uint16_t port = XRCE_DEFAULT_AGENT_PORT;
        if (parse_host_port(addr_locator, host, sizeof(host), &port) == 0) {
            size_t hlen = strlen(addr_locator);
            if (hlen == 0 || hlen + 1 > sizeof(host)) {
                free(st);
                return NROS_RMW_RET_INVALID_ARGUMENT;
            }
            memcpy(host, addr_locator, hlen + 1);
        }
        char port_str[8];
        snprintf(port_str, sizeof(port_str), "%u", (unsigned)port);

        /* Phase 129.NET.3 — UDP via the canonical `nros_platform_udp_*`
         * ABI. Platform-blind: works on any target with a wired
         * platform-provider. */
        st->use_custom_transport = true;
        nros_rmw_ret_t udp_ret = xrce_nros_udp_init(st, host, port_str);
        if (udp_ret != NROS_RMW_RET_OK) {
            free(st);
            return udp_ret;
        }
        uxr_init_session(&st->session, &st->custom.comm,
                         hash_session_key(node_name));
    }

    /* Topic / request / reply callbacks — single registration per
     * session. The session-state pointer is threaded through `args`
     * so the callbacks can find their slot pools without leaning on
     * a module global. */
    uxr_set_topic_callback(&st->session, xrce_topic_callback, st);
    uxr_set_request_callback(&st->session, xrce_request_callback, st);
    uxr_set_reply_callback(&st->session, xrce_reply_callback, st);

    if (!uxr_create_session_retries(&st->session, XRCE_SESSION_CREATION_RETRIES)) {
        /* Both UDP and `custom://` paths now go through
         * uxrCustomTransport — close via custom_transport. */
        uxr_close_custom_transport(&st->custom);
        free(st);
        return NROS_RMW_RET_ERROR;
    }

    st->output_reliable = uxr_create_output_reliable_stream(
        &st->session, st->output_reliable_buf,
        sizeof(st->output_reliable_buf), XRCE_STREAM_HISTORY);
    st->input_reliable = uxr_create_input_reliable_stream(
        &st->session, st->input_reliable_buf,
        sizeof(st->input_reliable_buf), XRCE_STREAM_HISTORY);

    /* Create the DDS participant. ID 1 is reserved for it. */
    st->participant_oid = uxr_object_id(1, UXR_PARTICIPANT_ID);

    char name_buf[XRCE_PARTICIPANT_NAME_BUF_SIZE];
    copy_truncating(name_buf, sizeof(name_buf), node_name);

    uint16_t req = uxr_buffer_create_participant_bin(
        &st->session, st->output_reliable, st->participant_oid,
        (uint16_t)domain_id, name_buf, UXR_REPLACE);

    uint8_t  status = 0;
    uint16_t requests[1] = { req };
    uint8_t  statuses[1] = { 0 };
    nros_rmw_ret_t cret = xrce_confirm_entities(st, requests, statuses, 1);
    (void)status;
    if (cret != NROS_RMW_RET_OK) {
        (void)uxr_delete_session(&st->session);
        uxr_close_custom_transport(&st->custom);
        free(st);
        return cret;
    }

    out->backend_data = st;
    return NROS_RMW_RET_OK;
}

nros_rmw_ret_t xrce_session_close(nros_rmw_session_t *session) {
    if (session == NULL) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    xrce_session_state_t *st = (xrce_session_state_t *)session->backend_data;
    if (st == NULL) {
        return NROS_RMW_RET_ERROR;
    }
    (void)uxr_delete_session(&st->session);
    /* All three transport paths (custom://, serial://, udp://) now
     * sit on top of uxrCustomTransport — the K.2.5.1.2.a fix routed
     * UDP through the same surface to match xrce-sys's legacy
     * shape. Close once, regardless of `use_custom_transport`. */
    uxr_close_custom_transport(&st->custom);
    free(st);
    session->backend_data = NULL;
    return NROS_RMW_RET_OK;
}

nros_rmw_ret_t xrce_session_drive_io(nros_rmw_session_t *session,
                                     int32_t timeout_ms) {
    if (session == NULL) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    xrce_session_state_t *st = (xrce_session_state_t *)session->backend_data;
    if (st == NULL) {
        return NROS_RMW_RET_ERROR;
    }
    int t = timeout_ms < 0 ? 0 : (int)timeout_ms;

    /* `uxr_run_session_time` returns as soon as the reliable output streams
     * are confirmed — so when the session holds a publisher with unconfirmed
     * WRITE_DATA (or a pending heartbeat) it returns almost immediately
     * (~0 us) instead of listening for `t` ms. XRCE is a *poll-based* backend
     * (no `set_wake_callback`): the executor's `spin_once(t)` paces by relying
     * on this call to block for `t`. When it returns early the spin loop
     * free-runs — a pub+sub node burns through a bounded loop in ~1 ms and
     * closes its session (DELETE_CLIENT) before its subscriber finishes DDS
     * discovery, so it never receives. See issue 0026.
     *
     * Drive the session across the whole `t` ms window — each pass services
     * inbound (delivering subscriber samples) — and yield ~1 ms when a pass
     * returns early, so the call consumes ~t ms wall-clock the way the caller
     * expects, without busy-spinning. (Mirrors the zpico_spin_once
     * `z_sleep_ms` fix for multi-threaded platforms.) */
    if (t == 0) {
        (void)uxr_run_session_time(&st->session, 0);
        return NROS_RMW_RET_OK;
    }
    struct timespec start;
    clock_gettime(CLOCK_MONOTONIC, &start);
    for (;;) {
        struct timespec now;
        clock_gettime(CLOCK_MONOTONIC, &now);
        long elapsed_ms =
            (now.tv_sec - start.tv_sec) * 1000L + (now.tv_nsec - start.tv_nsec) / 1000000L;
        int remaining = t - (int)elapsed_ms;
        if (remaining <= 0) {
            break;
        }
        (void)uxr_run_session_time(&st->session, remaining);
        /* If the run returned well before `remaining`, sleep ~1 ms so the next
         * pass picks up freshly-arrived inbound without busy-spinning. */
        struct timespec after;
        clock_gettime(CLOCK_MONOTONIC, &after);
        long pass_us =
            (after.tv_sec - now.tv_sec) * 1000000L + (after.tv_nsec - now.tv_nsec) / 1000L;
        if (pass_us < 1000) {
            struct timespec nap = {0, 1000000L}; /* 1 ms */
            nanosleep(&nap, NULL);
        }
    }
    return NROS_RMW_RET_OK;
}

/* Phase 124.F.2 — session-level connectivity probe.
 *
 * micro-XRCE-DDS-Client ships `uxr_ping_agent_session`: a single
 * GET_INFO round-trip over the already-open session that doesn't
 * disturb the rest of the application's streams. One attempt per
 * call — the runtime's `timeout_ms` is the per-attempt budget. */
nros_rmw_ret_t xrce_session_ping(nros_rmw_session_t *session,
                                 int32_t timeout_ms) {
    if (session == NULL) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    xrce_session_state_t *st = (xrce_session_state_t *)session->backend_data;
    if (st == NULL) {
        return NROS_RMW_RET_ERROR;
    }
    int t = timeout_ms < 0 ? 0 : (int)timeout_ms;
    bool ok = uxr_ping_agent_session(&st->session, t, 1);
    return ok ? NROS_RMW_RET_OK : NROS_RMW_RET_TIMEOUT;
}
