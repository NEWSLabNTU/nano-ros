// `_FeedbackMessage_` publisher/subscriber goal_id round-trip (233.6).
//
// Builds a `Fibonacci_FeedbackMessage_`-shaped descriptor via the K.7.4.b
// dynamic bridge, registers it, then publishes a hand-crafted nano-ros
// runtime payload (4-byte enc header + 16 raw UUID bytes + sequence<int32>
// body) and asserts the subscriber takes the same bytes back.
//
// Since 233.6 the action `goal_id` is a fixed `octet[16]` on both the wire
// and the IDL (ROS 2 `unique_identifier_msgs/UUID`), so the publisher and
// subscriber pass it straight through — the old `[4 u32=16]` length-prefix
// strip/reinsert adapter (`strip_feedback_goal_id_prefix` /
// `insert_goal_id_len_at`) was removed. This proves the generic
// `dds_stream_read_sample` path round-trips the
// `[octet goal_id[16]; sequence<int32> sequence;]` IDL shape — the same IDL
// the action runtime carries on the live wire.

#include <cstdint>
#include <cstdio>
#include <cstring>
#include <chrono>
#include <thread>

#include <dds/dds.h>
#include <dds/ddsc/dds_public_impl.h>

#include "nros/rmw_ret.h"
#include "nros/rmw_vtable.h"
#include "nros_rmw_cyclonedds.h"

// ABI mirror of `crate::bridge::Nros{Field,FieldKind}Descriptor`.
struct NrosFieldDescriptor {
    const char* name;
    uint32_t offset;
    uint32_t kind;
};
struct NrosFieldKindDescriptor {
    uint8_t kind;
    uint8_t _pad[3];
    uint32_t bound;
    uint32_t inner;
    const char* nested_name;
};

extern "C" const void* nros_cyclonedds_build_descriptor_from_schema(
    const char* type_name, const NrosFieldDescriptor* fields, uint32_t field_count,
    const NrosFieldKindDescriptor* kinds, uint32_t kind_count, int* out_err);

extern "C" void nros_rmw_cyclonedds_register_descriptor(const char* type_name,
                                                        const dds_topic_descriptor_t* descriptor);

namespace {
const nros_rmw_vtable_t* g_vt = nullptr;

constexpr uint8_t kKindUint8 = 1;
constexpr uint8_t kKindInt32 = 6;
constexpr uint8_t kKindNested = 15;
constexpr uint8_t kKindArray = 16;
constexpr uint8_t kKindSequence = 17;

#define EXPECT(cond, fmt, ...)                                                                     \
    do {                                                                                           \
        if (!(cond)) {                                                                             \
            std::fprintf(stderr, "FAIL %s:%d " fmt "\n", __FILE__, __LINE__, ##__VA_ARGS__);       \
            return 1;                                                                              \
        }                                                                                          \
    } while (0)
} // namespace

extern "C" nros_rmw_ret_t nros_rmw_cffi_register_named(const char* /*name*/,
                                                       const nros_rmw_vtable_t* vt) {
    g_vt = vt;
    return NROS_RMW_RET_OK;
}

int main() {
    if (nros_rmw_cyclonedds_register() != NROS_RMW_RET_OK || g_vt == nullptr) {
        std::fprintf(stderr, "register failed\n");
        return 1;
    }

    // Build the `Fibonacci_FeedbackMessage_` descriptor:
    //   struct FeedbackMessage_ {
    //       octet goal_id[16];                   // offset 0
    //       Feedback_ feedback;                  // offset 16
    //   };
    //   struct Feedback_ { sequence<int32> sequence; };
    //
    // Cyclone-IDL UUID is the inline 16-byte fixed array (matches the
    // wire-format expectation of `dds_stream_read_sample`). Rust ships
    // a `[4 u32=16]` length prefix before those 16 bytes; the
    // publisher's `strip_feedback_goal_id_prefix` helper removes it so
    // the body lines up with the descriptor.
    NrosFieldKindDescriptor kinds[] = {
        // kinds[0] — array<uint8, 16>
        {kKindArray, {0, 0, 0}, 16, 1, nullptr},
        // kinds[1] — uint8 (array elem)
        {kKindUint8, {0, 0, 0}, 0, 0, nullptr},
        // kinds[2] — Feedback_ (nested, first child = 3, child count = 1)
        {kKindNested, {0, 0, 0}, 1, 3, "example_interfaces/action/Fibonacci_Feedback"},
        // kinds[3] — sequence<kinds[4]>
        {kKindSequence, {0, 0, 0}, 0, 4, nullptr},
        // kinds[4] — int32 (sequence elem)
        {kKindInt32, {0, 0, 0}, 0, 0, nullptr},
    };
    // The dynamic bridge places nested-struct fields based on `offset`.
    // For our purposes we just need a layout Cyclone accepts; align
    // `feedback` at the next 8-byte boundary after the 16-byte
    // `goal_id` field.
    NrosFieldDescriptor fields[] = {
        {"goal_id", 0, 0},
        {"feedback", 16, 2},
    };

    int err = 0;
    const void* raw = nros_cyclonedds_build_descriptor_from_schema(
        "example_interfaces/action/Fibonacci_FeedbackMessage", fields, 2, kinds, 5, &err);
    EXPECT(raw != nullptr, "bridge returned NULL, err=%d", err);
    const auto* desc = static_cast<const dds_topic_descriptor_t*>(raw);
    EXPECT(desc->m_typename != nullptr, "NULL typename");
    EXPECT(std::strstr(desc->m_typename, "Fibonacci_FeedbackMessage_") != nullptr,
           "unexpected typename %s", desc->m_typename);

    // Register under the action backend's lookup table so
    // `publisher_create` / `subscriber_create` can find it.
    nros_rmw_cyclonedds_register_descriptor(desc->m_typename, desc);

    nros_rmw_session_t s{};
    s.node_name = "feedback_roundtrip";
    s.namespace_ = "/";
    if (g_vt->open(nullptr, 0, 88, s.node_name, &s) != NROS_RMW_RET_OK) {
        std::fprintf(stderr, "session open failed\n");
        return 2;
    }

    nros_rmw_qos_t qos = NROS_RMW_QOS_PROFILE_DEFAULT;

    nros_rmw_subscriber_t sub{};
    sub.topic_name = "rt/feedback_roundtrip";
    sub.type_name = desc->m_typename;
    sub.qos = qos;
    if (g_vt->create_subscriber(&s, sub.topic_name, sub.type_name, "", 88, &qos, &sub) !=
        NROS_RMW_RET_OK) {
        std::fprintf(stderr, "create_subscriber failed\n");
        return 3;
    }

    nros_rmw_publisher_t pub{};
    pub.topic_name = "rt/feedback_roundtrip";
    pub.type_name = desc->m_typename;
    pub.qos = qos;
    if (g_vt->create_publisher(&s, pub.topic_name, pub.type_name, "", 88, &qos, &pub) !=
        NROS_RMW_RET_OK) {
        std::fprintf(stderr, "create_publisher failed\n");
        return 4;
    }

    // Hand-build the nano-ros runtime wire payload (233.6 — goal_id is a fixed
    // `octet[16]` with NO length prefix, matching ROS 2
    // `unique_identifier_msgs/UUID`):
    //   [4 enc=00 01 00 00]
    //   [16 raw uuid bytes]   ← goal_id, inline, no prefix
    //   [4 u32=3]             ← sequence<int32> length
    //   [3*4 int32 elems]
    uint8_t wire[64] = {};
    size_t pos = 0;
    const uint8_t enc[4] = {0x00, 0x01, 0x00, 0x00};
    std::memcpy(wire + pos, enc, 4);
    pos += 4;
    const uint8_t goal_id_bytes[16] = {0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08,
                                       0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f, 0x10};
    std::memcpy(wire + pos, goal_id_bytes, 16);
    pos += 16;
    const uint32_t seq_len = 3;
    std::memcpy(wire + pos, &seq_len, 4);
    pos += 4;
    const int32_t seq_vals[3] = {0, 1, 1};
    std::memcpy(wire + pos, seq_vals, sizeof(seq_vals));
    pos += sizeof(seq_vals);
    const size_t wire_len = pos;

    // Brief stabilisation so writer-↔-reader match completes.
    std::this_thread::sleep_for(std::chrono::milliseconds(500));

    nros_rmw_ret_t pr = g_vt->publish_raw(&pub, wire, wire_len);
    EXPECT(pr == NROS_RMW_RET_OK, "publish_raw returned %d", static_cast<int>(pr));

    bool got = false;
    for (int i = 0; i < 200 && !got; ++i) {
        if (g_vt->has_data(&sub)) {
            got = true;
            break;
        }
        std::this_thread::sleep_for(std::chrono::milliseconds(10));
    }
    EXPECT(got, "no data delivered");

    uint8_t recv[256] = {};
    int32_t n = g_vt->try_recv_raw(&sub, recv, sizeof(recv));
    EXPECT(n > 0, "try_recv_raw returned %d", n);

    // 233.6 — the goal_id is a fixed `octet[16]` on both wire and IDL, so the
    // publisher/subscriber pass it straight through (no strip/reinsert). The
    // round-trip should preserve `[4 enc][16 uuid]` intact, with the sequence
    // body following (modulo any tail padding Cyclone adds on re-serialise).
    EXPECT(n >= 4 + 16 + 4 + 4, "recv too short: %d", n);
    EXPECT(std::memcmp(recv, enc, 4) == 0, "enc header drift");
    EXPECT(std::memcmp(recv + 4, goal_id_bytes, 16) == 0, "uuid bytes drift");

    // Body sanity: the sequence length should be 3, and the three int32
    // values should follow (offset = 4 enc + 16 uuid).
    uint32_t recv_seq_len = 0;
    std::memcpy(&recv_seq_len, recv + 4 + 16, 4);
    EXPECT(recv_seq_len == 3, "feedback sequence length expected 3 got %u", recv_seq_len);
    int32_t recv_vals[3] = {-1, -1, -1};
    std::memcpy(recv_vals, recv + 4 + 16 + 4, sizeof(recv_vals));
    EXPECT(recv_vals[0] == 0 && recv_vals[1] == 1 && recv_vals[2] == 1,
           "feedback sequence values drift: [%d,%d,%d]", recv_vals[0], recv_vals[1], recv_vals[2]);

    g_vt->destroy_publisher(&pub);
    g_vt->destroy_subscriber(&sub);
    (void)g_vt->close(&s);

    std::printf("OK feedback_roundtrip — %d recv bytes, fixed octet[16] goal_id round-trips\n", n);
    return 0;
}
