// Phase 212.K.7.4.b — C++ bridge for the Rust DescriptorBuilder.
//
// `dynamic_type.rs` flattens a Rust `nros_serdes::Message` field
// schema into a pair of ABI-stable arrays (`NrosFieldDescriptor[]` +
// `NrosFieldKindDescriptor[]`) and calls
// `nros_cyclonedds_build_descriptor_from_schema` (this file) to turn
// them into a Cyclone DDS `dds_topic_descriptor_t *`.
//
// ── Implementation choice ───────────────────────────────────────────
//
// Cyclone DDS 0.10.5 (our current pin) does **not** expose any
// `ddsi_dynamic_type_*` API surface — that machinery only lands in
// the `master` line behind `DDSI_INCLUDE_DYNAMIC_TYPES` and only
// becomes stable in 0.11+. We deliberately do not bump the pin
// (matches `ros-humble-cyclonedds` 0.10.5).
//
// Instead, this bridge **synthesises a complete `dds_topic_descriptor_t`
// by hand** — exactly the shape that Cyclone's IDL→C codegen
// (`idlc`) would emit for a static descriptor — and hands the pointer
// back to the Rust side. The descriptor is then registered in the
// existing `descriptors.cpp` table via
// `nros_rmw_cyclonedds_register_descriptor`, after which
// `publisher.cpp`, `subscriber.cpp` and `service.cpp` resolve it
// through `find_descriptor(type_name)` and pass it straight to
// `dds_create_topic`, `dds_stream_read_sample`,
// `dds_stream_write_sample`, `dds_stream_free_sample` — none of
// which need any internal API beyond the published `m_size`,
// `m_align`, `m_flagset`, `m_ops`, `m_keys`, `m_typename` fields.
//
// We chose this over reaching into Cyclone's internal headers
// (path 1 in the task spec) because:
//
//  * It only depends on Cyclone's *public* API (`dds_opcodes.h` +
//    `dds_public_impl.h` + `ddsrt/heap.h`). No private headers
//    pulled, no layering violation.
//  * It produces the *same* `dds_topic_descriptor_t` shape `idlc`
//    emits → every consumer in this crate (publisher / subscriber /
//    service / `SertypeMin`) keeps working unmodified.
//  * It avoids the partial `sertype_min` shortcut: that builder only
//    populates a `ddsi_sertype_default` for the publisher / subscriber
//    CDR-stream helpers, but `dds_create_topic` itself still needs a
//    full `dds_topic_descriptor_t`. A real descriptor satisfies both
//    in one go.
//
// ── Encoding ─────────────────────────────────────────────────────────
//
// For each top-level field we emit a sequence of `uint32_t` op-codes
// per the format documented in `dds/ddsc/dds_opcodes.h`:
//
//   * primitive          → `DDS_OP_ADR | DDS_OP_TYPE_<X>` + offset
//   * string             → `DDS_OP_ADR | DDS_OP_TYPE_STR` + offset
//   * bounded string     → `DDS_OP_ADR | DDS_OP_TYPE_BST` + offset + (bound+1)
//   * primitive array    → `DDS_OP_ADR | DDS_OP_TYPE_ARR | DDS_OP_SUBTYPE_<X>` + offset + N
//   * primitive sequence → `DDS_OP_ADR | DDS_OP_TYPE_SEQ | DDS_OP_SUBTYPE_<X>` + offset
//   * bounded primitive  → `DDS_OP_ADR | DDS_OP_TYPE_BSQ | DDS_OP_SUBTYPE_<X>` + offset + N
//     sequence
//   * nested struct      → `DDS_OP_ADR | DDS_OP_TYPE_EXT` + offset
//                          + jsr-delta-into-child-ops + 0
//
// The top-level op stream terminates in `DDS_OP_RTS`. Child struct
// op streams are appended after `DDS_OP_RTS` and reached via the JSR
// delta in the `EXT` op's flag word.
//
// ── Wide strings ─────────────────────────────────────────────────────
//
// Cyclone 0.10.5 does **not** ship a WString opcode. `WString` and
// `BoundedWString` field kinds therefore surface as
// `NROS_BRIDGE_ERR_UNSUPPORTED_FIELD_TYPE` — the registry surfaces
// this as `BuildError::UnsupportedFieldType` to the caller. ROS
// hardly ever uses w-strings on the wire so this is acceptable until
// the Cyclone pin moves.

#include <stddef.h>
#include <stdint.h>
#include <string.h>

#include <dds/dds.h>
#include <dds/ddsc/dds_opcodes.h>
#include <dds/ddsc/dds_public_impl.h>
#include <dds/ddsrt/heap.h>
#include <dds/ddsrt/string.h>

extern "C" {

// Mirror of `crate::bridge::NrosFieldDescriptor`.
struct NrosFieldDescriptor {
    const char* name;
    uint32_t offset;
    uint32_t kind;
};

// Mirror of `crate::bridge::NrosFieldKindDescriptor`.
struct NrosFieldKindDescriptor {
    uint8_t kind;
    uint8_t _pad[3];
    uint32_t bound;
    uint32_t inner;
    const char* nested_name;
};

// Mirror of `crate::bridge::BridgeError`.
enum NrosBridgeError {
    NROS_BRIDGE_ERR_NESTED_DEPTH_EXCEEDED = -1001,
    NROS_BRIDGE_ERR_UNSUPPORTED_FIELD_TYPE = -1002,
    NROS_BRIDGE_ERR_NULL_POINTER = -1003,
    NROS_BRIDGE_ERR_EMPTY_SCHEMA = -1004,
};

// Mirror of `crate::bridge::FieldKind` (kept in sync by hand — the
// Rust shim emits these tag values directly).
enum NrosFieldKind : uint8_t {
    NROS_FIELD_KIND_BOOL = 0,
    NROS_FIELD_KIND_UINT8 = 1,
    NROS_FIELD_KIND_INT8 = 2,
    NROS_FIELD_KIND_UINT16 = 3,
    NROS_FIELD_KIND_INT16 = 4,
    NROS_FIELD_KIND_UINT32 = 5,
    NROS_FIELD_KIND_INT32 = 6,
    NROS_FIELD_KIND_UINT64 = 7,
    NROS_FIELD_KIND_INT64 = 8,
    NROS_FIELD_KIND_FLOAT32 = 9,
    NROS_FIELD_KIND_FLOAT64 = 10,
    NROS_FIELD_KIND_STRING = 11,
    NROS_FIELD_KIND_WSTRING = 12,
    NROS_FIELD_KIND_BOUNDED_STRING = 13,
    NROS_FIELD_KIND_BOUNDED_WSTRING = 14,
    NROS_FIELD_KIND_NESTED = 15,
    NROS_FIELD_KIND_ARRAY = 16,
    NROS_FIELD_KIND_SEQUENCE = 17,
    NROS_FIELD_KIND_BOUNDED_SEQUENCE = 18,
};

} // extern "C"

namespace {

// ── Per-field size / alignment helpers ───────────────────────────────
//
// These mirror the in-memory layout that `dds_create_topic` /
// `dds_stream_*` assume for a given opcode subtype: 1, 2, 4 or 8
// bytes for primitives; pointer-sized for strings; `{ uint32_t,
// uint32_t, T* }` for sequences; sizeof(struct) for nested.
//
// We only ever use these to compute `m_size` / `m_align` so they
// don't need to match the host's actual struct layout exactly — they
// just have to be self-consistent: every offset the caller gave us
// has to lie inside the synthesised struct extent. The Rust walker
// passes offsets that already encode the host layout, so we just
// compute `max(offset + field_size)` and pick a reasonable alignment.

// Stable Cyclone-internal sequence header (all `dds_*_seq` IDL
// typedefs share this layout: `_maximum`, `_length`, `_buffer`,
// `_release` — see dds.h `DDS_SEQUENCE`).
struct dds_seq_layout {
    uint32_t _maximum;
    uint32_t _length;
    void* _buffer;
    bool _release;
};

constexpr uint32_t kSeqSize = sizeof(dds_seq_layout);
constexpr uint32_t kSeqAlign = alignof(dds_seq_layout);
constexpr uint32_t kStringSize = sizeof(char*);
constexpr uint32_t kStringAlign = alignof(char*);

bool primitive_size_align(uint8_t kind, uint32_t& size, uint32_t& align) {
    switch (kind) {
    case NROS_FIELD_KIND_BOOL:
    case NROS_FIELD_KIND_UINT8:
    case NROS_FIELD_KIND_INT8:
        size = 1;
        align = 1;
        return true;
    case NROS_FIELD_KIND_UINT16:
    case NROS_FIELD_KIND_INT16:
        size = 2;
        align = 2;
        return true;
    case NROS_FIELD_KIND_UINT32:
    case NROS_FIELD_KIND_INT32:
    case NROS_FIELD_KIND_FLOAT32:
        size = 4;
        align = 4;
        return true;
    case NROS_FIELD_KIND_UINT64:
    case NROS_FIELD_KIND_INT64:
    case NROS_FIELD_KIND_FLOAT64:
        size = 8;
        align = 8;
        return true;
    default:
        return false;
    }
}

// `DDS_OP_VAL_*` subtype tag for a primitive `NROS_FIELD_KIND_*`.
// Returns false if the kind is not a primitive Cyclone-0.10.5
// recognises as a stream element.
bool primitive_subtype(uint8_t kind, uint32_t& subtype) {
    switch (kind) {
    case NROS_FIELD_KIND_BOOL:
        subtype = DDS_OP_VAL_BLN;
        return true;
    case NROS_FIELD_KIND_UINT8:
    case NROS_FIELD_KIND_INT8:
        subtype = DDS_OP_VAL_1BY;
        return true;
    case NROS_FIELD_KIND_UINT16:
    case NROS_FIELD_KIND_INT16:
        subtype = DDS_OP_VAL_2BY;
        return true;
    case NROS_FIELD_KIND_UINT32:
    case NROS_FIELD_KIND_INT32:
    case NROS_FIELD_KIND_FLOAT32:
        subtype = DDS_OP_VAL_4BY;
        return true;
    case NROS_FIELD_KIND_UINT64:
    case NROS_FIELD_KIND_INT64:
    case NROS_FIELD_KIND_FLOAT64:
        subtype = DDS_OP_VAL_8BY;
        return true;
    case NROS_FIELD_KIND_STRING:
        subtype = DDS_OP_VAL_STR;
        return true;
    default:
        return false;
    }
}

// Equivalent `DDS_OP_TYPE_*` for a primitive kind. Same set as
// `primitive_subtype`, but shifted into the type slot.
bool primitive_type(uint8_t kind, uint32_t& type) {
    uint32_t st = 0;
    if (!primitive_subtype(kind, st)) return false;
    type = st << 16;
    return true;
}

// ── Type-name mangling ──────────────────────────────────────────────
//
// `pkg/msg/Name` → `pkg::msg::dds_::Name_`
// `pkg/srv/Name_Request` → `pkg::srv::dds_::Name_Request_`
// Names already in mangled form (containing `::`) pass through.
//
// Returns the malloc'd mangled copy (caller frees with `ddsrt_free`),
// or `nullptr` on allocation failure.
char* mangle_type_name(const char* raw) {
    if (raw == nullptr) return nullptr;
    if (strstr(raw, "::") != nullptr) {
        return ddsrt_strdup(raw);
    }
    const size_t raw_len = strlen(raw);
    // We need to find the **last two** `/` separators and replace
    // both with `::`, then inject `dds_::` before the leaf and append
    // a trailing `_`. Worst-case expansion: 2× (`/`=1 → `::`=2) +
    // `dds_::` (6) + trailing `_` (1) = +9 bytes.
    size_t out_cap = raw_len + 16;
    char* out = static_cast<char*>(ddsrt_malloc(out_cap));
    if (out == nullptr) return nullptr;

    // Locate last two slashes.
    const char* last_slash = nullptr;
    const char* prev_slash = nullptr;
    for (const char* p = raw; *p != 0; ++p) {
        if (*p == '/') {
            prev_slash = last_slash;
            last_slash = p;
        }
    }
    if (last_slash == nullptr) {
        // Bare name → return as-is.
        memcpy(out, raw, raw_len);
        out[raw_len] = 0;
        return out;
    }
    // Determine leaf bounds.
    size_t pos = 0;
    auto emit = [&](const char* p, size_t n) {
        if (pos + n + 1 > out_cap) return;
        memcpy(out + pos, p, n);
        pos += n;
    };
    if (prev_slash != nullptr) {
        // [pkg]/[middle]/[leaf]
        emit(raw, static_cast<size_t>(prev_slash - raw));
        emit("::", 2);
        emit(prev_slash + 1, static_cast<size_t>(last_slash - prev_slash - 1));
        emit("::", 2);
    } else {
        // [pkg]/[leaf]
        emit(raw, static_cast<size_t>(last_slash - raw));
        emit("::", 2);
    }
    emit("dds_::", 6);
    emit(last_slash + 1, raw_len - static_cast<size_t>(last_slash - raw) - 1);
    emit("_", 1);
    out[pos] = 0;
    return out;
}

// ── Bounded op-stream + storage extents ──────────────────────────────
//
// The compiled `m_ops[]` is a single `uint32_t[]` of bounded size:
// every top-level field consumes up to 5 words (opcode + offset +
// up to three extras for EXT / BST / BSQ); nested struct ops follow
// after the top-level `RTS`.
//
// `MAX_OPS` caps total compiled words. Generous enough for several
// hundred fields including a few nested structs.
constexpr size_t kMaxOpsWords = 4096;

struct OpsBuilder {
    uint32_t buf[kMaxOpsWords]{};
    size_t len = 0;

    bool push(uint32_t w) {
        if (len >= kMaxOpsWords) return false;
        buf[len++] = w;
        return true;
    }
};

// Forward decl — `emit_kind_block` is the single entry point used by
// both `emit_nested_body` (per-child) and the top-level walk in
// `nros_cyclonedds_build_descriptor_from_schema`.
bool emit_kind_block(OpsBuilder& ops, uint32_t kind_idx, const NrosFieldKindDescriptor* kinds,
                     uint32_t kind_count, uint32_t offset, int* out_err);

// Walk the synthetic top-level struct. After the top-level fields
// have emitted their ops + DDS_OP_RTS, the nested type bodies are
// appended in `kinds[]` order using `kind_idx` as the recursion root.
//
// Records (`emit_jsr_patch`) the absolute word offsets that need to
// be backfilled with the actual JSR delta once the nested body is
// emitted. This keeps the encoder single-pass.

// Phase 212.K.7.4.c — extended to support multiple Cyclone opcode shapes:
//   * EXT          → 3-word insn, jsr-delta + next-insn at offset +2 from opcode.
//   * SEQ|STU      → 4-word insn, jsr-delta + next-insn at offset +3 from opcode.
//   * BSQ|STU      → 5-word insn, jsr-delta + next-insn at offset +4 from opcode.
//   * ARR|STU      → 5-word insn, jsr-delta + next-insn at offset +3 from opcode
//                    (elem-size after the link, per `dds_opcodes.h:253`).
//
// `opcode_word` carries the absolute word index of the opcode itself —
// the backfill loop computes `delta = target_word - opcode_word` and
// writes the packed `(next_insn << 16) | (delta & 0xffff)` into
// `link_word`. `next_insn` is the constant insn width per shape (3 for
// EXT, 4 for SEQ, 5 for BSQ/ARR).
struct JsrPatch {
    size_t opcode_word;       // absolute word index of the opcode word
    size_t link_word;         // absolute word index of the link slot to backfill
    uint32_t target_kind_idx; // kind index whose body the JSR points to
    uint16_t next_insn;       // constant insn width to bake into the link's high16
};

constexpr size_t kMaxPatches = 256;

struct PatchTable {
    JsrPatch entries[kMaxPatches]{};
    size_t count = 0;

    bool push(size_t opcode_word, size_t link_word, uint32_t target, uint16_t next_insn) {
        if (count >= kMaxPatches) return false;
        entries[count++] = JsrPatch{opcode_word, link_word, target, next_insn};
        return true;
    }
};

// One entry per nested kind that has been (or will be) emitted as a
// child block — records the absolute word offset of that block's
// first op for the JSR-delta computation.
struct NestedOffset {
    uint32_t kind_idx;
    size_t ops_word;
};

constexpr size_t kMaxNestedBlocks = 64;

struct NestedTable {
    NestedOffset entries[kMaxNestedBlocks]{};
    size_t count = 0;

    bool push(uint32_t kind, size_t word) {
        if (count >= kMaxNestedBlocks) return false;
        entries[count++] = NestedOffset{kind, word};
        return true;
    }

    // Returns `size_t(-1)` if not yet emitted.
    size_t find(uint32_t kind) const {
        for (size_t i = 0; i < count; ++i) {
            if (entries[i].kind_idx == kind) return entries[i].ops_word;
        }
        return static_cast<size_t>(-1);
    }
};

// Build the entire ops stream (top-level fields + nested bodies).
//
// `field_kinds_emit_queue` carries the set of nested struct kind
// indices that still need their body emitted; we process them
// breadth-first and patch up the JSR deltas as we go.
struct BuildContext {
    OpsBuilder ops;
    PatchTable patches;
    NestedTable nested;
    uint8_t queue[kMaxNestedBlocks]{};
    size_t queue_head = 0;
    size_t queue_len = 0;
    int err = 0;

    bool enqueue(uint32_t kind_idx) {
        if (queue_len >= kMaxNestedBlocks) {
            err = NROS_BRIDGE_ERR_NESTED_DEPTH_EXCEEDED;
            return false;
        }
        // Dedup — same kind shouldn't be emitted twice.
        for (size_t i = queue_head; i < queue_len; ++i) {
            if (queue[i] == kind_idx) return true;
        }
        if (nested.find(kind_idx) != static_cast<size_t>(-1)) return true;
        queue[queue_len++] = static_cast<uint8_t>(kind_idx);
        return true;
    }
};

// Phase 212.K.7.4.c — compute the synthetic in-memory size of a nested
// struct kind, used by SEQ|STU / BSQ|STU / ARR|STU element-size slot
// and by `emit_nested_body`'s own offset walk for child placement.
//
// Mirrors `emit_nested_body`'s per-child sizing decisions byte-for-byte
// (any drift would corrupt the walker's `buffer + i * elem_size`
// stride). Recursion guarded by `depth`/`MAX_DEPTH` to bound at-worst
// pathological schemas (the front-end already enforces
// `MAX_NESTED_DEPTH`, this is belt-and-braces).
constexpr uint32_t kMaxNestedSizeDepth = 16;

uint32_t compute_nested_size(uint32_t kind_idx, const NrosFieldKindDescriptor* kinds,
                             uint32_t kind_count, uint32_t depth = 0) {
    if (depth >= kMaxNestedSizeDepth) return 0;
    if (kind_idx >= kind_count) return 0;
    const auto& k = kinds[kind_idx];
    if (k.kind != NROS_FIELD_KIND_NESTED) return 0;
    uint32_t synth_offset = 0;
    uint32_t max_align = 1;
    uint32_t bound_count = k.bound;
    uint32_t first_child = k.inner;
    for (uint32_t i = 0; i < bound_count; ++i) {
        uint32_t child_idx = first_child + i;
        if (child_idx >= kind_count) return 0;
        const auto& c = kinds[child_idx];
        uint32_t size = 0, align = 1;
        bool sized = false;
        switch (c.kind) {
        case NROS_FIELD_KIND_STRING:
        case NROS_FIELD_KIND_BOUNDED_STRING:
            size = kStringSize;
            align = kStringAlign;
            sized = true;
            break;
        case NROS_FIELD_KIND_SEQUENCE:
        case NROS_FIELD_KIND_BOUNDED_SEQUENCE:
            size = kSeqSize;
            align = kSeqAlign;
            sized = true;
            break;
        case NROS_FIELD_KIND_ARRAY: {
            if (c.inner >= kind_count) break;
            const auto& elem = kinds[c.inner];
            uint32_t esize = 0, ealign = 1;
            if (primitive_size_align(elem.kind, esize, ealign)) {
                size = esize * c.bound;
                align = ealign;
                sized = true;
            } else if (elem.kind == NROS_FIELD_KIND_NESTED) {
                uint32_t nsize = compute_nested_size(c.inner, kinds, kind_count, depth + 1);
                if (nsize == 0) return 0;
                size = nsize * c.bound;
                align = 8;
                sized = true;
            }
            break;
        }
        case NROS_FIELD_KIND_NESTED: {
            uint32_t nsize = compute_nested_size(child_idx, kinds, kind_count, depth + 1);
            if (nsize == 0) return 0;
            size = nsize;
            align = 8;
            sized = true;
            break;
        }
        default:
            if (primitive_size_align(c.kind, size, align)) {
                sized = true;
            }
            break;
        }
        if (!sized) return 0;
        synth_offset = (synth_offset + align - 1) & ~(align - 1);
        synth_offset += size;
        if (align > max_align) max_align = align;
    }
    // Round final size up to the max child alignment, matching
    // `compute_struct_size`'s discipline so `sizeof(struct)` matches.
    synth_offset = (synth_offset + max_align - 1) & ~(max_align - 1);
    if (synth_offset == 0) synth_offset = max_align;
    return synth_offset;
}

bool emit_nested_body(BuildContext& ctx, uint32_t kind_idx, const NrosFieldKindDescriptor* kinds,
                      uint32_t kind_count) {
    if (kind_idx >= kind_count) {
        ctx.err = NROS_BRIDGE_ERR_UNSUPPORTED_FIELD_TYPE;
        return false;
    }
    const auto& k = kinds[kind_idx];
    if (k.kind != NROS_FIELD_KIND_NESTED) {
        ctx.err = NROS_BRIDGE_ERR_UNSUPPORTED_FIELD_TYPE;
        return false;
    }
    // Mark this kind as emitted at the current op offset.
    if (!ctx.nested.push(kind_idx, ctx.ops.len)) {
        ctx.err = NROS_BRIDGE_ERR_NESTED_DEPTH_EXCEEDED;
        return false;
    }
    // Synthesise per-field offsets sequentially (we don't have real
    // struct offsets for child fields in the kinds[] flat table —
    // `kinds[child_idx]` only carries the kind, not the offset).
    // This is OK because Cyclone's op walker uses the offset purely
    // to index into the host struct for (de)serialisation. As long
    // as we tell `m_size` to match the synthetic offsets, it stays
    // self-consistent. Each child's offset is rounded up to its
    // alignment.
    uint32_t synth_offset = 0;
    uint32_t bound_count = k.bound;
    uint32_t first_child = k.inner;
    for (uint32_t i = 0; i < bound_count; ++i) {
        uint32_t child_idx = first_child + i;
        if (child_idx >= kind_count) {
            ctx.err = NROS_BRIDGE_ERR_UNSUPPORTED_FIELD_TYPE;
            return false;
        }
        // Round synth_offset to the child's alignment.
        const auto& c = kinds[child_idx];
        uint32_t size = 0, align = 1;
        bool sized = false;
        switch (c.kind) {
        case NROS_FIELD_KIND_STRING:
        case NROS_FIELD_KIND_BOUNDED_STRING:
            size = kStringSize;
            align = kStringAlign;
            sized = true;
            break;
        case NROS_FIELD_KIND_SEQUENCE:
        case NROS_FIELD_KIND_BOUNDED_SEQUENCE:
            size = kSeqSize;
            align = kSeqAlign;
            sized = true;
            break;
        case NROS_FIELD_KIND_ARRAY: {
            if (c.inner >= kind_count) break;
            const auto& elem = kinds[c.inner];
            uint32_t esize = 0, ealign = 1;
            if (primitive_size_align(elem.kind, esize, ealign)) {
                size = esize * c.bound;
                align = ealign;
                sized = true;
            } else if (elem.kind == NROS_FIELD_KIND_NESTED) {
                // Phase 212.K.7.4.c — derive nested-array stride from
                // the actual nested struct's compute_nested_size rather
                // than the ad-hoc 8-byte placeholder.
                uint32_t nsize = compute_nested_size(c.inner, kinds, kind_count);
                if (nsize > 0) {
                    size = nsize * c.bound;
                    align = 8;
                    sized = true;
                }
            }
            break;
        }
        case NROS_FIELD_KIND_NESTED: {
            // Phase 212.K.7.4.c — actual nested-struct size, not the
            // 8-byte ad-hoc placeholder the old emitter used. Falls
            // back to 8 on failure so the walk doesn't bail (the
            // `emit_kind_block` recursion below will surface the real
            // error if the nested kind is malformed).
            uint32_t nsize = compute_nested_size(child_idx, kinds, kind_count);
            size = nsize > 0 ? nsize : 8;
            align = 8;
            sized = true;
            break;
        }
        default:
            if (primitive_size_align(c.kind, size, align)) {
                sized = true;
            }
            break;
        }
        if (!sized) {
            ctx.err = NROS_BRIDGE_ERR_UNSUPPORTED_FIELD_TYPE;
            return false;
        }
        synth_offset = (synth_offset + align - 1) & ~(align - 1);
        if (!emit_kind_block(ctx.ops, child_idx, kinds, kind_count, synth_offset, &ctx.err)) {
            // Patch up any JSR target this child needs.
            if (kinds[child_idx].kind == NROS_FIELD_KIND_NESTED && ctx.err == 0) {
                // Re-queue + record patch — handled inline below.
            }
            return false;
        }
        // If the child was a nested struct, the emitter pushed an EXT
        // op with a placeholder JSR. We need to record the patch so
        // we can backfill once the nested body is emitted.
        if (kinds[child_idx].kind == NROS_FIELD_KIND_NESTED) {
            uint32_t inner_idx = kinds[child_idx].inner > 0
                                     ? child_idx /* the child itself is the kind to emit */
                                     : child_idx;
            // The JSR patch was already recorded by emit_kind_block via
            // ctx.patches (we add it here directly to keep that helper
            // signature simple).
            // No-op: emit_kind_block records the patch.
            (void)inner_idx;
        }
        synth_offset += size;
    }
    if (!ctx.ops.push(DDS_OP_RTS)) {
        ctx.err = NROS_BRIDGE_ERR_NESTED_DEPTH_EXCEEDED;
        return false;
    }
    return true;
}

// Emit ops for a single field-kind entry at the given byte offset
// (top-level offset, or synthesised offset for a nested struct's
// field). Records JSR patches into a global table — backfilled
// later.
//
// NB: This helper accesses the surrounding `BuildContext` via a
// thread-local pointer to keep the signature simple. Since the
// builder is invoked single-threadedly (the Rust registry mutex is
// held across `build_raw`), this is safe — we just need a reentrant
// way to find the patch table when emitting a top-level field's EXT.
thread_local BuildContext* t_ctx = nullptr;

bool emit_kind_block(OpsBuilder& ops, uint32_t kind_idx, const NrosFieldKindDescriptor* kinds,
                     uint32_t kind_count, uint32_t offset, int* out_err) {
    if (kind_idx >= kind_count) {
        if (out_err) *out_err = NROS_BRIDGE_ERR_UNSUPPORTED_FIELD_TYPE;
        return false;
    }
    const auto& k = kinds[kind_idx];

    switch (k.kind) {
    case NROS_FIELD_KIND_BOOL:
    case NROS_FIELD_KIND_UINT8:
    case NROS_FIELD_KIND_INT8:
    case NROS_FIELD_KIND_UINT16:
    case NROS_FIELD_KIND_INT16:
    case NROS_FIELD_KIND_UINT32:
    case NROS_FIELD_KIND_INT32:
    case NROS_FIELD_KIND_UINT64:
    case NROS_FIELD_KIND_INT64:
    case NROS_FIELD_KIND_FLOAT32:
    case NROS_FIELD_KIND_FLOAT64: {
        uint32_t type = 0;
        if (!primitive_type(k.kind, type)) {
            if (out_err) *out_err = NROS_BRIDGE_ERR_UNSUPPORTED_FIELD_TYPE;
            return false;
        }
        if (!ops.push(DDS_OP_ADR | type)) return false;
        if (!ops.push(offset)) return false;
        return true;
    }
    case NROS_FIELD_KIND_STRING:
        if (!ops.push(DDS_OP_ADR | DDS_OP_TYPE_STR)) return false;
        if (!ops.push(offset)) return false;
        return true;
    case NROS_FIELD_KIND_BOUNDED_STRING:
        if (!ops.push(DDS_OP_ADR | DDS_OP_TYPE_BST)) return false;
        if (!ops.push(offset)) return false;
        // Cyclone stores `bound + 1` (length cap including NUL).
        if (!ops.push(k.bound + 1)) return false;
        return true;
    case NROS_FIELD_KIND_WSTRING:
    case NROS_FIELD_KIND_BOUNDED_WSTRING:
        // Cyclone 0.10.5 has no wstring opcode.
        if (out_err) *out_err = NROS_BRIDGE_ERR_UNSUPPORTED_FIELD_TYPE;
        return false;
    case NROS_FIELD_KIND_ARRAY: {
        if (k.inner >= kind_count) {
            if (out_err) *out_err = NROS_BRIDGE_ERR_UNSUPPORTED_FIELD_TYPE;
            return false;
        }
        const auto& elem = kinds[k.inner];
        uint32_t st = 0;
        if (primitive_subtype(elem.kind, st)) {
            if (!ops.push(DDS_OP_ADR | DDS_OP_TYPE_ARR | (st << 8))) return false;
            if (!ops.push(offset)) return false;
            if (!ops.push(k.bound)) return false;
            return true;
        }
        if (elem.kind == NROS_FIELD_KIND_NESTED) {
            // Phase 212.K.7.4.c — 5-word ARR|SUBTYPE_STU shape from
            // `dds_opcodes.h:253`:
            //   [ADR, ARR, STU, f] [offset] [alen] [link] [elem-size]
            // walker steps `jmp ? jmp : 5` per ARR|s case (cdrstream.c:696).
            if (k.inner >= kind_count) {
                if (out_err) *out_err = NROS_BRIDGE_ERR_UNSUPPORTED_FIELD_TYPE;
                return false;
            }
            uint32_t elem_size = compute_nested_size(k.inner, kinds, kind_count);
            if (elem_size == 0) {
                if (out_err) *out_err = NROS_BRIDGE_ERR_UNSUPPORTED_FIELD_TYPE;
                return false;
            }
            size_t opcode_slot = ops.len;
            if (!ops.push(DDS_OP_ADR | DDS_OP_TYPE_ARR | (DDS_OP_VAL_STU << 8))) return false;
            if (!ops.push(offset)) return false;
            if (!ops.push(k.bound)) return false;
            size_t link_slot = ops.len;
            // (next_insn=5 << 16) | jsr-delta-placeholder.
            if (!ops.push((5u << 16))) return false;
            if (!ops.push(elem_size)) return false;
            if (t_ctx == nullptr) {
                if (out_err) *out_err = NROS_BRIDGE_ERR_NULL_POINTER;
                return false;
            }
            if (!t_ctx->patches.push(opcode_slot, link_slot, k.inner, 5)) {
                if (out_err) *out_err = NROS_BRIDGE_ERR_NESTED_DEPTH_EXCEEDED;
                return false;
            }
            if (!t_ctx->enqueue(k.inner)) {
                if (out_err) *out_err = t_ctx->err;
                return false;
            }
            return true;
        }
        // Other element kinds (string / nested-array / nested-seq) are
        // not yet implemented — surface unsupported.
        if (out_err) *out_err = NROS_BRIDGE_ERR_UNSUPPORTED_FIELD_TYPE;
        return false;
    }
    case NROS_FIELD_KIND_SEQUENCE: {
        if (k.inner >= kind_count) {
            if (out_err) *out_err = NROS_BRIDGE_ERR_UNSUPPORTED_FIELD_TYPE;
            return false;
        }
        const auto& elem = kinds[k.inner];
        uint32_t st = 0;
        if (primitive_subtype(elem.kind, st)) {
            if (!ops.push(DDS_OP_ADR | DDS_OP_TYPE_SEQ | (st << 8))) return false;
            if (!ops.push(offset)) return false;
            return true;
        }
        if (elem.kind == NROS_FIELD_KIND_NESTED) {
            // Phase 212.K.7.4.c — 4-word SEQ|SUBTYPE_STU shape from
            // `dds_opcodes.h:233`:
            //   [ADR, SEQ, STU, f] [offset] [elem-size] [link]
            // walker steps `jmp ? jmp : 4` per SEQ|s case (cdrstream.c:662).
            // Confirmed live by idlc witness in
            // `examples/threadx-linux/cpp/service-client/build-cyclonedds/
            // cyclonedds-ts/_genroot/action_msgs/msg/CancelGoal_Response.c`.
            uint32_t elem_size = compute_nested_size(k.inner, kinds, kind_count);
            if (elem_size == 0) {
                if (out_err) *out_err = NROS_BRIDGE_ERR_UNSUPPORTED_FIELD_TYPE;
                return false;
            }
            size_t opcode_slot = ops.len;
            if (!ops.push(DDS_OP_ADR | DDS_OP_TYPE_SEQ | (DDS_OP_VAL_STU << 8))) return false;
            if (!ops.push(offset)) return false;
            if (!ops.push(elem_size)) return false;
            size_t link_slot = ops.len;
            if (!ops.push((4u << 16))) return false;
            if (t_ctx == nullptr) {
                if (out_err) *out_err = NROS_BRIDGE_ERR_NULL_POINTER;
                return false;
            }
            if (!t_ctx->patches.push(opcode_slot, link_slot, k.inner, 4)) {
                if (out_err) *out_err = NROS_BRIDGE_ERR_NESTED_DEPTH_EXCEEDED;
                return false;
            }
            if (!t_ctx->enqueue(k.inner)) {
                if (out_err) *out_err = t_ctx->err;
                return false;
            }
            return true;
        }
        if (out_err) *out_err = NROS_BRIDGE_ERR_UNSUPPORTED_FIELD_TYPE;
        return false;
    }
    case NROS_FIELD_KIND_BOUNDED_SEQUENCE: {
        if (k.inner >= kind_count) {
            if (out_err) *out_err = NROS_BRIDGE_ERR_UNSUPPORTED_FIELD_TYPE;
            return false;
        }
        const auto& elem = kinds[k.inner];
        uint32_t st = 0;
        if (primitive_subtype(elem.kind, st)) {
            if (!ops.push(DDS_OP_ADR | DDS_OP_TYPE_BSQ | (st << 8))) return false;
            if (!ops.push(offset)) return false;
            if (!ops.push(k.bound)) return false;
            return true;
        }
        if (elem.kind == NROS_FIELD_KIND_NESTED) {
            // Phase 212.K.7.4.c — 5-word BSQ|SUBTYPE_STU shape from
            // `dds_opcodes.h:243`:
            //   [ADR, BSQ, STU, f] [offset] [sbound] [elem-size] [link]
            // walker steps `jmp ? jmp : 5` per BSQ|s case (bound_op=1
            // in cdrstream.c:662).
            uint32_t elem_size = compute_nested_size(k.inner, kinds, kind_count);
            if (elem_size == 0) {
                if (out_err) *out_err = NROS_BRIDGE_ERR_UNSUPPORTED_FIELD_TYPE;
                return false;
            }
            size_t opcode_slot = ops.len;
            if (!ops.push(DDS_OP_ADR | DDS_OP_TYPE_BSQ | (DDS_OP_VAL_STU << 8))) return false;
            if (!ops.push(offset)) return false;
            if (!ops.push(k.bound)) return false;
            if (!ops.push(elem_size)) return false;
            size_t link_slot = ops.len;
            if (!ops.push((5u << 16))) return false;
            if (t_ctx == nullptr) {
                if (out_err) *out_err = NROS_BRIDGE_ERR_NULL_POINTER;
                return false;
            }
            if (!t_ctx->patches.push(opcode_slot, link_slot, k.inner, 5)) {
                if (out_err) *out_err = NROS_BRIDGE_ERR_NESTED_DEPTH_EXCEEDED;
                return false;
            }
            if (!t_ctx->enqueue(k.inner)) {
                if (out_err) *out_err = t_ctx->err;
                return false;
            }
            return true;
        }
        if (out_err) *out_err = NROS_BRIDGE_ERR_UNSUPPORTED_FIELD_TYPE;
        return false;
    }
    case NROS_FIELD_KIND_NESTED: {
        // Phase 212.K.7.4.c — fix EXT to emit 3 words (not 4) per the
        // walker rule `ops += jmp ? jmp : 3;` in
        // `ddsi_cdrstream.c:798` + the shape doc-comment at
        // `dds_opcodes.h:267`. The 4th placeholder word the old
        // emitter wrote was visible to the walker as a stray opcode
        // whenever EXT was not the last top-level field. See
        // `docs/design/0030-sequence-of-nested.md` Risk #2.
        //
        //   [ADR, EXT, 0, f] [offset] [link]
        //   (elem-size only present when DDS_OP_FLAG_EXT external is set)
        size_t opcode_slot = ops.len;
        if (!ops.push(DDS_OP_ADR | DDS_OP_TYPE_EXT)) return false;
        if (!ops.push(offset)) return false;
        size_t link_slot = ops.len;
        if (!ops.push((3u << 16))) return false; // (next_insn=3) | jsr-placeholder
        if (t_ctx == nullptr) {
            if (out_err) *out_err = NROS_BRIDGE_ERR_NULL_POINTER;
            return false;
        }
        if (!t_ctx->patches.push(opcode_slot, link_slot, kind_idx, 3)) {
            if (out_err) *out_err = NROS_BRIDGE_ERR_NESTED_DEPTH_EXCEEDED;
            return false;
        }
        if (!t_ctx->enqueue(kind_idx)) {
            if (out_err) *out_err = t_ctx->err;
            return false;
        }
        return true;
    }
    default:
        if (out_err) *out_err = NROS_BRIDGE_ERR_UNSUPPORTED_FIELD_TYPE;
        return false;
    }
}

// Compute `m_size` from the top-level fields[] table.
uint32_t compute_struct_size(const NrosFieldDescriptor* fields, uint32_t field_count,
                             const NrosFieldKindDescriptor* kinds, uint32_t kind_count,
                             uint32_t& out_align, bool& out_fixed) {
    uint32_t end = 0;
    uint32_t max_align = 1;
    out_fixed = true;
    for (uint32_t i = 0; i < field_count; ++i) {
        const auto& f = fields[i];
        if (f.kind >= kind_count) continue;
        const auto& k = kinds[f.kind];
        uint32_t size = 0, align = 1;
        bool sized = false;
        switch (k.kind) {
        case NROS_FIELD_KIND_STRING:
        case NROS_FIELD_KIND_BOUNDED_STRING:
            size = kStringSize;
            align = kStringAlign;
            sized = true;
            out_fixed = false;
            break;
        case NROS_FIELD_KIND_SEQUENCE:
        case NROS_FIELD_KIND_BOUNDED_SEQUENCE:
            size = kSeqSize;
            align = kSeqAlign;
            sized = true;
            out_fixed = false;
            break;
        case NROS_FIELD_KIND_ARRAY: {
            if (k.inner < kind_count) {
                const auto& elem = kinds[k.inner];
                uint32_t esize = 0, ealign = 1;
                if (primitive_size_align(elem.kind, esize, ealign)) {
                    size = esize * k.bound;
                    align = ealign;
                    sized = true;
                }
            }
            break;
        }
        case NROS_FIELD_KIND_NESTED:
            size = 16; // conservative placeholder
            align = 8;
            sized = true;
            out_fixed = false;
            break;
        default:
            if (primitive_size_align(k.kind, size, align)) {
                sized = true;
            }
            break;
        }
        if (sized) {
            uint32_t field_end = f.offset + size;
            if (field_end > end) end = field_end;
            if (align > max_align) max_align = align;
        }
    }
    if (end < max_align) end = max_align;
    // Round size up to the alignment so `sizeof(T)` matches.
    end = (end + max_align - 1) & ~(max_align - 1);
    out_align = max_align;
    return end;
}

} // namespace

extern "C" {

// Build a Cyclone DDS topic descriptor from the flattened Rust
// schema. Returns a fully-populated, heap-owned
// `dds_topic_descriptor_t *` on success. Caller (the Rust registry)
// stores it; the descriptor is intentionally never freed during the
// process lifetime — the registry caches it and Cyclone holds onto
// `m_ops` / `m_typename` after `dds_create_topic`.
const void* nros_cyclonedds_build_descriptor_from_schema(const char* type_name,
                                                         const NrosFieldDescriptor* fields,
                                                         uint32_t field_count,
                                                         const NrosFieldKindDescriptor* kinds,
                                                         uint32_t kind_count, int* out_err) {
    // Defensive input checks. The Rust shim already validates these,
    // but the bridge is a C ABI boundary — assume nothing.
    if (type_name == nullptr || fields == nullptr || kinds == nullptr) {
        if (out_err != nullptr) *out_err = NROS_BRIDGE_ERR_NULL_POINTER;
        return nullptr;
    }
    if (field_count == 0 || kind_count == 0) {
        if (out_err != nullptr) *out_err = NROS_BRIDGE_ERR_EMPTY_SCHEMA;
        return nullptr;
    }

    BuildContext ctx;
    t_ctx = &ctx;

    // 1. Emit the top-level field ops.
    for (uint32_t i = 0; i < field_count; ++i) {
        if (!emit_kind_block(ctx.ops, fields[i].kind, kinds, kind_count, fields[i].offset,
                             &ctx.err)) {
            if (out_err != nullptr) {
                *out_err = ctx.err != 0 ? ctx.err : NROS_BRIDGE_ERR_UNSUPPORTED_FIELD_TYPE;
            }
            t_ctx = nullptr;
            return nullptr;
        }
    }
    if (!ctx.ops.push(DDS_OP_RTS)) {
        if (out_err != nullptr) *out_err = NROS_BRIDGE_ERR_NESTED_DEPTH_EXCEEDED;
        t_ctx = nullptr;
        return nullptr;
    }

    // 2. Emit each queued nested struct body, breadth-first. As we
    //    consume entries new nested kinds may be enqueued (a struct
    //    that has a field of another struct type).
    while (ctx.queue_head < ctx.queue_len) {
        uint32_t kind_idx = ctx.queue[ctx.queue_head++];
        if (!emit_nested_body(ctx, kind_idx, kinds, kind_count)) {
            if (out_err != nullptr) {
                *out_err = ctx.err != 0 ? ctx.err : NROS_BRIDGE_ERR_UNSUPPORTED_FIELD_TYPE;
            }
            t_ctx = nullptr;
            return nullptr;
        }
    }

    // 3. Backfill every JSR patch with the computed
    //    `(next_insn << 16) | (delta & 0xffff)` link word.
    //
    // Phase 212.K.7.4.c — generalised over all shapes (EXT, SEQ|STU,
    // BSQ|STU, ARR|STU). Each patch carries its opcode-word index
    // explicitly (so backfill doesn't have to reverse-engineer the
    // slot layout per shape) plus the constant `next_insn` width to
    // bake into the link's high16 (3/4/5 per shape).
    //
    // `delta` is a signed 16-bit word offset from the **opcode** word
    // (per `DDS_OP_ADR_JSR(o) = (int16_t)(o & 0xffff)` in
    // `dds_opcodes.h`); the walker reads it as `ops += jsr_delta`
    // where `ops` points at the opcode.
    for (size_t i = 0; i < ctx.patches.count; ++i) {
        const auto& pat = ctx.patches.entries[i];
        size_t target_word = ctx.nested.find(pat.target_kind_idx);
        if (target_word == static_cast<size_t>(-1)) {
            if (out_err != nullptr) *out_err = NROS_BRIDGE_ERR_UNSUPPORTED_FIELD_TYPE;
            t_ctx = nullptr;
            return nullptr;
        }
        int32_t delta = static_cast<int32_t>(target_word) - static_cast<int32_t>(pat.opcode_word);
        uint32_t encoded = (static_cast<uint32_t>(pat.next_insn) << 16) |
                           (static_cast<uint32_t>(delta) & 0xffffu);
        ctx.ops.buf[pat.link_word] = encoded;
    }

    t_ctx = nullptr;

    // 4. Compute size / alignment / flagset.
    uint32_t align = 1;
    bool fixed = true;
    uint32_t size = compute_struct_size(fields, field_count, kinds, kind_count, align, fixed);

    // 5. Allocate + copy the ops array into a stable ddsrt buffer.
    size_t nops_words = ctx.ops.len;
    uint32_t* ops_out = static_cast<uint32_t*>(ddsrt_malloc(nops_words * sizeof(uint32_t)));
    if (ops_out == nullptr) {
        if (out_err != nullptr) *out_err = NROS_BRIDGE_ERR_NULL_POINTER;
        return nullptr;
    }
    memcpy(ops_out, ctx.ops.buf, nops_words * sizeof(uint32_t));

    // 6. Type-name mangling.
    char* mangled = mangle_type_name(type_name);
    if (mangled == nullptr) {
        ddsrt_free(ops_out);
        if (out_err != nullptr) *out_err = NROS_BRIDGE_ERR_NULL_POINTER;
        return nullptr;
    }

    // 7. Allocate + populate the descriptor.
    auto* desc =
        static_cast<dds_topic_descriptor_t*>(ddsrt_calloc(1, sizeof(dds_topic_descriptor_t)));
    if (desc == nullptr) {
        ddsrt_free(ops_out);
        ddsrt_free(mangled);
        if (out_err != nullptr) *out_err = NROS_BRIDGE_ERR_NULL_POINTER;
        return nullptr;
    }

    // `dds_topic_descriptor_t` fields are `const`, so we have to use
    // `const_cast` to write into our freshly-allocated copy. This is
    // exactly what idlc-generated static descriptors look like once
    // their initializers run.
    auto* mut = const_cast<dds_topic_descriptor_t*>(desc);
    *const_cast<uint32_t*>(&mut->m_size) = size;
    *const_cast<uint32_t*>(&mut->m_align) = align;
    *const_cast<uint32_t*>(&mut->m_flagset) =
        fixed ? static_cast<uint32_t>(DDS_TOPIC_FIXED_SIZE) : 0u;
    *const_cast<uint32_t*>(&mut->m_nkeys) = 0;
    *const_cast<const char**>(&mut->m_typename) = mangled;
    *const_cast<const dds_key_descriptor_t**>(&mut->m_keys) = nullptr;
    *const_cast<uint32_t*>(&mut->m_nops) = static_cast<uint32_t>(nops_words);
    *const_cast<const uint32_t**>(&mut->m_ops) = ops_out;
    *const_cast<const char**>(&mut->m_meta) = "";

    return desc;
}

} // extern "C"
