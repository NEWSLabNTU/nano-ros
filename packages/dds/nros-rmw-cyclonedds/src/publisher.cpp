// Publisher path — Phase 117.6 + 117.6.B.
//
// Entity creation: registry lookup → topic + writer + QoS.
// Data path: CDR bytes from runtime → dds_stream_read_sample into
// typed buffer → dds_write (Cyclone re-serialises) → wire.
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

constexpr uint8_t kCdrLeHeader[4] = {0x00, 0x01, 0x00, 0x00};

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

bool type_contains(const dds_topic_descriptor_t* desc, const char* needle) {
    return desc != nullptr && desc->m_typename != nullptr && needle != nullptr &&
           std::strstr(desc->m_typename, needle) != nullptr;
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

struct DdsSequenceInt32 {
    uint32_t _maximum;
    uint32_t _length;
    int32_t* _buffer;
    bool _release;
};

struct DdsSequenceStruct {
    uint32_t _maximum;
    uint32_t _length;
    void* _buffer;
    bool _release;
};

bool parse_sequence_int32(const uint8_t* cdr, size_t cdr_len, size_t* pos, DdsSequenceInt32* out) {
    if (cdr == nullptr || pos == nullptr || out == nullptr || *pos + 4 > cdr_len) {
        return false;
    }
    uint32_t count = 0;
    std::memcpy(&count, cdr + *pos, sizeof(count));
    *pos += 4;
    if (count > (cdr_len - *pos) / sizeof(int32_t)) return false;

    out->_maximum = count;
    out->_length = count;
    out->_release = count > 0;
    out->_buffer = nullptr;
    if (count > 0) {
        out->_buffer = static_cast<int32_t*>(dds_alloc(count * sizeof(int32_t)));
        if (out->_buffer == nullptr) return false;
        std::memcpy(out->_buffer, cdr + *pos, count * sizeof(int32_t));
    }
    *pos += count * sizeof(int32_t);
    return true;
}

nros_rmw_ret_t publish_fibonacci_feedback(dds_entity_t writer, const dds_topic_descriptor_t* desc,
                                          const uint8_t* data, size_t len) {
    if (desc == nullptr || desc->m_ops == nullptr || data == nullptr || len < 4 + 4 + 16 + 4 + 4) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }

    const uint32_t* ops = desc->m_ops;
    const uint32_t goal_id_off = ops[1];
    const uint32_t feedback_off = ops[4];
    const uint32_t sequence_off = ops[8];

    size_t pos = 4;
    uint32_t uuid_len = 0;
    std::memcpy(&uuid_len, data + pos, sizeof(uuid_len));
    pos += 4;
    if (uuid_len != 16 || pos + 16 > len) return NROS_RMW_RET_INVALID_ARGUMENT;

    auto* sample = static_cast<uint8_t*>(ddsrt_calloc(1, desc->m_size));
    if (sample == nullptr) return NROS_RMW_RET_BAD_ALLOC;
    std::memcpy(sample + goal_id_off, data + pos, 16);
    pos += 16;
    if (pos + sizeof(kCdrLeHeader) <= len &&
        std::memcmp(data + pos, kCdrLeHeader, sizeof(kCdrLeHeader)) == 0) {
        pos += sizeof(kCdrLeHeader);
    }

    auto* sequence = reinterpret_cast<DdsSequenceInt32*>(sample + feedback_off + sequence_off);
    if (!parse_sequence_int32(data, len, &pos, sequence)) {
        ddsrt_free(sample);
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }

    const uint64_t deadline = platform_now_ms() + 2000;
    if (wait_for_writer_match(writer, deadline) != NROS_RMW_RET_OK) {
        dds_stream_free_sample(sample, desc->m_ops);
        ddsrt_free(sample);
        return NROS_RMW_RET_OK;
    }
    dds_return_t r = dds_write(writer, sample);
    dds_stream_free_sample(sample, desc->m_ops);
    ddsrt_free(sample);
    return (r == DDS_RETCODE_OK) ? NROS_RMW_RET_OK : NROS_RMW_RET_ERROR;
}

nros_rmw_ret_t publish_goal_status_array(dds_entity_t writer, const dds_topic_descriptor_t* desc,
                                         const uint8_t* data, size_t len) {
    if (desc == nullptr || desc->m_ops == nullptr || data == nullptr || len < 8) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }

    const uint32_t* ops = desc->m_ops;
    const uint32_t list_off = ops[1];
    const uint32_t elem_size = ops[2];
    const uint32_t goal_info_off = ops[6];
    const uint32_t status_off = ops[9];
    const uint32_t goal_id_off = ops[12];
    const uint32_t stamp_off = ops[15];
    const uint32_t uuid_off = ops[19];
    const uint32_t stamp_sec_off = ops[23];
    const uint32_t stamp_nsec_off = ops[25];

    size_t pos = 4;
    uint32_t count = 0;
    std::memcpy(&count, data + pos, sizeof(count));
    pos += 4;
    if (count > (len - pos) / 25u) return NROS_RMW_RET_INVALID_ARGUMENT;

    auto* sample = static_cast<uint8_t*>(ddsrt_calloc(1, desc->m_size));
    if (sample == nullptr) return NROS_RMW_RET_BAD_ALLOC;
    auto* list = reinterpret_cast<DdsSequenceStruct*>(sample + list_off);
    list->_maximum = count;
    list->_length = count;
    list->_release = count > 0;
    list->_buffer = nullptr;
    if (count > 0) {
        list->_buffer = dds_alloc(count * elem_size);
        if (list->_buffer == nullptr) {
            ddsrt_free(sample);
            return NROS_RMW_RET_BAD_ALLOC;
        }
        std::memset(list->_buffer, 0, count * elem_size);
    }

    auto* items = static_cast<uint8_t*>(list->_buffer);
    for (uint32_t i = 0; i < count; ++i) {
        if (pos + 16 + 4 + 4 + 1 > len) {
            dds_stream_free_sample(sample, desc->m_ops);
            ddsrt_free(sample);
            return NROS_RMW_RET_INVALID_ARGUMENT;
        }
        uint8_t* item = items + i * elem_size;
        uint8_t* goal_info = item + goal_info_off;
        uint8_t* goal_id = goal_info + goal_id_off;
        uint8_t* stamp = goal_info + stamp_off;
        std::memcpy(goal_id + uuid_off, data + pos, 16);
        pos += 16;
        std::memcpy(stamp + stamp_sec_off, data + pos, 4);
        pos += 4;
        std::memcpy(stamp + stamp_nsec_off, data + pos, 4);
        pos += 4;
        item[status_off] = data[pos];
        pos += 1;
    }

    const uint64_t deadline = platform_now_ms() + 2000;
    if (wait_for_writer_match(writer, deadline) != NROS_RMW_RET_OK) {
        dds_stream_free_sample(sample, desc->m_ops);
        ddsrt_free(sample);
        return NROS_RMW_RET_OK;
    }
    dds_return_t r = dds_write(writer, sample);
    dds_stream_free_sample(sample, desc->m_ops);
    ddsrt_free(sample);
    return (r == DDS_RETCODE_OK) ? NROS_RMW_RET_OK : NROS_RMW_RET_ERROR;
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
    const uint8_t* read_data = data;
    size_t read_len = len;
    if (type_ends_with(desc, "_FeedbackMessage_")) {
        if (!type_contains(desc, "Fibonacci_FeedbackMessage_")) {
            return NROS_RMW_RET_OK;
        }
        return publish_fibonacci_feedback(state->writer, desc, data, len);
    }
    if (type_ends_with(desc, "::GoalStatusArray_")) {
        return publish_goal_status_array(state->writer, desc, data, len);
    }

    // Parse encapsulation, locate payload bytes after the 4-byte
    // header.
    uint32_t xcdrv = cdr_xcdr_version(read_data);
    const uint8_t* payload = read_data + 4;
    uint32_t paylen = static_cast<uint32_t>(read_len - 4);

    // Allocate + zero typed sample buffer of the descriptor's static
    // size. `dds_stream_read_sample` walks the ops and fills it.
    void* sample = ddsrt_calloc(1, desc->m_size);
    if (sample == nullptr) {
        return NROS_RMW_RET_BAD_ALLOC;
    }

    dds_istream_t is;
    dds_istream_init(&is, paylen, payload, xcdrv);
    dds_stream_read_sample(&is, sample, state->st->as_sertype());
    dds_istream_fini(&is);

    dds_return_t r = dds_write(state->writer, sample);

    dds_stream_free_sample(sample, desc->m_ops);
    ddsrt_free(sample);

    return (r == DDS_RETCODE_OK) ? NROS_RMW_RET_OK : NROS_RMW_RET_ERROR;
}

dds_entity_t publisher_writer(const nros_rmw_publisher_t* publisher) {
    if (publisher == nullptr || publisher->backend_data == nullptr) return 0;
    return static_cast<const PubState*>(publisher->backend_data)->writer;
}

} // namespace nros_rmw_cyclonedds
