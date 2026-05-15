#ifndef NROS_RMW_XRCE_C_INTERNAL_H
#define NROS_RMW_XRCE_C_INTERNAL_H

/* Shared declarations across vtable.c / session.c / publisher.c /
 * subscriber.c / service.c / transport_custom.c.
 *
 * Phase 115.K.2 — backend state lives on the heap (one
 * `xrce_session_state` per session, malloc'd at `open`, parked in
 * `nros_rmw_session_t::backend_data`). Per-entity state lives in slots
 * inside that struct; entity shells get a pointer to the matching slot
 * via their `backend_data` field. Mirrors the design ground truth in
 * `packages/xrce/nros-rmw-xrce/src/lib.rs` but without the
 * module-static `XrceSessionState` it relies on.
 */

#include "nros/rmw_entity.h"
#include "nros/rmw_event.h"
#include "nros/rmw_ret.h"

#include <uxr/client/client.h>
#include <uxr/client/core/session/common_create_entities.h>

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/* ---- Tunables (must mirror packages/xrce/nros-rmw-xrce/build.rs
 *      defaults so the C backend behaves the same as the Rust one
 *      under nominal config). ---- */

#define XRCE_MAX_SUBSCRIBERS       8
#define XRCE_MAX_SERVICE_SERVERS   4
#define XRCE_MAX_SERVICE_CLIENTS   4
#define XRCE_BUFFER_SIZE           1024
#define XRCE_STREAM_HISTORY        4
/* Stream buffer sized after the largest MTU compiled in. Embedded
 * (no_POSIX) builds drop the UDP profile, so fall back to the custom
 * transport MTU. */
#if defined(UCLIENT_PROFILE_UDP)
#define XRCE_STREAM_BUFFER_SIZE    (UXR_CONFIG_UDP_TRANSPORT_MTU * XRCE_STREAM_HISTORY)
#else
#define XRCE_STREAM_BUFFER_SIZE    (UXR_CONFIG_CUSTOM_TRANSPORT_MTU * XRCE_STREAM_HISTORY)
#endif
#define XRCE_DDS_NAME_BUF_SIZE     128
#define XRCE_PARTICIPANT_NAME_BUF_SIZE 64
#define XRCE_ENTITY_CREATION_TIMEOUT_MS 1000
#define XRCE_SESSION_FLUSH_TIMEOUT_MS   100
#define XRCE_SESSION_CREATION_RETRIES   3

/* Default agent UDP port, matches Micro-XRCE-DDS-Agent's default. */
#define XRCE_DEFAULT_AGENT_PORT 2018

/* Bounded busy-wait for service replies (ms). */
#define XRCE_SERVICE_REPLY_TIMEOUT_MS 50
#define XRCE_SERVICE_REPLY_TOTAL_MS   5000

/* ---- Per-entity slots ----------------------------------------------- */

/* Subscriber slot — single-message ringbuffer with a `has_data` flag.
 * Phase 115.K.2 keeps overflow handling minimal: oversized messages
 * flag `overflow` and drop. Phase 115.K.2.x follow-ups can grow this
 * to a real ringbuffer if topics show drop pressure.
 *
 * TODO 115.K.2.x: deadline tracking, async wakers. The Rust impl carries
 * `deadline_cb`, `last_msg_at_ms`, etc. Skipped here per K.2 scope.
 */
typedef struct xrce_subscriber_slot {
    uint8_t   data[XRCE_BUFFER_SIZE];
    size_t    len;
    bool      has_data;
    bool      overflow;
    /* `locked` mirrors the Rust impl: callbacks observing this drop
     * the message rather than overwriting a buffer mid-read. */
    bool      locked;
    uint16_t  datareader_id;
    bool      active;
} xrce_subscriber_slot;

/* Service-server slot — request inbox. */
typedef struct xrce_service_server_slot {
    uint8_t        data[XRCE_BUFFER_SIZE];
    size_t         len;
    bool           has_request;
    bool           overflow;
    SampleIdentity sample_id;
    uint16_t       replier_id;
    bool           active;
} xrce_service_server_slot;

/* Service-client slot — reply inbox. */
typedef struct xrce_service_client_slot {
    uint8_t   data[XRCE_BUFFER_SIZE];
    size_t    len;
    bool      has_reply;
    bool      overflow;
    uint16_t  requester_id;
    bool      active;
} xrce_service_client_slot;

/* ---- Per-session state ---------------------------------------------- */

struct xrce_session_state {
    /* Transport — UDP (POSIX builds only) or custom. Only one is
     * live at a time; the mode is captured at open via the locator
     * scheme. Embedded builds drop the UDP profile entirely, so the
     * field is gated to keep the struct size predictable across
     * targets. */
#if defined(UCLIENT_PROFILE_UDP)
    uxrUDPTransport      udp;
#endif
    uxrCustomTransport   custom;
    bool                 use_custom_transport;
    /* Phase 115.K.2.5.1.2.a-fix-transport — POSIX UDP via custom
     * transport. `udp_bridge.fd` is set by `xrce_posix_udp_init`
     * and read by the per-session trampolines through
     * `uxrCustomTransport.args`. */
    struct {
        int fd;
        void *sock;
        void *endpoint;
    }                    udp_bridge;
    /* Phase 115.K.2.5.1.5-serial — POSIX serial transport via
     * custom transport. Same shape as `udp_bridge`: an `int fd`
     * threaded through the trampolines via `uxrCustomTransport.args`. */
    struct {
        int fd;
    }                    serial_bridge;

    uxrSession           session;

    /* Reliable streams. The buffers must outlive the session. */
    uint8_t              output_reliable_buf[XRCE_STREAM_BUFFER_SIZE];
    uint8_t              input_reliable_buf [XRCE_STREAM_BUFFER_SIZE];
    uxrStreamId          output_reliable;
    uxrStreamId          input_reliable;

    /* Participant + entity-id allocator. */
    uxrObjectId          participant_oid;
    uint16_t             next_entity_id;

    /* Per-entity slot pools. */
    xrce_subscriber_slot     subscriber_slots[XRCE_MAX_SUBSCRIBERS];
    xrce_service_server_slot service_server_slots[XRCE_MAX_SERVICE_SERVERS];
    xrce_service_client_slot service_client_slots[XRCE_MAX_SERVICE_CLIENTS];
};

typedef struct xrce_session_state xrce_session_state_t;

/* Per-publisher state. */
typedef struct xrce_publisher_state {
    xrce_session_state_t *session_state;
    uxrObjectId           datawriter_oid;
} xrce_publisher_state;

/* Per-subscriber state — the slot lives inside the session state. */
typedef struct xrce_subscriber_state {
    xrce_session_state_t *session_state;
    xrce_subscriber_slot *slot;
    uxrObjectId           datareader_oid;
} xrce_subscriber_state;

/* Per-service-server state. */
typedef struct xrce_service_server_state {
    xrce_session_state_t     *session_state;
    xrce_service_server_slot *slot;
    uxrObjectId               replier_oid;
} xrce_service_server_state;

/* Per-service-client state. */
typedef struct xrce_service_client_state {
    xrce_session_state_t     *session_state;
    xrce_service_client_slot *slot;
    uxrObjectId               requester_oid;
} xrce_service_client_state;

/* ---- Helpers -------------------------------------------------------- */

/* Allocate the next entity id of the given type. Mirrors the Rust
 * impl's `alloc_entity_id`. */
uxrObjectId xrce_alloc_entity_id(xrce_session_state_t *st, uint8_t type);

/* Run the agent until all `count` request statuses are received,
 * returning OK only if every status is `UXR_STATUS_OK` /
 * `UXR_STATUS_OK_MATCHED`. */
nros_rmw_ret_t xrce_confirm_entities(xrce_session_state_t *st,
                                     const uint16_t *requests,
                                     uint8_t        *statuses,
                                     size_t          count);

/* DDS topic-name conversion. Strips a leading '/' and prepends "rt/"
 * unless `avoid_ros_prefix` is non-zero. Writes a NUL-terminated
 * string into `out` (capacity `out_cap`); truncates if too long. */
void xrce_dds_topic_name(const char *topic_name, int avoid_ros_prefix,
                         char *out, size_t out_cap);
void xrce_dds_request_topic(const char *service_name, char *out, size_t out_cap);
void xrce_dds_reply_topic  (const char *service_name, char *out, size_t out_cap);
void xrce_dds_request_type (const char *type_name,    char *out, size_t out_cap);
void xrce_dds_reply_type   (const char *type_name,    char *out, size_t out_cap);

/* QoS mapping. */
uxrQoS_t xrce_map_qos(const nros_rmw_qos_t *qos);

/* ---- session.c ---- */
nros_rmw_ret_t xrce_session_open(const char *locator, uint8_t mode,
                                 uint32_t domain_id, const char *node_name,
                                 nros_rmw_session_t *out);
nros_rmw_ret_t xrce_session_close(nros_rmw_session_t *session);
nros_rmw_ret_t xrce_session_drive_io(nros_rmw_session_t *session,
                                     int32_t timeout_ms);
/* Phase 124.F.2 — connectivity probe via `uxr_ping_agent_session`. */
nros_rmw_ret_t xrce_session_ping(nros_rmw_session_t *session,
                                 int32_t timeout_ms);

/* ---- publisher.c ---- */
nros_rmw_ret_t xrce_publisher_create(nros_rmw_session_t *session,
                                     const char *topic_name,
                                     const char *type_name,
                                     const char *type_hash,
                                     uint32_t domain_id,
                                     const nros_rmw_qos_t *qos,
                                     nros_rmw_publisher_t *out);
void           xrce_publisher_destroy(nros_rmw_publisher_t *publisher);
nros_rmw_ret_t xrce_publisher_publish_raw(nros_rmw_publisher_t *publisher,
                                          const uint8_t *data, size_t len);
/* Phase 124.E.3 — streamed publish via `uxr_prepare_output_stream`. */
nros_rmw_ret_t xrce_publisher_publish_streamed(
        nros_rmw_publisher_t *publisher,
        void (*size_cb)(size_t *out_total_len, void *user_ctx),
        void (*chunk_cb)(uint8_t *out_buf, size_t cap,
                         size_t *out_written, void *user_ctx),
        void *user_ctx);

/* ---- subscriber.c ---- */
nros_rmw_ret_t xrce_subscriber_create(nros_rmw_session_t *session,
                                      const char *topic_name,
                                      const char *type_name,
                                      const char *type_hash,
                                      uint32_t domain_id,
                                      const nros_rmw_qos_t *qos,
                                      nros_rmw_subscriber_t *out);
void           xrce_subscriber_destroy(nros_rmw_subscriber_t *subscriber);
int32_t        xrce_subscriber_try_recv_raw(nros_rmw_subscriber_t *subscriber,
                                            uint8_t *buf, size_t buf_len);
int32_t        xrce_subscriber_has_data(nros_rmw_subscriber_t *subscriber);

/* Topic data callback — single instance per session, registered at
 * session_open. Exposed so session.c can pass its address to
 * `uxr_set_topic_callback`. */
void xrce_topic_callback(uxrSession *session,
                         uxrObjectId object_id,
                         uint16_t request_id,
                         uxrStreamId stream_id,
                         struct ucdrBuffer *ub,
                         uint16_t length,
                         void *args);

/* ---- service.c ---- */
nros_rmw_ret_t xrce_service_server_create(nros_rmw_session_t *session,
                                          const char *service_name,
                                          const char *type_name,
                                          const char *type_hash,
                                          uint32_t domain_id,
                                          nros_rmw_service_server_t *out);
void           xrce_service_server_destroy(nros_rmw_service_server_t *server);
int32_t        xrce_service_try_recv_request(nros_rmw_service_server_t *server,
                                             uint8_t *buf, size_t buf_len,
                                             int64_t *seq_out);
int32_t        xrce_service_has_request(nros_rmw_service_server_t *server);
nros_rmw_ret_t xrce_service_send_reply(nros_rmw_service_server_t *server,
                                       int64_t seq,
                                       const uint8_t *data, size_t len);

nros_rmw_ret_t xrce_service_client_create(nros_rmw_session_t *session,
                                          const char *service_name,
                                          const char *type_name,
                                          const char *type_hash,
                                          uint32_t domain_id,
                                          nros_rmw_service_client_t *out);
void           xrce_service_client_destroy(nros_rmw_service_client_t *client);
int32_t        xrce_service_call_raw(nros_rmw_service_client_t *client,
                                     const uint8_t *request, size_t req_len,
                                     uint8_t *reply_buf, size_t reply_buf_len);

void xrce_request_callback(uxrSession *session,
                           uxrObjectId object_id,
                           uint16_t request_id,
                           SampleIdentity *sample_id,
                           struct ucdrBuffer *ub,
                           uint16_t length,
                           void *args);
void xrce_reply_callback(uxrSession *session,
                         uxrObjectId object_id,
                         uint16_t request_id,
                         uint16_t reply_id,
                         struct ucdrBuffer *ub,
                         uint16_t length,
                         void *args);

/* ---- transport_custom.c (Phase 115.K.2.4) -------------------------- */

/* Install a runtime-supplied transport vtable into the session's
 * `uxrCustomTransport`. Trampolines fan out to the user's
 * open/close/write/read callbacks. The session.c open path consults
 * `xrce_custom_transport_is_armed()` after a `custom://` locator and
 * routes accordingly. */
struct xrce_custom_ops_slot;
int  xrce_custom_transport_is_armed(void);
nros_rmw_ret_t xrce_custom_transport_install(xrce_session_state_t *st,
                                             bool framing);

/* Phase 115.K.2.5.1.2.a-fix-transport — POSIX UDP via custom
 * transport. Replaces the K.2.1 `uxr_init_udp_transport` direct
 * path. Resolves `host`/`port`, opens a connected UDP socket,
 * and wires `xrce_session_state_t::custom` with trampolines that
 * drive the socket via `poll()` + `recv()` / `send()`. The
 * resulting transport behaves like the legacy `xrce-sys` shape
 * the agent has interop'd with for years. */
nros_rmw_ret_t xrce_posix_udp_init(xrce_session_state_t *st,
                                   const char *host, const char *port);

/* Zephyr UDP via the canonical nros platform networking ABI. Uses the same
 * Micro-XRCE custom transport shape as POSIX UDP, but delegates socket and
 * endpoint storage to nros_platform_udp_* instead of POSIX sockets. */
nros_rmw_ret_t xrce_zephyr_udp_init(xrce_session_state_t *st,
                                    const char *host, const char *port);

/* Phase 115.K.2.5.1.5-serial — POSIX serial transport via custom
 * transport. Opens a tty/pty `path`, configures termios (raw mode,
 * 8N1, baud from `XRCE_SERIAL_BAUD` env or 115200), and registers
 * read/write trampolines. framing=true (HDLC). */
nros_rmw_ret_t xrce_posix_serial_init(xrce_session_state_t *st,
                                      const char *path);

#ifdef __cplusplus
}
#endif

#endif /* NROS_RMW_XRCE_C_INTERNAL_H */
