/* Phase 115.K.2.2 — publisher path.
 *
 * Mirrors the Rust impl's `XrceSession::create_publisher` /
 * `XrcePublisher::publish_raw` shape; bin-create only (no QoS XML
 * fallback in the C backend — see internal.h for the K.2 scope
 * gaps).
 */

#include "internal.h"

#include "nros/rmw_ret.h"

#include <stdlib.h>
#include <string.h>

#include <uxr/client/client.h>
#include <uxr/client/core/session/object_id.h>
#include <uxr/client/core/session/write_access.h>
#include <ucdr/microcdr.h>

nros_rmw_ret_t xrce_publisher_create(nros_rmw_session_t *session,
                                     const char *topic_name,
                                     const char *type_name,
                                     const char *type_hash,
                                     uint32_t domain_id,
                                     const nros_rmw_qos_t *qos,
                                     const nros_rmw_publisher_options_t *options,
                                     nros_rmw_publisher_t *out) {
    (void)type_hash;
    (void)domain_id;
    (void)options;

    if (session == NULL || out == NULL || topic_name == NULL || type_name == NULL) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    xrce_session_state_t *st = (xrce_session_state_t *)session->backend_data;
    if (st == NULL) {
        return NROS_RMW_RET_ERROR;
    }

    xrce_publisher_state *ps = (xrce_publisher_state *)
        calloc(1, sizeof(xrce_publisher_state));
    if (ps == NULL) {
        return NROS_RMW_RET_BAD_ALLOC;
    }
    ps->session_state = st;

    /* Allocate 3 entity ids (TOPIC, PUBLISHER, DATAWRITER). */
    uxrObjectId topic_oid = xrce_alloc_entity_id(st, UXR_TOPIC_ID);
    uxrObjectId pub_oid   = xrce_alloc_entity_id(st, UXR_PUBLISHER_ID);
    uxrObjectId dw_oid    = xrce_alloc_entity_id(st, UXR_DATAWRITER_ID);

    int avoid_ros = 0;
    if (qos != NULL) {
        avoid_ros = qos->avoid_ros_namespace_conventions != 0;
    }

    char dds_topic[XRCE_DDS_NAME_BUF_SIZE];
    char dds_type[XRCE_DDS_NAME_BUF_SIZE];
    xrce_dds_topic_name(topic_name, avoid_ros, dds_topic, sizeof(dds_topic));
    /* Type name: copy as-is. */
    size_t tn_len = strlen(type_name);
    if (tn_len + 1 > sizeof(dds_type)) {
        tn_len = sizeof(dds_type) - 1;
    }
    memcpy(dds_type, type_name, tn_len);
    dds_type[tn_len] = '\0';

    uxrQoS_t xrce_qos = xrce_map_qos(qos);

    uint16_t req_topic = uxr_buffer_create_topic_bin(
        &st->session, st->output_reliable, topic_oid, st->participant_oid,
        dds_topic, dds_type, UXR_REPLACE);
    uint16_t req_pub = uxr_buffer_create_publisher_bin(
        &st->session, st->output_reliable, pub_oid, st->participant_oid,
        UXR_REPLACE);
    uint16_t req_dw = uxr_buffer_create_datawriter_bin(
        &st->session, st->output_reliable, dw_oid, pub_oid, topic_oid,
        xrce_qos, UXR_REPLACE);

    uint16_t requests[3] = { req_topic, req_pub, req_dw };
    uint8_t  statuses[3] = { 0, 0, 0 };
    nros_rmw_ret_t cret = xrce_confirm_entities(st, requests, statuses, 3);
    if (cret != NROS_RMW_RET_OK) {
        free(ps);
        return cret;
    }

    ps->datawriter_oid = dw_oid;
    out->backend_data = ps;
    out->can_loan_messages = false;
    return NROS_RMW_RET_OK;
}

void xrce_publisher_destroy(nros_rmw_publisher_t *publisher) {
    if (publisher == NULL || publisher->backend_data == NULL) {
        return;
    }
    xrce_publisher_state *ps = (xrce_publisher_state *)publisher->backend_data;
    xrce_session_state_t *st = ps->session_state;

    /* Best-effort delete of the datawriter entity. We don't wait for
     * status — close-time teardown should not block on agent acks. */
    (void)uxr_buffer_delete_entity(&st->session, st->output_reliable,
                                   ps->datawriter_oid);
    (void)uxr_run_session_time(&st->session, 0);

    free(ps);
    publisher->backend_data = NULL;
}

nros_rmw_ret_t xrce_publisher_publish_raw(nros_rmw_publisher_t *publisher,
                                          const uint8_t *data, size_t len) {
    if (publisher == NULL || publisher->backend_data == NULL) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    if (data == NULL && len > 0) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    xrce_publisher_state *ps = (xrce_publisher_state *)publisher->backend_data;
    xrce_session_state_t *st = ps->session_state;

    /* XRCE-DDS interop: strip the 4-byte CDR encapsulation header the executor
     * prepends. The XRCE DATA payload carries the bare serialized sample; the
     * agent owns the DDS-side representation header. Real PX4 / ROS 2 endpoints
     * read headerless XRCE payloads — sending the header makes our samples
     * unparseable to them (and is symmetric with the subscriber, which
     * re-prepends the header on receive). */
    const uint8_t *body = data;
    size_t body_len = len;
    if (body_len >= XRCE_CDR_HEADER_LEN) {
        body += XRCE_CDR_HEADER_LEN;
        body_len -= XRCE_CDR_HEADER_LEN;
    }

    /* Try the non-fragmented fast path first. */
    uint16_t req = uxr_buffer_topic(
        &st->session, st->output_reliable, ps->datawriter_oid,
        (uint8_t *)(uintptr_t)body, body_len);
    if (req != UXR_INVALID_REQUEST_ID) {
        /* Flush so the bytes reach the agent without waiting for the
         * next drive_io tick. Mirrors the Rust impl. */
        (void)uxr_run_session_time(&st->session, 0);
        return NROS_RMW_RET_OK;
    }

    /* TODO 115.K.2.x: fragmented fallback via
     * `uxr_prepare_output_stream_fragmented` for messages larger than
     * a single stream slot. The Rust impl has it; skipped here until
     * a smoke test demonstrates the need. */
    return NROS_RMW_RET_MESSAGE_TOO_LARGE;
}

/* Phase 124.E.3 — streamed publish.
 *
 * `uxr_prepare_output_stream` reserves a `len`-byte WRITE_DATA
 * submessage in the reliable output stream and hands back a
 * `ucdrBuffer` whose `iterator` points straight at the payload
 * region. The user's `chunk_cb` writes directly into that region —
 * no per-publisher staging buffer — and we advance the cursor by
 * the reported byte count. Once the full `total` is delivered the
 * session is flushed so the bytes reach the agent immediately
 * (mirrors `publish_raw`). */
nros_rmw_ret_t xrce_publisher_publish_streamed(
        nros_rmw_publisher_t *publisher,
        void (*size_cb)(size_t *out_total_len, void *user_ctx),
        void (*chunk_cb)(uint8_t *out_buf, size_t cap,
                         size_t *out_written, void *user_ctx),
        void *user_ctx) {
    if (publisher == NULL || publisher->backend_data == NULL ||
        size_cb == NULL || chunk_cb == NULL) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    xrce_publisher_state *ps = (xrce_publisher_state *)publisher->backend_data;
    xrce_session_state_t *st = ps->session_state;

    size_t total = 0;
    size_cb(&total, user_ctx);
    if (total == 0) {
        return NROS_RMW_RET_OK; /* nothing to publish */
    }
    if (total > UINT32_MAX) {
        return NROS_RMW_RET_MESSAGE_TOO_LARGE;
    }

    /* XRCE-DDS interop: the executor's serialized `total` bytes start with the
     * 4-byte CDR encapsulation header, which must NOT be on the XRCE wire (the
     * agent owns the DDS representation header). Same contract as `publish_raw`
     * / the subscriber re-prepend. We can't strip from the zero-copy stream
     * region after the fact, so stage the full message, then copy the
     * header-stripped body into the reserved (`total - 4`) slot. */
    if (total < XRCE_CDR_HEADER_LEN) {
        return NROS_RMW_RET_ERROR; /* malformed: no room for a CDR header */
    }
    uint8_t *stage = (uint8_t *)malloc(total);
    if (stage == NULL) {
        return NROS_RMW_RET_BAD_ALLOC;
    }
    size_t staged = 0;
    while (staged < total) {
        size_t cap = total - staged;
        size_t written = 0;
        chunk_cb(stage + staged, cap, &written, user_ctx);
        if (written == 0) {
            break; /* EOF from the user before `total` was met */
        }
        if (written > cap) {
            written = cap; /* defensive clamp against a misbehaving cb */
        }
        staged += written;
    }
    if (staged != total) {
        free(stage);
        return NROS_RMW_RET_ERROR; /* size_cb / chunk_cb disagreed */
    }

    size_t body_len = total - XRCE_CDR_HEADER_LEN;
    ucdrBuffer ub;
    uint16_t req = uxr_prepare_output_stream(
        &st->session, st->output_reliable, ps->datawriter_oid,
        &ub, (uint32_t)body_len);
    if (req == UXR_INVALID_REQUEST_ID) {
        /* `body_len` exceeds a single stream slot. No fragmented path
         * in the K.2 backend yet — same gap as `publish_raw`. */
        free(stage);
        return NROS_RMW_RET_MESSAGE_TOO_LARGE;
    }
    if ((size_t)(ub.final - ub.iterator) < body_len) {
        free(stage);
        return NROS_RMW_RET_MESSAGE_TOO_LARGE;
    }
    memcpy(ub.iterator, stage + XRCE_CDR_HEADER_LEN, body_len);
    ub.iterator += body_len;
    free(stage);

    (void)uxr_run_session_time(&st->session, 0);
    return NROS_RMW_RET_OK;
}
