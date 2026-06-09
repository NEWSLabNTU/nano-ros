use rosidl_parser::{
    FieldType,
    ast::ConstantValue,
    idl::{ast::ConstantValue as IdlConstantValue, types::IdlType},
};

/// ROS 2 edition for type hash generation
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RosEdition {
    /// ROS 2 Humble: type hash = "TypeHashNotSupported"
    #[default]
    Humble,
    /// ROS 2 Iron+: type hash = "RIHS01_<sha256>" (placeholder until computed)
    Iron,
}

impl RosEdition {
    /// Get the type hash string for this edition.
    pub fn type_hash(&self) -> &'static str {
        match self {
            RosEdition::Humble => "TypeHashNotSupported",
            RosEdition::Iron => {
                "RIHS01_0000000000000000000000000000000000000000000000000000000000000000"
            }
        }
    }
}

/// Code generation backend selection
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CodegenBackend {
    /// rclrs backend - generates two-layer types (RMW + idiomatic) with C FFI
    /// Requires rosidl_runtime_rs and links against ROS 2 C libraries
    #[default]
    Rclrs,

    /// nros backend - generates single-layer pure Rust types
    /// Uses heapless collections for no_std compatibility
    /// No C dependencies, suitable for embedded RTOS platforms
    Nros,
}

/// Extension trait for FieldType providing type checking methods
pub trait FieldTypeExt {
    /// Check if this is a sequence (unbounded or bounded)
    fn is_sequence(&self) -> bool;

    /// Check if this is a primitive (no conversion needed)
    fn is_primitive(&self) -> bool;

    /// Check if this is a string type (String, BoundedString, WString, BoundedWString)
    fn is_string(&self) -> bool;

    /// Check if this is specifically an unbounded string
    fn is_unbounded_string(&self) -> bool;

    /// Check if this is specifically a bounded string
    fn is_bounded_string(&self) -> bool;

    /// Check if this is a WString type (unbounded or bounded)
    fn is_wstring(&self) -> bool;

    /// Check if this is specifically an unbounded WString
    fn is_unbounded_wstring(&self) -> bool;

    /// Check if this is specifically a bounded WString
    fn is_bounded_wstring(&self) -> bool;

    /// Check if this is a sequence of primitives (can be copied directly)
    fn is_primitive_sequence(&self) -> bool;

    /// Check if this is a sequence of strings (any string type)
    fn is_string_sequence(&self) -> bool;

    /// Check if this is a sequence of unbounded strings
    fn is_unbounded_string_sequence(&self) -> bool;

    /// Check if this is a sequence of bounded strings
    fn is_bounded_string_sequence(&self) -> bool;

    /// Check if this is an array (needs clone, not conversion)
    fn is_array(&self) -> bool;

    /// Check if this is a large array (> 32 elements, needs big_array for serde)
    fn is_large_array(&self) -> bool;

    /// Check if this is an array of primitives (can be cloned directly)
    fn is_primitive_array(&self) -> bool;

    /// Check if this is an array of strings (any string type)
    fn is_string_array(&self) -> bool;

    /// Check if this is an array of unbounded strings
    fn is_unbounded_string_array(&self) -> bool;

    /// Check if this is an array of bounded strings
    fn is_bounded_string_array(&self) -> bool;

    /// Check if this is an array of unbounded wstrings
    fn is_unbounded_wstring_array(&self) -> bool;

    /// Check if this is an array of bounded wstrings
    fn is_bounded_wstring_array(&self) -> bool;

    /// Check if this is a sequence of unbounded wstrings
    fn is_unbounded_wstring_sequence(&self) -> bool;

    /// Check if this is a sequence of bounded wstrings
    fn is_bounded_wstring_sequence(&self) -> bool;

    /// Check if this is an array of nested messages (needs element conversion)
    fn is_nested_array(&self) -> bool;

    /// Check if this is a bounded sequence (vs unbounded)
    fn is_bounded_sequence(&self) -> bool;
}

impl FieldTypeExt for FieldType {
    fn is_sequence(&self) -> bool {
        matches!(
            self,
            FieldType::Sequence { .. } | FieldType::BoundedSequence { .. }
        )
    }

    fn is_primitive(&self) -> bool {
        matches!(self, FieldType::Primitive(_))
    }

    fn is_string(&self) -> bool {
        matches!(
            self,
            FieldType::String
                | FieldType::BoundedString(_)
                | FieldType::WString
                | FieldType::BoundedWString(_)
        )
    }

    fn is_unbounded_string(&self) -> bool {
        matches!(self, FieldType::String)
    }

    fn is_bounded_string(&self) -> bool {
        matches!(self, FieldType::BoundedString(_))
    }

    fn is_wstring(&self) -> bool {
        matches!(self, FieldType::WString | FieldType::BoundedWString(_))
    }

    fn is_unbounded_wstring(&self) -> bool {
        matches!(self, FieldType::WString)
    }

    fn is_bounded_wstring(&self) -> bool {
        matches!(self, FieldType::BoundedWString(_))
    }

    fn is_primitive_sequence(&self) -> bool {
        match self {
            FieldType::Sequence { element_type }
            | FieldType::BoundedSequence { element_type, .. } => {
                matches!(**element_type, FieldType::Primitive(_))
            }
            _ => false,
        }
    }

    fn is_string_sequence(&self) -> bool {
        match self {
            FieldType::Sequence { element_type }
            | FieldType::BoundedSequence { element_type, .. } => element_type.is_string(),
            _ => false,
        }
    }

    fn is_unbounded_string_sequence(&self) -> bool {
        match self {
            FieldType::Sequence { element_type }
            | FieldType::BoundedSequence { element_type, .. } => element_type.is_unbounded_string(),
            _ => false,
        }
    }

    fn is_bounded_string_sequence(&self) -> bool {
        match self {
            FieldType::Sequence { element_type }
            | FieldType::BoundedSequence { element_type, .. } => element_type.is_bounded_string(),
            _ => false,
        }
    }

    fn is_array(&self) -> bool {
        matches!(self, FieldType::Array { .. })
    }

    fn is_large_array(&self) -> bool {
        matches!(self, FieldType::Array { size, .. } if *size > 32)
    }

    fn is_primitive_array(&self) -> bool {
        match self {
            FieldType::Array { element_type, .. } => {
                matches!(**element_type, FieldType::Primitive(_))
            }
            _ => false,
        }
    }

    fn is_string_array(&self) -> bool {
        match self {
            FieldType::Array { element_type, .. } => element_type.is_string(),
            _ => false,
        }
    }

    fn is_unbounded_string_array(&self) -> bool {
        match self {
            FieldType::Array { element_type, .. } => element_type.is_unbounded_string(),
            _ => false,
        }
    }

    fn is_bounded_string_array(&self) -> bool {
        match self {
            FieldType::Array { element_type, .. } => element_type.is_bounded_string(),
            _ => false,
        }
    }

    fn is_unbounded_wstring_array(&self) -> bool {
        match self {
            FieldType::Array { element_type, .. } => matches!(**element_type, FieldType::WString),
            _ => false,
        }
    }

    fn is_bounded_wstring_array(&self) -> bool {
        match self {
            FieldType::Array { element_type, .. } => {
                matches!(**element_type, FieldType::BoundedWString(_))
            }
            _ => false,
        }
    }

    fn is_unbounded_wstring_sequence(&self) -> bool {
        match self {
            FieldType::Sequence { element_type }
            | FieldType::BoundedSequence { element_type, .. } => {
                matches!(**element_type, FieldType::WString)
            }
            _ => false,
        }
    }

    fn is_bounded_wstring_sequence(&self) -> bool {
        match self {
            FieldType::Sequence { element_type }
            | FieldType::BoundedSequence { element_type, .. } => {
                matches!(**element_type, FieldType::BoundedWString(_))
            }
            _ => false,
        }
    }

    fn is_nested_array(&self) -> bool {
        match self {
            FieldType::Array { element_type, .. } => {
                matches!(**element_type, FieldType::NamespacedType { .. })
            }
            _ => false,
        }
    }

    fn is_bounded_sequence(&self) -> bool {
        matches!(self, FieldType::BoundedSequence { .. })
    }
}

/// Convert a ConstantValue to a Rust code string
pub fn constant_value_to_rust(value: &ConstantValue) -> String {
    match value {
        ConstantValue::Integer(i) => i.to_string(),
        ConstantValue::UInteger(u) => u.to_string(),
        ConstantValue::Float(f) => {
            // Ensure float literals always have decimal point
            let s = f.to_string();
            if s.contains('.') || s.contains('e') || s.contains('E') {
                s
            } else {
                format!("{}.0", s)
            }
        }
        ConstantValue::Bool(b) => b.to_string(),
        ConstantValue::String(s) => format!("\"{}\"", s.escape_default()),
        ConstantValue::Array(values) => {
            let inner = values
                .iter()
                .map(constant_value_to_rust)
                .collect::<Vec<_>>()
                .join(", ");
            format!("[{}]", inner)
        }
    }
}

/// Rust keywords that need to be escaped
const RUST_KEYWORDS: &[&str] = &[
    "as", "break", "const", "continue", "crate", "else", "enum", "extern", "false", "fn", "for",
    "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut", "pub", "ref", "return",
    "self", "Self", "static", "struct", "super", "trait", "true", "type", "unsafe", "use", "where",
    "while", "async", "await", "dyn", "abstract", "become", "box", "do", "final", "macro",
    "override", "priv", "typeof", "unsized", "virtual", "yield", "try",
];

/// Escape Rust keywords by appending underscore
pub fn escape_keyword(name: &str) -> String {
    if RUST_KEYWORDS.contains(&name) {
        format!("{}_", name)
    } else {
        name.to_string()
    }
}

/// Get the Rust type string for a field type
/// If `rmw_layer` is true, returns RMW types (rosidl_runtime_rs::*), else idiomatic types
/// `current_package` is used to detect self-references and use `crate::` instead of `pkg::`
pub fn rust_type_for_field(
    field_type: &FieldType,
    rmw_layer: bool,
    current_package: Option<&str>,
) -> String {
    match field_type {
        FieldType::Primitive(prim) => prim.rust_type().to_string(),

        FieldType::String => {
            if rmw_layer {
                "crate::rosidl_runtime_rs::String".to_string()
            } else {
                "std::string::String".to_string()
            }
        }

        FieldType::BoundedString(size) => {
            if rmw_layer {
                format!("crate::rosidl_runtime_rs::BoundedString<{}>", size)
            } else {
                // Idiomatic layer uses String even for bounded
                "std::string::String".to_string()
            }
        }

        FieldType::WString => {
            if rmw_layer {
                "crate::rosidl_runtime_rs::WString".to_string()
            } else {
                "std::string::String".to_string()
            }
        }

        FieldType::BoundedWString(size) => {
            if rmw_layer {
                format!("crate::rosidl_runtime_rs::BoundedWString<{}>", size)
            } else {
                "std::string::String".to_string()
            }
        }

        FieldType::Array { element_type, size } => {
            let elem = rust_type_for_field(element_type, rmw_layer, current_package);
            format!("[{}; {}]", elem, size)
        }

        FieldType::Sequence { element_type } => {
            let elem = rust_type_for_field(element_type, rmw_layer, current_package);
            if rmw_layer {
                format!("crate::rosidl_runtime_rs::Sequence<{}>", elem)
            } else {
                format!("std::vec::Vec<{}>", elem)
            }
        }

        FieldType::BoundedSequence {
            element_type,
            max_size,
        } => {
            let elem = rust_type_for_field(element_type, rmw_layer, current_package);
            if rmw_layer {
                format!(
                    "crate::rosidl_runtime_rs::BoundedSequence<{}, {}>",
                    elem, max_size
                )
            } else {
                // Idiomatic layer uses Vec even for bounded
                format!("std::vec::Vec<{}>", elem)
            }
        }

        FieldType::NamespacedType { package, name } => {
            // Check if this is a self-reference (same package referencing itself)
            let is_self_ref = package.as_deref() == current_package;

            if let Some(pkg) = package {
                if is_self_ref {
                    // Self-reference: use crate:: instead of pkg::
                    if rmw_layer {
                        // RMW layer uses rmw submodule: crate::msg::rmw::Type
                        format!("crate::msg::rmw::{}", name)
                    } else {
                        // Idiomatic layer: types are re-exported from msg module
                        format!("crate::msg::{}", name)
                    }
                } else {
                    // Cross-package reference
                    if rmw_layer {
                        // RMW layer uses rmw submodule: pkg::msg::rmw::Type
                        format!("{}::msg::rmw::{}", pkg, name)
                    } else {
                        // Idiomatic layer: types are re-exported from msg module
                        format!("{}::msg::{}", pkg, name)
                    }
                }
            } else {
                // Local same-package type reference (no package specified)
                if rmw_layer {
                    // RMW layer uses rmw submodule
                    format!("crate::msg::rmw::{}", name)
                } else {
                    // Idiomatic layer: types are re-exported from msg module
                    format!("crate::msg::{}", name)
                }
            }
        }
    }
}

/// Get the Rust type string for a constant
/// Similar to `rust_type_for_field` but uses `&'static str` for string types
/// since constants must be const-compatible
pub fn rust_type_for_constant(field_type: &FieldType) -> String {
    match field_type {
        FieldType::Primitive(prim) => prim.rust_type().to_string(),

        // All string types become &'static str for constants
        FieldType::String
        | FieldType::BoundedString(_)
        | FieldType::WString
        | FieldType::BoundedWString(_) => "&'static str".to_string(),

        // Arrays, sequences, and namespaced types are not typically used as constants
        // but we handle them for completeness
        FieldType::Array { element_type, size } => {
            let elem = rust_type_for_constant(element_type);
            format!("[{}; {}]", elem, size)
        }

        FieldType::Sequence { element_type } => {
            let elem = rust_type_for_constant(element_type);
            format!("&'static [{}]", elem)
        }

        FieldType::BoundedSequence {
            element_type,
            max_size: _,
        } => {
            let elem = rust_type_for_constant(element_type);
            format!("&'static [{}]", elem)
        }

        FieldType::NamespacedType { package, name } => {
            if let Some(pkg) = package {
                format!("{}::msg::{}", pkg, name)
            } else {
                format!("crate::msg::{}", name)
            }
        }
    }
}

// ============================================================================
// nros Type Mapping
// ============================================================================

/// Default string capacity for nros heapless strings
pub const NROS_DEFAULT_STRING_CAPACITY: usize = 256;

/// Default sequence capacity for nros heapless vectors
pub const NROS_DEFAULT_SEQUENCE_CAPACITY: usize = 64;

/// Configuration for nros code generation mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NrosCodegenMode {
    /// Crate mode: each package is a separate crate.
    /// Self-refs use `crate::msg::Type`, cross-refs use `pkg::msg::Type`.
    #[default]
    Crate,
    /// Inline mode: all packages in a single module tree (for build.rs).
    /// Self-refs use `super::Type`, cross-refs use `super::super::super::pkg::msg::Type`.
    /// Template uses `nros_core::` prefix instead of direct imports.
    Inline,
}

/// Get the Rust type string for a field type using nros backend
/// Returns heapless types for no_std compatibility
/// `current_package` is used to detect self-references and use `crate::` instead of `pkg::`
pub fn nros_type_for_field(field_type: &FieldType, current_package: Option<&str>) -> String {
    nros_type_for_field_with_mode(field_type, current_package, NrosCodegenMode::Crate)
}

/// Get the Rust type string for a field type using nros backend with explicit mode
pub fn nros_type_for_field_with_mode(
    field_type: &FieldType,
    current_package: Option<&str>,
    mode: NrosCodegenMode,
) -> String {
    let inline = mode == NrosCodegenMode::Inline;

    match field_type {
        FieldType::Primitive(prim) => prim.rust_type().to_string(),

        FieldType::String => {
            if inline {
                format!(
                    "nros_core::heapless::String<{}>",
                    NROS_DEFAULT_STRING_CAPACITY
                )
            } else {
                format!("heapless::String<{}>", NROS_DEFAULT_STRING_CAPACITY)
            }
        }

        FieldType::BoundedString(size) => {
            if inline {
                format!("nros_core::heapless::String<{}>", size)
            } else {
                format!("heapless::String<{}>", size)
            }
        }

        FieldType::WString => {
            // WString maps to regular heapless::String (UTF-8)
            if inline {
                format!(
                    "nros_core::heapless::String<{}>",
                    NROS_DEFAULT_STRING_CAPACITY
                )
            } else {
                format!("heapless::String<{}>", NROS_DEFAULT_STRING_CAPACITY)
            }
        }

        FieldType::BoundedWString(size) => {
            if inline {
                format!("nros_core::heapless::String<{}>", size)
            } else {
                format!("heapless::String<{}>", size)
            }
        }

        FieldType::Array { element_type, size } => {
            let elem = nros_type_for_field_with_mode(element_type, current_package, mode);
            format!("[{}; {}]", elem, size)
        }

        FieldType::Sequence { element_type } => {
            let elem = nros_type_for_field_with_mode(element_type, current_package, mode);
            if inline {
                format!(
                    "nros_core::heapless::Vec<{}, {}>",
                    elem, NROS_DEFAULT_SEQUENCE_CAPACITY
                )
            } else {
                format!(
                    "heapless::Vec<{}, {}>",
                    elem, NROS_DEFAULT_SEQUENCE_CAPACITY
                )
            }
        }

        FieldType::BoundedSequence {
            element_type,
            max_size,
        } => {
            let elem = nros_type_for_field_with_mode(element_type, current_package, mode);
            if inline {
                format!("nros_core::heapless::Vec<{}, {}>", elem, max_size)
            } else {
                format!("heapless::Vec<{}, {}>", elem, max_size)
            }
        }

        FieldType::NamespacedType { package, name } => {
            // Check if this is a self-reference (same package referencing itself)
            let is_self_ref = package.as_deref() == current_package;

            if inline {
                // Inline mode: use super-based references
                // From a message file at <pkg>/msg/<type>.rs:
                //   self-ref: super::<Type> (one level up to msg/)
                //   cross-ref: super::super::super::<pkg>::msg::<Type> (up to root)
                if let Some(pkg) = package {
                    if is_self_ref {
                        format!("super::{}", name)
                    } else {
                        format!("super::super::super::{}::msg::{}", pkg, name)
                    }
                } else {
                    // Local same-package type reference
                    format!("super::{}", name)
                }
            } else {
                // Crate mode: use crate:: and pkg:: references
                if let Some(pkg) = package {
                    if is_self_ref {
                        format!("crate::msg::{}", name)
                    } else {
                        format!("{}::msg::{}", pkg, name)
                    }
                } else {
                    format!("crate::msg::{}", name)
                }
            }
        }
    }
}

/// Like [`nros_type_for_field_with_mode`], but overrides the capacity of a
/// **top-level unbounded** `String` / `WString` / `Sequence` container with
/// `cap` (resolved per-field from `nros-codegen.toml`, RFC-0033). All other
/// field shapes — bounded strings/sequences, arrays, primitives, nested types,
/// and sequence *element* types — keep their default rendering. Used by the
/// `owned` storage mode; the resolver picks `cap`.
pub fn nros_type_for_field_with_capacity(
    field_type: &FieldType,
    current_package: Option<&str>,
    mode: NrosCodegenMode,
    cap: usize,
) -> String {
    let inline = mode == NrosCodegenMode::Inline;
    let prefix = if inline {
        "nros_core::heapless::"
    } else {
        "heapless::"
    };
    match field_type {
        FieldType::String | FieldType::WString => format!("{prefix}String<{cap}>"),
        FieldType::Sequence { element_type } => {
            let elem = nros_type_for_field_with_mode(element_type, current_package, mode);
            format!("{prefix}Vec<{elem}, {cap}>")
        }
        // Bounded / array / primitive / nested — capacity is not configurable.
        _ => nros_type_for_field_with_mode(field_type, current_package, mode),
    }
}

/// Heap (`mode = "heap"`, RFC-0033) rendering of a **top-level unbounded**
/// `String` / `WString` / `Sequence`: `alloc`-backed `nros_core::heap::{String,
/// Vec}` rather than a fixed-capacity `heapless` container. Element types keep
/// their default (fixed-capacity) rendering. The `nros_core::heap::` path works
/// in both crate and inline modes.
pub fn nros_type_for_field_heap(
    field_type: &FieldType,
    current_package: Option<&str>,
    mode: NrosCodegenMode,
) -> String {
    match field_type {
        FieldType::String | FieldType::WString => "nros_core::heap::String".to_string(),
        FieldType::Sequence { element_type } => {
            let elem = nros_type_for_field_with_mode(element_type, current_package, mode);
            format!("nros_core::heap::Vec<{elem}>")
        }
        _ => nros_type_for_field_with_mode(field_type, current_package, mode),
    }
}

/// Get the Rust type string for a constant using nros backend
/// Similar to `nros_type_for_field` but uses `&'static str` for string types
pub fn nros_type_for_constant(field_type: &FieldType) -> String {
    match field_type {
        FieldType::Primitive(prim) => prim.rust_type().to_string(),

        // All string types become &'static str for constants
        FieldType::String
        | FieldType::BoundedString(_)
        | FieldType::WString
        | FieldType::BoundedWString(_) => "&'static str".to_string(),

        FieldType::Array { element_type, size } => {
            let elem = nros_type_for_constant(element_type);
            format!("[{}; {}]", elem, size)
        }

        FieldType::Sequence { element_type } | FieldType::BoundedSequence { element_type, .. } => {
            let elem = nros_type_for_constant(element_type);
            format!("&'static [{}]", elem)
        }

        FieldType::NamespacedType { package, name } => {
            if let Some(pkg) = package {
                format!("{}::msg::{}", pkg, name)
            } else {
                format!("crate::msg::{}", name)
            }
        }
    }
}

/// Convert snake_case to UpperCamelCase
pub fn to_upper_camel_case(s: &str) -> String {
    s.split('_')
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().chain(chars).collect(),
                None => String::new(),
            }
        })
        .collect()
}

// Re-export to_snake_case from utils to ensure consistent behavior
pub use crate::utils::to_snake_case;

// ============================================================================
// IDL Type Support
// ============================================================================

/// Get the Rust type string for an IDL type
/// If `rmw_layer` is true, returns RMW types (rosidl_runtime_rs::*), else idiomatic types
/// `current_package` is used to detect self-references and use `crate::` instead of `pkg::`
pub fn rust_type_for_idl(
    idl_type: &IdlType,
    rmw_layer: bool,
    current_package: Option<&str>,
) -> String {
    match idl_type {
        IdlType::Primitive(prim) => prim.to_rust_type().to_string(),

        IdlType::String(None) => {
            if rmw_layer {
                "crate::rosidl_runtime_rs::String".to_string()
            } else {
                "std::string::String".to_string()
            }
        }

        IdlType::String(Some(bound)) => {
            if rmw_layer {
                format!("crate::rosidl_runtime_rs::BoundedString<{}>", bound)
            } else {
                "std::string::String".to_string()
            }
        }

        IdlType::WString(None) => {
            if rmw_layer {
                "crate::rosidl_runtime_rs::WString".to_string()
            } else {
                "std::string::String".to_string()
            }
        }

        IdlType::WString(Some(bound)) => {
            if rmw_layer {
                format!("crate::rosidl_runtime_rs::BoundedWString<{}>", bound)
            } else {
                "std::string::String".to_string()
            }
        }

        IdlType::Sequence(element_type, None) => {
            let elem = rust_type_for_idl(element_type, rmw_layer, current_package);
            if rmw_layer {
                format!("crate::rosidl_runtime_rs::Sequence<{}>", elem)
            } else {
                format!("std::vec::Vec<{}>", elem)
            }
        }

        IdlType::Sequence(element_type, Some(bound)) => {
            let elem = rust_type_for_idl(element_type, rmw_layer, current_package);
            if rmw_layer {
                format!(
                    "crate::rosidl_runtime_rs::BoundedSequence<{}, {}>",
                    elem, bound
                )
            } else {
                format!("std::vec::Vec<{}>", elem)
            }
        }

        IdlType::Array(element_type, dimensions) => {
            let elem = rust_type_for_idl(element_type, rmw_layer, current_package);
            let mut result = elem;
            for dim in dimensions.iter().rev() {
                result = format!("[{}; {}]", result, dim);
            }
            result
        }

        IdlType::UserDefined(name) => {
            // Local type reference (same package)
            if rmw_layer {
                format!("crate::msg::rmw::{}", name)
            } else {
                // Idiomatic layer: types are re-exported from msg module
                format!("crate::msg::{}", name)
            }
        }

        IdlType::Scoped(path) => {
            // Scoped name like package::msg::Type
            // Assume format: [package, interface_type, typename]
            if path.len() >= 3 {
                let package = &path[0];
                let typename = &path[path.len() - 1];

                // Check if this is a self-reference
                let is_self_ref = package.as_str() == current_package.unwrap_or("");

                if is_self_ref {
                    if rmw_layer {
                        format!("crate::msg::rmw::{}", typename)
                    } else {
                        // Idiomatic layer: types are re-exported from msg module
                        format!("crate::msg::{}", typename)
                    }
                } else if rmw_layer {
                    format!("{}::msg::rmw::{}", package, typename)
                } else {
                    // Idiomatic layer: types are re-exported from msg module
                    format!("{}::msg::{}", package, typename)
                }
            } else if path.len() == 1 {
                // Simple name - treat as local type
                if rmw_layer {
                    format!("crate::msg::rmw::{}", path[0])
                } else {
                    // Idiomatic layer: types are re-exported from msg module
                    format!("crate::msg::{}", path[0])
                }
            } else {
                // Fallback
                path.join("::")
            }
        }
    }
}

/// Get the Rust type string for an IDL constant
/// Similar to `rust_type_for_idl` but uses `&'static str` for string types
/// since constants must be const-compatible
pub fn rust_type_for_idl_constant(idl_type: &IdlType) -> String {
    match idl_type {
        IdlType::Primitive(prim) => prim.to_rust_type().to_string(),

        // All string types become &'static str for constants
        IdlType::String(_) | IdlType::WString(_) => "&'static str".to_string(),

        // Arrays and sequences
        IdlType::Array(element_type, dimensions) => {
            let elem = rust_type_for_idl_constant(element_type);
            let mut result = elem;
            for dim in dimensions.iter().rev() {
                result = format!("[{}; {}]", result, dim);
            }
            result
        }

        IdlType::Sequence(element_type, _) => {
            let elem = rust_type_for_idl_constant(element_type);
            format!("&'static [{}]", elem)
        }

        // User-defined types (enums, etc.)
        IdlType::UserDefined(name) => format!("crate::msg::{}", name),

        IdlType::Scoped(path) => {
            if path.len() >= 3 {
                let package = &path[0];
                let typename = &path[path.len() - 1];
                format!("{}::msg::{}", package, typename)
            } else if path.len() == 1 {
                format!("crate::msg::{}", path[0])
            } else {
                path.join("::")
            }
        }
    }
}

/// Convert an IDL constant value to Rust code string
pub fn idl_constant_value_to_rust(value: &IdlConstantValue) -> String {
    match value {
        IdlConstantValue::Integer(i) => i.to_string(),
        IdlConstantValue::Float(f) => {
            // Ensure float literals always have decimal point
            if f.is_finite() {
                if f.fract() == 0.0 {
                    format!("{:.1}", f) // Ensure .0 suffix
                } else {
                    f.to_string()
                }
            } else {
                f.to_string()
            }
        }
        IdlConstantValue::Boolean(b) => b.to_string(),
        IdlConstantValue::String(s) | IdlConstantValue::WString(s) => {
            format!("\"{}\"", s.escape_default())
        }
    }
}

/// Extension trait for IdlType providing type checking methods
pub trait IdlTypeExt {
    /// Check if this is a wide string
    fn is_wide_string(&self) -> bool;

    /// Check if this is a sequence
    fn is_sequence(&self) -> bool;

    /// Check if this is an array
    fn is_array(&self) -> bool;

    /// Check if this is a primitive
    fn is_primitive(&self) -> bool;

    /// Check if this is a string type
    fn is_string(&self) -> bool;
}

impl IdlTypeExt for IdlType {
    fn is_wide_string(&self) -> bool {
        matches!(self, IdlType::WString(_))
    }

    fn is_sequence(&self) -> bool {
        matches!(self, IdlType::Sequence(_, _))
    }

    fn is_array(&self) -> bool {
        matches!(self, IdlType::Array(_, _))
    }

    fn is_primitive(&self) -> bool {
        matches!(self, IdlType::Primitive(_))
    }

    fn is_string(&self) -> bool {
        matches!(self, IdlType::String(_) | IdlType::WString(_))
    }
}

/// Convert IDL primitive to .msg primitive type
pub fn idl_primitive_to_primitive(
    idl_prim: &rosidl_parser::idl::types::IdlPrimitiveType,
) -> rosidl_parser::PrimitiveType {
    use rosidl_parser::{PrimitiveType, idl::types::IdlPrimitiveType};

    match idl_prim {
        IdlPrimitiveType::Short => PrimitiveType::Int16,
        IdlPrimitiveType::UnsignedShort => PrimitiveType::UInt16,
        IdlPrimitiveType::Long => PrimitiveType::Int32,
        IdlPrimitiveType::UnsignedLong => PrimitiveType::UInt32,
        IdlPrimitiveType::LongLong => PrimitiveType::Int64,
        IdlPrimitiveType::UnsignedLongLong => PrimitiveType::UInt64,
        IdlPrimitiveType::Float => PrimitiveType::Float32,
        IdlPrimitiveType::Double => PrimitiveType::Float64,
        IdlPrimitiveType::LongDouble => PrimitiveType::Float64, // Map to f64
        IdlPrimitiveType::Char => PrimitiveType::Char,
        IdlPrimitiveType::Wchar => PrimitiveType::UInt16, // Wchar is 16-bit
        IdlPrimitiveType::Boolean => PrimitiveType::Bool,
        IdlPrimitiveType::Octet => PrimitiveType::Byte,
        IdlPrimitiveType::Int8 => PrimitiveType::Int8,
        IdlPrimitiveType::Uint8 => PrimitiveType::UInt8,
        IdlPrimitiveType::Int16 => PrimitiveType::Int16,
        IdlPrimitiveType::Uint16 => PrimitiveType::UInt16,
        IdlPrimitiveType::Int32 => PrimitiveType::Int32,
        IdlPrimitiveType::Uint32 => PrimitiveType::UInt32,
        IdlPrimitiveType::Int64 => PrimitiveType::Int64,
        IdlPrimitiveType::Uint64 => PrimitiveType::UInt64,
    }
}

/// Convert IDL annotation value to .msg constant value
pub fn annotation_value_to_constant_value(
    ann_val: &rosidl_parser::idl::ast::AnnotationValue,
) -> rosidl_parser::ast::ConstantValue {
    use rosidl_parser::{ast::ConstantValue, idl::ast::AnnotationValue};

    match ann_val {
        AnnotationValue::Integer(i) => ConstantValue::Integer(*i),
        AnnotationValue::Float(f) => ConstantValue::Float(*f),
        AnnotationValue::String(s) => ConstantValue::String(s.clone()),
        AnnotationValue::Boolean(b) => ConstantValue::Bool(*b),
        AnnotationValue::Identifier(id) => {
            // For identifiers, we need to check if they're boolean keywords
            match id.as_str() {
                "TRUE" | "True" | "true" => ConstantValue::Bool(true),
                "FALSE" | "False" | "false" => ConstantValue::Bool(false),
                // For other identifiers, treat as string
                _ => ConstantValue::String(id.clone()),
            }
        }
    }
}

// ============================================================================
// C Type Mapping (for nros-c)
// ============================================================================

/// Default string capacity for C strings (matches nros-c)
pub const C_DEFAULT_STRING_CAPACITY: usize = 256;

/// Default sequence capacity for C arrays
pub const C_DEFAULT_SEQUENCE_CAPACITY: usize = 64;

/// Get the C base type string for a field type (without array suffix)
/// For array declarations in C, the array suffix comes after the variable name.
/// Use `c_array_suffix_for_field` to get the suffix.
pub fn c_type_for_field(field_type: &FieldType, _current_package: Option<&str>) -> String {
    match field_type {
        FieldType::Primitive(prim) => c_primitive_type(prim),

        // Strings use char as base type, with array suffix for the size
        FieldType::String | FieldType::BoundedString(_) => "char".to_string(),

        FieldType::WString | FieldType::BoundedWString(_) => "char".to_string(),

        // Arrays use the element type as base type, with array suffix for the size
        FieldType::Array { element_type, .. } => c_type_for_field(element_type, _current_package),

        // Sequences use anonymous struct
        FieldType::Sequence { element_type } => {
            let elem = c_type_for_field(element_type, _current_package);
            let elem_suffix = c_array_suffix_for_field(element_type);
            format!(
                "struct {{ uint32_t size; {} data{}[{}]; }}",
                elem, elem_suffix, C_DEFAULT_SEQUENCE_CAPACITY
            )
        }

        FieldType::BoundedSequence {
            element_type,
            max_size,
        } => {
            let elem = c_type_for_field(element_type, _current_package);
            let elem_suffix = c_array_suffix_for_field(element_type);
            format!(
                "struct {{ uint32_t size; {} data{}[{}]; }}",
                elem, elem_suffix, max_size
            )
        }

        FieldType::NamespacedType { package, name } => {
            let pkg = package.as_deref().or(_current_package).unwrap_or("");
            format!(
                "struct {}_msg_{}",
                to_c_package_name(pkg),
                to_snake_case(name)
            )
        }
    }
}

/// Like [`c_type_for_field`], but overrides the capacity of a **top-level
/// unbounded** `Sequence` with `cap` (RFC-0033, `owned` mode). Strings carry
/// their capacity in the array suffix, not the base type, so the string arms
/// are unchanged here — see [`c_array_suffix_for_field_with_capacity`].
pub fn c_type_for_field_with_capacity(
    field_type: &FieldType,
    current_package: Option<&str>,
    cap: usize,
) -> String {
    match field_type {
        FieldType::Sequence { element_type } => {
            let elem = c_type_for_field(element_type, current_package);
            let elem_suffix = c_array_suffix_for_field(element_type);
            format!("struct {{ uint32_t size; {elem} data{elem_suffix}[{cap}]; }}")
        }
        _ => c_type_for_field(field_type, current_package),
    }
}

/// Heap (`mode = "heap"`, RFC-0033) C rendering of a **top-level unbounded**
/// `String` / `WString` or primitive `Sequence`, mirroring rclc's
/// `rosidl_runtime_c__{String, <T>__Sequence}`: a `{ T* data; size_t size;
/// size_t capacity; }` struct (no inline array). The deserializer mallocs;
/// `<struct>_fini` frees. Returns `None` for shapes C-heap does not yet support
/// (sequences of strings / nested messages) so the caller rejects them.
pub fn c_type_for_field_heap(
    field_type: &FieldType,
    current_package: Option<&str>,
) -> Option<String> {
    match field_type {
        // rclc `rosidl_runtime_c__String` shape.
        FieldType::String | FieldType::WString => {
            Some("struct { char* data; size_t size; size_t capacity; }".to_string())
        }
        FieldType::Sequence { element_type } => match element_type.as_ref() {
            FieldType::Primitive(_) => {
                let elem = c_type_for_field(element_type, current_package);
                Some(format!(
                    "struct {{ {elem}* data; size_t size; size_t capacity; }}"
                ))
            }
            // Heap array of fixed-capacity strings (unbounded count, bounded
            // element): `{ char (*data)[N]; size_t size; size_t capacity; }` — a
            // single-level heap allocation; each element is an inline `char[N]`.
            FieldType::String | FieldType::WString => Some(format!(
                "struct {{ char (*data)[{}]; size_t size; size_t capacity; }}",
                C_DEFAULT_STRING_CAPACITY
            )),
            FieldType::BoundedString(n) | FieldType::BoundedWString(n) => Some(format!(
                "struct {{ char (*data)[{n}]; size_t size; size_t capacity; }}"
            )),
            // Heap array of nested message structs (each a fixed inline struct;
            // its own heap fields are released by the element's `_fini`, which
            // the sequence's `_fini` calls per element).
            FieldType::NamespacedType { .. } => {
                let elem = c_type_for_field(element_type, current_package);
                Some(format!(
                    "struct {{ {elem}* data; size_t size; size_t capacity; }}"
                ))
            }
            _ => None,
        },
        _ => None,
    }
}

/// Like [`c_array_suffix_for_field`], but overrides the capacity of a
/// **top-level unbounded** `String` / `WString` (the `char[N]` size) with `cap`.
pub fn c_array_suffix_for_field_with_capacity(field_type: &FieldType, cap: usize) -> String {
    match field_type {
        FieldType::String | FieldType::WString => format!("[{cap}]"),
        _ => c_array_suffix_for_field(field_type),
    }
}

/// Get the C array suffix for a field type (e.g., "[256]" for strings, "[3]" for arrays)
/// This comes after the field name in C declarations: `char name[256];`
pub fn c_array_suffix_for_field(field_type: &FieldType) -> String {
    match field_type {
        FieldType::String => format!("[{}]", C_DEFAULT_STRING_CAPACITY),
        FieldType::BoundedString(size) => format!("[{}]", size),
        FieldType::WString => format!("[{}]", C_DEFAULT_STRING_CAPACITY),
        FieldType::BoundedWString(size) => format!("[{}]", size),
        FieldType::Array { element_type, size } => {
            // For nested arrays (rare), we need to combine suffixes
            let inner_suffix = c_array_suffix_for_field(element_type);
            format!("[{}]{}", size, inner_suffix)
        }
        // Sequences and other types don't have array suffixes (they're inline structs or scalars)
        _ => String::new(),
    }
}

/// Convert a primitive type to its C equivalent
fn c_primitive_type(prim: &rosidl_parser::PrimitiveType) -> String {
    use rosidl_parser::PrimitiveType;
    match prim {
        PrimitiveType::Bool => "bool".to_string(),
        PrimitiveType::Byte => "uint8_t".to_string(),
        PrimitiveType::Char => "char".to_string(),
        PrimitiveType::Int8 => "int8_t".to_string(),
        PrimitiveType::Int16 => "int16_t".to_string(),
        PrimitiveType::Int32 => "int32_t".to_string(),
        PrimitiveType::Int64 => "int64_t".to_string(),
        PrimitiveType::UInt8 => "uint8_t".to_string(),
        PrimitiveType::UInt16 => "uint16_t".to_string(),
        PrimitiveType::UInt32 => "uint32_t".to_string(),
        PrimitiveType::UInt64 => "uint64_t".to_string(),
        PrimitiveType::Float32 => "float".to_string(),
        PrimitiveType::Float64 => "double".to_string(),
    }
}

/// Convert package name to C-compatible identifier (replace - with _)
pub fn to_c_package_name(name: &str) -> String {
    name.replace('-', "_")
}

/// Get the C type string for a constant
pub fn c_type_for_constant(field_type: &FieldType) -> String {
    match field_type {
        FieldType::Primitive(prim) => c_primitive_type(prim),
        // String constants are const char*
        FieldType::String
        | FieldType::BoundedString(_)
        | FieldType::WString
        | FieldType::BoundedWString(_) => "const char*".to_string(),
        // Arrays and sequences shouldn't normally be constants
        _ => "void*".to_string(),
    }
}

/// Get the CDR write method name for a C primitive type
pub fn c_cdr_write_method(prim: &rosidl_parser::PrimitiveType) -> &'static str {
    use rosidl_parser::PrimitiveType;
    match prim {
        PrimitiveType::Bool => "write_bool",
        PrimitiveType::Byte => "write_u8",
        PrimitiveType::Char => "write_u8",
        PrimitiveType::Int8 => "write_i8",
        PrimitiveType::Int16 => "write_i16",
        PrimitiveType::Int32 => "write_i32",
        PrimitiveType::Int64 => "write_i64",
        PrimitiveType::UInt8 => "write_u8",
        PrimitiveType::UInt16 => "write_u16",
        PrimitiveType::UInt32 => "write_u32",
        PrimitiveType::UInt64 => "write_u64",
        PrimitiveType::Float32 => "write_f32",
        PrimitiveType::Float64 => "write_f64",
    }
}

/// Get the CDR read method name for a C primitive type
pub fn c_cdr_read_method(prim: &rosidl_parser::PrimitiveType) -> &'static str {
    use rosidl_parser::PrimitiveType;
    match prim {
        PrimitiveType::Bool => "read_bool",
        PrimitiveType::Byte => "read_u8",
        PrimitiveType::Char => "read_u8",
        PrimitiveType::Int8 => "read_i8",
        PrimitiveType::Int16 => "read_i16",
        PrimitiveType::Int32 => "read_i32",
        PrimitiveType::Int64 => "read_i64",
        PrimitiveType::UInt8 => "read_u8",
        PrimitiveType::UInt16 => "read_u16",
        PrimitiveType::UInt32 => "read_u32",
        PrimitiveType::UInt64 => "read_u64",
        PrimitiveType::Float32 => "read_f32",
        PrimitiveType::Float64 => "read_f64",
    }
}

// ============================================================================
// C++ Type Mapping (for nros-cpp)
// ============================================================================

/// Default string capacity for C++ FixedString (matches C API)
pub const CPP_DEFAULT_STRING_CAPACITY: usize = 256;

/// Default sequence capacity for C++ FixedSequence (matches C API)
pub const CPP_DEFAULT_SEQUENCE_CAPACITY: usize = 64;

/// Get the Rust `#[repr(C)]` type for a field (for C++ FFI glue)
pub fn repr_c_type_for_field(field_type: &FieldType, current_package: Option<&str>) -> String {
    match field_type {
        FieldType::Primitive(prim) => repr_c_primitive_type(prim).to_string(),

        // Strings map to [u8; N] (repr(C) compatible with char[N])
        FieldType::String => format!("[u8; {}]", CPP_DEFAULT_STRING_CAPACITY),
        FieldType::BoundedString(size) => format!("[u8; {}]", size),
        FieldType::WString => format!("[u8; {}]", CPP_DEFAULT_STRING_CAPACITY),
        FieldType::BoundedWString(size) => format!("[u8; {}]", size),

        // Arrays use [T; N]
        FieldType::Array { element_type, size } => {
            let elem = repr_c_type_for_field(element_type, current_package);
            format!("[{}; {}]", elem, size)
        }

        // Sequences use named struct type (generated separately)
        FieldType::Sequence { .. } | FieldType::BoundedSequence { .. } => {
            // This will be overridden by the caller with the sequence struct name
            String::new()
        }

        FieldType::NamespacedType { package, name } => {
            // When package is None the type is from the current package
            let pkg = package.as_deref().or(current_package).unwrap_or("unknown");
            format!("{}_msg_{}_t", to_c_package_name(pkg), to_snake_case(name))
        }
    }
}

/// Get the Rust `#[repr(C)]` primitive type
fn repr_c_primitive_type(prim: &rosidl_parser::PrimitiveType) -> &'static str {
    use rosidl_parser::PrimitiveType;
    match prim {
        PrimitiveType::Bool => "bool",
        PrimitiveType::Byte => "u8",
        PrimitiveType::Char => "u8",
        PrimitiveType::Int8 => "i8",
        PrimitiveType::Int16 => "i16",
        PrimitiveType::Int32 => "i32",
        PrimitiveType::Int64 => "i64",
        PrimitiveType::UInt8 => "u8",
        PrimitiveType::UInt16 => "u16",
        PrimitiveType::UInt32 => "u32",
        PrimitiveType::UInt64 => "u64",
        PrimitiveType::Float32 => "f32",
        PrimitiveType::Float64 => "f64",
    }
}

/// Get the C++ type for a field (for C++ header generation)
///
/// Uses `FixedString<N>` for strings and `FixedSequence<T,N>` for sequences.
pub fn cpp_type_for_field(field_type: &FieldType, current_package: Option<&str>) -> String {
    match field_type {
        FieldType::Primitive(prim) => c_primitive_type(prim),

        FieldType::String => format!("nros::FixedString<{}>", CPP_DEFAULT_STRING_CAPACITY),
        FieldType::BoundedString(size) => format!("nros::FixedString<{}>", size),
        FieldType::WString => format!("nros::FixedString<{}>", CPP_DEFAULT_STRING_CAPACITY),
        FieldType::BoundedWString(size) => format!("nros::FixedString<{}>", size),

        FieldType::Array { element_type, size } => {
            let elem = cpp_type_for_field(element_type, current_package);
            format!("{}[{}]", elem, size)
        }

        FieldType::Sequence { element_type } => {
            let elem = cpp_type_for_field(element_type, current_package);
            format!(
                "nros::FixedSequence<{}, {}>",
                elem, CPP_DEFAULT_SEQUENCE_CAPACITY
            )
        }

        FieldType::BoundedSequence {
            element_type,
            max_size,
        } => {
            let elem = cpp_type_for_field(element_type, current_package);
            format!("nros::FixedSequence<{}, {}>", elem, max_size)
        }

        FieldType::NamespacedType { package, name } => {
            // Always emit fully qualified names so the type resolves correctly from any
            // namespace context (pkg::msg for messages, pkg::srv for services).
            let pkg = package.as_deref().or(current_package);
            if let Some(pkg) = pkg {
                format!("{}::msg::{}", pkg, name)
            } else {
                name.clone()
            }
        }
    }
}

/// Like [`cpp_type_for_field`], but overrides the capacity of a **top-level
/// unbounded** `String` / `WString` / `Sequence` with `cap` (RFC-0033, `owned`).
pub fn cpp_type_for_field_with_capacity(
    field_type: &FieldType,
    current_package: Option<&str>,
    cap: usize,
) -> String {
    match field_type {
        FieldType::String | FieldType::WString => format!("nros::FixedString<{cap}>"),
        FieldType::Sequence { element_type } => {
            let elem = cpp_type_for_field(element_type, current_package);
            format!("nros::FixedSequence<{elem}, {cap}>")
        }
        _ => cpp_type_for_field(field_type, current_package),
    }
}

/// Heap (`mode = "heap"`, RFC-0033) C++ rendering of a **top-level unbounded**
/// `String` / `WString` (`nros::HeapString`) or primitive `Sequence`
/// (`nros::HeapSequence<elem>`) — both `{ ptr; size_t size; size_t capacity; }`
/// allocated via the shared platform allocator. Returns `None` for shapes
/// C++-heap does not yet support (sequences of strings / nested messages).
pub fn cpp_type_for_field_heap(
    field_type: &FieldType,
    current_package: Option<&str>,
) -> Option<String> {
    match field_type {
        FieldType::String | FieldType::WString => Some("nros::HeapString".to_string()),
        // Primitive / string / nested elements are all trivially-copyable inline
        // structs (`FixedString<N>` is `char[N]`; nested message structs are POD),
        // so `nros::HeapSequence<T>` (raw-malloc'd) is safe for each.
        FieldType::Sequence { element_type } => match element_type.as_ref() {
            FieldType::Primitive(_)
            | FieldType::String
            | FieldType::WString
            | FieldType::BoundedString(_)
            | FieldType::BoundedWString(_)
            | FieldType::NamespacedType { .. } => {
                let elem = cpp_type_for_field(element_type, current_package);
                Some(format!("nros::HeapSequence<{elem}>"))
            }
            _ => None,
        },
        _ => None,
    }
}

/// Like [`repr_c_type_for_field`], but overrides the `[u8; N]` size of a
/// **top-level unbounded** `String` / `WString` with `cap`. Sequences keep the
/// empty (caller-overridden) repr; their capacity flows via the sequence struct.
pub fn repr_c_type_for_field_with_capacity(
    field_type: &FieldType,
    current_package: Option<&str>,
    cap: usize,
) -> String {
    match field_type {
        FieldType::String | FieldType::WString => format!("[u8; {cap}]"),
        _ => repr_c_type_for_field(field_type, current_package),
    }
}

/// Get the C++ array suffix for a field type (e.g., "[3]" for fixed arrays)
///
/// Unlike C where strings use array suffix, C++ uses `FixedString<N>` which
/// doesn't need a suffix. Only fixed arrays need the suffix.
pub fn cpp_array_suffix_for_field(field_type: &FieldType) -> String {
    match field_type {
        FieldType::Array { element_type, size } => {
            let inner_suffix = cpp_array_suffix_for_field(element_type);
            format!("[{}]{}", size, inner_suffix)
        }
        _ => String::new(),
    }
}

/// Compute the maximum serialized CDR size for a set of C++ FFI fields.
///
/// Uses conservative estimates:
/// - Primitives: type size + up to 7 bytes alignment padding
/// - Strings: 4 (length) + capacity + 1 (null) + 3 (alignment)
/// - Arrays: element_count × element_size (with alignment)
/// - Sequences: 4 (length) + capacity × element_size (with alignment)
/// - Nested: uses a fixed estimate (512 bytes per nested type)
pub fn compute_serialized_size_max(fields: &[super::templates::CppFfiField]) -> usize {
    let mut size = 4; // CDR header

    for field in fields {
        if field.is_primitive {
            // Primitive: type size + alignment
            size += primitive_cdr_size(&field.cdr_write_method) + 7;
        } else if field.is_string {
            // String: 4 (len) + capacity + 1 (null) + 3 (pad)
            let cap = if field.repr_c_type.starts_with("[u8;") {
                // Parse capacity from "[u8; N]"
                field
                    .repr_c_type
                    .trim_start_matches("[u8; ")
                    .trim_end_matches(']')
                    .parse::<usize>()
                    .unwrap_or(CPP_DEFAULT_STRING_CAPACITY)
            } else {
                CPP_DEFAULT_STRING_CAPACITY
            };
            size += 4 + cap + 1 + 3;
        } else if field.is_array {
            if field.is_primitive_element {
                let elem_size = primitive_cdr_size(&field.element_cdr_write_method);
                size += field.array_size * (elem_size + 7);
            } else if field.is_string_element {
                size += field.array_size * (4 + CPP_DEFAULT_STRING_CAPACITY + 4);
            } else {
                // Nested array
                size += field.array_size * 512;
            }
        } else if field.is_sequence {
            let cap = field.sequence_capacity;
            size += 4; // length prefix
            if field.is_primitive_element {
                let elem_size = primitive_cdr_size(&field.element_cdr_write_method);
                size += cap * (elem_size + 7);
            } else if field.is_string_element {
                size += cap * (4 + CPP_DEFAULT_STRING_CAPACITY + 4);
            } else {
                size += cap * 512;
            }
        } else if field.is_nested {
            size += 512;
        }
    }

    size
}

/// Get the CDR size of a primitive type based on its write method name
fn primitive_cdr_size(write_method: &str) -> usize {
    match write_method {
        "write_bool" | "write_u8" | "write_i8" => 1,
        "write_u16" | "write_i16" => 2,
        "write_u32" | "write_i32" | "write_f32" => 4,
        "write_u64" | "write_i64" | "write_f64" => 8,
        _ => 4,
    }
}
