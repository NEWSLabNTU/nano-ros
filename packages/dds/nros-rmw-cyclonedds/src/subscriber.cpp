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

#include <cstdint>
#include <cstdlib>
#include <cstring>
#include <new>

namespace nros_rmw_cyclonedds {

namespace {

struct SubState {
    dds_entity_t topic{0};
    dds_entity_t reader{0};
    const dds_topic_descriptor_t *desc{nullptr};
    SertypeMin                   *st{nullptr};
};

inline SubState *as_state(nros_rmw_subscriber_t *s) {
    return static_cast<SubState *>(s->backend_data);
}

} // namespace

nros_rmw_ret_t subscriber_create(nros_rmw_session_t *session,
                                 const char *topic_name, const char *type_name,
                                 const char * /*type_hash*/,
                                 uint32_t /*domain_id*/,
                                 const nros_rmw_qos_t *qos,
                                 nros_rmw_subscriber_t *out) {
    if (out == nullptr || session == nullptr || topic_name == nullptr ||
        type_name == nullptr) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    out->backend_data      = nullptr;
    out->can_loan_messages = false;

    dds_entity_t pp = session_participant(session);
    if (pp == 0) {
        return NROS_RMW_RET_ERROR;
    }

    const dds_topic_descriptor_t *desc = find_descriptor(type_name);
    if (desc == nullptr) {
        return NROS_RMW_RET_UNSUPPORTED;
    }

    auto *state = new (std::nothrow) SubState();
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
    state->desc  = desc;

    dds_qos_t *dq = (qos != nullptr) ? make_dds_qos(qos) : nullptr;
    dds_entity_t reader = dds_create_reader(pp, topic, dq, nullptr);
    if (dq != nullptr) {
        dds_delete_qos(dq);
    }
    if (reader < 0) {
        (void) dds_delete(topic);
        delete state;
        return NROS_RMW_RET_ERROR;
    }
    state->reader = reader;

    state->st = new (std::nothrow) SertypeMin(desc);
    if (state->st == nullptr) {
        (void) dds_delete(reader);
        (void) dds_delete(topic);
        delete state;
        return NROS_RMW_RET_BAD_ALLOC;
    }

    out->backend_data = state;
    return NROS_RMW_RET_OK;
}

void subscriber_destroy(nros_rmw_subscriber_t *subscriber) {
    if (subscriber == nullptr) return;
    SubState *state = as_state(subscriber);
    if (state == nullptr) return;
    if (state->reader > 0) (void) dds_delete(state->reader);
    if (state->topic > 0)  (void) dds_delete(state->topic);
    delete state->st;
    delete state;
    subscriber->backend_data = nullptr;
}

int32_t subscriber_try_recv_raw(nros_rmw_subscriber_t *subscriber,
                                uint8_t *buf, size_t buf_len) {
    if (subscriber == nullptr || buf == nullptr) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    SubState *state = as_state(subscriber);
    if (state == nullptr || state->desc == nullptr || state->st == nullptr) {
        return NROS_RMW_RET_ERROR;
    }

    void *samples[1] = {nullptr};
    dds_sample_info_t si[1];
    dds_return_t taken = dds_take(state->reader, samples, si, 1, 1);
    if (taken < 0) {
        return NROS_RMW_RET_ERROR;
    }
    if (taken == 0 || !si[0].valid_data) {
        if (taken > 0) {
            // We allocated a sample (Cyclone borrowed-on-take with
            // NULL pre-init), return the loan.
            (void) dds_return_loan(state->reader, samples, taken);
        }
        return NROS_RMW_RET_NO_DATA;
    }

    // Serialise the typed sample back to CDR (XCDR1, native byte
    // order). Cyclone's ostream grows on demand via realloc.
    dds_ostream_t os;
    dds_ostream_init(&os, 0, 1 /*xcdr1*/);
    bool ok = dds_stream_write_sample(&os, samples[0], state->st->as_sertype());
    (void) dds_return_loan(state->reader, samples, taken);

    if (!ok) {
        dds_ostream_fini(&os);
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

    uint32_t paylen = os.m_index;
    uint32_t total  = paylen + 4;
    if (buf_len < total) {
        dds_ostream_fini(&os);
        return NROS_RMW_RET_BUFFER_TOO_SMALL;
    }
    buf[0] = kEncId[0];
    buf[1] = kEncId[1];
    buf[2] = kEncOpts[0];
    buf[3] = kEncOpts[1];
    std::memcpy(buf + 4, os.m_buffer, paylen);
    dds_ostream_fini(&os);

    return static_cast<int32_t>(total);
}

int32_t subscriber_has_data(nros_rmw_subscriber_t *subscriber) {
    if (subscriber == nullptr || subscriber->backend_data == nullptr) return 0;
    SubState *state = as_state(subscriber);
    uint32_t status = 0;
    if (dds_get_status_changes(state->reader, &status) != DDS_RETCODE_OK) {
        return 0;
    }
    return (status & DDS_DATA_AVAILABLE_STATUS) ? 1 : 0;
}

dds_entity_t subscriber_reader(const nros_rmw_subscriber_t *subscriber) {
    if (subscriber == nullptr || subscriber->backend_data == nullptr) return 0;
    return static_cast<const SubState *>(subscriber->backend_data)->reader;
}

} // namespace nros_rmw_cyclonedds
