//! IDL (Interface Definition Language) parser for OMG IDL 4.2 subset.
//!
//! This module implements a parser for the IDL format used by ROS 2, which supports
//! advanced features like annotations, constant modules, wide strings, and enums.

pub mod ast;
pub mod lexer;
pub mod parser;
pub mod types;

pub use ast::{
    Annotation, ConstantModule, EnumDef, EnumVariant, IdlFile, IdlModule, IdlStruct, StructMember,
};
pub use lexer::{IdlToken, IdlTokenKind};
pub use parser::{IdlParseError, parse_idl_file};
pub use types::{IdlPrimitiveType, IdlType};
