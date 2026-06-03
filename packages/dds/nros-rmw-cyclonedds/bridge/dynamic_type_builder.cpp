// Phase 212.K.7.4 — C++ bridge for the Rust DescriptorBuilder.
//
// `dynamic_type.rs` flattens a Rust `nros_serdes::Message` field
// schema into a pair of ABI-stable arrays (`NrosFieldDescriptor[]` +
// `NrosFieldKindDescriptor[]`) and calls
// `nros_cyclonedds_build_descriptor_from_schema` (this file) to turn
// them into a Cyclone DDS `dds_topic_descriptor_t *`.
//
// **STUB STATUS (212.K.7.4 initial landing).** Cyclone DDS 0.10.5
// (our current pin) does not yet expose the
// `ddsi_dynamic_type_*` API surface in its public headers — it lives
// behind `DDSI_INCLUDE_DYNAMIC_TYPES` in `master`. Until we either
// bump the pin to 0.11+ (Phase 117.X follow-up) or vendor the
// dynamic-types TU directly, this bridge returns
// `BridgeError::UnsupportedFieldType` (-1002) for every call,
// matching the Rust-side BuildError variant the task spec calls out
// as a permissible stub.
//
// The walker shape, ABI layout, and error mapping are all stable —
// only the call into `ddsi_dynamic_type_create_*` / `..._add_member`
// / `dds_topic_descriptor_from_dynamic_type` is missing. When the
// real API lands, only the body of `nros_cyclonedds_build_descriptor_from_schema`
// changes; the Rust shim's ABI does not.
//
// See `dds/ddsi/ddsi_dynamic_type.h` (Cyclone master) for the target
// API:
//   * ddsi_dynamic_type_create_struct(domain, name)
//   * ddsi_dynamic_type_create_{primitive, string, wstring, …}
//   * ddsi_dynamic_type_create_{sequence, bounded_sequence, array}
//   * ddsi_dynamic_type_add_member(parent, child, name, offset, …)
//   * ddsi_dynamic_type_register(parent, &topic_descriptor_out)

#include <stddef.h>
#include <stdint.h>

extern "C" {

// Mirror of `crate::bridge::NrosFieldDescriptor`.
struct NrosFieldDescriptor {
    const char *name;
    uint32_t offset;
    uint32_t kind;
};

// Mirror of `crate::bridge::NrosFieldKindDescriptor`.
struct NrosFieldKindDescriptor {
    uint8_t kind;
    uint8_t _pad[3];
    uint32_t bound;
    uint32_t inner;
    const char *nested_name;
};

// Mirror of `crate::bridge::BridgeError`.
enum NrosBridgeError {
    NROS_BRIDGE_ERR_NESTED_DEPTH_EXCEEDED = -1001,
    NROS_BRIDGE_ERR_UNSUPPORTED_FIELD_TYPE = -1002,
    NROS_BRIDGE_ERR_NULL_POINTER = -1003,
    NROS_BRIDGE_ERR_EMPTY_SCHEMA = -1004,
};

// Mirror of `crate::bridge::FieldKind`. The Rust shim emits these
// tag values directly; we keep an enum mirror for future use when
// the real Cyclone dynamic-type calls land.
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

// FieldType → Cyclone primitive mapping table (target shape, for the
// real implementation):
//
// | FieldKind          | Cyclone API call                                       |
// |--------------------|--------------------------------------------------------|
// | Bool               | ddsi_dynamic_type_create_primitive(DDS_DYNAMIC_BOOLEAN) |
// | Uint8 / Int8       | ddsi_dynamic_type_create_primitive(DDS_DYNAMIC_{U,}INT8) |
// | Uint16 / Int16     | …_create_primitive(DDS_DYNAMIC_{U,}INT16)              |
// | Uint32 / Int32     | …_create_primitive(DDS_DYNAMIC_{U,}INT32)              |
// | Uint64 / Int64     | …_create_primitive(DDS_DYNAMIC_{U,}INT64)              |
// | Float32 / Float64  | …_create_primitive(DDS_DYNAMIC_FLOAT{32,64})           |
// | String             | ddsi_dynamic_type_create_string()                      |
// | WString            | ddsi_dynamic_type_create_wstring()                     |
// | BoundedString(N)   | ddsi_dynamic_type_create_bounded_string(N)             |
// | BoundedWString(N)  | ddsi_dynamic_type_create_bounded_wstring(N)            |
// | Nested(name)       | ddsi_dynamic_type_create_struct(domain, mangled(name)) |
// | Array(N, inner)    | ddsi_dynamic_type_create_array(inner, N)               |
// | Sequence(inner)    | ddsi_dynamic_type_create_sequence(inner, UNBOUNDED)    |
// | BoundedSequence(N) | ddsi_dynamic_type_create_sequence(inner, N)            |
//
// Type-name mangling (ROS → Cyclone) follows
// rmw_cyclonedds_cpp::namespaces_to_dds_namespaces:
//   "std_msgs/msg/String" → "std_msgs::msg::dds_::String_"

// Build a Cyclone DDS topic descriptor from the flattened Rust
// schema. Stub until Cyclone 0.11+'s dynamic-types API lands; for
// now signals "unsupported" so the Rust BuildError surfaces
// cleanly. The Rust unit-test stub overrides this symbol via
// `#[unsafe(no_mangle)]` in `bridge::test_stub`.
const void *nros_cyclonedds_build_descriptor_from_schema(
    const char *type_name,
    const NrosFieldDescriptor *fields,
    uint32_t field_count,
    const NrosFieldKindDescriptor *kinds,
    uint32_t kind_count,
    int *out_err) {
    // Defensive input checks. The Rust shim already validates these,
    // but the bridge is a C ABI boundary — assume nothing.
    if (type_name == nullptr || fields == nullptr || kinds == nullptr) {
        if (out_err != nullptr) {
            *out_err = NROS_BRIDGE_ERR_NULL_POINTER;
        }
        return nullptr;
    }
    if (field_count == 0 || kind_count == 0) {
        if (out_err != nullptr) {
            *out_err = NROS_BRIDGE_ERR_EMPTY_SCHEMA;
        }
        return nullptr;
    }

    // TODO(K.7 follow-up): bump Cyclone pin to a release that exposes
    // ddsi_dynamic_type_*, then walk kinds[] bottom-up:
    //   1. For each NESTED entry, recursively build the child
    //      struct first (using kinds[i].inner + kinds[i].bound).
    //   2. For ARRAY / SEQUENCE / BOUNDED_SEQUENCE, build the inner
    //      first.
    //   3. Build the top-level struct, add each field via
    //      ddsi_dynamic_type_add_member.
    //   4. Call ddsi_dynamic_type_register; the out-param is the
    //      `dds_topic_descriptor_t *` we return here.

    (void) fields;
    (void) kinds;
    if (out_err != nullptr) {
        *out_err = NROS_BRIDGE_ERR_UNSUPPORTED_FIELD_TYPE;
    }
    return nullptr;
}

// The Rust registry calls this after a successful build so the
// existing C++ `descriptors.cpp` table is in sync. Declared
// `extern "C"` in `src/descriptors.cpp`; this bridge TU is a no-op
// re-declaration anchor so a build without the legacy registry TU
// still links cleanly. The real symbol is provided by
// `src/descriptors.cpp` once linked into the same archive.
//
// Intentionally NOT defined here — relying on the linker to find
// the version exported from `descriptors.cpp`. If the bridge TU is
// built into an archive WITHOUT `descriptors.cpp` (uncommon), the
// link fails loudly — better than a silent stub.

} // extern "C"
