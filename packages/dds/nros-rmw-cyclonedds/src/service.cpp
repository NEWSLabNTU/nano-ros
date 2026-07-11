// Services — Phase 117.X.3 + 117.12.B (cdds_request_header_t-shaped wire).
//
// Service traffic uses per-service typed Request / Response topics
// matching stock `rmw_cyclonedds_cpp`'s shape. Each typed struct
// carries the request-id correlation header inline as the first two
// fields:
//
//     struct <Pkg>::srv::dds_::<Svc>_Request_ {
//         unsigned long long rmw_writer_guid;   // lower 8 bytes of RTPS GUID
//         long long          rmw_sequence_number;  // monotonic per-client
//         /* user fields ... */
//     };
//
// This is bit-equivalent to upstream's `cdds_request_header_t
// request_header;` (see `rmw_cyclonedds_cpp/src/serdata.hpp:73-77`,
// `{uint64_t guid; int64_t seq;}` — 16 bytes) followed by user
// fields. Stock `rclcpp` clients/servers therefore match by
// `(topic_name, type_name)` and exchange byte-equal CDR.
//
// The 117.X.1 codegen helper injects the two header fields at IDL
// time when processing `.srv` inputs; consumers call
// `nros_rmw_cyclonedds_generate_from_msg(... INTERFACES <Foo.srv>)`
// and get the right typed struct without further manual work.
//
// Wire data path (per-call):
//
//   service_call_raw   user-CDR bytes
//                       → build wire CDR `[encap][16-byte-header]
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

#include <cstdint>
#include <cstdlib>
#include <cstring>
#include <new>
#if !defined(NROS_PLATFORM_FREERTOS) && !defined(NROS_PLATFORM_THREADX)
#include <atomic>
#include <random>
#endif

#if defined(NROS_PLATFORM_FREERTOS) || defined(NROS_PLATFORM_THREADX)
namespace std {
enum memory_order {
    memory_order_relaxed,
    memory_order_acquire,
    memory_order_release,
};
} // namespace std
#endif

namespace nros_rmw_cyclonedds {

namespace {

// Phase 192.4 — read a uint64 from the environment, falling back to a baked
// default. getenv returns null on RTOS targets with no environment, so the
// defaults apply there. No function-local statics (would need __cxa_guard on
// embedded); callers read once into a local where it matters.
uint64_t env_u64(const char* name, uint64_t fallback) {
    const char* v = std::getenv(name);
    if (v == nullptr || v[0] == '\0') return fallback;
    char* end = nullptr;
    // Phase 203 — use the global-namespace C name. picolibc's `<cstdlib>` on
    // the riscv64/threadx cross does **not** alias every C function into
    // `std::` (only a subset — `getenv` is in, `strtoull` is not), so a
    // `std::strtoull` reference fails to compile on the embedded build. The
    // unqualified name resolves to the C declaration via `<stdlib.h>`.
    unsigned long long parsed = ::strtoull(v, &end, 10);
    return (end != v && parsed > 0) ? static_cast<uint64_t>(parsed) : fallback;
}

// Default Cyclone service request/reply match timing (ms). Tunable at runtime
// via NROS_CYCLONE_MATCH_{TIMEOUT,POLL}_MS without recompiling.
constexpr uint64_t kDefaultMatchTimeoutMs = 5000;
constexpr uint64_t kDefaultMatchPollMs = 5;

#if defined(NROS_PLATFORM_FREERTOS) || defined(NROS_PLATFORM_THREADX)
struct ServiceAtomicI64 {
    int64_t value;

    explicit ServiceAtomicI64(int64_t initial = 0) : value(initial) {}

    int64_t load(std::memory_order) const { return value; }
    void store(int64_t next, std::memory_order) { value = next; }
    int64_t fetch_add(int64_t delta, std::memory_order) {
        int64_t previous = value;
        value += delta;
        return previous;
    }
};
#else
using ServiceAtomicI64 = std::atomic<int64_t>;
#endif

constexpr std::size_t kRequestSlots = 32;
constexpr std::size_t kMaxTopicName = 256;
constexpr uint8_t kCdrLeHeader[4] = {0x00, 0x01, 0x00, 0x00};

// Wire-framing field widths. These are intentionally *separate* consts
// even where they share the value 4 — conflating the CDR encapsulation
// header with a CDR length prefix or the GetResult status field is the
// exact off-by-N class this section (192.2) exists to kill.
constexpr std::size_t kEncapLen = sizeof(kCdrLeHeader);      // 4-byte CDR encapsulation header
constexpr std::size_t kGuidBytes = 8;                        // request_header GUID (LE u64)
constexpr std::size_t kSeqBytes = 8;                         // request_header sequence (LE u64)
constexpr std::size_t kHeaderBytes = kGuidBytes + kSeqBytes; // 16-byte inlined request_header
constexpr std::size_t kCdrLenPrefix = sizeof(uint32_t); // 4-byte CDR sequence/array length field
constexpr std::size_t kStatusFieldLen = 4;              // GetResult status int8 + 3 pad
constexpr std::size_t kGoalUuidLen = 16;                // action goal_id UUID (uuid[16])

// Round @p pos up to the 4-byte CDR member alignment.
inline std::size_t cdr_align4(std::size_t pos) {
    return (pos + 3u) & ~std::size_t{3u};
}
// Per-call scratch ceiling. Tunable via env if a future user needs
// it; 64 KiB covers ROS 2's default service payload size budget.
constexpr std::size_t kWireScratch = 65536;

struct RequestId {
    uint64_t guid;
    int64_t seq;
};

struct RequestSlot {
    RequestId id{};
    bool in_use{false};
};

struct ServerState {
    dds_entity_t request_topic{0};
    dds_entity_t reply_topic{0};
    dds_entity_t reader{0};
    dds_entity_t writer{0};
    const dds_topic_descriptor_t* req_desc{nullptr};
    const dds_topic_descriptor_t* rep_desc{nullptr};
    SertypeMin* req_st{nullptr};
    SertypeMin* rep_st{nullptr};
    RequestSlot slots[kRequestSlots];
};

struct ClientState {
    dds_entity_t request_topic{0};
    dds_entity_t reply_topic{0};
    dds_entity_t writer{0};
    dds_entity_t reader{0};
    const dds_topic_descriptor_t* req_desc{nullptr};
    const dds_topic_descriptor_t* rep_desc{nullptr};
    SertypeMin* req_st{nullptr};
    SertypeMin* rep_st{nullptr};
    uint64_t my_guid{0};
    ServiceAtomicI64 next_seq{0};
    // Phase 130.8 — non-blocking send/recv split. `pending_seq`
    // tracks the in-flight request issued via
    // `service_send_request_raw`; `pending_request` holds the wire CDR
    // until Cyclone reports the request writer matched a server reader.
    // Service QoS is VOLATILE, so writing before the match can silently
    // drop the request.
    ServiceAtomicI64 pending_seq{-1};
    uint8_t pending_request[kWireScratch]{};
    std::size_t pending_request_len{0};
};

bool service_topic_name(const char* service_name, const char* prefix, const char* suffix, char* out,
                        std::size_t out_cap) {
    if (service_name == nullptr || prefix == nullptr || suffix == nullptr || out == nullptr) {
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
//
// Phase 11W.12: the nros codegen emits SERVICE_NAME with a trailing
// underscore (`<pkg>::srv::dds_::<Svc>_`, mirroring the message
// `<Type>_` convention), but the registered DDS request/response types
// — and stock `rmw_cyclonedds_cpp` — are `<pkg>::srv::dds_::<Svc>_Request_`
// / `_Response_` with a *single* underscore before the suffix. Strip one
// trailing underscore from the base so both the no-trailing-`_` form
// (used by the backend's own roundtrip tests) and the codegen's
// trailing-`_` form resolve to the same registered descriptor.
bool service_type_name(const char* base, const char* suffix, char* out, std::size_t out_cap) {
    std::size_t blen = std::strlen(base);
    if (blen > 0 && base[blen - 1] == '_') {
        --blen;
    }
    std::size_t slen = std::strlen(suffix);
    if (blen + slen + 1 > out_cap) return false;
    std::memcpy(out, base, blen);
    std::memcpy(out + blen, suffix, slen);
    out[blen + slen] = '\0';
    return true;
}

uint32_t cdr_xcdr_version(const uint8_t* bytes) {
    uint8_t lo = bytes[1];
    if (lo == 0x06 || lo == 0x07 || lo == 0x0a || lo == 0x0b) return 2;
    return 1;
}

// Pull the lower 8 bytes of the 16-byte RTPS GUID from a Cyclone
// writer. Upstream `rmw_cyclonedds_cpp` stashes a 64-bit guid in
// `cdds_request_header_t` rather than the full 128-bit RTPS GUID, so
// we follow the same convention to stay wire-compatible. Returns 0 if
// dds_get_guid fails — caller must fall back to a random value.
uint64_t writer_guid_lo64(dds_entity_t writer) {
    dds_guid_t g{};
    if (dds_get_guid(writer, &g) != DDS_RETCODE_OK) return 0;
    uint64_t v = 0;
    std::memcpy(&v, g.v, 8);
    return v;
}

// Encode int64 little-endian into 8 bytes.
inline void put_le64(uint8_t* out, int64_t v) {
    for (int i = 0; i < 8; ++i) {
        out[i] = static_cast<uint8_t>((v >> (i * 8)) & 0xff);
    }
}
inline int64_t get_le64(const uint8_t* in) {
    int64_t v = 0;
    for (int i = 0; i < 8; ++i) {
        v |= static_cast<int64_t>(in[i]) << (i * 8);
    }
    return v;
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

struct DdsSequenceInt32 {
    uint32_t _maximum;
    uint32_t _length;
    int32_t* _buffer;
    bool _release;
};

bool strip_nested_cdr_at(const uint8_t* in, size_t in_len, size_t nested_off, uint8_t* out,
                         size_t out_cap, size_t* out_len) {
    if (in == nullptr || out == nullptr || out_len == nullptr ||
        nested_off + sizeof(kCdrLeHeader) > in_len || in_len - sizeof(kCdrLeHeader) > out_cap) {
        return false;
    }
    if (std::memcmp(in + nested_off, kCdrLeHeader, sizeof(kCdrLeHeader)) != 0) {
        return false;
    }
    std::memcpy(out, in, nested_off);
    std::memcpy(out + nested_off, in + nested_off + sizeof(kCdrLeHeader),
                in_len - nested_off - sizeof(kCdrLeHeader));
    *out_len = in_len - sizeof(kCdrLeHeader);
    return true;
}

bool strip_goal_id_len_at(const uint8_t* in, size_t in_len, size_t len_off, uint8_t* out,
                          size_t out_cap, size_t* out_len) {
    if (in == nullptr || out == nullptr || out_len == nullptr || len_off + kCdrLenPrefix > in_len ||
        in_len - kCdrLenPrefix > out_cap) {
        return false;
    }
    if (in[len_off] != kGoalUuidLen || in[len_off + 1] != 0 || in[len_off + 2] != 0 ||
        in[len_off + 3] != 0) {
        return false;
    }
    std::memcpy(out, in, len_off);
    std::memcpy(out + len_off, in + len_off + kCdrLenPrefix, in_len - len_off - kCdrLenPrefix);
    *out_len = in_len - kCdrLenPrefix;
    return true;
}

// Phase 171.0.b: Cyclone 0.10.5's public `dds_stream_read_sample`
// helper crashes on the generated Fibonacci_GetResult_Response_ dynamic
// sequence path when fed the CDR assembled by nros. Build the generated
// C layout directly for this smoke-test action until the generic raw-CDR
// writer path is replaced.
nros_rmw_ret_t write_fibonacci_get_result_response(dds_entity_t writer,
                                                   const dds_topic_descriptor_t* desc,
                                                   const uint8_t* wire_cdr, size_t wire_len) {
    if (desc == nullptr || desc->m_ops == nullptr || wire_cdr == nullptr ||
        wire_len < kEncapLen + kHeaderBytes + kStatusFieldLen + kCdrLenPrefix) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }

    const uint32_t* ops = desc->m_ops;
    const uint32_t guid_off = ops[1];
    const uint32_t seq_off = ops[3];
    const uint32_t status_off = ops[5];
    const uint32_t result_off = ops[7];

    size_t pos = kEncapLen + kHeaderBytes;
    int8_t status = static_cast<int8_t>(wire_cdr[pos]);
    pos += 1;
    pos = cdr_align4(pos);
    if (pos + sizeof(kCdrLeHeader) <= wire_len &&
        std::memcmp(wire_cdr + pos, kCdrLeHeader, sizeof(kCdrLeHeader)) == 0) {
        pos += sizeof(kCdrLeHeader);
    }
    if (pos + kCdrLenPrefix > wire_len) return NROS_RMW_RET_INVALID_ARGUMENT;

    uint32_t count = 0;
    std::memcpy(&count, wire_cdr + pos, sizeof(count));
    pos += kCdrLenPrefix;
    if (count > (wire_len - pos) / sizeof(int32_t)) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }

    auto* sample = static_cast<uint8_t*>(std::calloc(1, desc->m_size));
    if (sample == nullptr) return NROS_RMW_RET_BAD_ALLOC;
    std::memcpy(sample + guid_off, wire_cdr + kEncapLen, kGuidBytes);
    std::memcpy(sample + seq_off, wire_cdr + kEncapLen + kGuidBytes, kSeqBytes);
    std::memcpy(sample + status_off, &status, sizeof(status));

    auto* sequence = reinterpret_cast<DdsSequenceInt32*>(sample + result_off);
    sequence->_maximum = count;
    sequence->_length = count;
    sequence->_release = true;
    sequence->_buffer = static_cast<int32_t*>(dds_alloc(count * sizeof(int32_t)));
    if (count > 0 && sequence->_buffer == nullptr) {
        std::free(sample);
        return NROS_RMW_RET_BAD_ALLOC;
    }
    std::memcpy(sequence->_buffer, wire_cdr + pos, count * sizeof(int32_t));

    dds_return_t r = dds_write(writer, sample);
    dds_stream_free_sample(sample, desc->m_ops);
    std::free(sample);
    return (r == DDS_RETCODE_OK) ? NROS_RMW_RET_OK : NROS_RMW_RET_ERROR;
}

int32_t take_fibonacci_get_result_response_wire(const void* sample,
                                                const dds_topic_descriptor_t* desc,
                                                uint8_t* out_buf, size_t out_cap) {
    if (sample == nullptr || desc == nullptr || desc->m_ops == nullptr || out_buf == nullptr) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    const uint32_t* ops = desc->m_ops;
    const auto* bytes = static_cast<const uint8_t*>(sample);
    const uint32_t guid_off = ops[1];
    const uint32_t seq_off = ops[3];
    const uint32_t status_off = ops[5];
    const uint32_t result_off = ops[7];
    const auto* sequence = reinterpret_cast<const DdsSequenceInt32*>(bytes + result_off);
    const uint32_t count = sequence->_length;
    // Wire layout: [encap][guid][seq][status+pad][count][int32 data...]
    constexpr size_t status_off_wire = kEncapLen + kHeaderBytes;
    constexpr size_t count_off_wire = status_off_wire + kStatusFieldLen;
    constexpr size_t data_off_wire = count_off_wire + kCdrLenPrefix;
    const size_t total = data_off_wire + count * sizeof(int32_t);
    if (out_cap < total) return NROS_RMW_RET_BUFFER_TOO_SMALL;

    std::memcpy(out_buf, kCdrLeHeader, sizeof(kCdrLeHeader));
    std::memcpy(out_buf + kEncapLen, bytes + guid_off, kGuidBytes);
    std::memcpy(out_buf + kEncapLen + kGuidBytes, bytes + seq_off, kSeqBytes);
    out_buf[status_off_wire] = bytes[status_off];
    out_buf[status_off_wire + 1] = 0;
    out_buf[status_off_wire + 2] = 0;
    out_buf[status_off_wire + 3] = 0;
    std::memcpy(out_buf + count_off_wire, &count, sizeof(count));
    if (count > 0) {
        if (sequence->_buffer == nullptr) return NROS_RMW_RET_INVALID_ARGUMENT;
        std::memcpy(out_buf + data_off_wire, sequence->_buffer, count * sizeof(int32_t));
    }
    return static_cast<int32_t>(total);
}

// (Issue #68) `insert_goal_id_len_at` was removed: it re-inserted a pre-233.6
// `uint32(16)` goal_id length prefix on the service-request receive path, which a
// real `rcl_action` peer never sends and the post-233.6 action core no longer
// reads. See `split_wire_header`.

// Construct the wire CDR for a typed struct that has the 16-byte
// request_header inlined at offset 0. Inputs:
//   user_bytes    runtime-supplied CDR with 4-byte encap + user fields
//                 (no header).
//   id            request_id to inject.
// Outputs:
//   wire_cdr      buffer of size at least len(user_bytes) + 16.
// Returns total wire byte count, or negative on error.
int32_t build_wire_with_header(const uint8_t* user_bytes, size_t user_len, const RequestId& id,
                               uint8_t* wire_cdr, size_t wire_cap) {
    if (user_len < kEncapLen) return NROS_RMW_RET_INVALID_ARGUMENT;
    size_t total = user_len + kHeaderBytes;
    if (total > wire_cap) return NROS_RMW_RET_BUFFER_TOO_SMALL;
    // Encap copied verbatim.
    std::memcpy(wire_cdr, user_bytes, kEncapLen);
    // Little-endian guid.
    put_le64(wire_cdr + kEncapLen, static_cast<int64_t>(id.guid));
    // Little-endian seq.
    put_le64(wire_cdr + kEncapLen + kGuidBytes, id.seq);
    // User payload after encap.
    std::memcpy(wire_cdr + kEncapLen + kHeaderBytes, user_bytes + kEncapLen, user_len - kEncapLen);
    return static_cast<int32_t>(total);
}

// Inverse: parse the wire CDR (with leading header) into
// `(out_id, user_bytes_with_encap)`. The wire's 4-byte encap is
// preserved in `user_out` so the runtime sees a normal CDR-shaped
// payload (encap + user fields).
//
// Returns user-payload length (incl. 4-byte encap) on success, or
// negative error.
int32_t split_wire_header(const uint8_t* wire_cdr, size_t wire_len,
                          const dds_topic_descriptor_t* payload_desc, RequestId* out_id,
                          uint8_t* user_out, size_t user_cap) {
    if (wire_len < kEncapLen + kHeaderBytes) return NROS_RMW_RET_INVALID_ARGUMENT;
    if (out_id != nullptr) {
        out_id->guid = static_cast<uint64_t>(get_le64(wire_cdr + kEncapLen));
        out_id->seq = get_le64(wire_cdr + kEncapLen + kGuidBytes);
    }
    size_t user_len = wire_len - kHeaderBytes; // (encap stays + user fields)
    if (user_len > user_cap) return NROS_RMW_RET_BUFFER_TOO_SMALL;
    // Encap.
    std::memcpy(user_out, wire_cdr, kEncapLen);
    // User fields.
    std::memcpy(user_out + kEncapLen, wire_cdr + kEncapLen + kHeaderBytes, user_len - kEncapLen);
    // Phase 233.6 completion (issue #68) — do NOT re-insert a goal_id `uint32(16)`
    // length prefix here. ROS 2 `rcl_action` (over rmw_cyclonedds_cpp) serialises
    // the SendGoal/GetResult request's `goal_id` as a fixed `uint8[16]` array — no
    // length prefix — and the nano action core now reads it that way too
    // (`action_core::read_goal_id`, post-233.6). The old `insert_goal_id_len_at`
    // call mirrored the pre-233.6 prefixed framing; 233.6 dropped the subscriber.cpp
    // insert mirror but missed THIS service-request site, so a real rcl_action client's
    // goal arrived with a spurious `10 00 00 00` before the UUID → `order` read 4 bytes
    // early → "Goal was rejected" (order garbage). The wire payload already matches the
    // bare-array form after the request-header strip above; pass it through unchanged.
    (void)payload_desc;
    return static_cast<int32_t>(user_len);
}

// Run dds_stream_read_sample on @p wire_cdr, then dds_write. Caller
// owns @p wire_cdr.
nros_rmw_ret_t write_typed(dds_entity_t writer, const dds_topic_descriptor_t* desc,
                           const SertypeMin* st, const uint8_t* wire_cdr, size_t wire_len) {
    if (writer <= 0 || desc == nullptr || st == nullptr || wire_cdr == nullptr ||
        wire_len < kEncapLen) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    uint8_t adjusted[kWireScratch];
    const uint8_t* read_cdr = wire_cdr;
    size_t read_len = wire_len;
    size_t adjusted_len = 0;
    if (type_ends_with(desc, "_SendGoal_Request_") || type_ends_with(desc, "_GetResult_Request_")) {
        if (strip_goal_id_len_at(wire_cdr, wire_len, kEncapLen + kHeaderBytes, adjusted,
                                 sizeof(adjusted), &adjusted_len)) {
            read_cdr = adjusted;
            read_len = adjusted_len;
        }
    } else if (type_ends_with(desc, "_GetResult_Response_")) {
        if (strip_nested_cdr_at(wire_cdr, wire_len, kEncapLen + kHeaderBytes + kStatusFieldLen,
                                adjusted, sizeof(adjusted), &adjusted_len)) {
            read_cdr = adjusted;
            read_len = adjusted_len;
        }
    }
    if (type_ends_with(desc, "_SendGoal_Request_")) {
        if (strip_nested_cdr_at(read_cdr, read_len, kEncapLen + kHeaderBytes + kGoalUuidLen,
                                adjusted, sizeof(adjusted), &adjusted_len)) {
            read_cdr = adjusted;
            read_len = adjusted_len;
        }
    }

    if (type_ends_with(desc, "_SendGoal_Request_") || type_ends_with(desc, "_SendGoal_Response_") ||
        type_ends_with(desc, "_GetResult_Request_")) {
        void* sample = std::calloc(1, desc->m_size);
        if (sample == nullptr) return NROS_RMW_RET_BAD_ALLOC;
        size_t payload_len = read_len - kEncapLen;
        if (payload_len > desc->m_size) payload_len = desc->m_size;
        std::memcpy(sample, read_cdr + kEncapLen, payload_len);
        dds_return_t r = dds_write(writer, sample);
        std::free(sample);
        return (r == DDS_RETCODE_OK) ? NROS_RMW_RET_OK : NROS_RMW_RET_ERROR;
    }
    if (type_contains(desc, "Fibonacci_GetResult_Response_")) {
        return write_fibonacci_get_result_response(writer, desc, read_cdr, read_len);
    }

    uint32_t xcdrv = cdr_xcdr_version(read_cdr);
    void* sample = std::calloc(1, desc->m_size);
    if (sample == nullptr) return NROS_RMW_RET_BAD_ALLOC;

    dds_istream_t is;
    dds_istream_init(&is, static_cast<uint32_t>(read_len - kEncapLen), read_cdr + kEncapLen, xcdrv);
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
int32_t take_typed_wire(dds_entity_t reader, const SertypeMin* st, uint8_t* out_buf,
                        size_t out_cap) {
    void* samples[1] = {nullptr};
    dds_sample_info_t si[1];
    dds_return_t taken = dds_take(reader, samples, si, 1, 1);
    if (taken < 0) return NROS_RMW_RET_ERROR;
    if (taken == 0 || !si[0].valid_data) {
        if (taken > 0) (void)dds_return_loan(reader, samples, taken);
        return NROS_RMW_RET_NO_DATA;
    }

    if (type_contains(st->descriptor(), "Fibonacci_GetResult_Response_")) {
        int32_t total =
            take_fibonacci_get_result_response_wire(samples[0], st->descriptor(), out_buf, out_cap);
        (void)dds_return_loan(reader, samples, taken);
        return total;
    }

    dds_ostream_t os;
    dds_ostream_init(&os, 0, 1 /*xcdr1*/);
    bool ok = dds_stream_write_sample(&os, samples[0], st->as_sertype());
    if (!ok && (type_ends_with(st->descriptor(), "_SendGoal_Request_") ||
                type_ends_with(st->descriptor(), "_SendGoal_Response_") ||
                type_ends_with(st->descriptor(), "_GetResult_Request_"))) {
        uint32_t total = kEncapLen + st->descriptor()->m_size;
        if (out_cap < total) {
            (void)dds_return_loan(reader, samples, taken);
            dds_ostream_fini(&os);
            return NROS_RMW_RET_BUFFER_TOO_SMALL;
        }
#if defined(__BYTE_ORDER__) && (__BYTE_ORDER__ == __ORDER_LITTLE_ENDIAN__)
        out_buf[0] = 0x00;
        out_buf[1] = 0x01;
#else
        out_buf[0] = 0x00;
        out_buf[1] = 0x00;
#endif
        out_buf[2] = 0;
        out_buf[3] = 0;
        std::memcpy(out_buf + kEncapLen, samples[0], st->descriptor()->m_size);
        (void)dds_return_loan(reader, samples, taken);
        dds_ostream_fini(&os);
        return static_cast<int32_t>(total);
    }
    (void)dds_return_loan(reader, samples, taken);
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
    uint32_t total = paylen + kEncapLen;
    if (out_cap < total) {
        dds_ostream_fini(&os);
        return NROS_RMW_RET_BUFFER_TOO_SMALL;
    }
    out_buf[0] = kEncId[0];
    out_buf[1] = kEncId[1];
    out_buf[2] = 0;
    out_buf[3] = 0;
    std::memcpy(out_buf + kEncapLen, os.m_buffer, paylen);
    dds_ostream_fini(&os);
    return static_cast<int32_t>(total);
}

// Action sub-services reuse one service-create path but each carries a
// distinct DDS type. The action layer (`executor/action.rs`) passes the
// bare action type `<pkg>::action::dds_::<A>_` for both the send_goal
// and get_result services, distinguishing them only by the service
// name suffix `<action>/_action/{send_goal,get_result}` (see
// `ActionInfo::{send_goal,get_result}_key`). Map that suffix to the
// rosidl-synthesised wrapper base — `<A>_SendGoal_` / `<A>_GetResult_`
// — so the `_Request_`/`_Response_` lookup resolves the right
// descriptor. Non-action services pass through unchanged. This keeps
// the backend-agnostic contract (and the zenoh keyexpr) untouched.
bool action_effective_base(const char* service_name, const char* type_name, char* out,
                           std::size_t out_cap) {
    const char* infix = nullptr;
    std::size_t nlen = std::strlen(service_name);
    auto ends_with = [&](const char* suf) {
        std::size_t slen = std::strlen(suf);
        return nlen >= slen && std::strcmp(service_name + nlen - slen, suf) == 0;
    };
    if (ends_with("/_action/send_goal")) {
        infix = "_SendGoal_";
    } else if (ends_with("/_action/get_result")) {
        infix = "_GetResult_";
    }
    if (infix == nullptr) {
        std::size_t blen = std::strlen(type_name);
        if (blen + 1 > out_cap) return false;
        std::memcpy(out, type_name, blen + 1);
        return true;
    }
    // Strip the single trailing `_` from the action base, append the
    // wrapper infix (which itself ends in `_`, the marker the later
    // `service_type_name` strips before adding `_Request_`).
    std::size_t blen = std::strlen(type_name);
    if (blen > 0 && type_name[blen - 1] == '_') --blen;
    std::size_t ilen = std::strlen(infix);
    if (blen + ilen + 1 > out_cap) return false;
    std::memcpy(out, type_name, blen);
    std::memcpy(out + blen, infix, ilen);
    out[blen + ilen] = '\0';
    return true;
}

// `ros_form_to_dds` moved to descriptors.cpp (shared with `action_topic_type`
// / `find_descriptor`); declared in descriptors.hpp.

bool descriptors_for_service(const char* service_name, const char* type_name,
                             const dds_topic_descriptor_t** out_req,
                             const dds_topic_descriptor_t** out_rep) {
    char dds_type[kMaxTopicName];
    if (!ros_form_to_dds(type_name, dds_type, sizeof(dds_type))) {
        return false;
    }
    char base[kMaxTopicName];
    if (!action_effective_base(service_name, dds_type, base, sizeof(base))) {
        return false;
    }
    char req_type[kMaxTopicName];
    char rep_type[kMaxTopicName];
    if (!service_type_name(base, "_Request_", req_type, sizeof(req_type))) {
        return false;
    }
    if (!service_type_name(base, "_Response_", rep_type, sizeof(rep_type))) {
        return false;
    }
    *out_req = find_descriptor(req_type);
    *out_rep = find_descriptor(rep_type);
    return *out_req != nullptr && *out_rep != nullptr;
}

uint64_t random_seed_word() {
#if defined(NROS_PLATFORM_FREERTOS) || defined(NROS_PLATFORM_THREADX)
    return platform_random_u64();
#else
    std::random_device rd;
    return (static_cast<uint64_t>(rd()) << 32) ^ rd();
#endif
}

uint64_t random_guid64() {
    return random_seed_word();
}

bool request_writer_matched(dds_entity_t writer) {
    dds_publication_matched_status_t status{};
    return dds_get_publication_matched_status(writer, &status) == DDS_RETCODE_OK &&
           status.current_count > 0;
}

nros_rmw_ret_t wait_for_request_match(dds_entity_t writer, uint64_t deadline_ms) {
    const uint64_t poll_ms = env_u64("NROS_CYCLONE_MATCH_POLL_MS", kDefaultMatchPollMs);
    while (platform_now_ms() < deadline_ms) {
        if (request_writer_matched(writer)) return NROS_RMW_RET_OK;
        platform_sleep_ms(static_cast<uint32_t>(poll_ms));
    }
    return NROS_RMW_RET_TIMEOUT;
}

nros_rmw_ret_t maybe_flush_request(ClientState* state) {
    if (state == nullptr || state->pending_request_len == 0) {
        return NROS_RMW_RET_OK;
    }
    // Services use RELIABLE + VOLATILE QoS. A write before the client
    // request writer has matched the server request reader can be accepted
    // locally but never delivered, which loses the first nonblocking action
    // send_goal request. Keep the buffered request pending until discovery
    // reports a match.
    if (!request_writer_matched(state->writer)) {
        return NROS_RMW_RET_OK;
    }
    nros_rmw_ret_t r = write_typed(state->writer, state->req_desc, state->req_st,
                                   state->pending_request, state->pending_request_len);
    if (r == NROS_RMW_RET_OK) {
        state->pending_request_len = 0;
    }
    return r;
}

} // namespace

// =========================================================================
// Service server
// =========================================================================

nros_rmw_ret_t service_server_create(nros_rmw_session_t* session, const char* service_name,
                                     const char* type_name, const char* /*type_hash*/,
                                     uint32_t /*domain_id*/, const nros_rmw_qos_t* qos,
                                     nros_rmw_service_server_t* out) {
    if (out == nullptr || session == nullptr || service_name == nullptr || type_name == nullptr) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    out->backend_data = nullptr;

    dds_entity_t pp = session_participant(session);
    if (pp == 0) return NROS_RMW_RET_ERROR;

    const dds_topic_descriptor_t* req_desc = nullptr;
    const dds_topic_descriptor_t* rep_desc = nullptr;
    if (!descriptors_for_service(service_name, type_name, &req_desc, &rep_desc)) {
        return NROS_RMW_RET_UNSUPPORTED;
    }

    char req_topic[kMaxTopicName];
    char rep_topic[kMaxTopicName];
    if (!service_topic_name(service_name, "rq", "Request", req_topic, sizeof(req_topic)) ||
        !service_topic_name(service_name, "rr", "Reply", rep_topic, sizeof(rep_topic))) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }

    auto* state = new (std::nothrow) ServerState();
    if (state == nullptr) return NROS_RMW_RET_BAD_ALLOC;
    state->req_desc = req_desc;
    state->rep_desc = rep_desc;

    state->request_topic = dds_create_topic(pp, req_desc, req_topic, nullptr, nullptr);
    state->reply_topic = dds_create_topic(pp, rep_desc, rep_topic, nullptr, nullptr);
    if (state->request_topic < 0 || state->reply_topic < 0) {
        if (state->request_topic > 0) (void)dds_delete(state->request_topic);
        if (state->reply_topic > 0) (void)dds_delete(state->reply_topic);
        delete state;
        return NROS_RMW_RET_ERROR;
    }

    // Phase 193.1b: honour the caller's profile, defaulting to
    // `rmw_qos_profile_services_default` (RELIABLE + VOLATILE +
    // KEEP_LAST(10)) — the stock-RMW-interop default (without it Cyclone
    // defaults to KEEP_LAST(1), surprising stock clients). One profile
    // applied to both request reader + reply writer.
    nros_rmw_qos_t svc_qos = qos != nullptr ? *qos : NROS_RMW_QOS_PROFILE_SERVICES_DEFAULT;
    dds_qos_t* dq_reader = make_dds_qos(&svc_qos);
    dds_qos_t* dq_writer = make_dds_qos(&svc_qos);
    state->reader = dds_create_reader(pp, state->request_topic, dq_reader, nullptr);
    state->writer = dds_create_writer(pp, state->reply_topic, dq_writer, nullptr);
    if (dq_reader != nullptr) dds_delete_qos(dq_reader);
    if (dq_writer != nullptr) dds_delete_qos(dq_writer);
    if (state->reader < 0 || state->writer < 0) {
        if (state->reader > 0) (void)dds_delete(state->reader);
        if (state->writer > 0) (void)dds_delete(state->writer);
        (void)dds_delete(state->request_topic);
        (void)dds_delete(state->reply_topic);
        delete state;
        return NROS_RMW_RET_ERROR;
    }

    state->req_st = new (std::nothrow) SertypeMin(req_desc);
    state->rep_st = new (std::nothrow) SertypeMin(rep_desc);
    if (state->req_st == nullptr || state->rep_st == nullptr) {
        delete state->req_st;
        delete state->rep_st;
        (void)dds_delete(state->reader);
        (void)dds_delete(state->writer);
        (void)dds_delete(state->request_topic);
        (void)dds_delete(state->reply_topic);
        delete state;
        return NROS_RMW_RET_BAD_ALLOC;
    }

    out->backend_data = state;
    // Phase 177.36 — register both endpoints with the node graph (server:
    // request reader + reply writer; client: request writer + reply reader).
    graph_track_reader(session_graph(session), state->reader);
    graph_track_writer(session_graph(session), state->writer);
    return NROS_RMW_RET_OK;
}

void service_server_destroy(nros_rmw_service_server_t* server) {
    if (server == nullptr || server->backend_data == nullptr) return;
    auto* state = static_cast<ServerState*>(server->backend_data);
    if (state->reader > 0) (void)dds_delete(state->reader);
    if (state->writer > 0) (void)dds_delete(state->writer);
    if (state->request_topic > 0) (void)dds_delete(state->request_topic);
    if (state->reply_topic > 0) (void)dds_delete(state->reply_topic);
    delete state->req_st;
    delete state->rep_st;
    delete state;
    server->backend_data = nullptr;
}

int32_t service_try_recv_request(nros_rmw_service_server_t* server, uint8_t* buf, size_t buf_len,
                                 int64_t* seq_out) {
    if (server == nullptr || server->backend_data == nullptr || buf == nullptr) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    auto* state = static_cast<ServerState*>(server->backend_data);

    uint8_t wire[kWireScratch];
    int32_t wire_len = take_typed_wire(state->reader, state->req_st, wire, sizeof(wire));
    if (wire_len <= 0) return wire_len;

    RequestId id{};
    int32_t user_len =
        split_wire_header(wire, static_cast<size_t>(wire_len), state->req_desc, &id, buf, buf_len);
    if (user_len < 0) return user_len;

    // Allocate a slot to remember the (writer_guid, seq) pair so the
    // matching `service_send_reply` can echo it back.
    for (std::size_t i = 0; i < kRequestSlots; ++i) {
        if (!state->slots[i].in_use) {
            state->slots[i].id = id;
            state->slots[i].in_use = true;
            if (seq_out != nullptr) *seq_out = static_cast<int64_t>(i);
            return user_len;
        }
    }
    return NROS_RMW_RET_WOULD_BLOCK;
}

int32_t service_has_request(nros_rmw_service_server_t* server) {
    if (server == nullptr || server->backend_data == nullptr) return 0;
    auto* state = static_cast<ServerState*>(server->backend_data);
    uint32_t status = 0;
    if (dds_get_status_changes(state->reader, &status) != DDS_RETCODE_OK) return 0;
    return (status & DDS_DATA_AVAILABLE_STATUS) ? 1 : 0;
}

nros_rmw_ret_t service_send_reply(nros_rmw_service_server_t* server, int64_t seq,
                                  const uint8_t* data, size_t len) {
    if (server == nullptr || server->backend_data == nullptr || data == nullptr || seq < 0 ||
        static_cast<std::size_t>(seq) >= kRequestSlots) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    auto* state = static_cast<ServerState*>(server->backend_data);
    auto& slot = state->slots[seq];
    if (!slot.in_use) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    // Wait for the reply reader before writing (services are RELIABLE +
    // VOLATILE, so a write before the reader matches is silently dropped).
    // Prefer the firm `current_count > 0` match (the nano-ros↔nano-ros fast
    // path). But stock `rmw_cyclonedds_cpp` clients on Cyclone 0.10.5 can leave
    // the writer's `current_count` at 0 even after the reply reader has been
    // discovered (`total_count > 0`) and is waiting — an under-reported
    // cross-RMW match-state. In that case, after a short grace, write anyway:
    // the discovered reader is present and the VOLATILE write reaches it.
    // Without this the server hangs the full timeout and the stock
    // `ros2 service call` gives up (117.12.B.1).
    const uint64_t deadline =
        platform_now_ms() + env_u64("NROS_CYCLONE_MATCH_TIMEOUT_MS", kDefaultMatchTimeoutMs);
    const uint64_t grace_until = platform_now_ms() + 750;
    bool ready = false;
    while (platform_now_ms() < deadline) {
        dds_publication_matched_status_t st{};
        if (dds_get_publication_matched_status(state->writer, &st) == DDS_RETCODE_OK) {
            if (st.current_count > 0) {
                ready = true;
                break;
            }
            if (st.total_count > 0 && platform_now_ms() >= grace_until) {
                ready = true;
                break;
            }
        }
        platform_sleep_ms(5);
    }
    if (!ready) return NROS_RMW_RET_TIMEOUT;

    uint8_t wire[kWireScratch];
    int32_t wire_len = build_wire_with_header(data, len, slot.id, wire, sizeof(wire));
    nros_rmw_ret_t r;
    if (wire_len < 0) {
        r = static_cast<nros_rmw_ret_t>(wire_len);
    } else {
        r = write_typed(state->writer, state->rep_desc, state->rep_st, wire,
                        static_cast<size_t>(wire_len));
    }
    slot.in_use = false;
    return r;
}

// =========================================================================
// Service client
// =========================================================================

nros_rmw_ret_t service_client_create(nros_rmw_session_t* session, const char* service_name,
                                     const char* type_name, const char* /*type_hash*/,
                                     uint32_t /*domain_id*/, const nros_rmw_qos_t* qos,
                                     nros_rmw_service_client_t* out) {
    if (out == nullptr || session == nullptr || service_name == nullptr || type_name == nullptr) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    out->backend_data = nullptr;

    dds_entity_t pp = session_participant(session);
    if (pp == 0) return NROS_RMW_RET_ERROR;

    const dds_topic_descriptor_t* req_desc = nullptr;
    const dds_topic_descriptor_t* rep_desc = nullptr;
    if (!descriptors_for_service(service_name, type_name, &req_desc, &rep_desc)) {
        return NROS_RMW_RET_UNSUPPORTED;
    }

    char req_topic[kMaxTopicName];
    char rep_topic[kMaxTopicName];
    if (!service_topic_name(service_name, "rq", "Request", req_topic, sizeof(req_topic)) ||
        !service_topic_name(service_name, "rr", "Reply", rep_topic, sizeof(rep_topic))) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }

    auto* state = new (std::nothrow) ClientState();
    if (state == nullptr) return NROS_RMW_RET_BAD_ALLOC;
    state->req_desc = req_desc;
    state->rep_desc = rep_desc;
    state->next_seq.store(0, std::memory_order_relaxed);

    state->request_topic = dds_create_topic(pp, req_desc, req_topic, nullptr, nullptr);
    state->reply_topic = dds_create_topic(pp, rep_desc, rep_topic, nullptr, nullptr);
    if (state->request_topic < 0 || state->reply_topic < 0) {
        if (state->request_topic > 0) (void)dds_delete(state->request_topic);
        if (state->reply_topic > 0) (void)dds_delete(state->reply_topic);
        delete state;
        return NROS_RMW_RET_ERROR;
    }

    // Phase 193.1b: honour the caller's profile, defaulting to
    // `rmw_qos_profile_services_default` (stock-RMW-interop default).
    nros_rmw_qos_t svc_qos = qos != nullptr ? *qos : NROS_RMW_QOS_PROFILE_SERVICES_DEFAULT;
    dds_qos_t* dq_writer = make_dds_qos(&svc_qos);
    dds_qos_t* dq_reader = make_dds_qos(&svc_qos);
    state->writer = dds_create_writer(pp, state->request_topic, dq_writer, nullptr);
    state->reader = dds_create_reader(pp, state->reply_topic, dq_reader, nullptr);
    if (dq_writer != nullptr) dds_delete_qos(dq_writer);
    if (dq_reader != nullptr) dds_delete_qos(dq_reader);
    if (state->writer < 0 || state->reader < 0) {
        if (state->writer > 0) (void)dds_delete(state->writer);
        if (state->reader > 0) (void)dds_delete(state->reader);
        (void)dds_delete(state->request_topic);
        (void)dds_delete(state->reply_topic);
        delete state;
        return NROS_RMW_RET_ERROR;
    }

    state->req_st = new (std::nothrow) SertypeMin(req_desc);
    state->rep_st = new (std::nothrow) SertypeMin(rep_desc);
    if (state->req_st == nullptr || state->rep_st == nullptr) {
        delete state->req_st;
        delete state->rep_st;
        (void)dds_delete(state->writer);
        (void)dds_delete(state->reader);
        (void)dds_delete(state->request_topic);
        (void)dds_delete(state->reply_topic);
        delete state;
        return NROS_RMW_RET_BAD_ALLOC;
    }

    // Use the lower 8 bytes of the writer's RTPS GUID as the client
    // identity. Falls back to a random 64-bit value if dds_get_guid
    // fails or returns an all-zero prefix.
    state->my_guid = writer_guid_lo64(state->writer);
    if (state->my_guid == 0) {
        state->my_guid = random_guid64();
    }

    // Cyclone DDS 0.10.5 can miss local delivery when multiple service
    // clients are created back-to-back on one participant. Action clients
    // create send_goal/cancel/get_result clients in sequence, so leave a
    // small discovery window between creations.
    platform_sleep_ms(100);

    out->backend_data = state;
    // Phase 177.36 — register both endpoints with the node graph (server:
    // request reader + reply writer; client: request writer + reply reader).
    graph_track_reader(session_graph(session), state->reader);
    graph_track_writer(session_graph(session), state->writer);
    return NROS_RMW_RET_OK;
}

void service_client_destroy(nros_rmw_service_client_t* client) {
    if (client == nullptr || client->backend_data == nullptr) return;
    auto* state = static_cast<ClientState*>(client->backend_data);
    if (state->writer > 0) (void)dds_delete(state->writer);
    if (state->reader > 0) (void)dds_delete(state->reader);
    if (state->request_topic > 0) (void)dds_delete(state->request_topic);
    if (state->reply_topic > 0) (void)dds_delete(state->reply_topic);
    delete state->req_st;
    delete state->rep_st;
    delete state;
    client->backend_data = nullptr;
}

int32_t service_call_raw(nros_rmw_service_client_t* client, const uint8_t* request, size_t req_len,
                         uint8_t* reply_buf, size_t reply_buf_len) {
    if (client == nullptr || client->backend_data == nullptr || request == nullptr ||
        reply_buf == nullptr || req_len < 4) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    auto* state = static_cast<ClientState*>(client->backend_data);

    RequestId my_id{};
    my_id.guid = state->my_guid;
    my_id.seq = state->next_seq.fetch_add(1, std::memory_order_relaxed);

    // 5 s total timeout covers request-reader match plus reply. Service
    // QoS is VOLATILE, so the first write must wait until discovery has
    // matched the client writer with the server request reader.
    const uint64_t deadline =
        platform_now_ms() + env_u64("NROS_CYCLONE_MATCH_TIMEOUT_MS", kDefaultMatchTimeoutMs);
    nros_rmw_ret_t match = wait_for_request_match(state->writer, deadline);
    if (match != NROS_RMW_RET_OK) return match;

    uint8_t wire_req[kWireScratch];
    int32_t wire_len = build_wire_with_header(request, req_len, my_id, wire_req, sizeof(wire_req));
    if (wire_len < 0) return wire_len;
    nros_rmw_ret_t pr = write_typed(state->writer, state->req_desc, state->req_st, wire_req,
                                    static_cast<size_t>(wire_len));
    if (pr != NROS_RMW_RET_OK) return pr;

    while (platform_now_ms() < deadline) {
        uint32_t status = 0;
        if (dds_get_status_changes(state->reader, &status) == DDS_RETCODE_OK &&
            (status & DDS_DATA_AVAILABLE_STATUS)) {
            uint8_t wire_rep[kWireScratch];
            int32_t wlen =
                take_typed_wire(state->reader, state->rep_st, wire_rep, sizeof(wire_rep));
            if (wlen == NROS_RMW_RET_NO_DATA) {
                platform_sleep_ms(2);
                continue;
            }
            if (wlen < 0) return wlen;

            RequestId got_id{};
            int32_t user_len =
                split_wire_header(wire_rep, static_cast<size_t>(wlen), state->rep_desc, &got_id,
                                  reply_buf, reply_buf_len);
            if (user_len < 0) return user_len;
            if (got_id.seq == my_id.seq && got_id.guid == my_id.guid) {
                return user_len;
            }
            // Reply for a different in-flight call from the same
            // client (impossible in single-shot tests, defensive
            // here). Drop and keep polling.
            continue;
        }
        platform_sleep_ms(5);
    }
    return NROS_RMW_RET_TIMEOUT;
}

// Phase 130.8 — non-blocking send/recv split. Mirrors
// `xrce_service_send_request_raw` / `_try_recv_reply_raw` in the
// XRCE backend. Lets the executor's spin loop poll for a late-
// arriving reply without re-sending the request or blocking 5 s
// inside `call_raw` (Phase 127.C.4 root cause class).
nros_rmw_ret_t service_send_request_raw(nros_rmw_service_client_t* client, const uint8_t* request,
                                        size_t req_len) {
    if (client == nullptr || client->backend_data == nullptr || request == nullptr || req_len < 4) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    auto* state = static_cast<ClientState*>(client->backend_data);
    if (state->pending_seq.load(std::memory_order_acquire) >= 0) {
        // The upper layers clear their own in-flight guard on timeout before
        // retrying. Mirror that abandon here so a slow first request doesn't
        // wedge every later call; stale late replies are filtered by seq/guid.
        state->pending_request_len = 0;
        state->pending_seq.store(-1, std::memory_order_release);
    }

    RequestId my_id{};
    my_id.guid = state->my_guid;
    my_id.seq = state->next_seq.fetch_add(1, std::memory_order_relaxed);

    uint8_t wire_req[kWireScratch];
    int32_t wire_len = build_wire_with_header(request, req_len, my_id, wire_req, sizeof(wire_req));
    if (wire_len < 0) return wire_len;

    std::memcpy(state->pending_request, wire_req, static_cast<size_t>(wire_len));
    state->pending_request_len = static_cast<size_t>(wire_len);
    state->pending_seq.store(my_id.seq, std::memory_order_release);
    nros_rmw_ret_t pr = maybe_flush_request(state);
    if (pr < 0 && pr != NROS_RMW_RET_NO_DATA) {
        state->pending_request_len = 0;
        state->pending_seq.store(-1, std::memory_order_release);
        return pr;
    }
    return NROS_RMW_RET_OK;
}

int32_t service_try_recv_reply_raw(nros_rmw_service_client_t* client, uint8_t* reply_buf,
                                   size_t reply_buf_len) {
    if (client == nullptr || client->backend_data == nullptr || reply_buf == nullptr) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    auto* state = static_cast<ClientState*>(client->backend_data);

    int64_t pending = state->pending_seq.load(std::memory_order_acquire);
    if (pending < 0) {
        return NROS_RMW_RET_NO_DATA;
    }

    nros_rmw_ret_t flush = maybe_flush_request(state);
    if (flush < 0) {
        if (flush != NROS_RMW_RET_NO_DATA) {
            state->pending_request_len = 0;
            state->pending_seq.store(-1, std::memory_order_release);
        }
        return flush;
    }

    uint32_t status = 0;
    if (dds_get_status_changes(state->reader, &status) != DDS_RETCODE_OK ||
        !(status & DDS_DATA_AVAILABLE_STATUS)) {
        return NROS_RMW_RET_NO_DATA;
    }

    uint8_t wire_rep[kWireScratch];
    int32_t wlen = take_typed_wire(state->reader, state->rep_st, wire_rep, sizeof(wire_rep));
    if (wlen == NROS_RMW_RET_NO_DATA) return NROS_RMW_RET_NO_DATA;
    if (wlen < 0) return wlen;

    RequestId got_id{};
    int32_t user_len = split_wire_header(wire_rep, static_cast<size_t>(wlen), state->rep_desc,
                                         &got_id, reply_buf, reply_buf_len);
    if (user_len < 0) return user_len;

    if (got_id.seq == pending && got_id.guid == state->my_guid) {
        state->pending_seq.store(-1, std::memory_order_release);
        return user_len;
    }
    // Reply for a different in-flight call (impossible in single-
    // shot tests; defensive). Drop, surface as NoData so the
    // executor retries on the next spin tick.
    return NROS_RMW_RET_NO_DATA;
}

} // namespace nros_rmw_cyclonedds
