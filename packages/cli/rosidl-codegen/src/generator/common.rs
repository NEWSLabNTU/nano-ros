use crate::{
    config::{CapacityResolver, FieldKind as CapFieldKind, StorageMode},
    templates::{CField, CppFfiField, CppField, FieldKind, NrosField, SequenceStructDef},
    types::{
        C_DEFAULT_SEQUENCE_CAPACITY, CPP_DEFAULT_SEQUENCE_CAPACITY, CPP_DEFAULT_STRING_CAPACITY,
        NrosCodegenMode, c_array_suffix_for_field, c_array_suffix_for_field_with_capacity,
        c_cdr_read_method, c_cdr_write_method, c_type_for_field, c_type_for_field_heap,
        c_type_for_field_with_capacity, cpp_array_suffix_for_field, cpp_type_for_field,
        cpp_type_for_field_heap, cpp_type_for_field_with_capacity, escape_keyword,
        nros_type_for_field_heap, nros_type_for_field_with_capacity, nros_type_for_field_with_mode,
        repr_c_type_for_field, repr_c_type_for_field_with_capacity, to_c_package_name,
    },
    utils::to_snake_case,
};
use rosidl_parser::{FieldType, PrimitiveType};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum GeneratorError {
    #[error("Template rendering failed: {0}")]
    TemplateError(#[from] askama::Error),

    #[error("Invalid message structure: {0}")]
    InvalidMessage(String),

    #[error(
        "{package}/{message}.{field}: storage mode '{mode}' is not yet supported \
         (Phase 229 ships 'owned'; 'heap' lands in 229.5, 'borrowed' in 229.6)"
    )]
    UnsupportedStorageMode {
        package: String,
        message: String,
        field: String,
        mode: &'static str,
    },

    #[error(
        "{package}/{message}.{field}: `borrowed` mode does not support element \
         type `{element}` — only fixed-width primitive sequences (`uint8[]`, \
         `int8[]`, `bool[]`, `float32[]`, `uint16[]`, …) and strings can borrow \
         zero-copy. Sequences of strings or nested messages have no fixed-width \
         byte span; use `mode = \"heap\"` or `\"owned\"` for those fields."
    )]
    UnsupportedBorrowedElement {
        package: String,
        message: String,
        field: String,
        element: String,
    },
}

/// Resolve the borrowed-view field type + full `CdrReader` read expression for
/// a `borrowed`-mode field (RFC-0033, Phase 229.6, issue 0007).
///
/// Returns `(borrowed_rust_type, read_expr)`:
/// - **strings** → `&'a str` via `reader.read_string()?`,
/// - **single-byte sequences** (`uint8[]`/`byte[]`/`int8[]`/`bool[]`) → true
///   `&'a [u8]` slices (no alignment concern),
/// - **multi-byte numeric sequences** (`float32[]`, `uint16[]`, …) → an
///   alignment-agnostic `nros_core::LeSliceView<'a, T>` (the alignment guard):
///   borrows the raw LE bytes zero-copy, decodes per element on access, so the
///   receive buffer need not be `T`-aligned.
///
/// Sequences of strings or nested messages are rejected — their elements are
/// not fixed-width byte spans, so they cannot borrow zero-copy.
fn nros_borrowed_view_for_field(
    field_type: &FieldType,
    package_name: &str,
    message_name: &str,
    field_name: &str,
) -> Result<(String, String), GeneratorError> {
    let unsupported = |elem: &str| GeneratorError::UnsupportedBorrowedElement {
        package: package_name.to_string(),
        message: message_name.to_string(),
        field: field_name.to_string(),
        element: elem.to_string(),
    };
    // Multi-byte numeric → alignment-agnostic `LeSliceView<'a, T>`.
    let le_view = |t: &str| {
        (
            format!("nros_core::LeSliceView<'a, {t}>"),
            format!("reader.read_le_slice::<{t}>()?"),
        )
    };
    match field_type {
        FieldType::String | FieldType::WString => {
            Ok(("&'a str".to_string(), "reader.read_string()?".to_string()))
        }
        FieldType::Sequence { element_type } => match element_type.as_ref() {
            FieldType::Primitive(PrimitiveType::UInt8 | PrimitiveType::Byte) => Ok((
                "&'a [u8]".to_string(),
                "reader.read_slice_u8()?".to_string(),
            )),
            FieldType::Primitive(PrimitiveType::Int8) => Ok((
                "&'a [u8]".to_string(),
                "reader.read_slice_i8()?".to_string(),
            )),
            FieldType::Primitive(PrimitiveType::Bool) => Ok((
                "&'a [u8]".to_string(),
                "reader.read_slice_bool()?".to_string(),
            )),
            FieldType::Primitive(PrimitiveType::UInt16) => Ok(le_view("u16")),
            FieldType::Primitive(PrimitiveType::Int16) => Ok(le_view("i16")),
            FieldType::Primitive(PrimitiveType::UInt32) => Ok(le_view("u32")),
            FieldType::Primitive(PrimitiveType::Int32) => Ok(le_view("i32")),
            FieldType::Primitive(PrimitiveType::UInt64) => Ok(le_view("u64")),
            FieldType::Primitive(PrimitiveType::Int64) => Ok(le_view("i64")),
            FieldType::Primitive(PrimitiveType::Float32) => Ok(le_view("f32")),
            FieldType::Primitive(PrimitiveType::Float64) => Ok(le_view("f64")),
            other => Err(unsupported(&format!("{other:?}"))),
        },
        other => Err(unsupported(&format!("{other:?}"))),
    }
}

/// Resolve the borrowed-view **C** field type + the `nros/borrowed.h` reader
/// function for a `borrowed`-mode field (RFC-0033, Phase 235, issue 0021).
///
/// Returns `(borrowed_c_type, borrow_fn)` — both `nros/borrowed.h` symbols. All
/// three readers share the signature
/// `int32_t fn(const uint8_t** ptr, const uint8_t* end, const uint8_t* origin, T* out)`,
/// so the C template calls them uniformly:
/// - **strings** → `nros_borrowed_str_t` / `nros_cdr_borrow_string`,
/// - **single-byte sequences** (`uint8[]`/`byte[]`/`int8[]`/`bool[]`) →
///   `nros_borrowed_bytes_t` / `nros_cdr_borrow_bytes`,
/// - **multi-byte numeric sequences** (`float32[]`, `uint16[]`, …) → the
///   alignment-agnostic `nros_le_slice_view_<t>_t` / `nros_cdr_borrow_le_slice_<t>`.
///
/// Sequences of strings or nested messages are rejected (no fixed-width byte
/// span) — same policy as the Rust generator.
fn c_borrowed_view_for_field(
    field_type: &FieldType,
    package_name: &str,
    message_name: &str,
    field_name: &str,
) -> Result<(String, String), GeneratorError> {
    let unsupported = |elem: &str| GeneratorError::UnsupportedBorrowedElement {
        package: package_name.to_string(),
        message: message_name.to_string(),
        field: field_name.to_string(),
        element: elem.to_string(),
    };
    let le_view = |suffix: &str| {
        (
            format!("nros_le_slice_view_{suffix}_t"),
            format!("nros_cdr_borrow_le_slice_{suffix}"),
        )
    };
    let bytes = || {
        (
            "nros_borrowed_bytes_t".to_string(),
            "nros_cdr_borrow_bytes".to_string(),
        )
    };
    match field_type {
        FieldType::String | FieldType::WString => Ok((
            "nros_borrowed_str_t".to_string(),
            "nros_cdr_borrow_string".to_string(),
        )),
        FieldType::Sequence { element_type } => match element_type.as_ref() {
            FieldType::Primitive(
                PrimitiveType::UInt8
                | PrimitiveType::Byte
                | PrimitiveType::Int8
                | PrimitiveType::Bool,
            ) => Ok(bytes()),
            FieldType::Primitive(PrimitiveType::UInt16) => Ok(le_view("u16")),
            FieldType::Primitive(PrimitiveType::Int16) => Ok(le_view("i16")),
            FieldType::Primitive(PrimitiveType::UInt32) => Ok(le_view("u32")),
            FieldType::Primitive(PrimitiveType::Int32) => Ok(le_view("i32")),
            FieldType::Primitive(PrimitiveType::UInt64) => Ok(le_view("u64")),
            FieldType::Primitive(PrimitiveType::Int64) => Ok(le_view("i64")),
            FieldType::Primitive(PrimitiveType::Float32) => Ok(le_view("f32")),
            FieldType::Primitive(PrimitiveType::Float64) => Ok(le_view("f64")),
            other => Err(unsupported(&format!("{other:?}"))),
        },
        other => Err(unsupported(&format!("{other:?}"))),
    }
}

/// Determine the exhaustive FieldKind enum variant for a given ROS 2 field type
/// This function provides compile-time guarantees that all field type combinations are handled
pub(crate) fn determine_field_kind(field_type: &FieldType) -> FieldKind {
    match field_type {
        // Scalar types
        FieldType::Primitive(_) => FieldKind::Primitive,

        FieldType::String => FieldKind::UnboundedString,
        FieldType::BoundedString(_) => FieldKind::BoundedString,

        FieldType::WString => FieldKind::UnboundedWString,
        FieldType::BoundedWString(_) => FieldKind::BoundedWString,

        FieldType::NamespacedType { .. } => FieldKind::NestedMessage,

        // Array types
        FieldType::Array { element_type, size } => {
            // Arrays > 32 elements don't impl Copy/Clone in Rust
            if *size > 32 {
                return FieldKind::LargeArray;
            }

            match element_type.as_ref() {
                FieldType::Primitive(_) => FieldKind::PrimitiveArray,

                FieldType::String => FieldKind::UnboundedStringArray,
                FieldType::BoundedString(_) => FieldKind::BoundedStringArray,

                FieldType::WString => FieldKind::UnboundedWStringArray,
                FieldType::BoundedWString(_) => FieldKind::BoundedWStringArray,

                _ => FieldKind::NestedMessageArray,
            }
        }

        // Bounded sequences (T[<=N])
        FieldType::BoundedSequence { element_type, .. } => match element_type.as_ref() {
            FieldType::Primitive(_) => FieldKind::BoundedPrimitiveSequence,

            FieldType::String => FieldKind::BoundedUnboundedStringSequence,
            FieldType::BoundedString(_) => FieldKind::BoundedBoundedStringSequence,

            FieldType::WString => FieldKind::BoundedUnboundedWStringSequence,
            FieldType::BoundedWString(_) => FieldKind::BoundedBoundedWStringSequence,

            _ => FieldKind::BoundedNestedMessageSequence,
        },

        // Unbounded sequences (T[])
        FieldType::Sequence { element_type } => match element_type.as_ref() {
            FieldType::Primitive(_) => FieldKind::UnboundedPrimitiveSequence,

            FieldType::String => FieldKind::UnboundedUnboundedStringSequence,
            FieldType::BoundedString(_) => FieldKind::UnboundedBoundedStringSequence,

            FieldType::WString => FieldKind::UnboundedUnboundedWStringSequence,
            FieldType::BoundedWString(_) => FieldKind::UnboundedBoundedWStringSequence,

            _ => FieldKind::UnboundedNestedMessageSequence,
        },
    }
}

/// Get the CDR primitive method name for a primitive type
pub(super) fn primitive_to_cdr_method(prim: &rosidl_parser::PrimitiveType) -> String {
    use rosidl_parser::PrimitiveType;
    match prim {
        PrimitiveType::Bool => "bool".to_string(),
        PrimitiveType::Byte => "u8".to_string(),
        PrimitiveType::Char => "u8".to_string(),
        PrimitiveType::Int8 => "i8".to_string(),
        PrimitiveType::UInt8 => "u8".to_string(),
        PrimitiveType::Int16 => "i16".to_string(),
        PrimitiveType::UInt16 => "u16".to_string(),
        PrimitiveType::Int32 => "i32".to_string(),
        PrimitiveType::UInt32 => "u32".to_string(),
        PrimitiveType::Int64 => "i64".to_string(),
        PrimitiveType::UInt64 => "u64".to_string(),
        PrimitiveType::Float32 => "f32".to_string(),
        PrimitiveType::Float64 => "f64".to_string(),
    }
}

/// Convert a Message field to NrosField with explicit codegen mode.
///
/// `resolver` supplies the per-field capacity for **unbounded** sequence/string
/// fields (RFC-0033). Bounded fields, arrays, primitives, and nested types are
/// unaffected. A non-`owned` storage mode is rejected in Phase 229 (`heap` and
/// `borrowed` land in 229.5 / 229.6).
pub(super) fn field_to_nros_field_with_mode(
    field: &rosidl_parser::Field,
    package_name: &str,
    message_name: &str,
    resolver: &CapacityResolver,
    mode: NrosCodegenMode,
) -> Result<NrosField, GeneratorError> {
    let name = escape_keyword(&field.name);

    // Resolve per-field capacity for the two configurable shapes: an unbounded
    // string and an unbounded sequence. Everything else keeps default rendering.
    let cap_kind = match &field.field_type {
        FieldType::String | FieldType::WString => Some(CapFieldKind::String),
        FieldType::Sequence { .. } => Some(CapFieldKind::Sequence),
        _ => None,
    };
    let mut is_heap = false;
    let mut is_borrowed = false;
    let mut borrowed_rust_type = String::new();
    let mut borrowed_read_expr = String::new();
    let rust_type = if let Some(kind) = cap_kind {
        let storage = resolver.resolve(package_name, message_name, &field.name, kind);
        match storage.mode {
            StorageMode::Owned => nros_type_for_field_with_capacity(
                &field.field_type,
                Some(package_name),
                mode,
                storage.cap,
            ),
            StorageMode::Heap => {
                is_heap = true;
                nros_type_for_field_heap(&field.field_type, Some(package_name), mode)
            }
            // RFC-0033 `borrowed` (Phase 229.6, issue 0007): the owned `{Msg}`
            // struct keeps a default-capacity owned container for the publish
            // path; the additionally-emitted `{Msg}View<'a>` borrows this field
            // zero-copy (see `borrowed_rust_type` / `borrowed_read_method`).
            StorageMode::Borrowed => {
                is_borrowed = true;
                let (bt, expr) = nros_borrowed_view_for_field(
                    &field.field_type,
                    package_name,
                    message_name,
                    &field.name,
                )?;
                borrowed_rust_type = bt;
                borrowed_read_expr = expr;
                nros_type_for_field_with_capacity(
                    &field.field_type,
                    Some(package_name),
                    mode,
                    storage.cap,
                )
            }
        }
    } else {
        nros_type_for_field_with_mode(&field.field_type, Some(package_name), mode)
    };

    // Determine field properties
    let (is_primitive, primitive_method) = match &field.field_type {
        FieldType::Primitive(prim) => (true, primitive_to_cdr_method(prim)),
        _ => (false, String::new()),
    };

    let is_string = matches!(
        &field.field_type,
        FieldType::String
            | FieldType::BoundedString(_)
            | FieldType::WString
            | FieldType::BoundedWString(_)
    );

    let (is_array, array_size) = match &field.field_type {
        FieldType::Array { size, .. } => (true, *size),
        _ => (false, 0),
    };

    let is_sequence = matches!(
        &field.field_type,
        FieldType::Sequence { .. } | FieldType::BoundedSequence { .. }
    );

    let is_nested = matches!(&field.field_type, FieldType::NamespacedType { .. });

    // Element type info for arrays and sequences
    let (is_primitive_element, is_string_element, element_primitive_method) =
        match &field.field_type {
            FieldType::Array { element_type, .. }
            | FieldType::Sequence { element_type }
            | FieldType::BoundedSequence { element_type, .. } => match element_type.as_ref() {
                FieldType::Primitive(prim) => (true, false, primitive_to_cdr_method(prim)),
                FieldType::String
                | FieldType::BoundedString(_)
                | FieldType::WString
                | FieldType::BoundedWString(_) => (false, true, String::new()),
                _ => (false, false, String::new()),
            },
            _ => (false, false, String::new()),
        };

    Ok(NrosField {
        name,
        rust_type,
        primitive_method,
        element_primitive_method,
        array_size,
        is_primitive,
        is_string,
        is_array,
        is_sequence,
        is_nested,
        is_primitive_element,
        is_string_element,
        is_large_array: array_size > 32,
        is_heap,
        is_borrowed,
        borrowed_rust_type,
        borrowed_read_expr,
    })
}

/// Convert a Message field to NrosField (crate mode).
pub(super) fn field_to_nros_field(
    field: &rosidl_parser::Field,
    package_name: &str,
    message_name: &str,
    resolver: &CapacityResolver,
) -> Result<NrosField, GeneratorError> {
    field_to_nros_field_with_mode(
        field,
        package_name,
        message_name,
        resolver,
        NrosCodegenMode::Crate,
    )
}

/// Build a CField from a field type.
///
/// `resolver` supplies the per-field capacity for **unbounded** sequence/string
/// fields (RFC-0033). A non-`owned` storage mode is rejected in Phase 229.
pub(super) fn build_c_field(
    name: &str,
    field_type: &FieldType,
    current_package: Option<&str>,
    message_name: &str,
    resolver: &CapacityResolver,
) -> Result<CField, GeneratorError> {
    let escaped_name = escape_keyword(name);

    // Resolve per-field capacity for the two configurable shapes.
    let cap_kind = match field_type {
        FieldType::String | FieldType::WString => Some(CapFieldKind::String),
        FieldType::Sequence { .. } => Some(CapFieldKind::Sequence),
        _ => None,
    };
    let unsupported = |mode: &'static str| GeneratorError::UnsupportedStorageMode {
        package: current_package.unwrap_or("").to_string(),
        message: message_name.to_string(),
        field: name.to_string(),
        mode,
    };
    // (c_type, array_suffix, is_heap, resolved owned sequence capacity).
    let mut is_heap = false;
    let mut is_borrowed = false;
    let mut borrowed_c_type = String::new();
    let mut borrowed_read_fn = String::new();
    let mut owned_seq_cap: Option<usize> = None;
    let (c_type, array_suffix) = if let Some(kind) = cap_kind {
        let package = current_package.unwrap_or("");
        let storage = resolver.resolve(package, message_name, name, kind);
        match storage.mode {
            StorageMode::Owned => {
                if matches!(field_type, FieldType::Sequence { .. }) {
                    owned_seq_cap = Some(storage.cap);
                }
                (
                    c_type_for_field_with_capacity(field_type, current_package, storage.cap),
                    c_array_suffix_for_field_with_capacity(field_type, storage.cap),
                )
            }
            StorageMode::Heap => match c_type_for_field_heap(field_type, current_package) {
                // Heap-backed `{ T* data; size_t size, capacity; }` — no suffix,
                // unbounded. Mallocs on deserialize; freed by `_fini`.
                Some(ty) => {
                    is_heap = true;
                    (ty, String::new())
                }
                None => return Err(unsupported("heap")),
            },
            // RFC-0033 `borrowed` (Phase 235, issue 0021): the owned `{Msg}`
            // struct keeps its resolved-capacity container for the publish path;
            // the emitted `{Msg}_View` borrows this field zero-copy (see
            // `borrowed_c_type` / `borrowed_read_fn`).
            StorageMode::Borrowed => {
                is_borrowed = true;
                let (bt, bfn) = c_borrowed_view_for_field(field_type, package, message_name, name)?;
                borrowed_c_type = bt;
                borrowed_read_fn = bfn;
                if matches!(field_type, FieldType::Sequence { .. }) {
                    owned_seq_cap = Some(storage.cap);
                }
                (
                    c_type_for_field_with_capacity(field_type, current_package, storage.cap),
                    c_array_suffix_for_field_with_capacity(field_type, storage.cap),
                )
            }
        }
    } else {
        (
            c_type_for_field(field_type, current_package),
            c_array_suffix_for_field(field_type),
        )
    };

    // Determine type characteristics
    let (is_primitive, primitive_type) = match field_type {
        FieldType::Primitive(prim) => (true, Some(prim)),
        _ => (false, None),
    };

    let is_string = matches!(
        field_type,
        FieldType::String
            | FieldType::BoundedString(_)
            | FieldType::WString
            | FieldType::BoundedWString(_)
    );

    let is_array = matches!(field_type, FieldType::Array { .. });
    let is_sequence = matches!(
        field_type,
        FieldType::Sequence { .. } | FieldType::BoundedSequence { .. }
    );
    let is_nested = matches!(field_type, FieldType::NamespacedType { .. });

    // Get array/sequence info. Owned unbounded sequences use the resolved
    // capacity; heap sequences are unbounded (capacity unused → 0).
    let (array_size, sequence_capacity) = match field_type {
        FieldType::Array { size, .. } => (*size, 0),
        FieldType::Sequence { .. } => (0, owned_seq_cap.unwrap_or(C_DEFAULT_SEQUENCE_CAPACITY)),
        FieldType::BoundedSequence { max_size, .. } => (0, *max_size),
        _ => (0, 0),
    };

    // Get element info for arrays/sequences
    let (is_primitive_element, is_string_element, element_type) = match field_type {
        FieldType::Array { element_type, .. }
        | FieldType::Sequence { element_type }
        | FieldType::BoundedSequence { element_type, .. } => {
            let is_prim = matches!(element_type.as_ref(), FieldType::Primitive(_));
            let is_str = matches!(
                element_type.as_ref(),
                FieldType::String
                    | FieldType::BoundedString(_)
                    | FieldType::WString
                    | FieldType::BoundedWString(_)
            );
            (is_prim, is_str, Some(element_type.as_ref()))
        }
        _ => (false, false, None),
    };

    // Get CDR methods
    let (cdr_write_method, cdr_read_method) = if let Some(prim) = primitive_type {
        (
            c_cdr_write_method(prim).to_string(),
            c_cdr_read_method(prim).to_string(),
        )
    } else {
        (String::new(), String::new())
    };

    let (element_cdr_write_method, element_cdr_read_method) =
        if let Some(FieldType::Primitive(prim)) = element_type {
            (
                c_cdr_write_method(prim).to_string(),
                c_cdr_read_method(prim).to_string(),
            )
        } else {
            (String::new(), String::new())
        };

    // Get nested struct names (use current_package for intra-package references)
    let nested_struct_name = if let FieldType::NamespacedType { package, name } = field_type {
        let pkg = package.as_deref().or(current_package).unwrap_or("");
        format!("{}_msg_{}", to_c_package_name(pkg), to_snake_case(name))
    } else {
        String::new()
    };

    let element_struct_name =
        if let Some(FieldType::NamespacedType { package, name }) = element_type {
            let pkg = package.as_deref().or(current_package).unwrap_or("");
            format!("{}_msg_{}", to_c_package_name(pkg), to_snake_case(name))
        } else {
            String::new()
        };

    Ok(CField {
        name: escaped_name,
        c_type,
        array_suffix,
        cdr_write_method,
        cdr_read_method,
        element_cdr_write_method,
        element_cdr_read_method,
        array_size,
        sequence_capacity,
        nested_struct_name,
        element_struct_name,
        is_primitive,
        is_string,
        is_array,
        is_sequence,
        is_nested,
        is_primitive_element,
        is_string_element,
        is_heap,
        is_borrowed,
        borrowed_c_type,
        borrowed_read_fn,
    })
}

/// Resolved C++ storage for a field (RFC-0033). `Owned(cap)` keeps a
/// fixed-capacity container (`cap` = `Some` only for the configurable
/// string/sequence shapes); `Heap` is an `nros::HeapSequence<T>` primitive
/// sequence.
#[derive(Clone, Copy)]
pub(super) enum CppStorage {
    Owned(Option<usize>),
    Heap,
    /// RFC-0033 `borrowed` (Phase 235): a zero-copy view into the CDR buffer.
    /// Carries the resolved owned capacity too — the owned `{Msg}` struct keeps
    /// a fixed-capacity container for the publish path; only `{Msg}View` borrows.
    Borrowed(CppBorrow, Option<usize>),
}

/// The borrowed C++ view kind for a `mode = "borrowed"` field (Phase 235).
/// Carries `'static` strings so `CppStorage` stays `Copy`.
#[derive(Clone, Copy)]
pub(super) enum CppBorrow {
    /// `nros::StringView` via `CdrReader::read_string`.
    Str,
    /// `nros::Span<cpp>` via `reader` (`read_slice_u8`/`_i8`/`_bool`).
    Bytes {
        cpp: &'static str,
        reader: &'static str,
    },
    /// `nros::LeSpan<cpp>` via `read_le_slice::<suffix>` (alignment-agnostic).
    Le {
        cpp: &'static str,
        suffix: &'static str,
    },
}

impl CppBorrow {
    /// The C++ view type for the header (`{Msg}View`) field.
    pub(super) fn cpp_view_type(self) -> String {
        match self {
            CppBorrow::Str => "nros::StringView".to_string(),
            CppBorrow::Bytes { cpp, .. } => format!("nros::Span<{cpp}>"),
            CppBorrow::Le { cpp, .. } => format!("nros::LeSpan<{cpp}>"),
        }
    }
    /// The `CdrReader` call (no `?`) that borrows this field's bytes.
    pub(super) fn reader_call(self) -> String {
        match self {
            CppBorrow::Str => "read_string()".to_string(),
            CppBorrow::Bytes { reader, .. } => format!("{reader}()"),
            CppBorrow::Le { suffix, .. } => format!("read_le_slice::<{suffix}>()"),
        }
    }
    /// `true` for the `LeSpan` case (the FFI extracts `.as_bytes().as_ptr()`
    /// instead of `.as_ptr()`).
    pub(super) fn is_le(self) -> bool {
        matches!(self, CppBorrow::Le { .. })
    }
}

/// Resolve the borrowed C++ view kind for a `borrowed`-mode field. Strings and
/// byte/numeric sequences borrow; sequences of strings or nested messages are
/// rejected (no fixed-width byte span) — same policy as Rust and C.
fn cpp_borrow_kind(
    field_type: &FieldType,
    package_name: &str,
    message_name: &str,
    field_name: &str,
) -> Result<CppBorrow, GeneratorError> {
    let unsupported = |elem: &str| GeneratorError::UnsupportedBorrowedElement {
        package: package_name.to_string(),
        message: message_name.to_string(),
        field: field_name.to_string(),
        element: elem.to_string(),
    };
    match field_type {
        FieldType::String | FieldType::WString => Ok(CppBorrow::Str),
        FieldType::Sequence { element_type } => match element_type.as_ref() {
            FieldType::Primitive(PrimitiveType::UInt8 | PrimitiveType::Byte) => {
                Ok(CppBorrow::Bytes {
                    cpp: "uint8_t",
                    reader: "read_slice_u8",
                })
            }
            FieldType::Primitive(PrimitiveType::Int8) => Ok(CppBorrow::Bytes {
                cpp: "int8_t",
                reader: "read_slice_i8",
            }),
            FieldType::Primitive(PrimitiveType::Bool) => Ok(CppBorrow::Bytes {
                cpp: "uint8_t",
                reader: "read_slice_bool",
            }),
            FieldType::Primitive(PrimitiveType::UInt16) => Ok(CppBorrow::Le {
                cpp: "uint16_t",
                suffix: "u16",
            }),
            FieldType::Primitive(PrimitiveType::Int16) => Ok(CppBorrow::Le {
                cpp: "int16_t",
                suffix: "i16",
            }),
            FieldType::Primitive(PrimitiveType::UInt32) => Ok(CppBorrow::Le {
                cpp: "uint32_t",
                suffix: "u32",
            }),
            FieldType::Primitive(PrimitiveType::Int32) => Ok(CppBorrow::Le {
                cpp: "int32_t",
                suffix: "i32",
            }),
            FieldType::Primitive(PrimitiveType::UInt64) => Ok(CppBorrow::Le {
                cpp: "uint64_t",
                suffix: "u64",
            }),
            FieldType::Primitive(PrimitiveType::Int64) => Ok(CppBorrow::Le {
                cpp: "int64_t",
                suffix: "i64",
            }),
            FieldType::Primitive(PrimitiveType::Float32) => Ok(CppBorrow::Le {
                cpp: "float",
                suffix: "f32",
            }),
            FieldType::Primitive(PrimitiveType::Float64) => Ok(CppBorrow::Le {
                cpp: "double",
                suffix: "f64",
            }),
            other => Err(unsupported(&format!("{other:?}"))),
        },
        other => Err(unsupported(&format!("{other:?}"))),
    }
}

/// Resolve the per-field storage for a C++ field (RFC-0033). Owned shapes carry
/// their resolved capacity; `heap` is allowed only for primitive sequences
/// (rejected for heap strings / sequences of strings / nested messages, and for
/// `borrowed`). Shared by the C++ header + FFI builders so both agree.
pub(super) fn resolve_cap_override(
    name: &str,
    field_type: &FieldType,
    current_package: Option<&str>,
    message_name: &str,
    resolver: &CapacityResolver,
) -> Result<CppStorage, GeneratorError> {
    let kind = match field_type {
        FieldType::String | FieldType::WString => CapFieldKind::String,
        FieldType::Sequence { .. } => CapFieldKind::Sequence,
        _ => return Ok(CppStorage::Owned(None)),
    };
    let package = current_package.unwrap_or("");
    let unsupported = |mode: &'static str| GeneratorError::UnsupportedStorageMode {
        package: package.to_string(),
        message: message_name.to_string(),
        field: name.to_string(),
        mode,
    };
    let storage = resolver.resolve(package, message_name, name, kind);
    match storage.mode {
        StorageMode::Owned => Ok(CppStorage::Owned(Some(storage.cap))),
        StorageMode::Heap => {
            // Heap is only bridgeable for primitive sequences (see
            // cpp_type_for_field_heap); reject heap strings / non-primitive seqs.
            if cpp_type_for_field_heap(field_type, current_package).is_some() {
                Ok(CppStorage::Heap)
            } else {
                Err(unsupported("heap"))
            }
        }
        StorageMode::Borrowed => Ok(CppStorage::Borrowed(
            cpp_borrow_kind(field_type, package, message_name, name)?,
            Some(storage.cap),
        )),
    }
}

/// Build a CppField for C++ header generation.
///
/// `storage` is the resolved per-field storage (RFC-0033): an owned
/// fixed-capacity container or an `nros::HeapSequence<T>` heap sequence.
pub(super) fn build_cpp_field(
    name: &str,
    field_type: &FieldType,
    current_package: Option<&str>,
    storage: CppStorage,
) -> CppField {
    let escaped_name = escape_keyword(name);
    // RFC-0033 borrowed (Phase 235): the owned `{Msg}` struct keeps a
    // fixed-capacity container (publish path), while `{Msg}View` uses the
    // zero-copy view type (`nros::StringView` / `Span<T>` / `LeSpan<T>`).
    if let CppStorage::Borrowed(b, cap) = storage {
        let owned = match cap {
            Some(c) => cpp_type_for_field_with_capacity(field_type, current_package, c),
            None => cpp_type_for_field(field_type, current_package),
        };
        return CppField {
            name: escaped_name,
            cpp_type: owned,
            array_suffix: String::new(),
            is_borrowed: true,
            borrowed_cpp_type: b.cpp_view_type(),
        };
    }
    let cpp_type = match storage {
        CppStorage::Owned(Some(cap)) => {
            cpp_type_for_field_with_capacity(field_type, current_package, cap)
        }
        CppStorage::Owned(None) => cpp_type_for_field(field_type, current_package),
        CppStorage::Heap => cpp_type_for_field_heap(field_type, current_package)
            .expect("resolve_cap_override only yields Heap for bridgeable shapes"),
        CppStorage::Borrowed(..) => unreachable!("handled above"),
    };
    let array_suffix = cpp_array_suffix_for_field(field_type);

    // For arrays, the cpp_type already contains the base type, and array_suffix has [N]
    // For FixedString/FixedSequence, cpp_type is the full type, no suffix needed
    // But for fixed-size arrays of primitives, cpp_type is "int32_t[3]" — split it
    let (final_type, final_suffix) = if !array_suffix.is_empty() {
        // Array field: base type is without the [N] suffix
        let base = match field_type {
            FieldType::Array { element_type, .. } => {
                cpp_type_for_field(element_type, current_package)
            }
            _ => cpp_type,
        };
        (base, array_suffix)
    } else {
        (cpp_type, String::new())
    };

    CppField {
        name: escaped_name,
        cpp_type: final_type,
        array_suffix: final_suffix,
        is_borrowed: false,
        borrowed_cpp_type: String::new(),
    }
}

/// Build a CppFfiField and optional SequenceStructDef for Rust FFI glue generation
pub(super) fn build_cpp_ffi_field(
    name: &str,
    field_type: &FieldType,
    struct_name: &str,
    current_package: Option<&str>,
    storage: CppStorage,
) -> (CppFfiField, Option<SequenceStructDef>) {
    let escaped_name = escape_keyword(name);
    let is_heap = matches!(storage, CppStorage::Heap);
    let cap_override = match storage {
        CppStorage::Owned(cap) => cap,
        CppStorage::Heap => None,
        CppStorage::Borrowed(_, cap) => cap,
    };
    // RFC-0033 borrowed (Phase 235): the `{Msg}ViewRepr` FFI struct stores this
    // field as `nros_cpp_borrow_t { *const u8, usize }`, filled from the
    // `CdrReader` borrow method; the owned `{Msg}` repr keeps its owned type.
    let (is_borrowed, borrowed_reader_call, borrowed_le) = match storage {
        CppStorage::Borrowed(b, _) => (true, b.reader_call(), b.is_le()),
        _ => (false, String::new(), false),
    };

    // Determine type characteristics
    let (is_primitive, primitive_type) = match field_type {
        FieldType::Primitive(prim) => (true, Some(prim)),
        _ => (false, None),
    };

    let is_string = matches!(
        field_type,
        FieldType::String
            | FieldType::BoundedString(_)
            | FieldType::WString
            | FieldType::BoundedWString(_)
    );

    let is_array = matches!(field_type, FieldType::Array { .. });
    let is_sequence = matches!(
        field_type,
        FieldType::Sequence { .. } | FieldType::BoundedSequence { .. }
    );
    let is_nested = matches!(field_type, FieldType::NamespacedType { .. });

    // Array/sequence size info. Unbounded sequences use the resolved capacity.
    let (array_size, sequence_capacity) = match field_type {
        FieldType::Array { size, .. } => (*size, 0),
        FieldType::Sequence { .. } => (0, cap_override.unwrap_or(CPP_DEFAULT_SEQUENCE_CAPACITY)),
        FieldType::BoundedSequence { max_size, .. } => (0, *max_size),
        _ => (0, 0),
    };

    // Element type info
    let (is_primitive_element, is_string_element, element_type) = match field_type {
        FieldType::Array { element_type, .. }
        | FieldType::Sequence { element_type }
        | FieldType::BoundedSequence { element_type, .. } => {
            let is_prim = matches!(element_type.as_ref(), FieldType::Primitive(_));
            let is_str = matches!(
                element_type.as_ref(),
                FieldType::String
                    | FieldType::BoundedString(_)
                    | FieldType::WString
                    | FieldType::BoundedWString(_)
            );
            (is_prim, is_str, Some(element_type.as_ref()))
        }
        _ => (false, false, None),
    };

    // CDR methods for primitives
    let (cdr_write_method, cdr_read_method) = if let Some(prim) = primitive_type {
        (
            c_cdr_write_method(prim).to_string(),
            c_cdr_read_method(prim).to_string(),
        )
    } else {
        (String::new(), String::new())
    };

    let (element_cdr_write_method, element_cdr_read_method) =
        if let Some(FieldType::Primitive(prim)) = element_type {
            (
                c_cdr_write_method(prim).to_string(),
                c_cdr_read_method(prim).to_string(),
            )
        } else {
            (String::new(), String::new())
        };

    // Nested function names
    let nested_serialize_fn = if let FieldType::NamespacedType { package, name: n } = field_type {
        let pkg = package.as_deref().or(current_package).unwrap_or("unknown");
        format!(
            "serialize_{}_msg_{}_fields",
            to_c_package_name(pkg),
            to_snake_case(n)
        )
    } else {
        String::new()
    };

    let nested_deserialize_fn = if let FieldType::NamespacedType { package, name: n } = field_type {
        let pkg = package.as_deref().or(current_package).unwrap_or("unknown");
        format!(
            "deserialize_{}_msg_{}_fields",
            to_c_package_name(pkg),
            to_snake_case(n)
        )
    } else {
        String::new()
    };

    // issue #201 — recursive heap teardown (the Rust-glue analog of the C
    // `_fini` recursion): frees + nulls a message's heap allocations so the
    // deserializers' error paths can tear down locally-allocated element
    // buffers whose nested elements own heap memory.
    let nested_teardown_fn = if let FieldType::NamespacedType { package, name: n } = field_type {
        let pkg = package.as_deref().or(current_package).unwrap_or("unknown");
        format!(
            "teardown_{}_msg_{}_fields",
            to_c_package_name(pkg),
            to_snake_case(n)
        )
    } else {
        String::new()
    };

    // Element nested function names (for arrays/sequences of nested types)
    let (elem_nested_ser, elem_nested_deser, elem_nested_teardown) =
        if let Some(FieldType::NamespacedType { package, name: n }) = element_type {
            let pkg = package.as_deref().or(current_package).unwrap_or("unknown");
            (
                format!(
                    "serialize_{}_msg_{}_fields",
                    to_c_package_name(pkg),
                    to_snake_case(n)
                ),
                format!(
                    "deserialize_{}_msg_{}_fields",
                    to_c_package_name(pkg),
                    to_snake_case(n)
                ),
                format!(
                    "teardown_{}_msg_{}_fields",
                    to_c_package_name(pkg),
                    to_snake_case(n)
                ),
            )
        } else {
            (String::new(), String::new(), String::new())
        };

    // Compute repr(C) type
    let repr_c_type = if is_sequence {
        // Sequence uses named struct
        let seq_struct_name = format!("{}_{}_seq_t", struct_name, to_snake_case(name));
        seq_struct_name
    } else if is_heap && is_string {
        // Heap string → the shared pointer-trio repr (matches nros::HeapString).
        "nros_cpp_heap_str_t".to_string()
    } else {
        match cap_override {
            Some(cap) => repr_c_type_for_field_with_capacity(field_type, current_package, cap),
            None => repr_c_type_for_field(field_type, current_package),
        }
    };

    // The repr(C) type of a sequence element (Rust mirror). Shared by the
    // SequenceStructDef and — for heap sequences — `element_repr_type` (used for
    // `size_of::<T>()` + the `*mut T` cast in the FFI deserializer).
    let elem_repr_c = match element_type {
        Some(FieldType::Primitive(prim)) => {
            repr_c_type_for_field(&FieldType::Primitive(*prim), current_package)
        }
        Some(FieldType::String) | Some(FieldType::WString) => {
            format!("[u8; {}]", CPP_DEFAULT_STRING_CAPACITY)
        }
        Some(FieldType::BoundedString(sz)) | Some(FieldType::BoundedWString(sz)) => {
            format!("[u8; {sz}]")
        }
        Some(FieldType::NamespacedType { package, name: n }) => {
            // When package is None the element type is from the current package
            let pkg = package.as_deref().or(current_package).unwrap_or("unknown");
            format!("{}_msg_{}_t", to_c_package_name(pkg), to_snake_case(n))
        }
        _ => "u8".to_string(),
    };

    // Build sequence struct def if needed
    let seq_struct = if is_sequence {
        Some(SequenceStructDef {
            struct_name: format!("{}_{}_seq_t", struct_name, to_snake_case(name)),
            element_type: elem_repr_c.clone(),
            capacity: sequence_capacity,
            is_heap,
        })
    } else {
        None
    };

    // For a heap sequence the FFI deserializer needs the element repr type.
    let element_repr_type = if is_sequence {
        elem_repr_c
    } else {
        String::new()
    };

    // Use element nested functions for array/sequence elements
    let final_nested_ser = if is_nested {
        nested_serialize_fn
    } else {
        elem_nested_ser
    };
    let final_nested_deser = if is_nested {
        nested_deserialize_fn
    } else {
        elem_nested_deser
    };
    let final_nested_teardown = if is_nested {
        nested_teardown_fn
    } else {
        elem_nested_teardown
    };

    // String capacity for deserialization (resolved for unbounded strings).
    let string_capacity = match field_type {
        FieldType::String | FieldType::WString => {
            cap_override.unwrap_or(CPP_DEFAULT_STRING_CAPACITY)
        }
        FieldType::BoundedString(sz) | FieldType::BoundedWString(sz) => *sz,
        _ => 0,
    };

    let element_string_capacity = match element_type {
        Some(FieldType::String) | Some(FieldType::WString) => CPP_DEFAULT_STRING_CAPACITY,
        Some(FieldType::BoundedString(sz)) | Some(FieldType::BoundedWString(sz)) => *sz,
        _ => 0,
    };

    // `{Msg}ViewRepr` field type: borrowed → the shared `nros_cpp_borrow_t`
    // pointer+len; everything else keeps its owned repr (copied into the view).
    let view_repr_type = if is_borrowed {
        "nros_cpp_borrow_t".to_string()
    } else {
        repr_c_type.clone()
    };

    let field = CppFfiField {
        name: escaped_name,
        repr_c_type,
        cdr_write_method,
        cdr_read_method,
        element_cdr_write_method,
        element_cdr_read_method,
        array_size,
        sequence_capacity,
        nested_serialize_fn: final_nested_ser,
        nested_deserialize_fn: final_nested_deser,
        nested_teardown_fn: final_nested_teardown,
        string_capacity,
        element_string_capacity,
        is_primitive,
        is_string,
        is_array,
        is_sequence,
        is_nested,
        is_primitive_element,
        is_string_element,
        is_heap,
        element_repr_type,
        is_borrowed,
        borrowed_reader_call,
        borrowed_le,
        view_repr_type,
    };

    (field, seq_struct)
}

// ============================================================================
// nros-serdes::Message schema builder
// ============================================================================
//
// Emits the `impl ::nros_serdes::Message for <Msg>` block + any helper
// `pub const` items (NestedType + element FieldType statics) so backends
// like `nros-rmw-cyclonedds` (Phase 212.K.7.4-6) can walk the static
// field schema at runtime via `<M as Message>::FIELDS` / `TYPE_NAME`.
//
// Per-field expressions reference helper consts (`FT_<name>`, `NESTED_<name>`)
// rather than inlining `&FieldType::...` literals — `&FieldType::Foo` doesn't
// yield a `&'static FieldType` because the temporary is dropped at end of
// expression. Top-level `pub const` items live for `'static` and provide
// the stable address the recursive variants need.

/// Schema artefacts attached to a generated nros message struct.
///
/// `nros_type_name` is the package-qualified ROS type name (e.g.
/// `"std_msgs/msg/Header"`) used for `Message::TYPE_NAME`.
///
/// `helper_consts` is a (possibly empty) block of `pub const` items that
/// must be emitted in the same module as the message struct so the
/// recursive `FieldType::Array(_, &FT_FOO)` / `FieldType::Nested(&NESTED_FOO)`
/// references resolve to `'static` addresses.
///
/// `fields_block` is the body of the `Message::FIELDS` slice — one
/// `::nros_serdes::Field { … },` per IDL field, in declaration order.
#[derive(Debug, Clone, Default)]
pub struct NrosMessageSchema {
    pub nros_type_name: String,
    pub helper_consts: String,
    pub fields_block: String,
}

/// Build the [`NrosMessageSchema`] for a parsed `.msg` body.
///
/// Uses the standard message convention: struct identifier matches
/// `message_name`, and `TYPE_NAME` is `<pkg>/msg/<MessageName>`.
/// Helper consts are emitted unprefixed (`NESTED_<X>`, `FT_<X>_ELEM`)
/// since a `.msg` package emits a single Message impl per file.
pub fn build_nros_message_schema(
    package_name: &str,
    message_name: &str,
    fields: &[rosidl_parser::Field],
) -> NrosMessageSchema {
    let nros_type_name = format!("{}/msg/{}", package_name, message_name);
    build_nros_schema_for_struct(package_name, message_name, &nros_type_name, "", fields)
}

/// Build the [`NrosMessageSchema`] for a Rust struct whose identifier
/// differs from its `Message::TYPE_NAME` payload.
///
/// Used by the service / action emit paths (K.7.1.c) where the Rust
/// struct name is e.g. `AddTwoIntsRequest` but the wire type-name
/// follows rosidl convention (`example_interfaces/srv/AddTwoInts_Request`).
///
/// `struct_name` is the Rust ident referenced by `offset_of!` macros.
/// `nros_type_name` is the ROS-side type name string written into
/// `Message::TYPE_NAME`.
/// `const_prefix` namespaces helper consts (`<prefix>NESTED_<X>`,
/// `<prefix>FT_<X>_ELEM`) so multiple schemas emitted in the same
/// module (service Request + Response, action Goal/Result/Feedback)
/// don't collide on shared field names. Pass `""` for the single-schema
/// `.msg` case.
pub fn build_nros_schema_for_struct(
    package_name: &str,
    struct_name: &str,
    nros_type_name: &str,
    const_prefix: &str,
    fields: &[rosidl_parser::Field],
) -> NrosMessageSchema {
    build_nros_schema_for_struct_with_path(
        package_name,
        struct_name,
        nros_type_name,
        const_prefix,
        fields,
        &default_nested_type_path,
    )
}

/// Like [`build_nros_schema_for_struct`] but lets the caller override how
/// a `NamespacedType { package, name }` is rendered as a Rust path. The
/// default ([`default_nested_type_path`]) follows the `.msg` convention
/// (`crate::msg::<X>` / `<pkg>::msg::<X>`). The K.7.1.d action envelope
/// emit path uses a custom resolver to reach the action-self structs
/// (`<Action>Goal/Result/Feedback`, same module — bare ident) and the
/// `unique_identifier_msgs::msg::UUID` / `builtin_interfaces::msg::Time`
/// nested types (default path).
pub fn build_nros_schema_for_struct_with_path(
    package_name: &str,
    struct_name: &str,
    nros_type_name: &str,
    const_prefix: &str,
    fields: &[rosidl_parser::Field],
    nested_path_resolver: &dyn Fn(Option<&str>, &str, &str) -> String,
) -> NrosMessageSchema {
    let mut helper_consts = String::new();
    let mut fields_block = String::new();

    for field in fields {
        // Use the *raw* IDL field name for the schema (matches the .msg
        // source); the rendered struct field still goes through
        // `escape_keyword` to dodge Rust reserved words.
        let raw_name = &field.name;
        let access_name = escape_keyword(raw_name);
        let ty_expr = render_field_type_expr(
            raw_name,
            &field.field_type,
            package_name,
            const_prefix,
            &mut helper_consts,
            nested_path_resolver,
        );
        fields_block.push_str(&format!(
            "        ::nros_serdes::Field {{\n            \
             name: \"{name}\",\n            \
             ty: {ty_expr},\n            \
             offset: ::core::mem::offset_of!({msg}, {access}),\n        }},\n",
            name = raw_name,
            ty_expr = ty_expr,
            msg = struct_name,
            access = access_name,
        ));
    }

    NrosMessageSchema {
        nros_type_name: nros_type_name.to_string(),
        helper_consts,
        fields_block,
    }
}

/// Emit the FieldType expression for a single field. Recursive variants
/// hoist their inner FieldType / NestedType into a module-scoped
/// `pub const`, appended to `helper_consts`, and reference it by name.
///
/// `const_prefix` namespaces the emitted helper-const idents so multiple
/// schemas in the same module don't collide on shared field names.
fn render_field_type_expr(
    field_name: &str,
    field_type: &FieldType,
    package_name: &str,
    const_prefix: &str,
    helper_consts: &mut String,
    nested_path_resolver: &dyn Fn(Option<&str>, &str, &str) -> String,
) -> String {
    match field_type {
        FieldType::Primitive(prim) => primitive_field_type_expr(prim).to_string(),
        FieldType::String => "::nros_serdes::FieldType::String".to_string(),
        FieldType::WString => "::nros_serdes::FieldType::WString".to_string(),
        FieldType::BoundedString(n) => {
            format!("::nros_serdes::FieldType::BoundedString({})", n)
        }
        FieldType::BoundedWString(n) => {
            format!("::nros_serdes::FieldType::BoundedWString({})", n)
        }
        FieldType::NamespacedType { package, name } => {
            // Emit a NestedType helper const, sourcing TYPE_NAME + FIELDS
            // from the nested type's own Message impl so we never duplicate
            // the package/type-name string.
            let nested_const = format!("{}NESTED_{}", const_prefix, upper_ident(field_name));
            let nested_path = nested_path_resolver(package.as_deref(), name, package_name);
            helper_consts.push_str(&format!(
                "#[allow(non_upper_case_globals)]\n\
                 pub const {nested_const}: ::nros_serdes::NestedType = ::nros_serdes::NestedType {{\n    \
                 type_name: <{nested_path} as ::nros_serdes::Message>::TYPE_NAME,\n    \
                 fields: <{nested_path} as ::nros_serdes::Message>::FIELDS,\n}};\n",
            ));
            format!("::nros_serdes::FieldType::Nested(&{})", nested_const)
        }
        FieldType::Array { element_type, size } => {
            let elem_const = format!("{}FT_{}_ELEM", const_prefix, upper_ident(field_name));
            emit_element_const(
                &elem_const,
                field_name,
                element_type,
                package_name,
                const_prefix,
                helper_consts,
                nested_path_resolver,
            );
            format!("::nros_serdes::FieldType::Array({}, &{})", size, elem_const)
        }
        FieldType::Sequence { element_type } => {
            let elem_const = format!("{}FT_{}_ELEM", const_prefix, upper_ident(field_name));
            emit_element_const(
                &elem_const,
                field_name,
                element_type,
                package_name,
                const_prefix,
                helper_consts,
                nested_path_resolver,
            );
            format!("::nros_serdes::FieldType::Sequence(&{})", elem_const)
        }
        FieldType::BoundedSequence {
            element_type,
            max_size,
        } => {
            let elem_const = format!("{}FT_{}_ELEM", const_prefix, upper_ident(field_name));
            emit_element_const(
                &elem_const,
                field_name,
                element_type,
                package_name,
                const_prefix,
                helper_consts,
                nested_path_resolver,
            );
            format!(
                "::nros_serdes::FieldType::BoundedSequence({}, &{})",
                max_size, elem_const
            )
        }
    }
}

/// Emit a `pub const <ident>: FieldType = <expr>;` for the recursive
/// element of an Array / Sequence / BoundedSequence field.
fn emit_element_const(
    const_ident: &str,
    field_name: &str,
    element_type: &FieldType,
    package_name: &str,
    const_prefix: &str,
    helper_consts: &mut String,
    nested_path_resolver: &dyn Fn(Option<&str>, &str, &str) -> String,
) {
    // The inner expression is rendered with the *parent* field name so any
    // further-nested helpers stay scoped under the same FT_<FIELD>_ prefix.
    let inner_expr = render_field_type_expr(
        field_name,
        element_type,
        package_name,
        const_prefix,
        helper_consts,
        nested_path_resolver,
    );
    helper_consts.push_str(&format!(
        "#[allow(non_upper_case_globals)]\n\
         pub const {ident}: ::nros_serdes::FieldType = {inner};\n",
        ident = const_ident,
        inner = inner_expr,
    ));
}

/// Map an IDL primitive to its `::nros_serdes::FieldType::*` variant.
fn primitive_field_type_expr(prim: &PrimitiveType) -> &'static str {
    match prim {
        PrimitiveType::Bool => "::nros_serdes::FieldType::Bool",
        // ROS IDL `octet` / `byte` / `char` and `uint8` all map to Uint8 on
        // the wire (same single-byte CDR encoding).
        PrimitiveType::Byte | PrimitiveType::Char | PrimitiveType::UInt8 => {
            "::nros_serdes::FieldType::Uint8"
        }
        PrimitiveType::Int8 => "::nros_serdes::FieldType::Int8",
        PrimitiveType::UInt16 => "::nros_serdes::FieldType::Uint16",
        PrimitiveType::Int16 => "::nros_serdes::FieldType::Int16",
        PrimitiveType::UInt32 => "::nros_serdes::FieldType::Uint32",
        PrimitiveType::Int32 => "::nros_serdes::FieldType::Int32",
        PrimitiveType::UInt64 => "::nros_serdes::FieldType::Uint64",
        PrimitiveType::Int64 => "::nros_serdes::FieldType::Int64",
        PrimitiveType::Float32 => "::nros_serdes::FieldType::Float32",
        PrimitiveType::Float64 => "::nros_serdes::FieldType::Float64",
    }
}

/// Render the Rust path to a nested message type. Mirrors the
/// crate-mode rules in `nros_type_for_field_with_mode` for
/// `NamespacedType` so we can hand the type as `<Path as Message>`.
///
/// Default resolver passed to
/// [`build_nros_schema_for_struct_with_path`] — assumes the nested
/// type lives under `<pkg>::msg::<X>` (the `.msg` shape).
pub fn default_nested_type_path(pkg: Option<&str>, name: &str, current_package: &str) -> String {
    match pkg {
        Some(p) if p == current_package => format!("crate::msg::{}", name),
        Some(p) => format!("{}::msg::{}", p, name),
        None => format!("crate::msg::{}", name),
    }
}

// ============================================================================
// Action envelope schemas (Phase 212.K.7.1.d)
// ============================================================================

/// Schemas for the five rosidl action wire-envelope structs of a single
/// action package. Mirrors the upstream `rosidl_generator_cpp` shape:
///
/// - `<A>_SendGoal_Request`   { goal_id: UUID, goal: <A>Goal }
/// - `<A>_SendGoal_Response`  { accepted: bool, stamp: Time }
/// - `<A>_GetResult_Request`  { goal_id: UUID }
/// - `<A>_GetResult_Response` { status: int8, result: <A>Result }
/// - `<A>_FeedbackMessage`    { goal_id: UUID, feedback: <A>Feedback }
///
/// Field order, names, and types match
/// `rosidl_generator_cpp`'s `<action>__struct.hpp` exactly (verified
/// against `example_interfaces/action/Fibonacci`).
#[derive(Debug, Clone)]
pub struct ActionEnvelopeSchemas {
    pub send_goal_request: NrosMessageSchema,
    pub send_goal_response: NrosMessageSchema,
    pub get_result_request: NrosMessageSchema,
    pub get_result_response: NrosMessageSchema,
    pub feedback_message: NrosMessageSchema,
}

/// Build the [`ActionEnvelopeSchemas`] for an action, given the action's
/// host package + name. The five schemas reference both external types
/// (`unique_identifier_msgs::msg::UUID`, `builtin_interfaces::msg::Time`)
/// and action-self types (`<A>Goal/Result/Feedback`, same module — bare
/// idents). The custom path resolver fans them out.
pub fn build_action_envelope_schemas(
    package_name: &str,
    action_name: &str,
) -> ActionEnvelopeSchemas {
    use rosidl_parser::{Field, PrimitiveType};

    let goal_struct = format!("{}Goal", action_name);
    let result_struct = format!("{}Result", action_name);
    let feedback_struct = format!("{}Feedback", action_name);

    // Action-self struct path resolver: when the nested package matches
    // this action's host package AND the struct name matches one of the
    // three user-facing structs, reach it as a bare ident (same module).
    // Everything else falls through to the default `.msg` shape.
    let goal_clone = goal_struct.clone();
    let result_clone = result_struct.clone();
    let feedback_clone = feedback_struct.clone();
    let pkg_clone = package_name.to_string();
    let resolver = move |pkg: Option<&str>, name: &str, current_package: &str| -> String {
        if pkg == Some(pkg_clone.as_str())
            && (name == goal_clone || name == result_clone || name == feedback_clone)
        {
            name.to_string()
        } else {
            default_nested_type_path(pkg, name, current_package)
        }
    };

    let uuid_field = || Field {
        name: "goal_id".to_string(),
        field_type: FieldType::NamespacedType {
            package: Some("unique_identifier_msgs".to_string()),
            name: "UUID".to_string(),
        },
        default_value: None,
    };
    let self_field = |name: &str, struct_name: &str| Field {
        name: name.to_string(),
        field_type: FieldType::NamespacedType {
            package: Some(package_name.to_string()),
            name: struct_name.to_string(),
        },
        default_value: None,
    };

    // SendGoal_Request: goal_id (UUID), goal (<A>Goal)
    let send_goal_request = build_nros_schema_for_struct_with_path(
        package_name,
        &format!("{}_SendGoal_Request", action_name),
        &format!("{}/action/{}_SendGoal_Request", package_name, action_name),
        "SG_REQ_",
        &[uuid_field(), self_field("goal", &goal_struct)],
        &resolver,
    );

    // SendGoal_Response: accepted (bool), stamp (Time)
    let send_goal_response = build_nros_schema_for_struct_with_path(
        package_name,
        &format!("{}_SendGoal_Response", action_name),
        &format!("{}/action/{}_SendGoal_Response", package_name, action_name),
        "SG_RESP_",
        &[
            Field {
                name: "accepted".to_string(),
                field_type: FieldType::Primitive(PrimitiveType::Bool),
                default_value: None,
            },
            Field {
                name: "stamp".to_string(),
                field_type: FieldType::NamespacedType {
                    package: Some("builtin_interfaces".to_string()),
                    name: "Time".to_string(),
                },
                default_value: None,
            },
        ],
        &resolver,
    );

    // GetResult_Request: goal_id (UUID)
    let get_result_request = build_nros_schema_for_struct_with_path(
        package_name,
        &format!("{}_GetResult_Request", action_name),
        &format!("{}/action/{}_GetResult_Request", package_name, action_name),
        "GR_REQ_",
        &[uuid_field()],
        &resolver,
    );

    // GetResult_Response: status (int8), result (<A>Result)
    let get_result_response = build_nros_schema_for_struct_with_path(
        package_name,
        &format!("{}_GetResult_Response", action_name),
        &format!("{}/action/{}_GetResult_Response", package_name, action_name),
        "GR_RESP_",
        &[
            Field {
                name: "status".to_string(),
                field_type: FieldType::Primitive(PrimitiveType::Int8),
                default_value: None,
            },
            self_field("result", &result_struct),
        ],
        &resolver,
    );

    // FeedbackMessage: goal_id (UUID), feedback (<A>Feedback)
    let feedback_message = build_nros_schema_for_struct_with_path(
        package_name,
        &format!("{}_FeedbackMessage", action_name),
        &format!("{}/action/{}_FeedbackMessage", package_name, action_name),
        "FB_",
        &[uuid_field(), self_field("feedback", &feedback_struct)],
        &resolver,
    );

    ActionEnvelopeSchemas {
        send_goal_request,
        send_goal_response,
        get_result_request,
        get_result_response,
        feedback_message,
    }
}

/// Turn a field name into an UPPER_SNAKE_CASE identifier fragment for
/// use inside helper-const names (`NESTED_<X>`, `FT_<X>_ELEM`).
fn upper_ident(s: &str) -> String {
    // Strip a trailing `_` first — `escape_keyword` adds one for reserved
    // words, but it's stable to recompute via the raw IDL name. We keep
    // ASCII-safe transforms only; IDL field names are already
    // `[a-z][a-z0-9_]*`.
    s.trim_end_matches('_').to_ascii_uppercase()
}

#[cfg(test)]
mod schema_tests {
    use super::*;
    use rosidl_parser::{Field, PrimitiveType};

    fn prim_field(name: &str, prim: PrimitiveType) -> Field {
        Field {
            name: name.to_string(),
            field_type: FieldType::Primitive(prim),
            default_value: None,
        }
    }

    fn nested_field(name: &str, pkg: &str, ty: &str) -> Field {
        Field {
            name: name.to_string(),
            field_type: FieldType::NamespacedType {
                package: Some(pkg.to_string()),
                name: ty.to_string(),
            },
            default_value: None,
        }
    }

    #[test]
    fn primitive_only_emits_inline_field_type() {
        let schema = build_nros_message_schema(
            "std_msgs",
            "Int32",
            &[prim_field("data", PrimitiveType::Int32)],
        );
        assert_eq!(schema.nros_type_name, "std_msgs/msg/Int32");
        assert_eq!(schema.helper_consts, "");
        assert!(schema.fields_block.contains("name: \"data\","));
        assert!(
            schema
                .fields_block
                .contains("ty: ::nros_serdes::FieldType::Int32,")
        );
        assert!(
            schema
                .fields_block
                .contains("offset: ::core::mem::offset_of!(Int32, data)")
        );
    }

    #[test]
    fn nested_field_emits_nested_helper_const() {
        let schema = build_nros_message_schema(
            "std_msgs",
            "Header",
            &[
                nested_field("stamp", "builtin_interfaces", "Time"),
                Field {
                    name: "frame_id".to_string(),
                    field_type: FieldType::String,
                    default_value: None,
                },
            ],
        );
        assert!(
            schema
                .helper_consts
                .contains("pub const NESTED_STAMP: ::nros_serdes::NestedType")
        );
        assert!(
            schema
                .helper_consts
                .contains("<builtin_interfaces::msg::Time as ::nros_serdes::Message>::TYPE_NAME")
        );
        assert!(
            schema
                .fields_block
                .contains("ty: ::nros_serdes::FieldType::Nested(&NESTED_STAMP),")
        );
        assert!(
            schema
                .fields_block
                .contains("ty: ::nros_serdes::FieldType::String,")
        );
    }

    #[test]
    fn bounded_sequence_emits_element_const() {
        let schema = build_nros_message_schema(
            "test_msgs",
            "Bounded",
            &[Field {
                name: "items".to_string(),
                field_type: FieldType::BoundedSequence {
                    element_type: Box::new(FieldType::Primitive(PrimitiveType::UInt8)),
                    max_size: 16,
                },
                default_value: None,
            }],
        );
        assert!(
            schema
                .helper_consts
                .contains("pub const FT_ITEMS_ELEM: ::nros_serdes::FieldType")
        );
        assert!(
            schema
                .helper_consts
                .contains("= ::nros_serdes::FieldType::Uint8;")
        );
        assert!(
            schema
                .fields_block
                .contains("ty: ::nros_serdes::FieldType::BoundedSequence(16, &FT_ITEMS_ELEM),")
        );
    }

    #[test]
    fn bounded_string_inlines_capacity() {
        let schema = build_nros_message_schema(
            "test_msgs",
            "Strs",
            &[Field {
                name: "label".to_string(),
                field_type: FieldType::BoundedString(32),
                default_value: None,
            }],
        );
        assert!(schema.helper_consts.is_empty());
        assert!(
            schema
                .fields_block
                .contains("ty: ::nros_serdes::FieldType::BoundedString(32),")
        );
    }

    #[test]
    fn array_of_nested_emits_chained_consts() {
        let schema = build_nros_message_schema(
            "test_msgs",
            "Mixed",
            &[Field {
                name: "points".to_string(),
                field_type: FieldType::Array {
                    element_type: Box::new(FieldType::NamespacedType {
                        package: Some("geometry_msgs".to_string()),
                        name: "Point".to_string(),
                    }),
                    size: 3,
                },
                default_value: None,
            }],
        );
        // Array hoists FT_POINTS_ELEM; the nested type hoists NESTED_POINTS
        // (named after the parent field, since we scope inner consts under
        // the parent field's name).
        assert!(
            schema
                .helper_consts
                .contains("pub const NESTED_POINTS: ::nros_serdes::NestedType")
        );
        assert!(
            schema
                .helper_consts
                .contains("pub const FT_POINTS_ELEM: ::nros_serdes::FieldType = ::nros_serdes::FieldType::Nested(&NESTED_POINTS);")
        );
        assert!(
            schema
                .fields_block
                .contains("ty: ::nros_serdes::FieldType::Array(3, &FT_POINTS_ELEM),")
        );
    }

    #[test]
    fn self_package_nested_uses_crate_path() {
        let schema = build_nros_message_schema(
            "local_msgs",
            "Outer",
            &[nested_field("inner", "local_msgs", "Inner")],
        );
        assert!(
            schema
                .helper_consts
                .contains("<crate::msg::Inner as ::nros_serdes::Message>::TYPE_NAME")
        );
    }

    #[test]
    fn keyword_field_name_escapes_for_offset() {
        // `type` is a Rust keyword and gets a trailing underscore in the
        // host struct field — schema name stays raw, but offset_of!
        // must reference the escaped Rust field.
        let schema = build_nros_message_schema(
            "test_msgs",
            "Sample",
            &[prim_field("type", PrimitiveType::Int32)],
        );
        assert!(schema.fields_block.contains("name: \"type\","));
        assert!(schema.fields_block.contains("offset_of!(Sample, type_)"));
    }

    // ------------------------------------------------------------------
    // K.7.1.c — service Request/Response + action Goal/Result/Feedback
    //
    // These use `build_nros_schema_for_struct` directly to verify the
    // (struct_name, type_name, const_prefix) generalization.
    // ------------------------------------------------------------------

    #[test]
    fn service_request_schema_uses_srv_type_name_and_struct_offset() {
        let schema = build_nros_schema_for_struct(
            "example_interfaces",
            "AddTwoIntsRequest",
            "example_interfaces/srv/AddTwoInts_Request",
            "REQ_",
            &[
                prim_field("a", PrimitiveType::Int64),
                prim_field("b", PrimitiveType::Int64),
            ],
        );
        // Primitive-only schema needs no helper consts.
        assert_eq!(schema.helper_consts, "");
        assert_eq!(
            schema.nros_type_name,
            "example_interfaces/srv/AddTwoInts_Request"
        );
        // Offset must reference the Rust struct ident (AddTwoIntsRequest),
        // not the rosidl wire name.
        assert!(
            schema
                .fields_block
                .contains("offset: ::core::mem::offset_of!(AddTwoIntsRequest, a)")
        );
        assert!(
            schema
                .fields_block
                .contains("offset: ::core::mem::offset_of!(AddTwoIntsRequest, b)")
        );
        assert!(
            schema
                .fields_block
                .contains("ty: ::nros_serdes::FieldType::Int64,")
        );
    }

    #[test]
    fn service_response_schema_distinct_helper_const_prefix() {
        // Same field name on both halves with nested types — the
        // REQ_/RESP_ const prefix is what keeps the module-scope idents
        // distinct.
        let req = build_nros_schema_for_struct(
            "demo",
            "MoveRequest",
            "demo/srv/Move_Request",
            "REQ_",
            &[nested_field("header", "std_msgs", "Header")],
        );
        let resp = build_nros_schema_for_struct(
            "demo",
            "MoveResponse",
            "demo/srv/Move_Response",
            "RESP_",
            &[nested_field("header", "std_msgs", "Header")],
        );
        assert!(req.helper_consts.contains("pub const REQ_NESTED_HEADER:"));
        assert!(resp.helper_consts.contains("pub const RESP_NESTED_HEADER:"));
        // No collision: REQ_ ident never appears in the RESP_ block and vv.
        assert!(!req.helper_consts.contains("RESP_NESTED_HEADER"));
        assert!(!resp.helper_consts.contains("REQ_NESTED_HEADER"));
        assert!(
            req.fields_block
                .contains("ty: ::nros_serdes::FieldType::Nested(&REQ_NESTED_HEADER),")
        );
        assert!(
            resp.fields_block
                .contains("ty: ::nros_serdes::FieldType::Nested(&RESP_NESTED_HEADER),")
        );
    }

    /// Verifies action goal, result, and feedback type names follow ROSIDL convention.
    #[test]
    fn action_types_follow_rosidl_convention() {
        let goal = build_nros_schema_for_struct(
            "example_interfaces",
            "FibonacciGoal",
            "example_interfaces/action/Fibonacci_Goal",
            "GOAL_",
            &[prim_field("order", PrimitiveType::Int32)],
        );
        let result = build_nros_schema_for_struct(
            "example_interfaces",
            "FibonacciResult",
            "example_interfaces/action/Fibonacci_Result",
            "RESULT_",
            &[Field {
                name: "sequence".to_string(),
                field_type: FieldType::Sequence {
                    element_type: Box::new(FieldType::Primitive(PrimitiveType::Int32)),
                },
                default_value: None,
            }],
        );
        let feedback = build_nros_schema_for_struct(
            "example_interfaces",
            "FibonacciFeedback",
            "example_interfaces/action/Fibonacci_Feedback",
            "FEEDBACK_",
            &[Field {
                name: "sequence".to_string(),
                field_type: FieldType::Sequence {
                    element_type: Box::new(FieldType::Primitive(PrimitiveType::Int32)),
                },
                default_value: None,
            }],
        );
        assert_eq!(
            goal.nros_type_name,
            "example_interfaces/action/Fibonacci_Goal"
        );
        assert_eq!(
            result.nros_type_name,
            "example_interfaces/action/Fibonacci_Result"
        );
        assert_eq!(
            feedback.nros_type_name,
            "example_interfaces/action/Fibonacci_Feedback"
        );
        // Result and Feedback share field name `sequence` but the
        // RESULT_/FEEDBACK_ prefix keeps the FT_*_ELEM idents distinct.
        assert!(
            result
                .helper_consts
                .contains("pub const RESULT_FT_SEQUENCE_ELEM:")
        );
        assert!(
            feedback
                .helper_consts
                .contains("pub const FEEDBACK_FT_SEQUENCE_ELEM:")
        );
        assert!(!result.helper_consts.contains("FEEDBACK_FT_SEQUENCE_ELEM"));
        assert!(!feedback.helper_consts.contains("RESULT_FT_SEQUENCE_ELEM"));
        // Offsets reference the Rust struct ident.
        assert!(
            goal.fields_block
                .contains("offset: ::core::mem::offset_of!(FibonacciGoal, order)")
        );
        assert!(
            result
                .fields_block
                .contains("offset: ::core::mem::offset_of!(FibonacciResult, sequence)")
        );
        assert!(
            feedback
                .fields_block
                .contains("offset: ::core::mem::offset_of!(FibonacciFeedback, sequence)")
        );
    }

    // ------------------------------------------------------------------
    // K.7.1.d — action envelope schemas
    // ------------------------------------------------------------------

    #[test]
    fn action_envelope_send_goal_request_shape() {
        let envs = build_action_envelope_schemas("example_interfaces", "Fibonacci");
        let s = &envs.send_goal_request;
        assert_eq!(
            s.nros_type_name,
            "example_interfaces/action/Fibonacci_SendGoal_Request"
        );
        // goal_id resolves to the cross-package UUID type.
        assert!(
            s.helper_consts.contains("pub const SG_REQ_NESTED_GOAL_ID:"),
            "helper_consts = {}",
            s.helper_consts
        );
        assert!(
            s.helper_consts.contains(
                "<unique_identifier_msgs::msg::UUID as ::nros_serdes::Message>::TYPE_NAME"
            )
        );
        // goal resolves to the action-self struct (bare ident — same module).
        assert!(s.helper_consts.contains("pub const SG_REQ_NESTED_GOAL:"));
        assert!(
            s.helper_consts
                .contains("<FibonacciGoal as ::nros_serdes::Message>::TYPE_NAME")
        );
        // Offsets reference the envelope struct name (raw rosidl form).
        assert!(
            s.fields_block
                .contains("offset: ::core::mem::offset_of!(Fibonacci_SendGoal_Request, goal_id)")
        );
        assert!(
            s.fields_block
                .contains("offset: ::core::mem::offset_of!(Fibonacci_SendGoal_Request, goal)")
        );
        // Field order matches upstream: goal_id then goal.
        let goal_id_pos = s.fields_block.find("\"goal_id\"").unwrap();
        let goal_pos = s.fields_block.find("\"goal\"").unwrap();
        assert!(goal_id_pos < goal_pos);
    }

    #[test]
    fn action_envelope_send_goal_response_shape() {
        let envs = build_action_envelope_schemas("example_interfaces", "Fibonacci");
        let s = &envs.send_goal_response;
        assert_eq!(
            s.nros_type_name,
            "example_interfaces/action/Fibonacci_SendGoal_Response"
        );
        // stamp resolves to builtin_interfaces::msg::Time.
        assert!(s.helper_consts.contains("pub const SG_RESP_NESTED_STAMP:"));
        assert!(
            s.helper_consts
                .contains("<builtin_interfaces::msg::Time as ::nros_serdes::Message>::TYPE_NAME")
        );
        // accepted is a primitive (no helper const for that one).
        assert!(s.fields_block.contains("\"accepted\""));
        assert!(s.fields_block.contains("::nros_serdes::FieldType::Bool"));
        assert!(
            s.fields_block
                .contains("ty: ::nros_serdes::FieldType::Nested(&SG_RESP_NESTED_STAMP),")
        );
        // Field order: accepted then stamp (matches upstream Fibonacci_SendGoal_Response_).
        let acc_pos = s.fields_block.find("\"accepted\"").unwrap();
        let stamp_pos = s.fields_block.find("\"stamp\"").unwrap();
        assert!(acc_pos < stamp_pos);
    }

    #[test]
    fn action_envelope_get_result_request_shape() {
        let envs = build_action_envelope_schemas("example_interfaces", "Fibonacci");
        let s = &envs.get_result_request;
        assert_eq!(
            s.nros_type_name,
            "example_interfaces/action/Fibonacci_GetResult_Request"
        );
        // Single field: goal_id.
        assert!(s.helper_consts.contains("pub const GR_REQ_NESTED_GOAL_ID:"));
        assert!(s.fields_block.contains("\"goal_id\""));
        assert!(
            s.fields_block
                .contains("ty: ::nros_serdes::FieldType::Nested(&GR_REQ_NESTED_GOAL_ID),")
        );
        // No second field.
        assert_eq!(s.fields_block.matches("name: ").count(), 1);
    }

    #[test]
    fn action_envelope_get_result_response_shape() {
        let envs = build_action_envelope_schemas("example_interfaces", "Fibonacci");
        let s = &envs.get_result_response;
        assert_eq!(
            s.nros_type_name,
            "example_interfaces/action/Fibonacci_GetResult_Response"
        );
        // Field order: status (Int8) then result (Nested<FibonacciResult>).
        assert!(s.fields_block.contains("\"status\""));
        assert!(s.fields_block.contains("::nros_serdes::FieldType::Int8"));
        assert!(s.helper_consts.contains("pub const GR_RESP_NESTED_RESULT:"));
        assert!(
            s.helper_consts
                .contains("<FibonacciResult as ::nros_serdes::Message>::TYPE_NAME")
        );
        let status_pos = s.fields_block.find("\"status\"").unwrap();
        let result_pos = s.fields_block.find("\"result\"").unwrap();
        assert!(status_pos < result_pos);
    }

    #[test]
    fn action_envelope_feedback_message_shape() {
        let envs = build_action_envelope_schemas("example_interfaces", "Fibonacci");
        let s = &envs.feedback_message;
        assert_eq!(
            s.nros_type_name,
            "example_interfaces/action/Fibonacci_FeedbackMessage"
        );
        assert!(s.helper_consts.contains("pub const FB_NESTED_GOAL_ID:"));
        assert!(s.helper_consts.contains("pub const FB_NESTED_FEEDBACK:"));
        assert!(
            s.helper_consts
                .contains("<FibonacciFeedback as ::nros_serdes::Message>::TYPE_NAME")
        );
        let goal_id_pos = s.fields_block.find("\"goal_id\"").unwrap();
        let feedback_pos = s.fields_block.find("\"feedback\"").unwrap();
        assert!(goal_id_pos < feedback_pos);
    }

    #[test]
    fn action_envelope_prefixes_are_distinct_across_halves() {
        // Every envelope must use its own SG_REQ_ / SG_RESP_ / GR_REQ_
        // / GR_RESP_ / FB_ prefix so the module-scope `pub const`
        // idents don't collide. Shared field name `goal_id` lives on
        // SendGoal_Request, GetResult_Request, and FeedbackMessage —
        // the three matching NESTED_GOAL_ID consts must not clash.
        let envs = build_action_envelope_schemas("example_interfaces", "Fibonacci");
        assert!(envs.send_goal_request.helper_consts.contains("SG_REQ_"));
        assert!(envs.send_goal_response.helper_consts.contains("SG_RESP_"));
        assert!(envs.get_result_request.helper_consts.contains("GR_REQ_"));
        assert!(envs.get_result_response.helper_consts.contains("GR_RESP_"));
        assert!(envs.feedback_message.helper_consts.contains("FB_"));
        // Per-half: the OTHER half's prefix must not leak in.
        assert!(!envs.send_goal_request.helper_consts.contains("GR_REQ_"));
        assert!(!envs.feedback_message.helper_consts.contains("SG_REQ_"));
        assert!(!envs.get_result_request.helper_consts.contains("FB_NESTED"));
    }

    #[test]
    fn empty_request_schema_emits_no_fields_no_helpers() {
        // A trigger-style service has an empty request body.
        let schema = build_nros_schema_for_struct(
            "std_srvs",
            "TriggerRequest",
            "std_srvs/srv/Trigger_Request",
            "REQ_",
            &[],
        );
        assert_eq!(schema.helper_consts, "");
        assert_eq!(schema.fields_block, "");
        assert_eq!(schema.nros_type_name, "std_srvs/srv/Trigger_Request");
    }
}
