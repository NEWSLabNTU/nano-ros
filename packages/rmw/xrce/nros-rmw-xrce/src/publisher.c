/* Phase 115.K.2.2 — publisher path.
 *
 * Mirrors the Rust impl's `XrceSession::create_publisher` /
 * `XrcePublisher::publish_raw` shape; bin-create only (no QoS XML
 * fallback in the C backend — see internal.h for the K.2 scope
 * gaps).
 */

#include "internal.h"

#include "nros/rmw_ret.h"

#include <stdbool.h>
#include <stdlib.h>
#include <string.h>

#include <uxr/client/client.h>
#include <uxr/client/core/session/object_id.h>
#include <uxr/client/core/session/write_access.h>
#include <ucdr/microcdr.h>

rmw_ret_t xrce_publisher_create(const rmw_node_t* node,
                                     const rmw_message_type_support_t* type_support,
                                const char* topic_name,
                                     uint32_t domain_id,
                                     const rmw_qos_profile_t *qos,
                                     const rmw_publisher_options_t *options,
                                     rmw_publisher_t *out) {
    /* phase-406 W1 — one argument in, two locals out, so the body below is
       unchanged. A NULL type support is INVALID_ARGUMENT rather than an
       empty type: the identity is what the entity is keyed on, and one
       created without it matches nothing and reports nothing. */
    if (type_support == NULL) return NROS_RMW_RET_INVALID_ARGUMENT;
    const char* type_name = type_support->type_name;
    const char* type_hash = type_support->type_hash;
    (void)type_name;
    /* Phase 376 W5/B1 — the entity is created ON ITS NODE, as upstream does.
     * The node carries the route to its session (our `context`). */
    if (node == NULL) return NROS_RMW_RET_INVALID_ARGUMENT;
    rmw_session_t* session = node->session;
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
        nros_xrce_calloc(1, sizeof(xrce_publisher_state));
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
    rmw_ret_t cret = xrce_confirm_entities(st, requests, statuses, 3);
    if (cret != NROS_RMW_RET_OK) {
        nros_xrce_free(ps);
        return cret;
    }

    ps->datawriter_oid = dw_oid;
    out->backend_data = ps;
    out->can_loan_messages = false;
    return NROS_RMW_RET_OK;
}

rmw_ret_t xrce_publisher_destroy(rmw_publisher_t *publisher) {
    if (publisher == NULL || publisher->backend_data == NULL) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    xrce_publisher_state *ps = (xrce_publisher_state *)publisher->backend_data;
    xrce_session_state_t *st = ps->session_state;

    /* Phase 376 W5 — the slot reports now, but XRCE cannot know. The delete is
     * deliberately fire-and-forget (close-time teardown must not block on
     * agent acks), so the only failure this frame can see is a request that
     * would not BUFFER. That is worth reporting; the agent's own verdict is
     * not available at any price this path is willing to pay. */
    uint16_t req = uxr_buffer_delete_entity(&st->session, st->output_reliable,
                                            ps->datawriter_oid);
    (void)uxr_run_session_time(&st->session, 0);

    nros_xrce_free(ps);
    publisher->backend_data = NULL;
    return req == UXR_INVALID_REQUEST_ID ? NROS_RMW_RET_ERROR : NROS_RMW_RET_OK;
}

rmw_ret_t xrce_publisher_publish_raw(const rmw_publisher_t *publisher,
                                          rmw_byte_span_t payload) {
    /* phase-406 W2 — by value; unpacked so the body is unchanged. */
    const uint8_t *data = payload.data;
    const size_t len = payload.len;
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
/* Issue 0782 — the chunk-drive loop, factored out so it can be TESTED.
 *
 * It is four lines of index arithmetic across two destinations (a throwaway
 * encapsulation-header scratch, then the caller's stream slot) and it is the
 * only part of the streaming publish that a host without an XRCE agent can
 * exercise at all. Left inline it would have been verified by reading.
 *
 * Writes the caller's payload MINUS its leading 4-byte CDR encapsulation
 * header into `body[0 .. body_len)`. Returns how many payload bytes were
 * consumed in total (header included), which equals `total` on success and
 * less when `chunk_cb` reported EOF early.
 */
size_t xrce_drive_streamed_body(uint8_t *body, size_t body_len, size_t total,
                                void (*chunk_cb)(uint8_t *out_buf, size_t cap,
                                                 size_t *out_written, void *user_ctx),
                                void *user_ctx) {
    size_t consumed = 0;
    uint8_t encap[XRCE_CDR_HEADER_LEN];
    (void)body_len;

    while (consumed < total) {
        uint8_t *dst;
        size_t cap;
        if (consumed < XRCE_CDR_HEADER_LEN) {
            /* Still inside the encapsulation header: absorb into scratch and
             * drop it. `cap` is deliberately tiny — the contract lets the
             * backend call `chunk_cb` as many times as it likes. */
            dst = encap + consumed;
            cap = XRCE_CDR_HEADER_LEN - consumed;
        } else {
            dst = body + (consumed - XRCE_CDR_HEADER_LEN);
            cap = total - consumed;
        }
        size_t written = 0;
        chunk_cb(dst, cap, &written, user_ctx);
        if (written == 0) {
            break; /* EOF from the caller before `total` */
        }
        if (written > cap) {
            written = cap; /* defensive clamp against a misbehaving cb */
        }
        consumed += written;
    }
    return consumed;
}

rmw_ret_t xrce_publisher_publish_streamed(
        rmw_publisher_t *publisher,
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
     * / the subscriber re-prepend.
     *
     * Issue 0782 — this used to `malloc(total)`, stage the whole message and
     * memcpy the header-stripped body into the reserved slot. That was the ONLY
     * message-sized, per-publish allocation in this backend: every other
     * allocation here is create-time entity state, bounded and knowable at
     * design time. A variable-size allocation on the publish path is the
     * classic way to fragment a small FreeRTOS / Zephyr / NuttX heap, and a
     * caller reaching for `publish_streamed` is doing it precisely to CONTROL
     * memory — so handing it a hidden heap allocation inverted the point of the
     * slot.
     *
     * The header does not need a staging buffer, only somewhere to put four
     * bytes. Reserve `total - 4` up front, absorb the encapsulation header
     * through a 4-byte scratch, then let `chunk_cb` write the body STRAIGHT
     * into the stream slot. No allocation of any size. */
    if (total < XRCE_CDR_HEADER_LEN) {
        return NROS_RMW_RET_ERROR; /* malformed: no room for a CDR header */
    }
    size_t body_len = total - XRCE_CDR_HEADER_LEN;

    ucdrBuffer ub;
    uint16_t req = uxr_prepare_output_stream(
        &st->session, st->output_reliable, ps->datawriter_oid,
        &ub, (uint32_t)body_len);
    if (req == UXR_INVALID_REQUEST_ID) {
        /* `body_len` exceeds a single stream slot. No fragmented path
         * in the K.2 backend yet — same gap as `publish_raw`. */
        return NROS_RMW_RET_MESSAGE_TOO_LARGE;
    }
    if ((size_t)(ub.final - ub.iterator) < body_len) {
        return NROS_RMW_RET_MESSAGE_TOO_LARGE;
    }

    /* From here the slot is COMMITTED: `uxr` has no cancel for a prepared
     * output stream, so whatever is in it goes out on the next
     * `uxr_run_session_time`. A caller whose `chunk_cb` stops short of the
     * `total` its own `size_cb` promised therefore cannot be un-published —
     * the best available is to zero the remainder so the peer sees
     * deterministic padding rather than uninitialised stream memory, and to
     * report the error. zenoh's implementation of this slot CAN abort
     * (`z_bytes_writer_drop`); this one cannot, and that asymmetry is the
     * price of removing the staging buffer. Issue 0782 records it. */
    size_t consumed = xrce_drive_streamed_body(ub.iterator, body_len, total, chunk_cb, user_ctx);
    bool short_delivery = consumed != total;

    if (short_delivery) {
        size_t body_written =
            consumed > XRCE_CDR_HEADER_LEN ? consumed - XRCE_CDR_HEADER_LEN : 0;
        memset(ub.iterator + body_written, 0, body_len - body_written);
        ub.iterator += body_len;
        (void)uxr_run_session_time(&st->session, 0);
        return NROS_RMW_RET_ERROR; /* size_cb / chunk_cb disagreed */
    }

    ub.iterator += body_len;

    (void)uxr_run_session_time(&st->session, 0);
    return NROS_RMW_RET_OK;
}
