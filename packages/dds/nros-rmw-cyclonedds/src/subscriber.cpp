// Subscriber path — Phase 117.6 + 117.6.B.
//
// Entity creation: registry lookup → topic + reader + QoS.
// Data path: dds_take typed sample → dds_stream_write_sample to
// caller's CDR buffer (with 4-byte XCDR1-LE encapsulation header).

#include "internal.hpp"

#include "descriptors.hpp"
#include "qos.hpp"
#include "sertype_min.hpp"
#include "topic_prefix.hpp"

#include <dds/dds.h>
#include <dds/ddsi/ddsi_cdrstream.h>
#include <dds/ddsrt/heap.h>

#include <cstdint>
#include <cstdlib>
#include <cstring>
#include <new>

namespace nros_rmw_cyclonedds {

namespace {

struct SubState {
    dds_entity_t topic{0};
    dds_entity_t reader{0};
    const dds_topic_descriptor_t* desc{nullptr};
    SertypeMin* st{nullptr};
};

inline SubState* as_state(nros_rmw_subscription_t* s) {
    return static_cast<SubState*>(s->backend_data);
}

} // namespace

nros_rmw_ret_t subscription_create(nros_rmw_session_t* session, const char* topic_name,
                                 const char* type_name, const char* /*type_hash*/,
                                 uint32_t /*domain_id*/, const nros_rmw_qos_t* qos,
                                 const nros_rmw_subscription_options_t* /*options*/,
                                 nros_rmw_subscription_t* out) {
    if (out == nullptr || session == nullptr || topic_name == nullptr || type_name == nullptr) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    out->backend_data = nullptr;
    out->can_loan_messages = false;

    dds_entity_t pp = session_participant(session);
    if (pp == 0) {
        return NROS_RMW_RET_ERROR;
    }

    char eff_type[256];
    if (!action_topic_type(topic_name, type_name, eff_type, sizeof(eff_type))) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    const dds_topic_descriptor_t* desc = find_descriptor(eff_type);
    if (desc == nullptr) {
        return NROS_RMW_RET_UNSUPPORTED;
    }

    auto* state = new (std::nothrow) SubState();
    if (state == nullptr) {
        return NROS_RMW_RET_BAD_ALLOC;
    }

    // Phase 117.X.2: prepend `rt/` to match `rmw_cyclonedds_cpp`'s
    // wire-level topic naming.
    char prefixed[256];
    if (!topic_prefix::apply(topic_name, "rt", prefixed, sizeof(prefixed))) {
        delete state;
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    dds_entity_t topic = dds_create_topic(pp, desc, prefixed, nullptr, nullptr);
    if (topic < 0) {
        delete state;
        return NROS_RMW_RET_ERROR;
    }
    state->topic = topic;
    state->desc = desc;

    dds_qos_t* dq = (qos != nullptr) ? make_dds_qos(qos) : nullptr;
    dds_entity_t reader = dds_create_reader(pp, topic, dq, nullptr);
    if (dq != nullptr) {
        dds_delete_qos(dq);
    }
    if (reader < 0) {
        (void)dds_delete(topic);
        delete state;
        return NROS_RMW_RET_ERROR;
    }
    state->reader = reader;

    state->st = new (std::nothrow) SertypeMin(desc);
    if (state->st == nullptr) {
        (void)dds_delete(reader);
        (void)dds_delete(topic);
        delete state;
        return NROS_RMW_RET_BAD_ALLOC;
    }

    out->backend_data = state;
    graph_track_reader(session_graph(session), reader); // Phase 177.36
    return NROS_RMW_RET_OK;
}

void subscription_destroy(nros_rmw_subscription_t* subscriber) {
    if (subscriber == nullptr) return;
    SubState* state = as_state(subscriber);
    if (state == nullptr) return;
    if (state->reader > 0) (void)dds_delete(state->reader);
    if (state->topic > 0) (void)dds_delete(state->topic);
    delete state->st;
    delete state;
    subscriber->backend_data = nullptr;
}

int32_t subscription_try_recv_raw(nros_rmw_subscription_t* subscriber, uint8_t* buf, size_t buf_len) {
    if (subscriber == nullptr || buf == nullptr) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    SubState* state = as_state(subscriber);
    if (state == nullptr || state->desc == nullptr || state->st == nullptr) {
        return NROS_RMW_RET_ERROR;
    }

    // Phase 177.26.RX.2 — allocate the transient take buffer from Cyclone's
    // ddsrt heap, not libc. On RTOS targets (ThreadX, FreeRTOS) the libc heap
    // is separate from (and may be unconfigured relative to) the ddsrt heap, so
    // std::calloc returns nullptr and every take fails BAD_ALLOC. dds_take
    // deserialises into this buffer and dds_stream_free_sample frees nested
    // members through the ddsrt heap, so the buffer itself must match. Mirrors
    // the publisher path (Phase 177.22).
    void* sample = ddsrt_calloc(1, state->desc->m_size);
    if (sample == nullptr) {
        return NROS_RMW_RET_BAD_ALLOC;
    }
    void* samples[1] = {sample};
    dds_sample_info_t si[1];
    dds_return_t taken = dds_take(state->reader, samples, si, 1, 1);
    if (taken < 0) {
        ddsrt_free(sample);
        return NROS_RMW_RET_ERROR;
    }
    if (taken == 0 || !si[0].valid_data) {
        dds_stream_free_sample(sample, state->desc->m_ops);
        ddsrt_free(sample);
        return NROS_RMW_RET_NO_DATA;
    }

    // Serialise the typed sample back to CDR (XCDR1, native byte
    // order). Cyclone's ostream grows on demand via realloc.
    dds_ostream_t os;
    dds_ostream_init(&os, 0, 1 /*xcdr1*/);
    bool ok = dds_stream_write_sample(&os, sample, state->st->as_sertype());

    if (!ok) {
        dds_ostream_fini(&os);
        dds_stream_free_sample(sample, state->desc->m_ops);
        ddsrt_free(sample);
        return NROS_RMW_RET_ERROR;
    }

    // Prepend the 4-byte CDR encapsulation header. Native byte order
    // → 00 00 (BE) / 00 01 (LE) for the encoding identifier; options
    // = 00 00.
#if defined(__BYTE_ORDER__) && (__BYTE_ORDER__ == __ORDER_LITTLE_ENDIAN__)
    constexpr uint8_t kEncId[2] = {0x00, 0x01};
#else
    constexpr uint8_t kEncId[2] = {0x00, 0x00};
#endif
    constexpr uint8_t kEncOpts[2] = {0x00, 0x00};

    // 233.6 — the action `goal_id` is a fixed `octet[16]` on both the IDL
    // and the Rust runtime side now (ROS 2 `unique_identifier_msgs/UUID`),
    // so the serialised sample already matches what the Rust read path
    // expects — no length-prefix insert (the old `insert_goal_id_len_at`
    // mirror of `publisher.cpp::strip_feedback_goal_id_prefix` was removed
    // together with it).
    uint32_t paylen = os.m_index;
    uint32_t total = paylen + 4;
    if (buf_len < total) {
        dds_ostream_fini(&os);
        dds_stream_free_sample(sample, state->desc->m_ops);
        ddsrt_free(sample);
        return NROS_RMW_RET_BUFFER_TOO_SMALL;
    }
    buf[0] = kEncId[0];
    buf[1] = kEncId[1];
    buf[2] = kEncOpts[0];
    buf[3] = kEncOpts[1];
    std::memcpy(buf + 4, os.m_buffer, paylen);
    dds_ostream_fini(&os);
    dds_stream_free_sample(sample, state->desc->m_ops);
    ddsrt_free(sample);

    return static_cast<int32_t>(total);
}

// Phase 124.D.3 — native batch take. Cyclone DDS `dds_take` accepts
// (reader, buf, info, count, maxs) and returns N samples in one
// call. Serialise each typed sample back to CDR with the same
// encoding-header convention as `subscription_try_recv_raw`.
int32_t subscription_try_recv_sequence(nros_rmw_subscription_t* subscriber, uint8_t* buf,
                                     size_t per_msg_cap, size_t max_msgs, size_t* out_lens) {
    if (subscriber == nullptr || buf == nullptr || out_lens == nullptr) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    if (per_msg_cap == 0 || max_msgs == 0) {
        return 0;
    }
    SubState* state = as_state(subscriber);
    if (state == nullptr || state->desc == nullptr || state->st == nullptr) {
        return NROS_RMW_RET_ERROR;
    }

    // Stack-cap the per-call slot budget; Cyclone happily takes
    // any N but we want to bound the stack alloc. Larger callers
    // can issue multiple sequence-take rounds.
    constexpr size_t kMaxBatch = 32;
    const size_t take_n = max_msgs > kMaxBatch ? kMaxBatch : max_msgs;

    void* samples[kMaxBatch] = {nullptr};
    dds_sample_info_t si[kMaxBatch];

    dds_return_t taken = dds_take(state->reader, samples, si, take_n, take_n);
    if (taken < 0) {
        return NROS_RMW_RET_ERROR;
    }
    if (taken == 0) {
        return 0;
    }

#if defined(__BYTE_ORDER__) && (__BYTE_ORDER__ == __ORDER_LITTLE_ENDIAN__)
    constexpr uint8_t kEncId[2] = {0x00, 0x01};
#else
    constexpr uint8_t kEncId[2] = {0x00, 0x00};
#endif
    constexpr uint8_t kEncOpts[2] = {0x00, 0x00};

    size_t produced = 0;
    int32_t err = 0;
    for (dds_return_t i = 0; i < taken; ++i) {
        if (!si[i].valid_data) {
            continue;
        }
        dds_ostream_t os;
        dds_ostream_init(&os, 0, 1 /*xcdr1*/);
        bool ok = dds_stream_write_sample(&os, samples[i], state->st->as_sertype());
        if (!ok) {
            dds_ostream_fini(&os);
            err = NROS_RMW_RET_ERROR;
            break;
        }
        uint32_t paylen = os.m_index;
        uint32_t total = paylen + 4;
        if (per_msg_cap < total) {
            dds_ostream_fini(&os);
            err = NROS_RMW_RET_BUFFER_TOO_SMALL;
            break;
        }
        uint8_t* slot = buf + produced * per_msg_cap;
        slot[0] = kEncId[0];
        slot[1] = kEncId[1];
        slot[2] = kEncOpts[0];
        slot[3] = kEncOpts[1];
        std::memcpy(slot + 4, os.m_buffer, paylen);
        out_lens[produced] = total;
        produced++;
        dds_ostream_fini(&os);
    }

    // Return all loans (valid + invalid) in one call.
    (void)dds_return_loan(state->reader, samples, taken);

    if (err < 0) {
        return err;
    }
    return static_cast<int32_t>(produced);
}

int32_t subscription_has_data(nros_rmw_subscription_t* subscriber) {
    if (subscriber == nullptr || subscriber->backend_data == nullptr) return 0;
    // Cyclone's DATA_AVAILABLE status is edge-like for our executor use:
    // querying it as a pre-filter can clear/suppress the subsequent take
    // path while samples remain readable. This backend is poll-only, so a
    // conservative "maybe" keeps dispatch correct; try_recv_raw remains the
    // authoritative non-blocking check.
    return 1;
}

dds_entity_t subscription_reader(const nros_rmw_subscription_t* subscriber) {
    if (subscriber == nullptr || subscriber->backend_data == nullptr) return 0;
    return static_cast<const SubState*>(subscriber->backend_data)->reader;
}

} // namespace nros_rmw_cyclonedds
