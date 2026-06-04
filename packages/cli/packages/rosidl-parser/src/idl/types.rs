//! IDL type definitions and mappings.

use std::fmt;

/// IDL primitive types as defined in OMG IDL 4.2.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum IdlPrimitiveType {
    // Integer types
    Short,            // 16-bit signed
    UnsignedShort,    // 16-bit unsigned
    Long,             // 32-bit signed
    UnsignedLong,     // 32-bit unsigned
    LongLong,         // 64-bit signed
    UnsignedLongLong, // 64-bit unsigned

    // Fixed-size integer types (ROS 2 extension)
    Int8,
    Int16,
    Int32,
    Int64,
    Uint8,
    Uint16,
    Uint32,
    Uint64,

    // Floating point types
    Float,      // 32-bit
    Double,     // 64-bit
    LongDouble, // 128-bit (not fully supported in Rust)

    // Character types
    Char,  // 8-bit
    Wchar, // 16-bit wide character

    // Other types
    Boolean,
    Octet, // 8-bit unsigned
}

impl IdlPrimitiveType {
    /// Convert IDL primitive type to Rust type.
    pub fn to_rust_type(&self) -> &'static str {
        match self {
            IdlPrimitiveType::Short | IdlPrimitiveType::Int16 => "i16",
            IdlPrimitiveType::UnsignedShort | IdlPrimitiveType::Uint16 => "u16",
            IdlPrimitiveType::Long | IdlPrimitiveType::Int32 => "i32",
            IdlPrimitiveType::UnsignedLong | IdlPrimitiveType::Uint32 => "u32",
            IdlPrimitiveType::LongLong | IdlPrimitiveType::Int64 => "i64",
            IdlPrimitiveType::UnsignedLongLong | IdlPrimitiveType::Uint64 => "u64",
            IdlPrimitiveType::Int8 => "i8",
            IdlPrimitiveType::Uint8 | IdlPrimitiveType::Octet => "u8",
            IdlPrimitiveType::Float => "f32",
            IdlPrimitiveType::Double => "f64",
            IdlPrimitiveType::LongDouble => "f64", // Rust doesn't have f128
            IdlPrimitiveType::Char => "u8",
            IdlPrimitiveType::Wchar => "u16",
            IdlPrimitiveType::Boolean => "bool",
        }
    }

    /// Get the rosidl_runtime_rs type for this primitive (for sequences).
    pub fn to_runtime_type(&self) -> &'static str {
        match self {
            IdlPrimitiveType::Short | IdlPrimitiveType::Int16 => "I16",
            IdlPrimitiveType::UnsignedShort | IdlPrimitiveType::Uint16 => "U16",
            IdlPrimitiveType::Long | IdlPrimitiveType::Int32 => "I32",
            IdlPrimitiveType::UnsignedLong | IdlPrimitiveType::Uint32 => "U32",
            IdlPrimitiveType::LongLong | IdlPrimitiveType::Int64 => "I64",
            IdlPrimitiveType::UnsignedLongLong | IdlPrimitiveType::Uint64 => "U64",
            IdlPrimitiveType::Int8 => "I8",
            IdlPrimitiveType::Uint8 | IdlPrimitiveType::Octet => "U8",
            IdlPrimitiveType::Float => "F32",
            IdlPrimitiveType::Double => "F64",
            IdlPrimitiveType::LongDouble => "F64",
            IdlPrimitiveType::Char => "U8",
            IdlPrimitiveType::Wchar => "U16",
            IdlPrimitiveType::Boolean => "Bool",
        }
    }
}

impl fmt::Display for IdlPrimitiveType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            IdlPrimitiveType::Short => "short",
            IdlPrimitiveType::UnsignedShort => "unsigned short",
            IdlPrimitiveType::Long => "long",
            IdlPrimitiveType::UnsignedLong => "unsigned long",
            IdlPrimitiveType::LongLong => "long long",
            IdlPrimitiveType::UnsignedLongLong => "unsigned long long",
            IdlPrimitiveType::Int8 => "int8",
            IdlPrimitiveType::Int16 => "int16",
            IdlPrimitiveType::Int32 => "int32",
            IdlPrimitiveType::Int64 => "int64",
            IdlPrimitiveType::Uint8 => "uint8",
            IdlPrimitiveType::Uint16 => "uint16",
            IdlPrimitiveType::Uint32 => "uint32",
            IdlPrimitiveType::Uint64 => "uint64",
            IdlPrimitiveType::Float => "float",
            IdlPrimitiveType::Double => "double",
            IdlPrimitiveType::LongDouble => "long double",
            IdlPrimitiveType::Char => "char",
            IdlPrimitiveType::Wchar => "wchar",
            IdlPrimitiveType::Boolean => "boolean",
            IdlPrimitiveType::Octet => "octet",
        };
        write!(f, "{}", name)
    }
}

/// IDL type representation including complex types.
#[derive(Debug, Clone, PartialEq)]
pub enum IdlType {
    /// Primitive type
    Primitive(IdlPrimitiveType),

    /// String type (optionally bounded)
    String(Option<usize>),

    /// Wide string type (optionally bounded)
    WString(Option<usize>),

    /// Sequence type (element type, optional bound)
    Sequence(Box<IdlType>, Option<usize>),

    /// Array type (element type, dimensions)
    Array(Box<IdlType>, Vec<usize>),

    /// User-defined type (struct, enum, etc.)
    UserDefined(String),

    /// Scoped name (module::Type)
    Scoped(Vec<String>),
}

impl IdlType {
    /// Check if this type is a wide string.
    pub fn is_wide_string(&self) -> bool {
        matches!(self, IdlType::WString(_))
    }

    /// Check if this type is a sequence.
    pub fn is_sequence(&self) -> bool {
        matches!(self, IdlType::Sequence(_, _))
    }

    /// Check if this type is an array.
    pub fn is_array(&self) -> bool {
        matches!(self, IdlType::Array(_, _))
    }

    /// Get the bound if this is a bounded string or sequence.
    pub fn get_bound(&self) -> Option<usize> {
        match self {
            IdlType::String(bound) | IdlType::WString(bound) | IdlType::Sequence(_, bound) => {
                *bound
            }
            _ => None,
        }
    }
}

impl fmt::Display for IdlType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            IdlType::Primitive(p) => write!(f, "{}", p),
            IdlType::String(None) => write!(f, "string"),
            IdlType::String(Some(bound)) => write!(f, "string<{}>", bound),
            IdlType::WString(None) => write!(f, "wstring"),
            IdlType::WString(Some(bound)) => write!(f, "wstring<{}>", bound),
            IdlType::Sequence(elem, None) => write!(f, "sequence<{}>", elem),
            IdlType::Sequence(elem, Some(bound)) => write!(f, "sequence<{}, {}>", elem, bound),
            IdlType::Array(elem, dims) => {
                write!(f, "{}", elem)?;
                for dim in dims {
                    write!(f, "[{}]", dim)?;
                }
                Ok(())
            }
            IdlType::UserDefined(name) => write!(f, "{}", name),
            IdlType::Scoped(path) => write!(f, "{}", path.join("::")),
        }
    }
}
