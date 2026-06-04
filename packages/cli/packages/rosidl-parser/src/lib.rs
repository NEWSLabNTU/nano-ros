pub mod ast;
pub mod lexer;
pub mod parser;

// IDL module for OMG IDL 4.2 support
pub mod idl;

pub use ast::{Action, ActionSpec, Constant, Field, FieldType, Message, PrimitiveType, Service};
pub use lexer::{Token, TokenKind};
pub use parser::{ParseError, parse_action, parse_message, parse_service};

// Re-export IDL types
pub use idl::{
    Annotation, ConstantModule, EnumDef, IdlFile, IdlModule, IdlParseError, IdlPrimitiveType,
    IdlStruct, IdlToken, IdlTokenKind, IdlType, parse_idl_file,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_simple_message() {
        let input = "int32 x\nfloat64 y\n";
        let result = parse_message(input);
        assert!(result.is_ok());
        let msg = result.unwrap();
        assert_eq!(msg.fields.len(), 2);
    }
}
