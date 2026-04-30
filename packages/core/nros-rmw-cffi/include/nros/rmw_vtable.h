#ifndef NROS_RMW_VTABLE_H
#define NROS_RMW_VTABLE_H

#include <stdint.h>
#include <stddef.h>

#include "nros/rmw_ret.h"

/**
 * @file rmw_vtable.h
 * @brief C function table for plugging third-party RMW backends into nros.
 *
 * Implement the functions in nros_rmw_vtable_t and call
 * nros_rmw_cffi_register() before creating any nros sessions.
 *
 * Return value conventions:
 *  - open:             non-null = success, NULL = error
 *  - close/drive_io/publish_raw/send_reply: 0 = success, negative
 *                      `nros_rmw_ret_t` (see <nros/rmw_ret.h>)
 *  - try_recv_raw:     positive = bytes received, 0 = no data, negative
 *                      `nros_rmw_ret_t`
 *  - try_recv_request: positive = bytes received (seq_out written),
 *                      0 = none, negative `nros_rmw_ret_t`
 *  - has_data/has_request: 1 = yes, 0 = no
 *  - call_raw:         positive = reply bytes, negative `nros_rmw_ret_t`
 *  - destroy_*:        void (best-effort cleanup)
 */

typedef void* nros_rmw_handle_t;

typedef struct nros_rmw_cffi_qos_t {
    uint8_t reliability;  /* 0=BestEffort, 1=Reliable */
    uint8_t durability;   /* 0=Volatile, 1=TransientLocal */
    uint8_t history;      /* 0=KeepLast, 1=KeepAll */
    uint32_t depth;
} nros_rmw_cffi_qos_t;

typedef struct nros_rmw_vtable_t {
    /* Session lifecycle */
    nros_rmw_handle_t (*open)(const char *locator, uint8_t mode,
                              uint32_t domain_id, const char *node_name);
    int32_t (*close)(nros_rmw_handle_t session);
    int32_t (*drive_io)(nros_rmw_handle_t session, int32_t timeout_ms);

    /* Publisher */
    nros_rmw_handle_t (*create_publisher)(nros_rmw_handle_t session,
        const char *topic_name, const char *type_name, const char *type_hash,
        uint32_t domain_id, const nros_rmw_cffi_qos_t *qos);
    void (*destroy_publisher)(nros_rmw_handle_t publisher);
    int32_t (*publish_raw)(nros_rmw_handle_t publisher,
        const uint8_t *data, size_t len);

    /* Subscriber */
    nros_rmw_handle_t (*create_subscriber)(nros_rmw_handle_t session,
        const char *topic_name, const char *type_name, const char *type_hash,
        uint32_t domain_id, const nros_rmw_cffi_qos_t *qos);
    void (*destroy_subscriber)(nros_rmw_handle_t subscriber);
    int32_t (*try_recv_raw)(nros_rmw_handle_t subscriber,
        uint8_t *buf, size_t buf_len);
    int32_t (*has_data)(nros_rmw_handle_t subscriber);

    /* Service Server */
    nros_rmw_handle_t (*create_service_server)(nros_rmw_handle_t session,
        const char *service_name, const char *type_name, const char *type_hash,
        uint32_t domain_id);
    void (*destroy_service_server)(nros_rmw_handle_t server);
    int32_t (*try_recv_request)(nros_rmw_handle_t server,
        uint8_t *buf, size_t buf_len, int64_t *seq_out);
    int32_t (*has_request)(nros_rmw_handle_t server);
    int32_t (*send_reply)(nros_rmw_handle_t server,
        int64_t seq, const uint8_t *data, size_t len);

    /* Service Client */
    nros_rmw_handle_t (*create_service_client)(nros_rmw_handle_t session,
        const char *service_name, const char *type_name, const char *type_hash,
        uint32_t domain_id);
    void (*destroy_service_client)(nros_rmw_handle_t client);
    int32_t (*call_raw)(nros_rmw_handle_t client,
        const uint8_t *request, size_t req_len,
        uint8_t *reply_buf, size_t reply_buf_len);
} nros_rmw_vtable_t;

/** Register a custom RMW backend. Call before creating any sessions. */
int32_t nros_rmw_cffi_register(const nros_rmw_vtable_t *vtable);

#endif /* NROS_RMW_VTABLE_H */
