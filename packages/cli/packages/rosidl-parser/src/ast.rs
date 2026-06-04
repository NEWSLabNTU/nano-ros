use serde::{Deserialize, Serialize};

/// Primitive types in ROS IDL
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PrimitiveType {
    Bool,
    Byte,
    Char,
    Int8,
    UInt8,
    Int16,
    UInt16,
    Int32,
    UInt32,
    Int64,
    UInt64,
    Float32,
    Float64,
}

impl PrimitiveType {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "bool" | "boolean" => Some(Self::Bool),
            "byte" | "octet" => Some(Self::Byte),
            "char" => Some(Self::Char),
            "int8" => Some(Self::Int8),
            "uint8" => Some(Self::UInt8),
            "int16" | "short" => Some(Self::Int16),
            "uint16" | "unsigned short" => Some(Self::UInt16),
            "int32" | "long" => Some(Self::Int32),
            "uint32" | "unsigned long" => Some(Self::UInt32),
            "int64" | "long long" => Some(Self::Int64),
            "uint64" | "unsigned long long" => Some(Self::UInt64),
            "float32" | "float" => Some(Self::Float32),
            "float64" | "double" => Some(Self::Float64),
            _ => None,
        }
    }

    pub fn rust_type(&self) -> &'static str {
        match self {
            Self::Bool => "bool",
            Self::Byte | Self::Char | Self::UInt8 => "u8",
            Self::Int8 => "i8",
            Self::Int16 => "i16",
            Self::UInt16 => "u16",
            Self::Int32 => "i32",
            Self::UInt32 => "u32",
            Self::Int64 => "i64",
            Self::UInt64 => "u64",
            Self::Float32 => "f32",
            Self::Float64 => "f64",
        }
    }
}

/// Field type specification
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FieldType {
    /// Primitive type
    Primitive(PrimitiveType),
    /// Unbounded string
    String,
    /// Bounded string (string<=N)
    BoundedString(usize),
    /// Unbounded wide string
    WString,
    /// Bounded wide string (wstring<=N)
    BoundedWString(usize),
    /// Fixed-size array (type[N])
    Array {
        element_type: Box<FieldType>,
        size: usize,
    },
    /// Unbounded sequence (type[])
    Sequence { element_type: Box<FieldType> },
    /// Bounded sequence (type[<=N])
    BoundedSequence {
        element_type: Box<FieldType>,
        max_size: usize,
    },
    /// Namespaced type (package/Type or Type)
    NamespacedType {
        package: Option<String>,
        name: String,
    },
}

/// Constant value
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ConstantValue {
    Integer(i64),
    UInteger(u64), // For values that exceed i64::MAX
    Float(f64),
    String(String),
    Bool(bool),
    Array(Vec<ConstantValue>),
}

/// Message field
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Field {
    pub field_type: FieldType,
    pub name: String,
    pub default_value: Option<ConstantValue>,
}

/// Message constant
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Constant {
    pub constant_type: FieldType,
    pub name: String,
    pub value: ConstantValue,
}

/// Message specification
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Message {
    pub fields: Vec<Field>,
    pub constants: Vec<Constant>,
}

/// Service specification
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Service {
    pub request: Message,
    pub response: Message,
}

/// Action specification sections
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActionSpec {
    pub goal: Message,
    pub result: Message,
    pub feedback: Message,
}

/// Action specification
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Action {
    pub spec: ActionSpec,
}

impl Message {
    pub fn new() -> Self {
        Self {
            fields: Vec::new(),
            constants: Vec::new(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.fields.is_empty() && self.constants.is_empty()
    }
}

impl Default for Message {
    fn default() -> Self {
        Self::new()
    }
}
