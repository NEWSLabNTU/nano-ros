// Phase 212.K.7.4.c — op-word audit for sequence-of-nested + array-of-nested
// + bounded-sequence-of-nested + EXT-fix-width emission.
//
// Builds a descriptor for an `action_msgs::msg::dds_::CancelGoal_Response_`-shaped
// schema by hand (without going through nros-serdes' static-schema walker) and
// asserts the exact op-word values at known offsets, then hands the descriptor
// to `dds_create_topic` on a live Cyclone participant to prove the walker
// accepts it without aborting on the recursive `dds_stream_countops`.

#include <cstdint>
#include <cstdio>

#include <dds/dds.h>
#include <dds/ddsc/dds_opcodes.h>
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

constexpr uint8_t kKindInt8 = 2;
constexpr uint8_t kKindInt32 = 6;
constexpr uint8_t kKindUint32 = 5;
constexpr uint8_t kKindNested = 15;
constexpr uint8_t kKindArray = 16;
constexpr uint8_t kKindSequence = 17;
constexpr uint8_t kKindBoundedSequence = 18;

#define EXPECT(cond, fmt, ...)                                                                     \
    do {                                                                                           \
        if (!(cond)) {                                                                             \
            std::fprintf(stderr, "FAIL %s:%d " fmt "\n", __FILE__, __LINE__, ##__VA_ARGS__);       \
            return 1;                                                                              \
        }                                                                                          \
    } while (0)

// ── Test 1: sequence<NestedT> in a top-level message ────────────────────
//
// Schema mirrors CancelGoal_Response:
//   message {
//     int8 return_code;            // offset 0
//     sequence<NestedT> goals_canceling; // offset 8 (kSeqSize = 24)
//   }
//   NestedT { uint32 x; uint32 y; }
int test_sequence_of_nested() {
    NrosFieldKindDescriptor kinds[] = {
        // kinds[0] — int8 (return_code)
        {kKindInt8, {0, 0, 0}, 0, 0, nullptr},
        // kinds[1] — sequence<kinds[2]>
        {kKindSequence, {0, 0, 0}, 0, 2, nullptr},
        // kinds[2] — NestedT (first child idx = 3, bound = 2)
        {kKindNested, {0, 0, 0}, 2, 3, "test_msgs/msg/NestedT"},
        // kinds[3] — NestedT.x (uint32)
        {kKindUint32, {0, 0, 0}, 0, 0, nullptr},
        // kinds[4] — NestedT.y (uint32)
        {kKindUint32, {0, 0, 0}, 0, 0, nullptr},
    };
    NrosFieldDescriptor fields[] = {
        {"return_code", 0, 0},
        {"goals_canceling", 8, 1},
    };

    int err = 0;
    const void* raw =
        nros_cyclonedds_build_descriptor_from_schema("test_msgs/msg/CancelGoalLike", fields, 2,
                                                     kinds, 5, &err);
    EXPECT(raw != nullptr, "bridge returned NULL, err=%d", err);
    const auto* desc = static_cast<const dds_topic_descriptor_t*>(raw);

    EXPECT(desc->m_ops != nullptr, "m_ops is NULL");
    EXPECT(desc->m_nops >= 9, "m_nops too small: %u", desc->m_nops);

    const uint32_t* ops = desc->m_ops;

    // Word 0: ADR | <int8> (FLAG_SGN not emitted by our bridge today;
    // value is INT8 → DDS_OP_VAL_1BY shifted into type slot).
    uint32_t expected_int8 = DDS_OP_ADR | (uint32_t(DDS_OP_VAL_1BY) << 16);
    EXPECT(ops[0] == expected_int8, "ops[0] expected 0x%08x got 0x%08x", expected_int8, ops[0]);
    // Word 1: return_code offset = 0.
    EXPECT(ops[1] == 0u, "ops[1] expected 0 got %u", ops[1]);

    // Word 2: ADR | SEQ | SUBTYPE_STU.
    uint32_t expected_seq_stu =
        DDS_OP_ADR | DDS_OP_TYPE_SEQ | (uint32_t(DDS_OP_VAL_STU) << 8);
    EXPECT(ops[2] == expected_seq_stu, "ops[2] expected 0x%08x got 0x%08x", expected_seq_stu,
           ops[2]);
    // Word 3: offset 8.
    EXPECT(ops[3] == 8u, "ops[3] expected 8 got %u", ops[3]);
    // Word 4: elem-size — NestedT is {u32, u32} = 8 bytes.
    EXPECT(ops[4] == 8u, "ops[4] (elem-size) expected 8 got %u", ops[4]);

    // Word 5: link word — high16 = next_insn = 4, low16 = jsr-delta
    // (signed int16 from opcode word 2). The nested body must be
    // placed after the top-level RTS, so jsr > 0.
    uint32_t link = ops[5];
    uint32_t next_insn = link >> 16;
    int16_t jsr_delta = static_cast<int16_t>(link & 0xffffu);
    EXPECT(next_insn == 4u, "link.next_insn expected 4 got %u", next_insn);
    EXPECT(jsr_delta > 0, "jsr_delta should be positive (forward jump), got %d", int(jsr_delta));

    // Word 6: top-level RTS.
    EXPECT(ops[6] == DDS_OP_RTS, "ops[6] expected RTS (0) got 0x%08x", ops[6]);

    // Walk to the nested body — `opcode_word + jsr_delta` = `2 + jsr` words.
    size_t nested_start = 2 + size_t(jsr_delta);
    EXPECT(nested_start < desc->m_nops, "jsr target out of range: %zu vs nops=%u", nested_start,
           desc->m_nops);
    // First nested child: ADR | INT32-typed primitive at offset 0
    // (offset is synth-inside the nested body; both u32 children sit
    // consecutively).
    uint32_t expected_u32 = DDS_OP_ADR | (uint32_t(DDS_OP_VAL_4BY) << 16);
    EXPECT(ops[nested_start] == expected_u32,
           "nested[0] expected 0x%08x (ADR|4BY) got 0x%08x", expected_u32, ops[nested_start]);

    // Hand it to Cyclone — recursive countops MUST NOT abort.
    dds_entity_t pp = dds_create_participant(99, nullptr, nullptr);
    EXPECT(pp >= 0, "dds_create_participant failed: %d", int(pp));
    dds_entity_t topic =
        dds_create_topic(pp, desc, "rt/seq_of_nested_audit", nullptr, nullptr);
    EXPECT(topic >= 0, "dds_create_topic failed: %d", int(topic));
    (void)dds_delete(pp);

    std::printf("OK sequence_of_nested ops[0..6]={0x%08x %u 0x%08x %u %u (link next=%u jsr=%d) "
                "0x%08x} nops=%u\n",
                ops[0], ops[1], ops[2], ops[3], ops[4], next_insn, int(jsr_delta), ops[6],
                desc->m_nops);
    return 0;
}

// ── Test 2: array<NestedT, 3> in a top-level message ────────────────────
//
// Cyclone shape per dds_opcodes.h:253 — [ADR ARR STU] [offset] [alen]
// [link] [elem-size], width 5, link's high16 = next_insn=5.
int test_array_of_nested() {
    NrosFieldKindDescriptor kinds[] = {
        // kinds[0] — array<kinds[1], 3>
        {kKindArray, {0, 0, 0}, 3, 1, nullptr},
        // kinds[1] — NestedT (first child idx = 2, bound = 2)
        {kKindNested, {0, 0, 0}, 2, 2, "test_msgs/msg/NestedT"},
        // kinds[2,3] — u32 + u32
        {kKindUint32, {0, 0, 0}, 0, 0, nullptr},
        {kKindUint32, {0, 0, 0}, 0, 0, nullptr},
    };
    NrosFieldDescriptor fields[] = {
        {"arr_field", 0, 0},
    };

    int err = 0;
    const void* raw = nros_cyclonedds_build_descriptor_from_schema(
        "test_msgs/msg/ArrayOfNested", fields, 1, kinds, 4, &err);
    EXPECT(raw != nullptr, "bridge returned NULL, err=%d", err);
    const auto* desc = static_cast<const dds_topic_descriptor_t*>(raw);

    const uint32_t* ops = desc->m_ops;

    // Word 0: ADR | ARR | SUBTYPE_STU.
    uint32_t expected_arr_stu =
        DDS_OP_ADR | DDS_OP_TYPE_ARR | (uint32_t(DDS_OP_VAL_STU) << 8);
    EXPECT(ops[0] == expected_arr_stu, "ops[0] expected 0x%08x got 0x%08x", expected_arr_stu,
           ops[0]);
    EXPECT(ops[1] == 0u, "offset expected 0 got %u", ops[1]);
    EXPECT(ops[2] == 3u, "alen expected 3 got %u", ops[2]);

    // Word 3: link (next_insn = 5 for ARR|STU, jsr forward).
    uint32_t link = ops[3];
    uint32_t next_insn = link >> 16;
    int16_t jsr_delta = static_cast<int16_t>(link & 0xffffu);
    EXPECT(next_insn == 5u, "ARR link.next_insn expected 5 got %u", next_insn);
    EXPECT(jsr_delta > 0, "ARR jsr_delta expected positive got %d", int(jsr_delta));

    // Word 4: elem-size = 8 (u32+u32 nested).
    EXPECT(ops[4] == 8u, "ARR elem-size expected 8 got %u", ops[4]);

    // Word 5: RTS.
    EXPECT(ops[5] == DDS_OP_RTS, "ARR RTS expected at ops[5] got 0x%08x", ops[5]);

    dds_entity_t pp = dds_create_participant(99, nullptr, nullptr);
    EXPECT(pp >= 0, "participant: %d", int(pp));
    dds_entity_t topic = dds_create_topic(pp, desc, "rt/arr_of_nested_audit", nullptr, nullptr);
    EXPECT(topic >= 0, "topic: %d", int(topic));
    (void)dds_delete(pp);

    std::printf("OK array_of_nested ops[0..5]={0x%08x %u %u (link next=%u jsr=%d) %u 0x%08x}\n",
                ops[0], ops[1], ops[2], next_insn, int(jsr_delta), ops[4], ops[5]);
    return 0;
}

// ── Test 3: bounded sequence<NestedT, 4> in a top-level message ─────────
//
// Cyclone shape per dds_opcodes.h:243 — [ADR BSQ STU] [offset] [sbound]
// [elem-size] [link], width 5, link's high16 = next_insn=5.
int test_bsq_of_nested() {
    NrosFieldKindDescriptor kinds[] = {
        // kinds[0] — bounded_sequence<kinds[1], 4>
        {kKindBoundedSequence, {0, 0, 0}, 4, 1, nullptr},
        // kinds[1] — NestedT (2 children at idx 2..3)
        {kKindNested, {0, 0, 0}, 2, 2, "test_msgs/msg/NestedT"},
        {kKindUint32, {0, 0, 0}, 0, 0, nullptr},
        {kKindUint32, {0, 0, 0}, 0, 0, nullptr},
    };
    NrosFieldDescriptor fields[] = {
        {"bseq_field", 0, 0},
    };

    int err = 0;
    const void* raw = nros_cyclonedds_build_descriptor_from_schema(
        "test_msgs/msg/BsqOfNested", fields, 1, kinds, 4, &err);
    EXPECT(raw != nullptr, "bridge returned NULL, err=%d", err);
    const auto* desc = static_cast<const dds_topic_descriptor_t*>(raw);

    const uint32_t* ops = desc->m_ops;

    uint32_t expected_bsq_stu =
        DDS_OP_ADR | DDS_OP_TYPE_BSQ | (uint32_t(DDS_OP_VAL_STU) << 8);
    EXPECT(ops[0] == expected_bsq_stu, "ops[0] expected 0x%08x got 0x%08x", expected_bsq_stu,
           ops[0]);
    EXPECT(ops[1] == 0u, "offset expected 0 got %u", ops[1]);
    EXPECT(ops[2] == 4u, "sbound expected 4 got %u", ops[2]);
    EXPECT(ops[3] == 8u, "elem-size expected 8 got %u", ops[3]);

    // Word 4: link.
    uint32_t link = ops[4];
    uint32_t next_insn = link >> 16;
    int16_t jsr_delta = static_cast<int16_t>(link & 0xffffu);
    EXPECT(next_insn == 5u, "BSQ link.next_insn expected 5 got %u", next_insn);
    EXPECT(jsr_delta > 0, "BSQ jsr_delta expected positive got %d", int(jsr_delta));

    EXPECT(ops[5] == DDS_OP_RTS, "BSQ RTS expected at ops[5] got 0x%08x", ops[5]);

    dds_entity_t pp = dds_create_participant(99, nullptr, nullptr);
    EXPECT(pp >= 0, "participant: %d", int(pp));
    dds_entity_t topic = dds_create_topic(pp, desc, "rt/bsq_of_nested_audit", nullptr, nullptr);
    EXPECT(topic >= 0, "topic: %d", int(topic));
    (void)dds_delete(pp);

    std::printf("OK bsq_of_nested\n");
    return 0;
}

// ── Test 4: EXT 3-word emission fix ─────────────────────────────────────
//
// Pre-K.7.4.c, EXT emitted 4 words (opcode + offset + link + stray 0),
// and the walker would mis-read the stray 0 as a follow-on opcode when
// EXT wasn't the last top-level insn. The fix: EXT now emits 3 words
// matching `dds_opcodes.h:267` + walker step `ops += jmp ? jmp : 3;`
// in `ddsi_cdrstream.c:798`.
//
// Schema: { Nested n1; int32 trailing; } — the trailing primitive sits
// at the slot that would have collided with the stray word.
int test_ext_three_word_emission() {
    NrosFieldKindDescriptor kinds[] = {
        // kinds[0] — Nested (first child idx = 2, bound = 1)
        {kKindNested, {0, 0, 0}, 1, 2, "test_msgs/msg/Inner"},
        // kinds[1] — int32 (trailing field)
        {kKindInt32, {0, 0, 0}, 0, 0, nullptr},
        // kinds[2] — Inner.x (int32)
        {kKindInt32, {0, 0, 0}, 0, 0, nullptr},
    };
    NrosFieldDescriptor fields[] = {
        {"n1", 0, 0},
        {"trailing", 16, 1},
    };

    int err = 0;
    const void* raw = nros_cyclonedds_build_descriptor_from_schema(
        "test_msgs/msg/ExtTrail", fields, 2, kinds, 3, &err);
    EXPECT(raw != nullptr, "bridge returned NULL, err=%d", err);
    const auto* desc = static_cast<const dds_topic_descriptor_t*>(raw);

    const uint32_t* ops = desc->m_ops;
    // Word 0: ADR | EXT.
    EXPECT(ops[0] == (DDS_OP_ADR | DDS_OP_TYPE_EXT), "ops[0] EXT expected got 0x%08x", ops[0]);
    EXPECT(ops[1] == 0u, "EXT offset");
    // Word 2: link — next_insn=3 in high16.
    uint32_t link = ops[2];
    uint32_t next_insn = link >> 16;
    EXPECT(next_insn == 3u, "EXT next_insn expected 3 got %u", next_insn);

    // The trailing int32 MUST sit at word 3, NOT word 4. Pre-fix, this
    // would have been at word 4 with a stray 0 at word 3.
    uint32_t expected_int32 = DDS_OP_ADR | (uint32_t(DDS_OP_VAL_4BY) << 16);
    EXPECT(ops[3] == expected_int32, "trailing int32 expected 0x%08x at ops[3] got 0x%08x",
           expected_int32, ops[3]);
    EXPECT(ops[4] == 16u, "trailing offset expected 16 got %u", ops[4]);
    EXPECT(ops[5] == DDS_OP_RTS, "RTS expected at ops[5] got 0x%08x", ops[5]);

    dds_entity_t pp = dds_create_participant(99, nullptr, nullptr);
    EXPECT(pp >= 0, "participant: %d", int(pp));
    dds_entity_t topic = dds_create_topic(pp, desc, "rt/ext_trail_audit", nullptr, nullptr);
    EXPECT(topic >= 0, "topic: %d", int(topic));
    (void)dds_delete(pp);

    std::printf("OK ext_three_word_emission (trailing field at ops[3] not ops[4])\n");
    return 0;
}

} // namespace

int main() {
    int rc = test_sequence_of_nested();
    if (rc != 0) return rc;
    rc = test_array_of_nested();
    if (rc != 0) return rc;
    rc = test_bsq_of_nested();
    if (rc != 0) return rc;
    rc = test_ext_three_word_emission();
    if (rc != 0) return rc;
    std::printf("ALL OK\n");
    return 0;
}
