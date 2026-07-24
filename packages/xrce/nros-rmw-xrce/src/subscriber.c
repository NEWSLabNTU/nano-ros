/* Phase 115.K.2.2 — subscriber path.
 *
 * Mirrors the Rust impl's `XrceSession::create_subscriber` /
 * `XrceSubscriber::try_recv_raw`. Single-slot ringbuffer: callbacks
 * overwrite stale data; oversize messages flag overflow and drop.
 *
 * The topic callback dispatches by datareader_id to the matching
 * slot in the per-session pool. It's registered ONCE in
 * `xrce_session_create` (see session.c) — re-registering per
 * subscriber would race with concurrent inbound messages.
 */

#include "internal.h"

#include "nros/rmw_ret.h"

#include <stdlib.h>
#include <string.h>

#include <uxr/client/client.h>
#include <uxr/client/core/session/object_id.h>

/* Topic callback — dispatches by datareader id. Registered once at
 * session_open via `uxr_set_topic_callback(..., xrce_topic_callback,
 * st)`. */
void xrce_topic_callback(uxrSession *session,
                         uxrObjectId object_id,
                         uint16_t request_id,
                         uxrStreamId stream_id,
                         struct ucdrBuffer *ub,
                         uint16_t length,
                         void *args) {
    (void)session;
    (void)request_id;
    (void)stream_id;

    xrce_session_state_t *st = (xrce_session_state_t *)args;
    if (st == NULL || ub == NULL) {
        return;
    }
    size_t len = (size_t)length;
    for (size_t i = 0; i < XRCE_MAX_SUBSCRIBERS; ++i) {
        xrce_subscriber_slot *slot = &st->subscriber_slots[i];
        if (!slot->active || slot->datareader_id != object_id.id) {
            continue;
        }
        /* Reader currently reading the slot — drop. */
        if (slot->locked) {
            return;
        }
        /* Phase 160.H.1 — ring full → drop the newest. Preserves
         * in-order delivery of the already-buffered messages; the
         * alternative (overwrite oldest) silently shifts the
         * sequence which is harder to diagnose. */
        if (slot->count >= XRCE_SUBSCRIBER_RING_DEPTH) {
            return;
        }
        xrce_subscriber_ring_entry *entry = &slot->entries[slot->write_idx];
        /* XRCE-DDS interop: the agent delivers the bare CDR-serialized sample,
         * WITHOUT the 4-byte CDR encapsulation header (representation id +
         * options) — that header lives on the DDS/RTPS side, which the agent
         * owns. nano-ros's deserializers (and every other RMW path) expect the
         * header, so prepend the little-endian header here. Real PX4 /
         * `uxrce_dds_client` and real ROS 2 nodes both send headerless XRCE
         * payloads; without this, deserialization is misaligned by 4 bytes and
         * every inbound sample is dropped. (Symmetric with the publish side,
         * which strips the header before `uxr_buffer_topic`.) */
        if (len + XRCE_CDR_HEADER_LEN > XRCE_BUFFER_SIZE) {
            entry->overflow = true;
            entry->len = 0;
        } else {
            entry->data[0] = 0x00; /* CDR_LE representation id */
            entry->data[1] = 0x01;
            entry->data[2] = 0x00; /* options */
            entry->data[3] = 0x00;
            memcpy(entry->data + XRCE_CDR_HEADER_LEN, ub->iterator, len);
            entry->len = len + XRCE_CDR_HEADER_LEN;
            entry->overflow = false;
        }
        slot->write_idx = (uint16_t)((slot->write_idx + 1) % XRCE_SUBSCRIBER_RING_DEPTH);
        slot->count++;
        return;
    }
    /* TODO 115.K.2.x: bump a per-session "unmatched callback" counter
     * for diagnostics when the slot pool is full. */
}

nros_rmw_ret_t xrce_subscription_create(nros_rmw_session_t *session,
                                      const char *topic_name,
                                      const char *type_name,
                                      const char *type_hash,
                                      uint32_t domain_id,
                                      const nros_rmw_qos_t *qos,
                                      const nros_rmw_subscription_options_t *options,
                                      nros_rmw_subscription_t *out) {
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

    /* Find a free slot. */
    xrce_subscriber_slot *slot = NULL;
    for (size_t i = 0; i < XRCE_MAX_SUBSCRIBERS; ++i) {
        if (!st->subscriber_slots[i].active) {
            slot = &st->subscriber_slots[i];
            break;
        }
    }
    if (slot == NULL) {
        return NROS_RMW_RET_ERROR;
    }

    xrce_subscriber_state *ss = (xrce_subscriber_state *)
        calloc(1, sizeof(xrce_subscriber_state));
    if (ss == NULL) {
        return NROS_RMW_RET_BAD_ALLOC;
    }
    ss->session_state = st;
    ss->slot          = slot;

    uxrObjectId topic_oid = xrce_alloc_entity_id(st, UXR_TOPIC_ID);
    uxrObjectId sub_oid   = xrce_alloc_entity_id(st, UXR_SUBSCRIBER_ID);
    uxrObjectId dr_oid    = xrce_alloc_entity_id(st, UXR_DATAREADER_ID);

    int avoid_ros = 0;
    if (qos != NULL) {
        avoid_ros = qos->avoid_ros_namespace_conventions != 0;
    }

    char dds_topic[XRCE_DDS_NAME_BUF_SIZE];
    char dds_type[XRCE_DDS_NAME_BUF_SIZE];
    xrce_dds_topic_name(topic_name, avoid_ros, dds_topic, sizeof(dds_topic));
    size_t tn_len = strlen(type_name);
    if (tn_len + 1 > sizeof(dds_type)) tn_len = sizeof(dds_type) - 1;
    memcpy(dds_type, type_name, tn_len);
    dds_type[tn_len] = '\0';

    uxrQoS_t xrce_qos = xrce_map_qos(qos);

    uint16_t req_topic = uxr_buffer_create_topic_bin(
        &st->session, st->output_reliable, topic_oid, st->participant_oid,
        dds_topic, dds_type, UXR_REPLACE);
    uint16_t req_sub = uxr_buffer_create_subscriber_bin(
        &st->session, st->output_reliable, sub_oid, st->participant_oid,
        UXR_REPLACE);
    uint16_t req_dr = uxr_buffer_create_datareader_bin(
        &st->session, st->output_reliable, dr_oid, sub_oid, topic_oid,
        xrce_qos, UXR_REPLACE);

    uint16_t requests[3] = { req_topic, req_sub, req_dr };
    uint8_t  statuses[3] = { 0, 0, 0 };
    nros_rmw_ret_t cret = xrce_confirm_entities(st, requests, statuses, 3);
    if (cret != NROS_RMW_RET_OK) {
        free(ss);
        return cret;
    }

    /* Register slot for callback dispatch. */
    slot->datareader_id = dr_oid.id;
    slot->write_idx     = 0;
    slot->read_idx      = 0;
    slot->count         = 0;
    slot->locked        = false;
    for (size_t i = 0; i < XRCE_SUBSCRIBER_RING_DEPTH; ++i) {
        slot->entries[i].len = 0;
        slot->entries[i].overflow = false;
    }
    slot->active        = true;
    ss->datareader_oid  = dr_oid;

    /* Request continuous data delivery. */
    uxrDeliveryControl delivery = {
        .max_samples           = UXR_MAX_SAMPLES_UNLIMITED,
        .max_elapsed_time      = UXR_MAX_ELAPSED_TIME_UNLIMITED,
        .max_bytes_per_second  = UXR_MAX_BYTES_PER_SECOND_UNLIMITED,
        .min_pace_period       = 0,
    };
    (void)uxr_buffer_request_data(&st->session, st->output_reliable,
                                  dr_oid, st->input_reliable, &delivery);
    (void)uxr_run_session_time(&st->session, XRCE_SESSION_FLUSH_TIMEOUT_MS);

    out->backend_data = ss;
    out->can_loan_messages = false;
    return NROS_RMW_RET_OK;
}

void xrce_subscription_destroy(nros_rmw_subscription_t *subscriber) {
    if (subscriber == NULL || subscriber->backend_data == NULL) {
        return;
    }
    xrce_subscriber_state *ss = (xrce_subscriber_state *)subscriber->backend_data;
    xrce_session_state_t *st = ss->session_state;

    if (ss->slot != NULL) {
        ss->slot->active = false;
        ss->slot->count = 0;
        ss->slot->write_idx = 0;
        ss->slot->read_idx = 0;
    }
    (void)uxr_buffer_delete_entity(&st->session, st->output_reliable,
                                   ss->datareader_oid);
    (void)uxr_run_session_time(&st->session, 0);

    free(ss);
    subscriber->backend_data = NULL;
}

int32_t xrce_subscription_try_recv_raw(nros_rmw_subscription_t *subscriber,
                                     uint8_t *buf, size_t buf_len) {
    if (subscriber == NULL || subscriber->backend_data == NULL) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    xrce_subscriber_state *ss = (xrce_subscriber_state *)subscriber->backend_data;
    xrce_subscriber_slot *slot = ss->slot;
    if (slot == NULL || slot->count == 0) {
        return NROS_RMW_RET_NO_DATA;
    }
    xrce_subscriber_ring_entry *entry = &slot->entries[slot->read_idx];
    /* Always consume the head slot regardless of outcome — overflow,
     * buffer-too-small, and successful read all advance the ring so a
     * single bad entry can't wedge the queue. */
    int32_t ret;
    if (entry->overflow) {
        ret = NROS_RMW_RET_MESSAGE_TOO_LARGE;
    } else if (entry->len > buf_len) {
        ret = NROS_RMW_RET_BUFFER_TOO_SMALL;
    } else {
        slot->locked = true;
        if (buf != NULL && entry->len > 0) {
            memcpy(buf, entry->data, entry->len);
        }
        slot->locked = false;
        ret = (int32_t)entry->len;
    }
    entry->len = 0;
    entry->overflow = false;
    slot->read_idx = (uint16_t)((slot->read_idx + 1) % XRCE_SUBSCRIBER_RING_DEPTH);
    slot->count--;
    return ret;
}

int32_t xrce_subscription_has_data(nros_rmw_subscription_t *subscriber) {
    if (subscriber == NULL || subscriber->backend_data == NULL) {
        return 0;
    }
    xrce_subscriber_state *ss = (xrce_subscriber_state *)subscriber->backend_data;
    if (ss->slot == NULL) {
        return 0;
    }
    return ss->slot->count > 0 ? 1 : 0;
}

/* Phase 231 (RFC-0038) — the XRCE backend already stages each message in a
 * static ring entry (`entry->data`), so it can hand the bytes to the callback
 * in place instead of copying into a caller buffer (copy #1 removed). */
int32_t xrce_subscription_supports_in_place(nros_rmw_subscription_t *subscriber) {
    (void)subscriber;
    return 1;
}

int32_t xrce_subscription_process_raw_in_place(
    nros_rmw_subscription_t *subscriber, void *ctx,
    void (*cb)(void *ctx, const uint8_t *ptr, size_t len)) {
    if (subscriber == NULL || subscriber->backend_data == NULL) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    xrce_subscriber_state *ss = (xrce_subscriber_state *)subscriber->backend_data;
    xrce_subscriber_slot *slot = ss->slot;
    if (slot == NULL || slot->count == 0) {
        return NROS_RMW_RET_NO_DATA;
    }
    xrce_subscriber_ring_entry *entry = &slot->entries[slot->read_idx];
    /* Always consume the head slot (overflow + success both advance) so a single
     * bad entry can't wedge the queue — mirrors try_recv_raw. */
    int32_t ret;
    if (entry->overflow) {
        ret = NROS_RMW_RET_MESSAGE_TOO_LARGE;
    } else {
        /* Borrow the ring entry in place — no copy into a caller buffer. The
         * callback must not re-enter this subscriber's receive (slot locked). */
        slot->locked = true;
        if (cb != NULL && entry->len > 0) {
            cb(ctx, entry->data, entry->len);
        }
        slot->locked = false;
        ret = 1; /* one message processed */
    }
    entry->len = 0;
    entry->overflow = false;
    slot->read_idx = (uint16_t)((slot->read_idx + 1) % XRCE_SUBSCRIBER_RING_DEPTH);
    slot->count--;
    return ret;
}
