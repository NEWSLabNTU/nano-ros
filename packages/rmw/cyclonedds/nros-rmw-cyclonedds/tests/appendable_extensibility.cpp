// phase-303 W1c (#0267) — ROS-edition wire-extensibility of generated
// descriptors.
//
// `nros_cyclonedds_build_descriptor_from_schema`'s `extensibility` argument
// selects FINAL (0, humble — byte-identical to pre-W1c) vs APPENDABLE
// (non-zero, iron/jazzy+). Appendable prefixes each aggregate op stream with
// `DDS_OP_DLC`, which is exactly how Cyclone derives extensibility
// (`dds_stream_extensibility` reads the leading op) and how it decides to wrap
// a nested struct in a DHEADER under XCDR2 — the fix for a modern ROS 2 peer
// that mis-walks nano-ros's FINAL/XCDR1 bytes across a domain_bridge republish.
//
// This test proves the op-stream shape (leading DLC) without a live peer.
// End-to-end wire delivery is the pending #0267 live-demo confirmation.

#include <cstdint>
#include <cstdio>

#include <dds/dds.h>
#include <dds/ddsc/dds_opcodes.h>
#include <dds/ddsc/dds_public_impl.h>

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
    const NrosFieldKindDescriptor* kinds, uint32_t kind_count, uint32_t extensibility,
    int* out_err);

namespace {
constexpr uint8_t kKindInt32 = 6;

// Same primitive schema as the smoke test — the extensibility is orthogonal to
// the field layout.
const NrosFieldKindDescriptor kinds[] = {
    {kKindInt32, {0, 0, 0}, 0, 0, nullptr},
};
const NrosFieldDescriptor fields[] = {
    {"a", 0, 0},
};

const dds_topic_descriptor_t* build(uint32_t ext) {
    int err = 0;
    const void* raw = nros_cyclonedds_build_descriptor_from_schema("test_msgs/msg/OneI32", fields,
                                                                   1, kinds, 1, ext, &err);
    return static_cast<const dds_topic_descriptor_t*>(raw);
}

constexpr uint8_t kKindNested = 15;
// Schema: Outer { Inner nested @ 0 }  where Inner { int32 v }.
const NrosFieldKindDescriptor nested_kinds[] = {
    {kKindNested,
     {0, 0, 0},
     /*bound=field_count*/ 1,
     /*inner=first_child*/ 1,
     "test_msgs/msg/Inner"},
    {kKindInt32, {0, 0, 0}, 0, 0, nullptr},
};
const NrosFieldDescriptor nested_fields[] = {
    {"nested", 0, 0},
};

const dds_topic_descriptor_t* build_nested(uint32_t ext) {
    int err = 0;
    const void* raw = nros_cyclonedds_build_descriptor_from_schema(
        "test_msgs/msg/Outer", nested_fields, 1, nested_kinds, 2, ext, &err);
    return static_cast<const dds_topic_descriptor_t*>(raw);
}

uint32_t count_dlc(const dds_topic_descriptor_t* d) {
    uint32_t n = 0;
    for (uint32_t i = 0; i < d->m_nops; ++i) {
        if (DDS_OP(d->m_ops[i]) == DDS_OP_DLC) ++n;
    }
    return n;
}
} // namespace

int main() {
    const auto* fin = build(0u); // FINAL (humble)
    const auto* app = build(1u); // APPENDABLE (iron/jazzy+)
    if (fin == nullptr || app == nullptr || fin->m_ops == nullptr || app->m_ops == nullptr) {
        std::fprintf(stderr, "build returned null\n");
        return 1;
    }

    // FINAL: the first op is the field ADR, NOT a DLC.
    if (DDS_OP(fin->m_ops[0]) == DDS_OP_DLC) {
        std::fprintf(stderr, "FINAL descriptor unexpectedly starts with DDS_OP_DLC\n");
        return 1;
    }
    // APPENDABLE: the first op IS DDS_OP_DLC — Cyclone reads this as appendable
    // and emits a DHEADER under XCDR2.
    if (DDS_OP(app->m_ops[0]) != DDS_OP_DLC) {
        std::fprintf(stderr, "APPENDABLE descriptor does not start with DDS_OP_DLC (op0=0x%08x)\n",
                     app->m_ops[0]);
        return 1;
    }
    // The DLC is a real extra op word.
    if (app->m_nops != fin->m_nops + 1) {
        std::fprintf(stderr, "expected appendable nops = final nops + 1, got %u vs %u\n",
                     app->m_nops, fin->m_nops);
        return 1;
    }

    // Nested case (the #0267 trigger): a struct-in-struct. FINAL has no DLC;
    // APPENDABLE has one per aggregate — the top-level AND the nested body — so
    // Cyclone wraps EACH in a DHEADER under XCDR2.
    const auto* nfin = build_nested(0u);
    const auto* napp = build_nested(1u);
    if (nfin == nullptr || napp == nullptr) {
        std::fprintf(stderr, "nested build returned null\n");
        return 1;
    }
    if (count_dlc(nfin) != 0) {
        std::fprintf(stderr, "FINAL nested has %u DLC(s), expected 0\n", count_dlc(nfin));
        return 1;
    }
    if (count_dlc(napp) != 2) {
        std::fprintf(stderr, "APPENDABLE nested has %u DLC(s), expected 2 (top + nested body)\n",
                     count_dlc(napp));
        return 1;
    }

    std::printf(
        "OK final op0=0x%08x appendable op0=0x%08x (DLC); nested DLCs final=0 appendable=2\n",
        fin->m_ops[0], app->m_ops[0]);
    return 0;
}
