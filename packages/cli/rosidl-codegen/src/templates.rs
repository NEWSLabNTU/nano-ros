use askama::Template;

// Custom Askama filters
pub mod filters {
    use crate::utils::to_snake_case;

    pub fn snake_case(s: &str) -> ::askama::Result<String> {
        Ok(to_snake_case(s))
    }
}

#[derive(Template)]
#[template(path = "cargo.toml.jinja", escape = "none")]
pub struct CargoTomlTemplate<'a> {
    pub package_name: &'a str,
    pub dependencies: &'a [String],
    pub needs_big_array: bool,
}

#[derive(Template)]
#[template(path = "build.rs.jinja", escape = "none")]
pub struct BuildRsTemplate;

#[derive(Template)]
#[template(path = "lib.rs.jinja", escape = "none")]
pub struct LibRsTemplate {
    pub has_messages: bool,
    pub has_services: bool,
    pub has_actions: bool,
}

#[derive(Template)]
#[template(path = "message_rmw.rs.jinja", escape = "none")]
pub struct MessageRmwTemplate<'a> {
    pub package_name: &'a str,
    pub message_name: &'a str,
    pub message_module: &'a str,
    pub fields: Vec<RmwField>,
    pub constants: Vec<MessageConstant>,
}

#[derive(Template)]
#[template(path = "message_idiomatic.rs.jinja", escape = "none")]
pub struct MessageIdiomaticTemplate<'a> {
    pub package_name: &'a str,
    pub message_name: &'a str,
    pub message_module: &'a str,
    pub fields: Vec<IdiomaticField>,
    pub constants: Vec<MessageConstant>,
}

pub struct RmwField {
    pub name: String,
    pub rust_type: String,
    pub default_value: String,
}

/// Exhaustive enum representing all possible ROS 2 IDL field types
/// This ensures compile-time checking that all cases are handled in templates
#[derive(Debug, Clone, PartialEq)]
pub enum FieldKind {
    // Scalar types (single values)
    Primitive,
    UnboundedString,
    BoundedString,
    UnboundedWString,
    BoundedWString,
    NestedMessage,

    // Array types (fixed-size)
    PrimitiveArray,
    UnboundedStringArray,
    BoundedStringArray,
    UnboundedWStringArray,
    BoundedWStringArray,
    NestedMessageArray,
    LargeArray, // Arrays > 32 elements (no Copy/Clone trait)

    // Bounded sequences (max_size specified: T[<=N])
    BoundedPrimitiveSequence,
    BoundedUnboundedStringSequence,  // string[<=N]
    BoundedBoundedStringSequence,    // string<=M[<=N]
    BoundedUnboundedWStringSequence, // wstring[<=N]
    BoundedBoundedWStringSequence,   // wstring<=M[<=N]
    BoundedNestedMessageSequence,

    // Unbounded sequences (no max_size: T[])
    UnboundedPrimitiveSequence,
    UnboundedUnboundedStringSequence,  // string[]
    UnboundedBoundedStringSequence,    // string<=M[]
    UnboundedUnboundedWStringSequence, // wstring[]
    UnboundedBoundedWStringSequence,   // wstring<=M[]
    UnboundedNestedMessageSequence,
}

pub struct IdiomaticField {
    pub name: String,
    pub rust_type: String,
    pub default_value: String,
    pub kind: FieldKind,
}

pub struct MessageConstant {
    pub name: String,
    pub rust_type: String,
    pub value: String,
}

#[derive(Template)]
#[template(path = "service_rmw.rs.jinja", escape = "none")]
pub struct ServiceRmwTemplate<'a> {
    pub package_name: &'a str,
    pub service_name: &'a str,
    pub request_fields: Vec<RmwField>,
    pub request_constants: Vec<MessageConstant>,
    pub response_fields: Vec<RmwField>,
    pub response_constants: Vec<MessageConstant>,
}

#[derive(Template)]
#[template(path = "service_idiomatic.rs.jinja", escape = "none")]
pub struct ServiceIdiomaticTemplate<'a> {
    pub package_name: &'a str,
    pub service_name: &'a str,
    pub request_fields: Vec<IdiomaticField>,
    pub request_constants: Vec<MessageConstant>,
    pub response_fields: Vec<IdiomaticField>,
    pub response_constants: Vec<MessageConstant>,
}

#[derive(Template)]
#[template(path = "action_rmw.rs.jinja", escape = "none")]
pub struct ActionRmwTemplate<'a> {
    pub package_name: &'a str,
    pub action_name: &'a str,
    pub goal_fields: Vec<RmwField>,
    pub goal_constants: Vec<MessageConstant>,
    pub result_fields: Vec<RmwField>,
    pub result_constants: Vec<MessageConstant>,
    pub feedback_fields: Vec<RmwField>,
    pub feedback_constants: Vec<MessageConstant>,
}

#[derive(Template)]
#[template(path = "action_idiomatic.rs.jinja", escape = "none")]
pub struct ActionIdiomaticTemplate<'a> {
    pub package_name: &'a str,
    pub action_name: &'a str,
    pub goal_fields: Vec<IdiomaticField>,
    pub goal_constants: Vec<MessageConstant>,
    pub result_fields: Vec<IdiomaticField>,
    pub result_constants: Vec<MessageConstant>,
    pub feedback_fields: Vec<IdiomaticField>,
    pub feedback_constants: Vec<MessageConstant>,
}

// ============================================================================
// nros Templates
// ============================================================================

/// Field metadata for nros code generation
#[derive(Debug, Clone)]
pub struct NrosField {
    pub name: String,
    pub rust_type: String,
    /// CDR primitive method name (e.g., "i32", "f64", "u8") - empty if not primitive
    pub primitive_method: String,
    /// For arrays/sequences: element primitive method - empty if not primitive element
    pub element_primitive_method: String,
    /// Array size for fixed arrays - 0 if not an array
    pub array_size: usize,

    // Type flags for template conditionals
    pub is_primitive: bool,
    pub is_string: bool,
    pub is_array: bool,
    pub is_sequence: bool,
    pub is_nested: bool,
    pub is_primitive_element: bool,
    pub is_string_element: bool,
    /// True if this is a fixed-size array with > 32 elements (no Default for [T; N] where N > 32)
    pub is_large_array: bool,
    /// RFC-0033: `mode = "heap"` — the field is an `alloc`-backed
    /// `nros_core::heap::{Vec, String}` rather than a fixed-capacity `heapless`
    /// container. Changes the deserialize codegen (growable, no `CapacityExceeded`).
    pub is_heap: bool,
}

#[derive(Template)]
#[template(path = "message_nros.rs.jinja", escape = "none")]
pub struct MessageNrosTemplate<'a> {
    pub package_name: &'a str,
    pub message_name: &'a str,
    pub type_hash: &'a str,
    pub fields: Vec<NrosField>,
    pub constants: Vec<MessageConstant>,
    /// True if there are fields to serialize/deserialize
    pub has_fields: bool,
    /// True if any field is a large array (> 32 elements), requiring manual Default impl
    pub has_large_array: bool,
    /// When true, uses nros_core:: prefixed imports instead of direct use statements
    pub inline_mode: bool,
    /// Pre-rendered `::nros_serdes::NestedType` / `FieldType` helper `pub const`
    /// items hoisted to module scope so recursive variants
    /// (`FieldType::Array(_, &FT_X)`) can reference `'static` addresses.
    pub schema_helper_consts: String,
    /// Pre-rendered body of `<Msg as ::nros_serdes::Message>::FIELDS`.
    pub schema_fields_block: String,
    /// `package/msg/MessageName` form for `Message::TYPE_NAME`.
    pub schema_type_name: String,
}

#[derive(Template)]
#[template(path = "service_nros.rs.jinja", escape = "none")]
pub struct ServiceNrosTemplate<'a> {
    pub package_name: &'a str,
    pub service_name: &'a str,
    pub type_hash: &'a str,
    pub request_fields: Vec<NrosField>,
    pub request_constants: Vec<MessageConstant>,
    pub response_fields: Vec<NrosField>,
    pub response_constants: Vec<MessageConstant>,
    /// True if request has fields to serialize/deserialize
    pub has_request_fields: bool,
    /// True if response has fields to serialize/deserialize
    pub has_response_fields: bool,
    /// True if request has a large array field (> 32 elements)
    pub has_request_large_array: bool,
    /// True if response has a large array field (> 32 elements)
    pub has_response_large_array: bool,
    /// When true, uses nros_core:: prefixed imports instead of direct use statements
    pub inline_mode: bool,
    // ---- nros_serdes::Message schema (Phase 212.K.7.1.c) ----
    /// Per-half helper `pub const` blocks (NESTED_*, FT_*_ELEM) hoisted
    /// into module scope. Prefixed `REQ_…` / `RESP_…` to avoid collision
    /// on common field names (e.g. both halves owning a `header` field).
    pub req_schema_helper_consts: String,
    /// `Field { … },` list rendered for the Request struct.
    pub req_schema_fields_block: String,
    /// `<pkg>/srv/<Svc>_Request` per rosidl convention.
    pub req_schema_type_name: String,
    pub resp_schema_helper_consts: String,
    pub resp_schema_fields_block: String,
    pub resp_schema_type_name: String,
}

#[derive(Template)]
#[template(path = "cargo_nros.toml.jinja", escape = "none")]
pub struct CargoNrosTomlTemplate<'a> {
    pub package_name: &'a str,
    pub package_version: &'a str,
    pub dependencies: &'a [String],
}

#[derive(Template)]
#[template(path = "lib_nros.rs.jinja", escape = "none")]
pub struct LibNrosRsTemplate {
    pub has_messages: bool,
    pub has_services: bool,
    pub has_actions: bool,
}

#[derive(Template)]
#[template(path = "action_nros.rs.jinja", escape = "none")]
pub struct ActionNrosTemplate<'a> {
    pub package_name: &'a str,
    pub action_name: &'a str,
    pub type_hash: &'a str,
    pub goal_fields: Vec<NrosField>,
    pub goal_constants: Vec<MessageConstant>,
    pub result_fields: Vec<NrosField>,
    pub result_constants: Vec<MessageConstant>,
    pub feedback_fields: Vec<NrosField>,
    pub feedback_constants: Vec<MessageConstant>,
    /// True if goal has fields to serialize/deserialize
    pub has_goal_fields: bool,
    /// True if result has fields to serialize/deserialize
    pub has_result_fields: bool,
    /// True if feedback has fields to serialize/deserialize
    pub has_feedback_fields: bool,
    /// True if goal has a large array field (> 32 elements)
    pub has_goal_large_array: bool,
    /// True if result has a large array field (> 32 elements)
    pub has_result_large_array: bool,
    /// True if feedback has a large array field (> 32 elements)
    pub has_feedback_large_array: bool,
    /// When true, uses nros_core:: prefixed imports instead of direct use statements
    pub inline_mode: bool,
    // ---- nros_serdes::Message schema (Phase 212.K.7.1.c) ----
    /// Per-half helper `pub const` blocks (NESTED_*, FT_*_ELEM) hoisted
    /// into module scope. Prefixed `GOAL_…` / `RESULT_…` / `FEEDBACK_…`
    /// to avoid collision on shared field names across halves.
    pub goal_schema_helper_consts: String,
    pub goal_schema_fields_block: String,
    /// `<pkg>/action/<Action>_Goal` per rosidl convention.
    pub goal_schema_type_name: String,
    pub result_schema_helper_consts: String,
    pub result_schema_fields_block: String,
    pub result_schema_type_name: String,
    pub feedback_schema_helper_consts: String,
    pub feedback_schema_fields_block: String,
    pub feedback_schema_type_name: String,
    // ---- Action envelope structs (Phase 212.K.7.1.d) ----
    //
    // The five rosidl-convention wire structs that wrap the user-facing
    // Goal/Result/Feedback for the action service-shape protocol. Each
    // ships its own `Serialize` / `Deserialize` / `RosMessage` /
    // `::nros_serdes::Message` impl just like the user-facing structs.
    /// `<A>_SendGoal_Request { goal_id: UUID, goal: <A>Goal }`
    pub send_goal_request_schema_helper_consts: String,
    pub send_goal_request_schema_fields_block: String,
    pub send_goal_request_schema_type_name: String,
    /// `<A>_SendGoal_Response { accepted: bool, stamp: Time }`
    pub send_goal_response_schema_helper_consts: String,
    pub send_goal_response_schema_fields_block: String,
    pub send_goal_response_schema_type_name: String,
    /// `<A>_GetResult_Request { goal_id: UUID }`
    pub get_result_request_schema_helper_consts: String,
    pub get_result_request_schema_fields_block: String,
    pub get_result_request_schema_type_name: String,
    /// `<A>_GetResult_Response { status: i8, result: <A>Result }`
    pub get_result_response_schema_helper_consts: String,
    pub get_result_response_schema_fields_block: String,
    pub get_result_response_schema_type_name: String,
    /// `<A>_FeedbackMessage { goal_id: UUID, feedback: <A>Feedback }`
    pub feedback_message_schema_helper_consts: String,
    pub feedback_message_schema_fields_block: String,
    pub feedback_message_schema_type_name: String,
}

// ============================================================================
// C Templates (for nros-c)
// ============================================================================

/// Field information for C code generation
#[derive(Clone)]
pub struct CField {
    pub name: String,
    /// Base C type (e.g., "int32_t", "char", "struct foo_msg")
    pub c_type: String,
    /// Array suffix for the field declaration (e.g., "[256]" for strings, "[3]" for arrays)
    /// This comes after the field name in C: `char name[256];`
    pub array_suffix: String,
    /// CDR write method name (e.g., "write_i32")
    pub cdr_write_method: String,
    /// CDR read method name (e.g., "read_i32")
    pub cdr_read_method: String,
    /// For arrays/sequences: element CDR write method
    pub element_cdr_write_method: String,
    /// For arrays/sequences: element CDR read method
    pub element_cdr_read_method: String,
    /// Array size for fixed arrays - 0 if not an array
    pub array_size: usize,
    /// Sequence capacity for bounded/unbounded sequences
    pub sequence_capacity: usize,
    /// Nested struct name (for nested messages)
    pub nested_struct_name: String,
    /// Element struct name (for arrays/sequences of nested messages)
    pub element_struct_name: String,

    // Type flags for template conditionals
    pub is_primitive: bool,
    pub is_string: bool,
    pub is_array: bool,
    pub is_sequence: bool,
    pub is_nested: bool,
    pub is_primitive_element: bool,
    pub is_string_element: bool,
    /// RFC-0033: `mode = "heap"` — the field is a heap-backed
    /// `{ T* data; size_t size; size_t capacity; }` (rclc `rosidl_runtime_c`
    /// pattern) rather than an inline fixed-capacity buffer. The deserialize
    /// codegen mallocs; `<struct>_fini` frees.
    pub is_heap: bool,
}

/// Constant for C code generation
pub struct CConstant {
    pub name: String,
    pub c_type: String,
    pub value: String,
}

#[derive(Template)]
#[template(path = "message_c.h.jinja", escape = "none")]
pub struct MessageCHeaderTemplate<'a> {
    pub package_name: &'a str,
    pub message_name: &'a str,
    pub type_hash: &'a str,
    pub guard_name: String,
    pub struct_name: String,
    pub constant_prefix: String,
    pub fields: Vec<CField>,
    pub constants: Vec<CConstant>,
    pub dependencies: Vec<String>,
    pub type_includes: Vec<String>,
    pub has_fields: bool,
}

#[derive(Template)]
#[template(path = "message_c.c.jinja", escape = "none")]
pub struct MessageCSourceTemplate<'a> {
    pub package_name: &'a str,
    pub message_name: &'a str,
    pub type_hash: &'a str,
    pub header_name: String,
    pub struct_name: String,
    pub fields: Vec<CField>,
    pub has_fields: bool,
}

#[derive(Template)]
#[template(path = "service_c.h.jinja", escape = "none")]
pub struct ServiceCHeaderTemplate<'a> {
    pub package_name: &'a str,
    pub service_name: &'a str,
    pub type_hash: &'a str,
    pub guard_name: String,
    pub service_struct_name: String,
    pub request_struct_name: String,
    pub response_struct_name: String,
    pub constant_prefix: String,
    pub request_fields: Vec<CField>,
    pub request_constants: Vec<CConstant>,
    pub response_fields: Vec<CField>,
    pub response_constants: Vec<CConstant>,
    pub dependencies: Vec<String>,
    pub type_includes: Vec<String>,
    pub has_request_fields: bool,
    pub has_response_fields: bool,
}

#[derive(Template)]
#[template(path = "service_c.c.jinja", escape = "none")]
pub struct ServiceCSourceTemplate<'a> {
    pub package_name: &'a str,
    pub service_name: &'a str,
    pub type_hash: &'a str,
    pub header_name: String,
    pub service_struct_name: String,
    pub request_struct_name: String,
    pub response_struct_name: String,
    pub request_fields: Vec<CField>,
    pub response_fields: Vec<CField>,
    pub has_request_fields: bool,
    pub has_response_fields: bool,
}

#[derive(Template)]
#[template(path = "action_c.h.jinja", escape = "none")]
pub struct ActionCHeaderTemplate<'a> {
    pub package_name: &'a str,
    pub action_name: &'a str,
    pub type_hash: &'a str,
    pub guard_name: String,
    pub action_struct_name: String,
    pub goal_struct_name: String,
    pub result_struct_name: String,
    pub feedback_struct_name: String,
    pub constant_prefix: String,
    pub goal_fields: Vec<CField>,
    pub goal_constants: Vec<CConstant>,
    pub result_fields: Vec<CField>,
    pub result_constants: Vec<CConstant>,
    pub feedback_fields: Vec<CField>,
    pub feedback_constants: Vec<CConstant>,
    pub dependencies: Vec<String>,
    pub type_includes: Vec<String>,
    pub has_goal_fields: bool,
    pub has_result_fields: bool,
    pub has_feedback_fields: bool,
}

#[derive(Template)]
#[template(path = "action_c.c.jinja", escape = "none")]
pub struct ActionCSourceTemplate<'a> {
    pub package_name: &'a str,
    pub action_name: &'a str,
    pub type_hash: &'a str,
    pub header_name: String,
    pub action_struct_name: String,
    pub goal_struct_name: String,
    pub result_struct_name: String,
    pub feedback_struct_name: String,
    pub goal_fields: Vec<CField>,
    pub result_fields: Vec<CField>,
    pub feedback_fields: Vec<CField>,
    pub has_goal_fields: bool,
    pub has_result_fields: bool,
    pub has_feedback_fields: bool,
}

// ============================================================================
// C++ Templates (for nros-cpp)
// ============================================================================

/// Field information for C++ FFI Rust code generation
#[derive(Clone)]
pub struct CppFfiField {
    pub name: String,
    /// Rust #[repr(C)] type (e.g., "i32", "[u8; 256]")
    pub repr_c_type: String,
    /// CDR write method (e.g., "write_i32", "write_string")
    pub cdr_write_method: String,
    /// CDR read method (e.g., "read_i32", "read_string")
    pub cdr_read_method: String,
    /// For arrays/sequences: element CDR write method
    pub element_cdr_write_method: String,
    /// For arrays/sequences: element CDR read method
    pub element_cdr_read_method: String,
    /// Array size for fixed arrays — 0 if not an array
    pub array_size: usize,
    /// Sequence capacity — 0 if not a sequence
    pub sequence_capacity: usize,
    /// Nested serialize function name (e.g., "serialize_pkg_msg_point_fields")
    pub nested_serialize_fn: String,
    /// Nested deserialize function name
    pub nested_deserialize_fn: String,
    /// String capacity (for string fields — used in deserialization)
    pub string_capacity: usize,
    /// Element string capacity (for arrays/sequences of strings)
    pub element_string_capacity: usize,

    // Type flags
    pub is_primitive: bool,
    pub is_string: bool,
    pub is_array: bool,
    pub is_sequence: bool,
    pub is_nested: bool,
    pub is_primitive_element: bool,
    pub is_string_element: bool,
}

/// C++ field info for header generation (uses FixedString/FixedSequence types)
#[derive(Clone)]
pub struct CppField {
    pub name: String,
    /// C++ type (e.g., "int32_t", "nros::FixedString<256>")
    pub cpp_type: String,
    /// Array suffix (e.g., "[3]" for fixed arrays)
    pub array_suffix: String,
}

/// Sequence helper struct definition for Rust #[repr(C)]
#[derive(Clone)]
pub struct SequenceStructDef {
    /// Struct name (e.g., "std_msgs_msg_string_data_seq_t")
    pub struct_name: String,
    /// Element type (e.g., "i32", "[u8; 256]")
    pub element_type: String,
    /// Capacity
    pub capacity: usize,
}

#[derive(Template)]
#[template(path = "message_cpp.hpp.jinja", escape = "none")]
pub struct MessageCppHeaderTemplate<'a> {
    pub package_name: &'a str,
    pub message_name: &'a str,
    pub type_hash: &'a str,
    pub guard_name: String,
    pub cpp_package: String,
    pub ffi_publish_fn: String,
    pub ffi_serialize_fn: String,
    pub ffi_deserialize_fn: String,
    pub fields: Vec<CppField>,
    pub constants: Vec<CConstant>,
    pub dependencies: Vec<String>,
    /// Same-package type includes (relative paths like "msg/pkg_msg_foo.hpp")
    pub intra_package_includes: Vec<String>,
    pub has_fields: bool,
    pub serialized_size_max: usize,
}

#[derive(Template)]
#[template(path = "message_cpp_ffi.rs.jinja", escape = "none")]
pub struct MessageCppFfiTemplate<'a> {
    pub package_name: &'a str,
    pub message_name: &'a str,
    pub repr_c_struct_name: String,
    pub ffi_publish_fn: String,
    pub ffi_serialize_fn: String,
    pub ffi_deserialize_fn: String,
    pub serialize_fn: String,
    pub deserialize_fn: String,
    pub fields: Vec<CppFfiField>,
    pub sequence_structs: Vec<SequenceStructDef>,
    pub has_fields: bool,
    pub serialized_size_max: usize,
}

#[derive(Template)]
#[template(path = "service_cpp.hpp.jinja", escape = "none")]
pub struct ServiceCppHeaderTemplate<'a> {
    pub package_name: &'a str,
    pub service_name: &'a str,
    pub type_hash: &'a str,
    pub guard_name: String,
    pub cpp_package: String,
    pub request_ffi_publish_fn: String,
    pub request_ffi_serialize_fn: String,
    pub request_ffi_deserialize_fn: String,
    pub response_ffi_publish_fn: String,
    pub response_ffi_serialize_fn: String,
    pub response_ffi_deserialize_fn: String,
    pub request_fields: Vec<CppField>,
    pub request_constants: Vec<CConstant>,
    pub response_fields: Vec<CppField>,
    pub response_constants: Vec<CConstant>,
    pub dependencies: Vec<String>,
    pub intra_package_includes: Vec<String>,
    pub has_request_fields: bool,
    pub has_response_fields: bool,
    pub request_serialized_size_max: usize,
    pub response_serialized_size_max: usize,
}

#[derive(Template)]
#[template(path = "action_cpp.hpp.jinja", escape = "none")]
pub struct ActionCppHeaderTemplate<'a> {
    pub package_name: &'a str,
    pub action_name: &'a str,
    pub type_hash: &'a str,
    pub guard_name: String,
    pub cpp_package: String,
    pub goal_ffi_publish_fn: String,
    pub goal_ffi_serialize_fn: String,
    pub goal_ffi_deserialize_fn: String,
    pub result_ffi_publish_fn: String,
    pub result_ffi_serialize_fn: String,
    pub result_ffi_deserialize_fn: String,
    pub feedback_ffi_publish_fn: String,
    pub feedback_ffi_serialize_fn: String,
    pub feedback_ffi_deserialize_fn: String,
    pub goal_fields: Vec<CppField>,
    pub goal_constants: Vec<CConstant>,
    pub result_fields: Vec<CppField>,
    pub result_constants: Vec<CConstant>,
    pub feedback_fields: Vec<CppField>,
    pub feedback_constants: Vec<CConstant>,
    pub dependencies: Vec<String>,
    pub intra_package_includes: Vec<String>,
    pub has_goal_fields: bool,
    pub has_result_fields: bool,
    pub has_feedback_fields: bool,
    pub goal_serialized_size_max: usize,
    pub result_serialized_size_max: usize,
    pub feedback_serialized_size_max: usize,
}
