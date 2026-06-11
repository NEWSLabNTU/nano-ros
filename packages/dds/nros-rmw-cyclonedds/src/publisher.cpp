// Publisher path — Phase 117.6 + 117.6.B + Phase 212.K.7.4.d.
//
// Entity creation: registry lookup → topic + writer + QoS.
// Data path: CDR bytes from runtime → dds_stream_read_sample into
// typed buffer → dds_write (Cyclone re-serialises) → wire.
//
// Phase 212.K.7.4.d retired the per-action manual ops-walking fast
// paths (`publish_goal_status_array` + `publish_fibonacci_feedback`).
// Those hardcoded `desc->m_ops[N]` offset reads under the assumption
// the descriptor was produced by the idlc static codegen; the K.7.4.b
// dynamic bridge emits structurally-identical-but-shifted op streams,
// so the hardcoded slot reads pointed at the wrong words and the
// server segfaulted on `memcpy(goal_id + uuid_off, ...)`. Both
// `GoalStatusArray_` and `_FeedbackMessage_` now flow through the
// same generic typed-sample path the rest of the backend uses, with
// one narrow wire-format adapter for the `_FeedbackMessage_` types
// (Rust serialises the action `goal_id` field with a `[4 u32=16]`
// length prefix as if it were a `sequence<octet>`, but the Cyclone
// IDL `Fibonacci_FeedbackMessage_ { octet goal_id[16]; … }` expects
// the 16 raw bytes inline). The receive side mirror is in
// `subscriber.cpp::insert_goal_id_len_at` (predates this commit).
//
// See `src/sertype_min.hpp` for the rationale behind the round-trip
// approach (Cyclone 0.10.5 doesn't expose the writer's internal
// sertype + serpool publicly; full zero-copy raw-CDR write is
// blocked on a future upstream API).

#include "internal.hpp"

#include "descriptors.hpp"
#include "qos.hpp"
#include "sertype_min.hpp"
#include "topic_prefix.hpp"

#include <dds/dds.h>
#include <dds/ddsi/ddsi_cdrstream.h>
#include <dds/ddsrt/heap.h>

#include <cstdlib>
#include <cstdint>
#include <cstring>
#include <new>

namespace nros_rmw_cyclonedds {

namespace {

struct PubState {
    dds_entity_t topic{0};
    dds_entity_t writer{0};
    const dds_topic_descriptor_t* desc{nullptr};
    SertypeMin* st{nullptr};
};

inline PubState* as_state(nros_rmw_publisher_t* p) {
    return static_cast<PubState*>(p->backend_data);
}

// Parse the 4-byte CDR encapsulation header (RTPS submessage prefix
// every ROS 2 publisher emits). Returns the XCDR version (1 or 2).
// `bytes` must be at least 4 bytes.
//
// Encoding identifier:
//   00 00 = CDR_BE, plain    → XCDR1
//   00 01 = CDR_LE, plain    → XCDR1
//   00 06 = D_CDR_LE         → XCDR2
//   00 07 = D_CDR_BE         → XCDR2
//   00 0a = PL_CDR2_LE       → XCDR2
//   00 0b = PL_CDR2_BE       → XCDR2
// Anything outside these is treated as XCDR1.
uint32_t cdr_xcdr_version(const uint8_t* bytes) {
    uint8_t lo = bytes[1];
    if (lo == 0x06 || lo == 0x07 || lo == 0x0a || lo == 0x0b) {
        return 2;
    }
    return 1;
}

bool type_ends_with(const dds_topic_descriptor_t* desc, const char* suffix) {
    if (desc == nullptr || desc->m_typename == nullptr || suffix == nullptr) {
        return false;
    }
    const std::size_t len = std::strlen(desc->m_typename);
    const std::size_t slen = std::strlen(suffix);
    return len >= slen && std::strcmp(desc->m_typename + len - slen, suffix) == 0;
}

bool writer_matched(dds_entity_t writer) {
    dds_publication_matched_status_t status{};
    return dds_get_publication_matched_status(writer, &status) == DDS_RETCODE_OK &&
           status.current_count > 0;
}

nros_rmw_ret_t wait_for_writer_match(dds_entity_t writer, uint64_t deadline_ms) {
    while (platform_now_ms() < deadline_ms) {
        if (writer_matched(writer)) return NROS_RMW_RET_OK;
        platform_sleep_ms(5);
    }
    return NROS_RMW_RET_TIMEOUT;
}

} // namespace

nros_rmw_ret_t publisher_create(nros_rmw_session_t* session, const char* topic_name,
                                const char* type_name, const char* /*type_hash*/,
                                uint32_t /*domain_id*/, const nros_rmw_qos_t* qos,
                                nros_rmw_publisher_t* out) {
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

    auto* state = new (std::nothrow) PubState();
    if (state == nullptr) {
        return NROS_RMW_RET_BAD_ALLOC;
    }

    // Phase 117.X.2: prepend `rt/` so we match `rmw_cyclonedds_cpp`'s
    // wire-level topic naming. Idempotent + env-opt-out via
    // NROS_RMW_CYCLONEDDS_SKIP_PREFIX=1.
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
    dds_entity_t writer = dds_create_writer(pp, topic, dq, nullptr);
    if (dq != nullptr) {
        dds_delete_qos(dq);
    }
    if (writer < 0) {
        (void)dds_delete(topic);
        delete state;
        return NROS_RMW_RET_ERROR;
    }
    state->writer = writer;

    state->st = new (std::nothrow) SertypeMin(desc);
    if (state->st == nullptr) {
        (void)dds_delete(writer);
        (void)dds_delete(topic);
        delete state;
        return NROS_RMW_RET_BAD_ALLOC;
    }

    out->backend_data = state;
    graph_track_writer(session_graph(session), writer); // Phase 177.36
    return NROS_RMW_RET_OK;
}

void publisher_destroy(nros_rmw_publisher_t* publisher) {
    if (publisher == nullptr) return;
    PubState* state = as_state(publisher);
    if (state == nullptr) return;
    if (state->writer > 0) (void)dds_delete(state->writer);
    if (state->topic > 0) (void)dds_delete(state->topic);
    delete state->st;
    delete state;
    publisher->backend_data = nullptr;
}

nros_rmw_ret_t publisher_publish_raw(nros_rmw_publisher_t* publisher, const uint8_t* data,
                                     size_t len) {
    if (publisher == nullptr || data == nullptr || len < 4) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    PubState* state = as_state(publisher);
    if (state == nullptr || state->desc == nullptr || state->st == nullptr) {
        return NROS_RMW_RET_ERROR;
    }
    const dds_topic_descriptor_t* desc = state->desc;

    // Parse encapsulation, copy the payload bytes after the 4-byte
    // header into a mutable scratch buffer (so we can splice out the
    // `_FeedbackMessage_` goal_id length prefix in place if needed).
    uint32_t xcdrv = cdr_xcdr_version(data);
    size_t body_len = len - 4;
    auto* body = static_cast<uint8_t*>(ddsrt_malloc(body_len > 0 ? body_len : 1));
    if (body == nullptr) {
        return NROS_RMW_RET_BAD_ALLOC;
    }
    if (body_len > 0) {
        std::memcpy(body, data + 4, body_len);
    }

    // 233.6 — the Rust runtime now serialises the action `goal_id` as the
    // fixed `octet[16]` the Cyclone IDL declares (no `[4 u32=16]`
    // sequence-style prefix), matching ROS 2 `unique_identifier_msgs/UUID`.
    // So the body already matches `dds_stream_read_sample`'s layout — no
    // strip needed (the old `strip_feedback_goal_id_prefix` adapter and its
    // `subscriber.cpp::insert_goal_id_len_at` receive-side mirror were both
    // removed together).

    // For action status (e.g. `GoalStatusArray_`) the publisher only
    // emits valid wire data once at least one reader has matched (the
    // action client's status sub). Without this gate the first
    // `dds_write` lands in an empty pub-set under VOLATILE QoS and is
    // silently dropped (Phase 171.0.a established the matched-status
    // gate for the service request path; the action status topic has
    // the same dependency).
    if (type_ends_with(desc, "::GoalStatusArray_")) {
        const uint64_t deadline = platform_now_ms() + 2000;
        if (wait_for_writer_match(state->writer, deadline) != NROS_RMW_RET_OK) {
            ddsrt_free(body);
            return NROS_RMW_RET_OK;
        }
    }

    // Allocate + zero typed sample buffer of the descriptor's static
    // size. `dds_stream_read_sample` walks the ops and fills it.
    void* sample = ddsrt_calloc(1, desc->m_size);
    if (sample == nullptr) {
        ddsrt_free(body);
        return NROS_RMW_RET_BAD_ALLOC;
    }

    dds_istream_t is;
    dds_istream_init(&is, static_cast<uint32_t>(body_len), body, xcdrv);
    dds_stream_read_sample(&is, sample, state->st->as_sertype());
    dds_istream_fini(&is);

    dds_return_t r = dds_write(state->writer, sample);

    dds_stream_free_sample(sample, desc->m_ops);
    ddsrt_free(sample);
    ddsrt_free(body);

    return (r == DDS_RETCODE_OK) ? NROS_RMW_RET_OK : NROS_RMW_RET_ERROR;
}

dds_entity_t publisher_writer(const nros_rmw_publisher_t* publisher) {
    if (publisher == nullptr || publisher->backend_data == nullptr) return 0;
    return static_cast<const PubState*>(publisher->backend_data)->writer;
}

} // namespace nros_rmw_cyclonedds
