// Services — Phase 117.X.3 (cdds_request_header_t-shaped wire).
//
// Service traffic uses per-service typed Request / Response topics
// matching stock `rmw_cyclonedds_cpp`'s shape. Each typed struct
// carries the request-id correlation header inline as the first two
// fields:
//
//     struct <Pkg>::srv::dds_::<Svc>_Request_ {
//         octet     rmw_writer_guid[16];   // RTPS GUID of client writer
//         long long rmw_sequence_number;   // monotonic per-client
//         /* user fields ... */
//     };
//
// This is bit-equivalent to upstream's
// `cdds_request_header_t request_header;` followed by user fields
// (cdds_request_header_t = `{uint8_t guid[16]; int64_t seq;}` —
// same 24-byte layout). Stock `rclcpp` clients/servers therefore
// match by `(topic_name, type_name)` and exchange compatible bytes.
//
// The 117.X.1 codegen helper injects the two header fields at IDL
// time when processing `.srv` inputs; consumers call
// `nros_rmw_cyclonedds_generate_from_msg(... INTERFACES <Foo.srv>)`
// and get the right typed struct without further manual work.
//
// Wire data path (per-call):
//
//   service_call_raw   user-CDR bytes
//                       → build wire CDR `[encap][24-byte-header]
//                          [user CDR after-encap]`
//                       → dds_stream_read_sample into typed struct
//                       → dds_write
//                       → poll reply reader, filter on
//                         (writer_guid, seq) match.
//
//   service_try_recv_request:  dds_take typed struct
//                       → dds_stream_write_sample → wire CDR
//                       → split: (header, user payload)
//                       → stash header in slot, return slot index.
//
//   service_send_reply: lookup slot → build wire CDR
//                       `[encap][header from slot][user reply]`
//                       → dds_stream_read_sample → dds_write.
//
// Slot table (32 entries, fixed) preserved verbatim from 117.7.B —
// only the wire location of the correlation pair changes.

#include "internal.hpp"

#include "descriptors.hpp"
#include "qos.hpp"
#include "sertype_min.hpp"
#include "topic_prefix.hpp"

#include <dds/dds.h>
#include <dds/ddsi/ddsi_cdrstream.h>

#include <atomic>
#include <chrono>
#include <cstdint>
#include <cstdlib>
#include <cstring>
#include <new>
#include <random>
#include <thread>

namespace nros_rmw_cyclonedds {

namespace {

constexpr std::size_t kRequestSlots = 32;
constexpr std::size_t kMaxTopicName = 256;
constexpr std::size_t kHeaderBytes  = 24;  // 16-byte guid + 8-byte seq

struct RequestId {
    uint8_t guid[16];
    int64_t seq;
};

struct RequestSlot {
    RequestId id{};
    bool      in_use{false};
};

struct ServerState {
    dds_entity_t request_topic{0};
    dds_entity_t reply_topic{0};
    dds_entity_t reader{0};
    dds_entity_t writer{0};
    const dds_topic_descriptor_t *req_desc{nullptr};
    const dds_topic_descriptor_t *rep_desc{nullptr};
    SertypeMin                   *req_st{nullptr};
    SertypeMin                   *rep_st{nullptr};
    RequestSlot                   slots[kRequestSlots];
};

struct ClientState {
    dds_entity_t request_topic{0};
    dds_entity_t reply_topic{0};
    dds_entity_t writer{0};
    dds_entity_t reader{0};
    const dds_topic_descriptor_t *req_desc{nullptr};
    const dds_topic_descriptor_t *rep_desc{nullptr};
    SertypeMin                   *req_st{nullptr};
    SertypeMin                   *rep_st{nullptr};
    uint8_t                       my_guid[16]{};
    std::atomic<int64_t>          next_seq{0};
};

bool service_topic_name(const char *service_name, const char *prefix,
                        const char *suffix, char *out, std::size_t out_cap) {
    if (service_name == nullptr || prefix == nullptr || suffix == nullptr ||
        out == nullptr) {
        return false;
    }
    char with_suffix[kMaxTopicName];
    std::size_t blen = std::strlen(service_name);
    std::size_t slen = std::strlen(suffix);
    if (blen + slen + 1 > sizeof(with_suffix)) return false;
    std::memcpy(with_suffix, service_name, blen);
    std::memcpy(with_suffix + blen, suffix, slen);
    with_suffix[blen + slen] = '\0';
    return topic_prefix::apply(with_suffix, prefix, out, out_cap);
}

// Build the suffixed type name `<base>_Request_` / `<base>_Response_`
// the codegen helper (117.X.1) registers. Returns false on overflow.
bool service_type_name(const char *base, const char *suffix, char *out,
                       std::size_t out_cap) {
    std::size_t blen = std::strlen(base);
    std::size_t slen = std::strlen(suffix);
    if (blen + slen + 1 > out_cap) return false;
    std::memcpy(out, base, blen);
    std::memcpy(out + blen, suffix, slen);
    out[blen + slen] = '\0';
    return true;
}

uint32_t cdr_xcdr_version(const uint8_t *bytes) {
    uint8_t lo = bytes[1];
    if (lo == 0x06 || lo == 0x07 || lo == 0x0a || lo == 0x0b) return 2;
    return 1;
}

// Pull the 16-byte RTPS GUID from a Cyclone writer.
void writer_guid_bytes(dds_entity_t writer, uint8_t out[16]) {
    dds_guid_t g{};
    std::memset(out, 0, 16);
    if (dds_get_guid(writer, &g) == DDS_RETCODE_OK) {
        std::memcpy(out, g.v, 16);
    }
}

// Encode int64 little-endian into 8 bytes.
inline void put_le64(uint8_t *out, int64_t v) {
    for (int i = 0; i < 8; ++i) {
        out[i] = static_cast<uint8_t>((v >> (i * 8)) & 0xff);
    }
}
inline int64_t get_le64(const uint8_t *in) {
    int64_t v = 0;
    for (int i = 0; i < 8; ++i) {
        v |= static_cast<int64_t>(in[i]) << (i * 8);
    }
    return v;
}

// Construct the wire CDR for a typed struct that has the 24-byte
// request_header inlined at offset 0. Inputs:
//   user_bytes    runtime-supplied CDR with 4-byte encap + user fields
//                 (no header).
//   id            request_id to inject.
// Outputs:
//   wire_cdr      buffer of size at least len(user_bytes) + 24.
// Returns total wire byte count, or negative on error.
int32_t build_wire_with_header(const uint8_t *user_bytes, size_t user_len,
                               const RequestId &id, uint8_t *wire_cdr,
                               size_t wire_cap) {
    if (user_len < 4) return NROS_RMW_RET_INVALID_ARGUMENT;
    size_t total = user_len + kHeaderBytes;
    if (total > wire_cap) return NROS_RMW_RET_BUFFER_TOO_SMALL;
    // 4-byte encap copied verbatim.
    std::memcpy(wire_cdr, user_bytes, 4);
    // 16-byte guid.
    std::memcpy(wire_cdr + 4, id.guid, 16);
    // 8-byte little-endian seq.
    put_le64(wire_cdr + 4 + 16, id.seq);
    // User payload after encap.
    std::memcpy(wire_cdr + 4 + 24, user_bytes + 4, user_len - 4);
    return static_cast<int32_t>(total);
}

// Inverse: parse the wire CDR (with leading header) into
// `(out_id, user_bytes_with_encap)`. The wire's 4-byte encap is
// preserved in `user_out` so the runtime sees a normal CDR-shaped
// payload (encap + user fields).
//
// Returns user-payload length (incl. 4-byte encap) on success, or
// negative error.
int32_t split_wire_header(const uint8_t *wire_cdr, size_t wire_len,
                          RequestId *out_id,
                          uint8_t *user_out, size_t user_cap) {
    if (wire_len < 4 + kHeaderBytes) return NROS_RMW_RET_INVALID_ARGUMENT;
    if (out_id != nullptr) {
        std::memcpy(out_id->guid, wire_cdr + 4, 16);
        out_id->seq = get_le64(wire_cdr + 4 + 16);
    }
    size_t user_len = wire_len - kHeaderBytes;  // (encap stays + user fields)
    if (user_len > user_cap) return NROS_RMW_RET_BUFFER_TOO_SMALL;
    // Encap.
    std::memcpy(user_out, wire_cdr, 4);
    // User fields.
    std::memcpy(user_out + 4, wire_cdr + 4 + kHeaderBytes, user_len - 4);
    return static_cast<int32_t>(user_len);
}

// Run dds_stream_read_sample on @p wire_cdr, then dds_write. Caller
// owns @p wire_cdr.
nros_rmw_ret_t write_typed(dds_entity_t writer,
                           const dds_topic_descriptor_t *desc,
                           const SertypeMin *st,
                           const uint8_t *wire_cdr, size_t wire_len) {
    if (writer <= 0 || desc == nullptr || st == nullptr ||
        wire_cdr == nullptr || wire_len < 4) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    uint32_t xcdrv = cdr_xcdr_version(wire_cdr);
    void *sample = std::calloc(1, desc->m_size);
    if (sample == nullptr) return NROS_RMW_RET_BAD_ALLOC;

    dds_istream_t is;
    dds_istream_init(&is, static_cast<uint32_t>(wire_len - 4),
                     wire_cdr + 4, xcdrv);
    dds_stream_read_sample(&is, sample, st->as_sertype());
    dds_istream_fini(&is);

    dds_return_t r = dds_write(writer, sample);
    dds_stream_free_sample(sample, desc->m_ops);
    std::free(sample);
    return (r == DDS_RETCODE_OK) ? NROS_RMW_RET_OK : NROS_RMW_RET_ERROR;
}

// Take a typed sample, reserialise via dds_stream_write_sample, and
// hand back the resulting wire CDR (with 4-byte encap prepended).
// Caller-owned scratch buf must be ≥ 8 + desc->m_size + extras.
//
// Returns wire byte count, NROS_RMW_RET_NO_DATA, or negative error.
int32_t take_typed_wire(dds_entity_t reader, const SertypeMin *st,
                        uint8_t *out_buf, size_t out_cap) {
    void *samples[1] = {nullptr};
    dds_sample_info_t si[1];
    dds_return_t taken = dds_take(reader, samples, si, 1, 1);
    if (taken < 0) return NROS_RMW_RET_ERROR;
    if (taken == 0 || !si[0].valid_data) {
        if (taken > 0) (void) dds_return_loan(reader, samples, taken);
        return NROS_RMW_RET_NO_DATA;
    }

    dds_ostream_t os;
    dds_ostream_init(&os, 0, 1 /*xcdr1*/);
    bool ok = dds_stream_write_sample(&os, samples[0], st->as_sertype());
    (void) dds_return_loan(reader, samples, taken);
    if (!ok) {
        dds_ostream_fini(&os);
        return NROS_RMW_RET_ERROR;
    }

#if defined(__BYTE_ORDER__) && (__BYTE_ORDER__ == __ORDER_LITTLE_ENDIAN__)
    constexpr uint8_t kEncId[2] = {0x00, 0x01};
#else
    constexpr uint8_t kEncId[2] = {0x00, 0x00};
#endif

    uint32_t paylen = os.m_index;
    uint32_t total  = paylen + 4;
    if (out_cap < total) {
        dds_ostream_fini(&os);
        return NROS_RMW_RET_BUFFER_TOO_SMALL;
    }
    out_buf[0] = kEncId[0];
    out_buf[1] = kEncId[1];
    out_buf[2] = 0;
    out_buf[3] = 0;
    std::memcpy(out_buf + 4, os.m_buffer, paylen);
    dds_ostream_fini(&os);
    return static_cast<int32_t>(total);
}

// Per-call scratch ceiling. Tunable via env if a future user needs
// it; 64 KiB covers ROS 2's default service payload size budget.
constexpr std::size_t kWireScratch = 65536;

bool descriptors_for_service(const char *type_name,
                             const dds_topic_descriptor_t **out_req,
                             const dds_topic_descriptor_t **out_rep) {
    char req_type[kMaxTopicName];
    char rep_type[kMaxTopicName];
    if (!service_type_name(type_name, "_Request_",  req_type, sizeof(req_type))) {
        return false;
    }
    if (!service_type_name(type_name, "_Response_", rep_type, sizeof(rep_type))) {
        return false;
    }
    *out_req = find_descriptor(req_type);
    *out_rep = find_descriptor(rep_type);
    return *out_req != nullptr && *out_rep != nullptr;
}

uint64_t random_seed_word() {
    std::random_device rd;
    return (static_cast<uint64_t>(rd()) << 32) ^ rd();
}

void fill_random_guid(uint8_t out[16]) {
    uint64_t a = random_seed_word();
    uint64_t b = random_seed_word();
    std::memcpy(out, &a, 8);
    std::memcpy(out + 8, &b, 8);
}

} // namespace

// =========================================================================
// Service server
// =========================================================================

nros_rmw_ret_t service_server_create(nros_rmw_session_t *session,
                                     const char *service_name,
                                     const char *type_name,
                                     const char * /*type_hash*/,
                                     uint32_t /*domain_id*/,
                                     nros_rmw_service_server_t *out) {
    if (out == nullptr || session == nullptr || service_name == nullptr ||
        type_name == nullptr) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    out->backend_data = nullptr;

    dds_entity_t pp = session_participant(session);
    if (pp == 0) return NROS_RMW_RET_ERROR;

    const dds_topic_descriptor_t *req_desc = nullptr;
    const dds_topic_descriptor_t *rep_desc = nullptr;
    if (!descriptors_for_service(type_name, &req_desc, &rep_desc)) {
        return NROS_RMW_RET_UNSUPPORTED;
    }

    char req_topic[kMaxTopicName];
    char rep_topic[kMaxTopicName];
    if (!service_topic_name(service_name, "rq", "Request", req_topic, sizeof(req_topic)) ||
        !service_topic_name(service_name, "rr", "Reply",   rep_topic, sizeof(rep_topic))) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }

    auto *state = new (std::nothrow) ServerState();
    if (state == nullptr) return NROS_RMW_RET_BAD_ALLOC;
    state->req_desc = req_desc;
    state->rep_desc = rep_desc;

    state->request_topic =
        dds_create_topic(pp, req_desc, req_topic, nullptr, nullptr);
    state->reply_topic =
        dds_create_topic(pp, rep_desc, rep_topic, nullptr, nullptr);
    if (state->request_topic < 0 || state->reply_topic < 0) {
        if (state->request_topic > 0) (void) dds_delete(state->request_topic);
        if (state->reply_topic   > 0) (void) dds_delete(state->reply_topic);
        delete state;
        return NROS_RMW_RET_ERROR;
    }

    // Phase 117.X.5: align with `rmw_qos_profile_services_default`
    // (RELIABLE + VOLATILE + KEEP_LAST(10)). Without this Cyclone
    // defaults to KEEP_LAST(1) which surprises stock RMW clients.
    nros_rmw_qos_t svc_qos = NROS_RMW_QOS_PROFILE_SERVICES_DEFAULT;
    dds_qos_t *dq_reader = make_dds_qos(&svc_qos);
    dds_qos_t *dq_writer = make_dds_qos(&svc_qos);
    state->reader = dds_create_reader(pp, state->request_topic, dq_reader, nullptr);
    state->writer = dds_create_writer(pp, state->reply_topic,   dq_writer, nullptr);
    if (dq_reader != nullptr) dds_delete_qos(dq_reader);
    if (dq_writer != nullptr) dds_delete_qos(dq_writer);
    if (state->reader < 0 || state->writer < 0) {
        if (state->reader > 0) (void) dds_delete(state->reader);
        if (state->writer > 0) (void) dds_delete(state->writer);
        (void) dds_delete(state->request_topic);
        (void) dds_delete(state->reply_topic);
        delete state;
        return NROS_RMW_RET_ERROR;
    }

    state->req_st = new (std::nothrow) SertypeMin(req_desc);
    state->rep_st = new (std::nothrow) SertypeMin(rep_desc);
    if (state->req_st == nullptr || state->rep_st == nullptr) {
        delete state->req_st;
        delete state->rep_st;
        (void) dds_delete(state->reader);
        (void) dds_delete(state->writer);
        (void) dds_delete(state->request_topic);
        (void) dds_delete(state->reply_topic);
        delete state;
        return NROS_RMW_RET_BAD_ALLOC;
    }

    out->backend_data = state;
    return NROS_RMW_RET_OK;
}

void service_server_destroy(nros_rmw_service_server_t *server) {
    if (server == nullptr || server->backend_data == nullptr) return;
    auto *state = static_cast<ServerState *>(server->backend_data);
    if (state->reader > 0) (void) dds_delete(state->reader);
    if (state->writer > 0) (void) dds_delete(state->writer);
    if (state->request_topic > 0) (void) dds_delete(state->request_topic);
    if (state->reply_topic   > 0) (void) dds_delete(state->reply_topic);
    delete state->req_st;
    delete state->rep_st;
    delete state;
    server->backend_data = nullptr;
}

int32_t service_try_recv_request(nros_rmw_service_server_t *server,
                                 uint8_t *buf, size_t buf_len,
                                 int64_t *seq_out) {
    if (server == nullptr || server->backend_data == nullptr || buf == nullptr) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    auto *state = static_cast<ServerState *>(server->backend_data);

    uint8_t wire[kWireScratch];
    int32_t wire_len = take_typed_wire(state->reader, state->req_st,
                                       wire, sizeof(wire));
    if (wire_len <= 0) return wire_len;

    RequestId id{};
    int32_t user_len = split_wire_header(wire, static_cast<size_t>(wire_len),
                                         &id, buf, buf_len);
    if (user_len < 0) return user_len;

    // Allocate a slot to remember the (writer_guid, seq) pair so the
    // matching `service_send_reply` can echo it back.
    for (std::size_t i = 0; i < kRequestSlots; ++i) {
        if (!state->slots[i].in_use) {
            state->slots[i].id     = id;
            state->slots[i].in_use = true;
            if (seq_out != nullptr) *seq_out = static_cast<int64_t>(i);
            return user_len;
        }
    }
    return NROS_RMW_RET_WOULD_BLOCK;
}

int32_t service_has_request(nros_rmw_service_server_t *server) {
    if (server == nullptr || server->backend_data == nullptr) return 0;
    auto *state = static_cast<ServerState *>(server->backend_data);
    uint32_t status = 0;
    if (dds_get_status_changes(state->reader, &status) != DDS_RETCODE_OK) return 0;
    return (status & DDS_DATA_AVAILABLE_STATUS) ? 1 : 0;
}

nros_rmw_ret_t service_send_reply(nros_rmw_service_server_t *server,
                                  int64_t seq, const uint8_t *data,
                                  size_t len) {
    if (server == nullptr || server->backend_data == nullptr ||
        data == nullptr || seq < 0 ||
        static_cast<std::size_t>(seq) >= kRequestSlots) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    auto *state = static_cast<ServerState *>(server->backend_data);
    auto &slot = state->slots[seq];
    if (!slot.in_use) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    uint8_t wire[kWireScratch];
    int32_t wire_len = build_wire_with_header(data, len, slot.id,
                                              wire, sizeof(wire));
    nros_rmw_ret_t r;
    if (wire_len < 0) {
        r = static_cast<nros_rmw_ret_t>(wire_len);
    } else {
        r = write_typed(state->writer, state->rep_desc, state->rep_st,
                        wire, static_cast<size_t>(wire_len));
    }
    slot.in_use = false;
    return r;
}

// =========================================================================
// Service client
// =========================================================================

nros_rmw_ret_t service_client_create(nros_rmw_session_t *session,
                                     const char *service_name,
                                     const char *type_name,
                                     const char * /*type_hash*/,
                                     uint32_t /*domain_id*/,
                                     nros_rmw_service_client_t *out) {
    if (out == nullptr || session == nullptr || service_name == nullptr ||
        type_name == nullptr) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    out->backend_data = nullptr;

    dds_entity_t pp = session_participant(session);
    if (pp == 0) return NROS_RMW_RET_ERROR;

    const dds_topic_descriptor_t *req_desc = nullptr;
    const dds_topic_descriptor_t *rep_desc = nullptr;
    if (!descriptors_for_service(type_name, &req_desc, &rep_desc)) {
        return NROS_RMW_RET_UNSUPPORTED;
    }

    char req_topic[kMaxTopicName];
    char rep_topic[kMaxTopicName];
    if (!service_topic_name(service_name, "rq", "Request", req_topic, sizeof(req_topic)) ||
        !service_topic_name(service_name, "rr", "Reply",   rep_topic, sizeof(rep_topic))) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }

    auto *state = new (std::nothrow) ClientState();
    if (state == nullptr) return NROS_RMW_RET_BAD_ALLOC;
    state->req_desc = req_desc;
    state->rep_desc = rep_desc;
    state->next_seq.store(0, std::memory_order_relaxed);

    state->request_topic =
        dds_create_topic(pp, req_desc, req_topic, nullptr, nullptr);
    state->reply_topic =
        dds_create_topic(pp, rep_desc, rep_topic, nullptr, nullptr);
    if (state->request_topic < 0 || state->reply_topic < 0) {
        if (state->request_topic > 0) (void) dds_delete(state->request_topic);
        if (state->reply_topic   > 0) (void) dds_delete(state->reply_topic);
        delete state;
        return NROS_RMW_RET_ERROR;
    }

    // Phase 117.X.5: services QoS profile alignment.
    nros_rmw_qos_t svc_qos = NROS_RMW_QOS_PROFILE_SERVICES_DEFAULT;
    dds_qos_t *dq_writer = make_dds_qos(&svc_qos);
    dds_qos_t *dq_reader = make_dds_qos(&svc_qos);
    state->writer = dds_create_writer(pp, state->request_topic, dq_writer, nullptr);
    state->reader = dds_create_reader(pp, state->reply_topic,   dq_reader, nullptr);
    if (dq_writer != nullptr) dds_delete_qos(dq_writer);
    if (dq_reader != nullptr) dds_delete_qos(dq_reader);
    if (state->writer < 0 || state->reader < 0) {
        if (state->writer > 0) (void) dds_delete(state->writer);
        if (state->reader > 0) (void) dds_delete(state->reader);
        (void) dds_delete(state->request_topic);
        (void) dds_delete(state->reply_topic);
        delete state;
        return NROS_RMW_RET_ERROR;
    }

    state->req_st = new (std::nothrow) SertypeMin(req_desc);
    state->rep_st = new (std::nothrow) SertypeMin(rep_desc);
    if (state->req_st == nullptr || state->rep_st == nullptr) {
        delete state->req_st;
        delete state->rep_st;
        (void) dds_delete(state->writer);
        (void) dds_delete(state->reader);
        (void) dds_delete(state->request_topic);
        (void) dds_delete(state->reply_topic);
        delete state;
        return NROS_RMW_RET_BAD_ALLOC;
    }

    // Use the writer's RTPS GUID as the client identity. Falls back
    // to a random 128-bit value if dds_get_guid fails.
    writer_guid_bytes(state->writer, state->my_guid);
    bool guid_zero = true;
    for (int i = 0; i < 16; ++i) {
        if (state->my_guid[i] != 0) { guid_zero = false; break; }
    }
    if (guid_zero) {
        fill_random_guid(state->my_guid);
    }

    out->backend_data = state;
    return NROS_RMW_RET_OK;
}

void service_client_destroy(nros_rmw_service_client_t *client) {
    if (client == nullptr || client->backend_data == nullptr) return;
    auto *state = static_cast<ClientState *>(client->backend_data);
    if (state->writer > 0) (void) dds_delete(state->writer);
    if (state->reader > 0) (void) dds_delete(state->reader);
    if (state->request_topic > 0) (void) dds_delete(state->request_topic);
    if (state->reply_topic   > 0) (void) dds_delete(state->reply_topic);
    delete state->req_st;
    delete state->rep_st;
    delete state;
    client->backend_data = nullptr;
}

int32_t service_call_raw(nros_rmw_service_client_t *client,
                         const uint8_t *request, size_t req_len,
                         uint8_t *reply_buf, size_t reply_buf_len) {
    if (client == nullptr || client->backend_data == nullptr ||
        request == nullptr || reply_buf == nullptr || req_len < 4) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    auto *state = static_cast<ClientState *>(client->backend_data);

    RequestId my_id{};
    std::memcpy(my_id.guid, state->my_guid, 16);
    my_id.seq = state->next_seq.fetch_add(1, std::memory_order_relaxed);

    uint8_t wire_req[kWireScratch];
    int32_t wire_len = build_wire_with_header(request, req_len, my_id,
                                              wire_req, sizeof(wire_req));
    if (wire_len < 0) return wire_len;
    nros_rmw_ret_t pr = write_typed(state->writer, state->req_desc,
                                    state->req_st, wire_req,
                                    static_cast<size_t>(wire_len));
    if (pr != NROS_RMW_RET_OK) return pr;

    // 5 s reply timeout — long enough to absorb cross-participant
    // SEDP propagation jitter on POSIX while still bounded so a
    // misconfigured peer doesn't hang the caller forever. nano-ros
    // applications that need a tighter deadline can wrap call_raw
    // with their own watchdog.
    const auto deadline = std::chrono::steady_clock::now() +
                          std::chrono::seconds(5);
    while (std::chrono::steady_clock::now() < deadline) {
        uint32_t status = 0;
        if (dds_get_status_changes(state->reader, &status) == DDS_RETCODE_OK
            && (status & DDS_DATA_AVAILABLE_STATUS)) {
            uint8_t wire_rep[kWireScratch];
            int32_t wlen = take_typed_wire(state->reader, state->rep_st,
                                            wire_rep, sizeof(wire_rep));
            if (wlen == NROS_RMW_RET_NO_DATA) {
                std::this_thread::sleep_for(std::chrono::milliseconds(2));
                continue;
            }
            if (wlen < 0) return wlen;

            RequestId got_id{};
            int32_t user_len =
                split_wire_header(wire_rep, static_cast<size_t>(wlen),
                                  &got_id, reply_buf, reply_buf_len);
            if (user_len < 0) return user_len;
            if (got_id.seq == my_id.seq &&
                std::memcmp(got_id.guid, my_id.guid, 16) == 0) {
                return user_len;
            }
            // Reply for a different in-flight call from the same
            // client (impossible in single-shot tests, defensive
            // here). Drop and keep polling.
            continue;
        }
        std::this_thread::sleep_for(std::chrono::milliseconds(5));
    }
    return NROS_RMW_RET_TIMEOUT;
}

} // namespace nros_rmw_cyclonedds
