//! IDL Abstract Syntax Tree (AST) definitions.

use super::types::IdlType;
use std::fmt;

/// Represents a complete IDL file.
#[derive(Debug, Clone, PartialEq)]
pub struct IdlFile {
    /// The root module (usually the package name)
    pub module: IdlModule,
    /// File path (for error reporting)
    pub file_path: Option<String>,
}

/// Represents an IDL module (can be nested).
#[derive(Debug, Clone, PartialEq)]
pub struct IdlModule {
    /// Module name
    pub name: String,
    /// Nested modules
    pub modules: Vec<IdlModule>,
    /// Struct definitions
    pub structs: Vec<IdlStruct>,
    /// Constant modules
    pub constant_modules: Vec<ConstantModule>,
    /// Enum definitions
    pub enums: Vec<EnumDef>,
}

/// Represents a struct definition.
#[derive(Debug, Clone, PartialEq)]
pub struct IdlStruct {
    /// Struct name
    pub name: String,
    /// Annotations applied to the struct
    pub annotations: Vec<Annotation>,
    /// Struct members/fields
    pub members: Vec<StructMember>,
}

/// Represents a struct member/field.
#[derive(Debug, Clone, PartialEq)]
pub struct StructMember {
    /// Member name
    pub name: String,
    /// Member type
    pub member_type: IdlType,
    /// Annotations applied to this member
    pub annotations: Vec<Annotation>,
}

/// Represents a constant module (nested module containing only constants).
#[derive(Debug, Clone, PartialEq)]
pub struct ConstantModule {
    /// Constant module name
    pub name: String,
    /// Constants in this module
    pub constants: Vec<IdlConstant>,
}

/// Represents a constant definition.
#[derive(Debug, Clone, PartialEq)]
pub struct IdlConstant {
    /// Constant name
    pub name: String,
    /// Constant type
    pub const_type: IdlType,
    /// Constant value
    pub value: ConstantValue,
}

/// Represents a constant value.
#[derive(Debug, Clone, PartialEq)]
pub enum ConstantValue {
    Integer(i64),
    Float(f64),
    Boolean(bool),
    String(String),
    WString(String),
}

impl fmt::Display for ConstantValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConstantValue::Integer(i) => write!(f, "{}", i),
            ConstantValue::Float(fl) => {
                // Format float with proper Rust syntax
                if fl.is_finite() {
                    if fl.fract() == 0.0 && !fl.is_nan() {
                        write!(f, "{:.1}", fl) // Ensure .0 suffix
                    } else {
                        write!(f, "{}", fl)
                    }
                } else {
                    write!(f, "{}", fl)
                }
            }
            ConstantValue::Boolean(b) => write!(f, "{}", b),
            ConstantValue::String(s) => write!(f, "\"{}\"", s.escape_default()),
            ConstantValue::WString(s) => write!(f, "\"{}\"", s.escape_default()),
        }
    }
}

/// Represents an enum definition.
#[derive(Debug, Clone, PartialEq)]
pub struct EnumDef {
    /// Enum name
    pub name: String,
    /// Enum variants
    pub variants: Vec<EnumVariant>,
}

/// Represents an enum variant.
#[derive(Debug, Clone, PartialEq)]
pub struct EnumVariant {
    /// Variant name
    pub name: String,
    /// Optional explicit value
    pub value: Option<i64>,
}

/// Represents an annotation (e.g., @key, @default, @verbatim).
#[derive(Debug, Clone, PartialEq)]
pub struct Annotation {
    /// Annotation name (without the @)
    pub name: String,
    /// Annotation parameters (key-value pairs)
    pub params: Vec<(String, AnnotationValue)>,
}

/// Represents an annotation parameter value.
#[derive(Debug, Clone, PartialEq)]
pub enum AnnotationValue {
    Integer(i64),
    Float(f64),
    Boolean(bool),
    String(String),
    Identifier(String),
}

impl Annotation {
    /// Create a new annotation with the given name.
    pub fn new(name: String) -> Self {
        Self {
            name,
            params: Vec::new(),
        }
    }

    /// Add a parameter to this annotation.
    pub fn add_param(&mut self, key: String, value: AnnotationValue) {
        self.params.push((key, value));
    }

    /// Get a parameter value by key.
    pub fn get_param(&self, key: &str) -> Option<&AnnotationValue> {
        self.params.iter().find(|(k, _)| k == key).map(|(_, v)| v)
    }

    /// Check if this annotation has a specific name.
    pub fn is_named(&self, name: &str) -> bool {
        self.name == name
    }
}

impl IdlFile {
    /// Create a new IDL file with the given root module.
    pub fn new(module: IdlModule) -> Self {
        Self {
            module,
            file_path: None,
        }
    }

    /// Set the file path for this IDL file.
    pub fn with_file_path(mut self, path: String) -> Self {
        self.file_path = Some(path);
        self
    }

    /// Find a struct by name in the module hierarchy.
    pub fn find_struct(&self, name: &str) -> Option<&IdlStruct> {
        self.module.find_struct(name)
    }

    /// Find a constant module by name.
    pub fn find_constant_module(&self, name: &str) -> Option<&ConstantModule> {
        self.module.find_constant_module(name)
    }
}

impl IdlModule {
    /// Create a new empty module with the given name.
    pub fn new(name: String) -> Self {
        Self {
            name,
            modules: Vec::new(),
            structs: Vec::new(),
            constant_modules: Vec::new(),
            enums: Vec::new(),
        }
    }

    /// Add a nested module.
    pub fn add_module(&mut self, module: IdlModule) {
        self.modules.push(module);
    }

    /// Add a struct definition.
    pub fn add_struct(&mut self, struct_def: IdlStruct) {
        self.structs.push(struct_def);
    }

    /// Add a constant module.
    pub fn add_constant_module(&mut self, const_mod: ConstantModule) {
        self.constant_modules.push(const_mod);
    }

    /// Add an enum definition.
    pub fn add_enum(&mut self, enum_def: EnumDef) {
        self.enums.push(enum_def);
    }

    /// Find a struct by name (recursively searches nested modules).
    pub fn find_struct(&self, name: &str) -> Option<&IdlStruct> {
        // Search in current module
        if let Some(s) = self.structs.iter().find(|s| s.name == name) {
            return Some(s);
        }

        // Search in nested modules
        for module in &self.modules {
            if let Some(s) = module.find_struct(name) {
                return Some(s);
            }
        }

        None
    }

    /// Find a constant module by name.
    pub fn find_constant_module(&self, name: &str) -> Option<&ConstantModule> {
        // Search in current module
        if let Some(cm) = self.constant_modules.iter().find(|cm| cm.name == name) {
            return Some(cm);
        }

        // Search in nested modules
        for module in &self.modules {
            if let Some(cm) = module.find_constant_module(name) {
                return Some(cm);
            }
        }

        None
    }
}

impl IdlStruct {
    /// Create a new struct with the given name.
    pub fn new(name: String) -> Self {
        Self {
            name,
            annotations: Vec::new(),
            members: Vec::new(),
        }
    }

    /// Add an annotation to this struct.
    pub fn add_annotation(&mut self, annotation: Annotation) {
        self.annotations.push(annotation);
    }

    /// Add a member to this struct.
    pub fn add_member(&mut self, member: StructMember) {
        self.members.push(member);
    }

    /// Check if this struct has a specific annotation.
    pub fn has_annotation(&self, name: &str) -> bool {
        self.annotations.iter().any(|a| a.is_named(name))
    }

    /// Get an annotation by name.
    pub fn get_annotation(&self, name: &str) -> Option<&Annotation> {
        self.annotations.iter().find(|a| a.is_named(name))
    }
}

impl StructMember {
    /// Create a new struct member.
    pub fn new(name: String, member_type: IdlType) -> Self {
        Self {
            name,
            member_type,
            annotations: Vec::new(),
        }
    }

    /// Add an annotation to this member.
    pub fn add_annotation(&mut self, annotation: Annotation) {
        self.annotations.push(annotation);
    }

    /// Check if this member has a specific annotation.
    pub fn has_annotation(&self, name: &str) -> bool {
        self.annotations.iter().any(|a| a.is_named(name))
    }

    /// Get an annotation by name.
    pub fn get_annotation(&self, name: &str) -> Option<&Annotation> {
        self.annotations.iter().find(|a| a.is_named(name))
    }

    /// Get the default value from @default annotation if present.
    pub fn get_default_value(&self) -> Option<&AnnotationValue> {
        self.get_annotation("default")
            .and_then(|a| a.get_param("value"))
    }
}

impl ConstantModule {
    /// Create a new constant module with the given name.
    pub fn new(name: String) -> Self {
        Self {
            name,
            constants: Vec::new(),
        }
    }

    /// Add a constant to this module.
    pub fn add_constant(&mut self, constant: IdlConstant) {
        self.constants.push(constant);
    }
}

impl EnumDef {
    /// Create a new enum with the given name.
    pub fn new(name: String) -> Self {
        Self {
            name,
            variants: Vec::new(),
        }
    }

    /// Add a variant to this enum.
    pub fn add_variant(&mut self, variant: EnumVariant) {
        self.variants.push(variant);
    }
}
