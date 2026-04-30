#ifndef NROS_RMW_VTABLE_H
#define NROS_RMW_VTABLE_H

#include <stdint.h>
#include <stddef.h>

#include "nros/rmw_ret.h"
#include "nros/rmw_entity.h"

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
 * optionally `loan_caps`) into it. The runtime fills the metadata
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

/**
 * Legacy void-pointer alias retained for backends that round-trip
 * opaque state through `backend_data`. Public function pointers use
 * the typed entity structs from `<nros/rmw_entity.h>`.
 */
typedef void* nros_rmw_handle_t;

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
     *  writes `out->backend_data` and optionally `out->loan_caps`. */
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
} nros_rmw_vtable_t;

/** Register a custom RMW backend. Call before creating any sessions.
 *  Returns NROS_RMW_RET_OK. */
nros_rmw_ret_t nros_rmw_cffi_register(const nros_rmw_vtable_t *vtable);

#endif /* NROS_RMW_VTABLE_H */
