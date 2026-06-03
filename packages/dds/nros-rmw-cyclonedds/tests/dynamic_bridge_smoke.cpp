// Phase 212.K.7.4.b — smoke test for the dynamic-type bridge.
//
// Calls `nros_cyclonedds_build_descriptor_from_schema` directly with
// a primitive-only schema, verifies the returned descriptor is
// non-NULL, has the expected mangled type-name, and is accepted by
// `dds_create_topic` on a real Cyclone participant.

#include <cstdint>
#include <cstdio>
#include <cstring>

#include <dds/dds.h>
#include <dds/ddsc/dds_public_impl.h>

// ABI mirror of `crate::bridge::NrosFieldDescriptor` /
// `NrosFieldKindDescriptor`. Kept in lockstep with
// `bridge/dynamic_type_builder.cpp`.
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

namespace {

constexpr uint8_t kKindInt32 = 6;
constexpr uint8_t kKindFloat64 = 10;
constexpr uint8_t kKindString = 11;

} // namespace

int main() {
    // Schema: { i32 a @ 0; f64 b @ 8; string s @ 16; }
    NrosFieldKindDescriptor kinds[] = {
        // kinds[0] — int32
        {kKindInt32, {0, 0, 0}, 0, 0, nullptr},
        // kinds[1] — float64
        {kKindFloat64, {0, 0, 0}, 0, 0, nullptr},
        // kinds[2] — string
        {kKindString, {0, 0, 0}, 0, 0, nullptr},
    };
    NrosFieldDescriptor fields[] = {
        {"a", 0, 0},
        {"b", 8, 1},
        {"s", 16, 2},
    };

    int err = 0;
    const void* raw = nros_cyclonedds_build_descriptor_from_schema("test_msgs/msg/PrimMix", fields,
                                                                   3, kinds, 3, &err);
    if (raw == nullptr) {
        std::fprintf(stderr, "bridge returned NULL, err=%d\n", err);
        return 1;
    }
    const auto* desc = static_cast<const dds_topic_descriptor_t*>(raw);
    if (desc->m_typename == nullptr) {
        std::fprintf(stderr, "descriptor has NULL m_typename\n");
        return 2;
    }
    if (std::strcmp(desc->m_typename, "test_msgs::msg::dds_::PrimMix_") != 0) {
        std::fprintf(stderr, "unexpected m_typename: %s\n", desc->m_typename);
        return 3;
    }
    if (desc->m_size == 0 || desc->m_align == 0) {
        std::fprintf(stderr, "bad size/align: size=%u align=%u\n", desc->m_size, desc->m_align);
        return 4;
    }
    if (desc->m_nops < 4 || desc->m_ops == nullptr) {
        std::fprintf(stderr, "bad ops: nops=%u ptr=%p\n", desc->m_nops, (const void*)desc->m_ops);
        return 5;
    }

    // Pass it to Cyclone: create a private-domain participant and
    // create a topic from our synthesised descriptor.
    dds_entity_t pp = dds_create_participant(98, nullptr, nullptr);
    if (pp < 0) {
        std::fprintf(stderr, "dds_create_participant failed: %d\n", static_cast<int>(pp));
        return 6;
    }
    dds_entity_t topic = dds_create_topic(pp, desc, "rt/dynamic_bridge_smoke", nullptr, nullptr);
    if (topic < 0) {
        std::fprintf(stderr, "dds_create_topic failed: %d\n", static_cast<int>(topic));
        (void)dds_delete(pp);
        return 7;
    }
    (void)dds_delete(pp); // cascades to topic
    std::printf("OK m_typename=%s m_size=%u m_align=%u m_nops=%u\n", desc->m_typename, desc->m_size,
                desc->m_align, desc->m_nops);
    return 0;
}
