//! Codegen view structs — the serde context each pack template renders from
//! (RFC-0068 Stage 3). No askama: every backend renders via `crate::render`
//! (minijinja) over these structs.

#[derive(serde::Serialize)]
pub struct CargoTomlTemplate<'a> {
    pub package_name: &'a str,
    pub dependencies: &'a [String],
    pub needs_big_array: bool,
}

#[derive(serde::Serialize)]
pub struct BuildRsTemplate;

#[derive(serde::Serialize)]
pub struct LibRsTemplate {
    pub has_messages: bool,
    pub has_services: bool,
    pub has_actions: bool,
}

#[derive(serde::Serialize)]
pub struct MessageRmwTemplate<'a> {
    pub package_name: &'a str,
    pub message_name: &'a str,
    pub message_module: &'a str,
    pub fields: Vec<RmwField>,
    pub constants: Vec<MessageConstant>,
}

#[derive(serde::Serialize)]
pub struct MessageIdiomaticTemplate<'a> {
    pub package_name: &'a str,
    pub message_name: &'a str,
    pub message_module: &'a str,
    pub fields: Vec<IdiomaticField>,
    pub constants: Vec<MessageConstant>,
}

#[derive(Clone, serde::Serialize)]
pub struct RmwField {
    pub name: String,
    /// RFC-0068 step 2 — the `rust_type_rmw` pack filter composes the Rust type
    /// string from these neutral facts (was the pre-baked `rust_type`).
    pub field_type: rosidl_parser::FieldType,
    pub current_package: String,
    pub default_value: String,
}

/// Exhaustive enum representing all possible ROS 2 IDL field types
/// This ensures compile-time checking that all cases are handled in templates
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
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

#[derive(Clone, serde::Serialize)]
pub struct IdiomaticField {
    pub name: String,
    /// RFC-0068 step 2 — composed by the `rust_type_idiomatic` pack filter.
    pub field_type: rosidl_parser::FieldType,
    pub current_package: String,
    pub default_value: String,
    pub kind: FieldKind,
}

#[derive(Clone, serde::Serialize)]
pub struct MessageConstant {
    pub name: String,
    pub rust_type: String,
    pub value: String,
}

#[derive(serde::Serialize)]
pub struct ServiceRmwTemplate<'a> {
    pub package_name: &'a str,
    pub service_name: &'a str,
    pub request_fields: Vec<RmwField>,
    pub request_constants: Vec<MessageConstant>,
    pub response_fields: Vec<RmwField>,
    pub response_constants: Vec<MessageConstant>,
}

#[derive(serde::Serialize)]
pub struct ServiceIdiomaticTemplate<'a> {
    pub package_name: &'a str,
    pub service_name: &'a str,
    pub request_fields: Vec<IdiomaticField>,
    pub request_constants: Vec<MessageConstant>,
    pub response_fields: Vec<IdiomaticField>,
    pub response_constants: Vec<MessageConstant>,
}

#[derive(serde::Serialize)]
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

#[derive(serde::Serialize)]
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
#[derive(Debug, Clone, serde::Serialize)]
pub struct NrosField {
    pub name: String,
    /// RFC-0068 step 2 — the `nros_type` pack filter composes the Rust type
    /// string from these neutral facts (was the pre-baked `rust_type`).
    /// `is_configurable`/`is_heap`/`cap`/`mode` pick the owned-capacity / heap /
    /// plain spelling; `current_package` drives the self-ref choice.
    pub field_type: rosidl_parser::FieldType,
    pub is_configurable: bool,
    pub cap: usize,
    pub mode: crate::types::NrosCodegenMode,
    pub current_package: String,
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
    /// RFC-0033 `mode = "view"` (Phase 229.6, issue 0007) — in the generated
    /// borrowed *view* (`{Msg}View<'a>`), this field is a zero-copy slice
    /// borrowing the receive buffer rather than an owned container. The owned
    /// `{Msg}` struct still renders this field with [`rust_type`](Self::rust_type)
    /// (default-capacity owned) for the publish path.
    pub is_borrowed: bool,
    /// Borrowed view type for this field (e.g. `&'a [u8]`, `&'a str`,
    /// `nros_core::LeSliceView<'a, f32>`). Empty unless
    /// [`is_borrowed`](Self::is_borrowed).
    pub borrowed_rust_type: String,
    /// Full `CdrReader` borrowed-read expression for the view's
    /// `deserialize_view` (e.g. `reader.read_slice_u8()?`,
    /// `reader.read_string()?`, `reader.read_le_slice::<f32>()?`). Empty for
    /// non-borrowed fields.
    pub borrowed_read_expr: String,
}

#[derive(serde::Serialize)]
pub struct MessageNrosTemplate<'a> {
    pub package_name: &'a str,
    pub message_name: &'a str,
    pub type_hash: &'a str,
    /// RFC-0052 W3a — `Some(4)` when the first field is a
    /// `std_msgs/Header` (or bare `builtin_interfaces/Time`): the CDR
    /// offset of `stamp.sec` (4-byte encapsulation header + zero
    /// preceding fields). `None` = no stamp to monitor.
    pub stamp_offset: Option<usize>,
    pub fields: Vec<NrosField>,
    pub constants: Vec<MessageConstant>,
    /// True if there are fields to serialize/deserialize
    pub has_fields: bool,
    /// True if any field is a large array (> 32 elements), requiring manual Default impl
    pub has_large_array: bool,
    /// RFC-0033 `borrowed` mode (Phase 229.6): true if any field resolves to
    /// `mode = "view"`, which additionally emits a `{Msg}View<'a>` zero-copy
    /// view + a `{Msg}Borrow` marker alongside the owned `{Msg}`.
    pub has_borrowed: bool,
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

#[derive(serde::Serialize)]
pub struct ServiceNrosTemplate<'a> {
    pub package_name: &'a str,
    pub service_name: &'a str,
    /// REP-2011 RIHS01 of `<pkg>/srv/<Srv>_Request` (distinct per member).
    pub request_type_hash: &'a str,
    /// REP-2011 RIHS01 of `<pkg>/srv/<Srv>_Response`.
    pub response_type_hash: &'a str,
    /// REP-2011 RIHS01 of the SERVICE `<pkg>/srv/<Srv>` (the 3-member DAG).
    pub service_hash: &'a str,
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
    /// Issue 0346 — RFC-0033 `borrowed`: this payload has at least one field
    /// that slices into the receive buffer, so a `_View` + `deserialize_view`
    /// is emitted beside the owned struct.
    pub has_borrowed_request: bool,
    pub has_borrowed_response: bool,
}

#[derive(serde::Serialize)]
pub struct CargoNrosTomlTemplate<'a> {
    pub package_name: &'a str,
    pub package_version: &'a str,
    pub dependencies: &'a [String],
    /// issue #234 — action packages emit an unconditional `nros-rmw` dep so the
    /// generated `impl RosAction`'s `register_protocol_types()` can register the
    /// `action_msgs` protocol types through the generic
    /// `nros_rmw::register_type_descriptor` seam (a no-op unless a
    /// descriptor-needing backend like Cyclone DDS is linked). `false` for plain
    /// msg/srv packages. Superseded the pre-#234 `rmw-cyclonedds`-feature +
    /// optional `nros-rmw-cyclonedds` dep, which was inert unless the consumer
    /// also enabled this crate's `rmw-cyclonedds` feature.
    pub has_actions: bool,
}

#[derive(serde::Serialize)]
pub struct LibNrosRsTemplate {
    pub has_messages: bool,
    pub has_services: bool,
    pub has_actions: bool,
}

#[derive(serde::Serialize)]
pub struct ActionNrosTemplate<'a> {
    pub package_name: &'a str,
    pub action_name: &'a str,
    /// The nine distinct REP-2011 RIHS01 hashes an action emits (one per
    /// generated struct + the action itself). Iron+ compute them; Humble passes
    /// the placeholder for all nine.
    pub goal_type_hash: &'a str,
    pub result_type_hash: &'a str,
    pub feedback_type_hash: &'a str,
    pub send_goal_request_type_hash: &'a str,
    pub send_goal_response_type_hash: &'a str,
    pub get_result_request_type_hash: &'a str,
    pub get_result_response_type_hash: &'a str,
    pub feedback_message_type_hash: &'a str,
    pub action_hash: &'a str,
    /// Issue #0292 — the `<Action>_SendGoal` / `<Action>_GetResult` SERVICE
    /// hashes (distinct from `action_hash`), emitted as `SEND_GOAL_SERVICE_HASH`
    /// / `GET_RESULT_SERVICE_HASH` so the zenoh service keyexpr matches a stock
    /// `rmw_zenoh_cpp` peer.
    pub send_goal_service_hash: &'a str,
    pub get_result_service_hash: &'a str,
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
    /// Issue 0346 — RFC-0033 `borrowed`: this payload has at least one field
    /// that slices into the receive buffer, so a `_View` + `deserialize_view`
    /// is emitted beside the owned struct.
    pub has_borrowed_goal: bool,
    pub has_borrowed_result: bool,
    pub has_borrowed_feedback: bool,
}

// ============================================================================
// C Templates (for nros-c)
// ============================================================================

/// Field information for C code generation
#[derive(Clone, serde::Serialize)]
pub struct CField {
    pub name: String,
    /// RFC-0068 Stage 3 — the neutral facts the `c_type` / `c_array_suffix` pack
    /// filters compose the C type string from (phase-335 step 2), replacing the
    /// pre-baked `c_type` / `array_suffix` strings. `field_type` is the parsed
    /// type; `is_configurable` marks an unbounded string/sequence whose storage
    /// was resolved; `cap` is that resolved capacity; `current_package` is the
    /// generating package (the `NamespacedType { package: None }` fallback).
    pub field_type: rosidl_parser::FieldType,
    pub is_configurable: bool,
    pub cap: usize,
    pub current_package: String,
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
    /// RFC-0033: `mode = "view"` (Phase 235, issue 0021). The owned
    /// `{Msg}` struct keeps a fixed-capacity container for the publish path;
    /// the additionally-emitted `{Msg}_View` borrows this field zero-copy via
    /// [`borrowed_c_type`](Self::borrowed_c_type) /
    /// [`borrowed_read_fn`](Self::borrowed_read_fn).
    pub is_borrowed: bool,
    /// Borrowed-view C type from `nros/view.h` (e.g. `nros_view_str_t`,
    /// `nros_view_bytes_t`, `nros_le_slice_view_f32_t`). Empty unless
    /// [`is_borrowed`](Self::is_borrowed).
    pub borrowed_c_type: String,
    /// `nros/view.h` reader for the borrowed view (e.g.
    /// `nros_cdr_borrow_string`); all share one signature. Empty unless
    /// [`is_borrowed`](Self::is_borrowed).
    pub borrowed_read_fn: String,
}

/// Constant for C code generation
#[derive(Clone, serde::Serialize)]
pub struct CConstant {
    pub name: String,
    pub c_type: String,
    pub value: String,
}

#[derive(serde::Serialize)]
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
    /// RFC-0033 borrowed (Phase 235): any field is `mode = "view"`, so the
    /// `{Msg}_View` + `{Msg}_deserialize_view` + `<nros/view.h>` include
    /// are emitted.
    pub has_borrowed: bool,
    /// issue 0896 layer 2 — the type's serialized-size bound under XCDR1, or
    /// `None` when the type is unbounded or a nested type was unresolvable.
    ///
    /// TWO constants rather than one maxed value: the publish helper writes
    /// XCDR1 (the only encoding this stack emits) while a receive buffer must
    /// hold either, and the two genuinely differ — XCDR2 adds a 4-byte DHEADER
    /// and aligns 8-byte primitives to 4. A single number would be silently
    /// wrong for one consumer.
    pub max_serialized_size_xcdr1: Option<usize>,
    /// The same under XCDR2. See [`Self::max_serialized_size_xcdr1`].
    pub max_serialized_size_xcdr2: Option<usize>,
    /// Why there is no bound, when there is none — either the unbounded member
    /// (`header.frame_id (string)`) or the nested type that could not be
    /// reached. Emitted as a header comment so a reader can tell "we looked and
    /// there is no bound" from "we could not look".
    pub unbounded_reason: Option<String>,
}

#[derive(serde::Serialize)]
pub struct MessageCSourceTemplate<'a> {
    pub package_name: &'a str,
    pub message_name: &'a str,
    pub type_hash: &'a str,
    pub header_name: String,
    pub struct_name: String,
    pub fields: Vec<CField>,
    pub has_fields: bool,
    /// See [`MessageCHeaderTemplate::has_borrowed`].
    pub has_borrowed: bool,
}

#[derive(serde::Serialize)]
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
    /// Issue 0346 — RFC-0033 `borrowed`: this payload has at least one field
    /// that slices into the receive buffer, so a `_View` + `deserialize_view`
    /// is emitted beside the owned struct.
    pub has_borrowed_request: bool,
    pub has_borrowed_response: bool,
}

#[derive(serde::Serialize)]
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
    /// Issue 0346 — RFC-0033 `borrowed`: this payload has at least one field
    /// that slices into the receive buffer, so a `_View` + `deserialize_view`
    /// is emitted beside the owned struct.
    pub has_borrowed_request: bool,
    pub has_borrowed_response: bool,
}

#[derive(serde::Serialize)]
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
    /// Issue 0346 — RFC-0033 `borrowed`: this payload has at least one field
    /// that slices into the receive buffer, so a `_View` + `deserialize_view`
    /// is emitted beside the owned struct.
    pub has_borrowed_goal: bool,
    pub has_borrowed_result: bool,
    pub has_borrowed_feedback: bool,
}

#[derive(serde::Serialize)]
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
    /// Issue 0346 — RFC-0033 `borrowed`: this payload has at least one field
    /// that slices into the receive buffer, so a `_View` + `deserialize_view`
    /// is emitted beside the owned struct.
    pub has_borrowed_goal: bool,
    pub has_borrowed_result: bool,
    pub has_borrowed_feedback: bool,
}

// ============================================================================
// C++ Templates (for nros-cpp)
// ============================================================================

/// Field information for C++ FFI Rust code generation
#[derive(Clone, serde::Serialize)]
pub struct CppFfiField {
    pub name: String,
    /// RFC-0068 step 2 — the `cpp_repr_c_type` / `cpp_view_repr_type` pack filters
    /// compose the Rust repr(C) type from these neutral facts (was the pre-baked
    /// `repr_c_type` / `view_repr_type`). `struct_name` is the parent message
    /// struct (the sequence repr is a generated `{struct}_{field}_seq_t` name);
    /// `cap` the resolved capacity; `current_package` the nested self-ref.
    pub field_type: rosidl_parser::FieldType,
    pub struct_name: String,
    pub cap: Option<usize>,
    pub current_package: String,
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
    /// Nested teardown function name (issue #201 — recursive heap free for
    /// the deserializers' error paths; empty for non-nested fields)
    pub nested_teardown_fn: String,
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
    /// RFC-0033 `mode = "heap"`: heap-backed primitive sequence — the repr is a
    /// `*mut T` pointer trio, (de)serialized via raw pointers + the shared
    /// `nros_platform_malloc`/`free` allocator.
    pub is_heap: bool,
    /// The element's repr(C) type (e.g. `u8`, `f32`) — used by the heap
    /// deserialize codegen for `size_of::<T>()` and the `*mut T` cast.
    pub element_repr_type: String,
    /// RFC-0033 `mode = "view"` (Phase 235). The `{Msg}ViewRepr` FFI struct
    /// stores this field as `nros_cpp_borrow_t`; `{Msg}_ffi_deserialize_view`
    /// fills it via [`borrowed_reader_call`](Self::borrowed_reader_call).
    pub is_borrowed: bool,
    /// `CdrReader` borrow call (no `?`), e.g. `read_string()`,
    /// `read_slice_u8()`, `read_le_slice::<f32>()`. Empty unless borrowed.
    pub borrowed_reader_call: String,
    /// `true` for the `LeSpan` case — the FFI takes `.as_bytes().as_ptr()`
    /// (element count) instead of `.as_ptr()` / `.len()` (byte slice / string).
    pub borrowed_le: bool,
}

/// C++ field info for header generation (uses FixedString/FixedSequence types)
#[derive(Clone, serde::Serialize)]
pub struct CppField {
    pub name: String,
    /// RFC-0068 step 2 — the `cpp_type` / `cpp_array_suffix` pack filters compose
    /// the C++ header type from these neutral facts (was the pre-baked `cpp_type`
    /// / `array_suffix`). `cap = Some(n)` is an owned-with-capacity string/seq;
    /// `is_heap` the heap bridge; `current_package` the self-ref fallback.
    pub field_type: rosidl_parser::FieldType,
    pub is_heap: bool,
    pub cap: Option<usize>,
    pub current_package: String,
    /// RFC-0033 `mode = "view"` (Phase 235): emitted in `{Msg}View`.
    pub is_borrowed: bool,
    /// Borrowed view type for `{Msg}View` (`nros::StringView` / `Span<T>` /
    /// `LeSpan<T>`). Empty unless [`is_borrowed`](Self::is_borrowed).
    pub borrowed_cpp_type: String,
}

/// Sequence helper struct definition for Rust #[repr(C)]
#[derive(Clone, serde::Serialize)]
pub struct SequenceStructDef {
    /// Struct name (e.g., "std_msgs_msg_string_data_seq_t")
    pub struct_name: String,
    /// Element type (e.g., "i32", "[u8; 256]")
    pub element_type: String,
    /// Capacity (fixed sequences only; unused for heap)
    pub capacity: usize,
    /// RFC-0033 `mode = "heap"`: the Rust mirror is a pointer trio
    /// `{ data: *mut T, size: usize, capacity: usize }` (matching
    /// `nros::HeapSequence<T>`) rather than the fixed `{ size: u32, data: [T; N] }`.
    pub is_heap: bool,
}

#[derive(serde::Serialize)]
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
    /// RFC-0033 borrowed (Phase 235): emit `{Msg}View` + `deserialize_view`
    /// + `<nros/span.hpp>` when any field is `mode = "view"`.
    pub has_borrowed: bool,
    /// FFI symbol for the borrowed deserializer (`{Msg}_ffi_deserialize_view`).
    pub ffi_deserialize_view_fn: String,
}

/// TYPES half of the split C++ FFI glue (phase-306 W1, issue 0253): the
/// crate-mangled items — repr(C) structs, sequence helpers, plain
/// `serialize_/deserialize_/teardown_*_fields` fns — safe to duplicate across
/// per-package FFI crates.
#[derive(serde::Serialize)]
pub struct MessageCppTypesTemplate<'a> {
    pub package_name: &'a str,
    pub message_name: &'a str,
    pub repr_c_struct_name: String,
    pub serialize_fn: String,
    pub deserialize_fn: String,
    /// issue #201 — recursive heap-teardown fn name (the `_fini` analog),
    /// emitted unconditionally (empty body when the message owns no heap).
    pub teardown_fn: String,
    pub fields: Vec<CppFfiField>,
    pub sequence_structs: Vec<SequenceStructDef>,
    pub has_fields: bool,
    /// RFC-0033: at least one heap (`mode = "heap"`) field — gates the
    /// `nros_platform_malloc`/`free` extern decls.
    pub has_heap: bool,
    /// RFC-0033: at least one heap **string** field — gates the shared
    /// `nros_cpp_heap_str_t` repr struct.
    pub has_heap_string: bool,
    /// RFC-0033 borrowed (Phase 235): emit `nros_cpp_borrow_t`, `{Msg}ViewRepr`
    /// and the plain borrowed deserializer.
    pub has_borrowed: bool,
    /// `{Msg}ViewRepr` — the repr(C) struct the borrowed FFI fills.
    pub view_repr_struct_name: String,
    /// Internal borrowed deserialize fn name.
    pub deserialize_view_fn: String,
}

/// EXPORTS half of the split C++ FFI glue (phase-306 W1, issue 0253): ONLY the
/// `#[unsafe(no_mangle)]` `nros_cpp_{publish,serialize,deserialize}_*` C-ABI
/// wrappers. Included solely by the OWNING package's crate so each symbol is
/// defined exactly once across any combination of interface archives.
#[derive(serde::Serialize)]
pub struct MessageCppExportsTemplate<'a> {
    pub package_name: &'a str,
    pub message_name: &'a str,
    pub repr_c_struct_name: String,
    pub ffi_publish_fn: String,
    pub ffi_serialize_fn: String,
    pub ffi_deserialize_fn: String,
    pub serialize_fn: String,
    pub deserialize_fn: String,
    /// Heap publish-path sizing reads the heap fields' runtime lengths.
    pub fields: Vec<CppFfiField>,
    pub has_fields: bool,
    pub serialized_size_max: usize,
    /// RFC-0033: gates the heap publish-buffer path.
    pub has_heap: bool,
    /// RFC-0033 borrowed (Phase 235): emit the `{Msg}_ffi_deserialize_view`
    /// export.
    pub has_borrowed: bool,
    /// `{Msg}ViewRepr` — the repr(C) struct the borrowed FFI fills.
    pub view_repr_struct_name: String,
    /// Internal borrowed deserialize fn name.
    pub deserialize_view_fn: String,
    /// Exported FFI symbol (`{Msg}_ffi_deserialize_view`).
    pub ffi_deserialize_view_fn: String,
}

#[derive(serde::Serialize)]
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

#[derive(serde::Serialize)]
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
