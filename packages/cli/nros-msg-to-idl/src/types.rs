//! ROS-`.msg` → IDL type mapping (mirror of `rosidl_adapter.msg.MSG_TYPE_TO_IDL`
//! + `get_idl_type`).

use core::fmt;

use crate::parser::RosType;

/// Errors raised by the `.msg` parser / IDL emitter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConvertError {
    /// A field line could not be split into `<type> <name>`.
    InvalidFieldDefinition(String),
    /// A type token contains array brackets but no opening `[`.
    MalformedArray(String),
    /// A string-bound or array-bound is not a positive integer.
    InvalidBound(String),
    /// `<pkg>/<Type>` reference is missing the package half and
    /// no context package was passed in.
    InvalidResourceName(String),
    /// Unsupported feature in the input `.msg` (constants with
    /// default values, etc.) — currently the port targets the
    /// `rcl_interfaces`/`std_msgs`/`geometry_msgs`/`sensor_msgs`
    /// common case; richer surfaces become explicit
    /// `Unsupported` errors so callers can fall back to python.
    Unsupported(String),
}

impl fmt::Display for ConvertError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidFieldDefinition(s) => write!(f, "invalid field definition: {s}"),
            Self::MalformedArray(s) => write!(f, "malformed array type: {s}"),
            Self::InvalidBound(s) => write!(f, "invalid bound: {s}"),
            Self::InvalidResourceName(s) => write!(f, "invalid resource name: {s}"),
            Self::Unsupported(s) => write!(f, "unsupported .msg feature: {s}"),
        }
    }
}

impl std::error::Error for ConvertError {}

/// Mirror of `rosidl_adapter.msg.MSG_TYPE_TO_IDL`.
pub(crate) fn primitive_idl(name: &str) -> Option<&'static str> {
    Some(match name {
        "bool" => "boolean",
        "byte" => "octet",
        "char" => "uint8",
        "int8" => "int8",
        "uint8" => "uint8",
        "int16" => "int16",
        "uint16" => "uint16",
        "int32" => "int32",
        "uint32" => "uint32",
        "int64" => "int64",
        "uint64" => "uint64",
        "float32" => "float",
        "float64" => "double",
        "string" => "string",
        "wstring" => "wstring",
        _ => return None,
    })
}

/// Equivalent of `rosidl_adapter.msg.get_idl_type(type_)`.
///
/// Returns the IDL spelling of a parsed ROS type, e.g.
///
/// - `int32` → `"int32"`
/// - `string<=20` → `"string<20>"`
/// - `geometry_msgs/Vector3` → `"geometry_msgs::msg::Vector3"`
/// - `uint8[10]` → `"uint8[10]"`
/// - `uint8[]` → `"sequence<uint8>"`
/// - `uint8[<=5]` → `"sequence<uint8, 5>"`
pub fn idl_type_for(ty: &RosType) -> String {
    let base = if let Some(prim) = primitive_idl(&ty.base) {
        if (prim == "string" || prim == "wstring") && ty.string_upper_bound.is_some() {
            format!("{prim}<{}>", ty.string_upper_bound.unwrap())
        } else {
            prim.to_string()
        }
    } else {
        // Non-primitive — `<pkg>::msg::<Type>`.
        format!("{}::msg::{}", ty.pkg.as_deref().unwrap_or(""), ty.base)
    };

    if !ty.is_array {
        return base;
    }

    if ty.is_fixed_size_array() {
        return format!("{base}[{}]", ty.array_size.unwrap());
    }

    match (ty.array_size, ty.is_upper_bound) {
        (Some(n), true) => format!("sequence<{base}, {n}>"),
        _ => format!("sequence<{base}>"),
    }
}
