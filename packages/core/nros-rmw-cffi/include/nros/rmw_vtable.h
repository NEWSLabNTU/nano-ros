#ifndef NROS_RMW_VTABLE_H
#define NROS_RMW_VTABLE_H

#include <stdint.h>
#include <stddef.h>

#include "nros/rmw_ret.h"
#include "nros/rmw_entity.h"
#include "nros/rmw_event.h"

#ifdef __cplusplus
extern "C" {
#endif

/**
 * @file rmw_vtable.h
 * @brief C function table for plugging third-party RMW backends into nros.
 *
 * Implement the functions in nros_rmw_vtable_t and call
 * nros_rmw_cffi_register() before creating any nros sessions.
 *
 * **Storage ownership.** The runtime owns the entity-struct storage
 * (`nros_rmw_session_t`, `nros_rmw_publisher_t`, `nros_rmw_subscriber_t`,
 * `nros_rmw_service_server_t`, `nros_rmw_service_client_t`). Each
 * `create_*` call receives a runtime-allocated, zero-initialised struct
 * via the `out` pointer; the backend writes its `backend_data` (and
 * `can_loan_messages` for pub/sub) into it. The runtime fills the metadata
 * fields (`topic_name`, `type_name`, `qos`) before calling
 * `create_*`; the backend reads them through the same struct.
 *
 * `destroy_*` releases the backend's `backend_data` only. The struct
 * shell stays valid until the runtime drops its owner.
 *
 * **Return-value conventions.**
 *  - `open` / `close` / `drive_io` / `create_*` / `publish_raw` /
 *    `send_reply`: `NROS_RMW_RET_OK` on success, negative
 *    `nros_rmw_ret_t` constant on error (see `<nros/rmw_ret.h>`).
 *  - `try_recv_raw` / `try_recv_request` / `call_raw`: non-negative =
 *    bytes produced, negative = `nros_rmw_ret_t` error.
 *  - `has_data` / `has_request`: 1 = yes, 0 = no.
 *  - `destroy_*`: void (best-effort cleanup).
 */

typedef struct nros_rmw_vtable_t {
    /* ---- Session lifecycle ---- */
    /** Open a session. The runtime supplies a zero-initialised
     *  `nros_rmw_session_t` via @p out with `node_name` /
     *  `namespace_` already filled. The backend writes
     *  `out->backend_data`. */
    nros_rmw_ret_t (*open)(const char *locator, uint8_t mode,
                           uint32_t domain_id, const char *node_name,
                           nros_rmw_session_t *out);
    nros_rmw_ret_t (*close)(nros_rmw_session_t *session);
    nros_rmw_ret_t (*drive_io)(nros_rmw_session_t *session, int32_t timeout_ms);

    /* ---- Publisher ---- */
    /** Create a publisher. The runtime fills `out->topic_name`,
     *  `out->type_name`, `out->qos` before this call; the backend
     *  writes `out->backend_data` and `out->can_loan_messages`. */
    nros_rmw_ret_t (*create_publisher)(nros_rmw_session_t *session,
        const char *topic_name, const char *type_name, const char *type_hash,
        uint32_t domain_id, const nros_rmw_qos_t *qos,
        nros_rmw_publisher_t *out);
    void (*destroy_publisher)(nros_rmw_publisher_t *publisher);
    nros_rmw_ret_t (*publish_raw)(nros_rmw_publisher_t *publisher,
        const uint8_t *data, size_t len);

    /* ---- Subscriber ---- */
    nros_rmw_ret_t (*create_subscriber)(nros_rmw_session_t *session,
        const char *topic_name, const char *type_name, const char *type_hash,
        uint32_t domain_id, const nros_rmw_qos_t *qos,
        nros_rmw_subscriber_t *out);
    void (*destroy_subscriber)(nros_rmw_subscriber_t *subscriber);
    int32_t (*try_recv_raw)(nros_rmw_subscriber_t *subscriber,
        uint8_t *buf, size_t buf_len);
    int32_t (*has_data)(nros_rmw_subscriber_t *subscriber);

    /* ---- Service Server ---- */
    nros_rmw_ret_t (*create_service_server)(nros_rmw_session_t *session,
        const char *service_name, const char *type_name, const char *type_hash,
        uint32_t domain_id,
        nros_rmw_service_server_t *out);
    void (*destroy_service_server)(nros_rmw_service_server_t *server);
    int32_t (*try_recv_request)(nros_rmw_service_server_t *server,
        uint8_t *buf, size_t buf_len, int64_t *seq_out);
    int32_t (*has_request)(nros_rmw_service_server_t *server);
    nros_rmw_ret_t (*send_reply)(nros_rmw_service_server_t *server,
        int64_t seq, const uint8_t *data, size_t len);

    /* ---- Service Client ---- */
    nros_rmw_ret_t (*create_service_client)(nros_rmw_session_t *session,
        const char *service_name, const char *type_name, const char *type_hash,
        uint32_t domain_id,
        nros_rmw_service_client_t *out);
    void (*destroy_service_client)(nros_rmw_service_client_t *client);
    int32_t (*call_raw)(nros_rmw_service_client_t *client,
        const uint8_t *request, size_t req_len,
        uint8_t *reply_buf, size_t reply_buf_len);

    /* ---- Phase 108 — status events (optional) ---- */
    /** Register a callback for a subscriber-side event. NULL function
     *  pointer = backend doesn't generate any subscriber events.
     *  Specific kind unsupported on a backend that supports some
     *  events = `NROS_RMW_RET_UNSUPPORTED` return.
     *  `deadline_ms` is consulted for `REQUESTED_DEADLINE_MISSED`
     *  only; ignored otherwise. */
    nros_rmw_ret_t (*register_subscriber_event)(
        nros_rmw_subscriber_t *subscriber,
        nros_rmw_event_kind_t  kind,
        uint32_t               deadline_ms,
        nros_rmw_event_callback_t cb,
        void                  *user_context);

    /** Register a callback for a publisher-side event. Same NULL /
     *  unsupported-kind conventions as `register_subscriber_event`.
     *  `deadline_ms` is consulted for `OFFERED_DEADLINE_MISSED` only. */
    nros_rmw_ret_t (*register_publisher_event)(
        nros_rmw_publisher_t  *publisher,
        nros_rmw_event_kind_t  kind,
        uint32_t               deadline_ms,
        nros_rmw_event_callback_t cb,
        void                  *user_context);

    /** Phase 108.B — manually assert this publisher's liveliness.
     *  Required for `MANUAL_BY_TOPIC` / `MANUAL_BY_NODE` liveliness
     *  kinds; no-op (return `NROS_RMW_RET_OK`) for other kinds.
     *  NULL function pointer = backend doesn't support manual
     *  liveliness; runtime returns `NROS_RMW_RET_OK` for AUTOMATIC /
     *  NONE callers and `NROS_RMW_RET_UNSUPPORTED` for MANUAL_*. */
    nros_rmw_ret_t (*assert_publisher_liveliness)(
        nros_rmw_publisher_t *publisher);

    /** Phase 110.0 — backend's next internal-event deadline in
     *  milliseconds from now (lease keepalive, heartbeat, reader
     *  ACK-NACK timeout, etc.). The runtime caps its `drive_io`
     *  timeout against `min(user_timeout, timer_deadline, this)` so
     *  quiet links don't wake early, see no user-visible work, and
     *  round-trip back into `drive_io`.
     *
     *  Returns a non-negative milliseconds value, or a negative value
     *  meaning "no internal deadline" (treat as `None`).
     *
     *  NULL function pointer is permitted — the runtime treats it the
     *  same as a negative return. */
    int32_t (*next_deadline_ms)(const nros_rmw_session_t *session);
} nros_rmw_vtable_t;

/** Register a custom RMW backend. Call before creating any sessions.
 *  Returns NROS_RMW_RET_OK. */
nros_rmw_ret_t nros_rmw_cffi_register(const nros_rmw_vtable_t *vtable);

#ifdef __cplusplus
}
#endif

#endif /* NROS_RMW_VTABLE_H */
