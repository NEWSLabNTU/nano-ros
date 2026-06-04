//! IDL parser implementation.

use super::{
    ast::*,
    lexer::{IdlLexer, IdlToken, IdlTokenKind},
    types::{IdlPrimitiveType, IdlType},
};
use std::fmt;

/// Parse error type.
#[derive(Debug, Clone)]
pub struct IdlParseError {
    pub message: String,
    pub line: usize,
    pub column: usize,
}

impl fmt::Display for IdlParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Parse error at {}:{}: {}",
            self.line, self.column, self.message
        )
    }
}

impl std::error::Error for IdlParseError {}

type ParseResult<T> = Result<T, IdlParseError>;

/// IDL parser.
pub struct IdlParser {
    tokens: Vec<IdlToken>,
    position: usize,
}

impl IdlParser {
    /// Create a new parser from tokens.
    fn new(tokens: Vec<IdlToken>) -> Self {
        Self {
            tokens,
            position: 0,
        }
    }

    /// Parse an IDL file from source string.
    pub fn parse(input: &str) -> ParseResult<IdlFile> {
        let mut lexer = IdlLexer::new(input);
        let tokens = lexer.tokenize().map_err(|e| IdlParseError {
            message: e,
            line: 1,
            column: 1,
        })?;

        let mut parser = Self::new(tokens);
        parser.parse_file()
    }

    /// Parse the entire file.
    fn parse_file(&mut self) -> ParseResult<IdlFile> {
        let module = self.parse_module()?;
        Ok(IdlFile::new(module))
    }

    /// Parse a module definition.
    fn parse_module(&mut self) -> ParseResult<IdlModule> {
        // Expect 'module' keyword
        self.expect_keyword(IdlTokenKind::Module)?;

        // Parse module name
        let name = self.expect_identifier()?;

        // Expect '{'
        self.expect_token(&IdlTokenKind::LBrace)?;

        // Create module
        let mut module = IdlModule::new(name);

        // Parse module contents
        while !self.check(&IdlTokenKind::RBrace) && !self.is_at_end() {
            // Check for constant module FIRST (before regular module check)
            // This is important because constant modules start with 'module' keyword too
            if self.peek_ahead_for_constant_module() {
                let const_mod = self.parse_constant_module()?;
                module.add_constant_module(const_mod);
            }
            // Check for nested module
            else if self.check(&IdlTokenKind::Module) {
                let nested_module = self.parse_module()?;
                module.add_module(nested_module);
            }
            // Check for struct
            else if self.peek_ahead_for_struct() {
                let struct_def = self.parse_struct()?;
                module.add_struct(struct_def);
            }
            // Check for enum
            else if self.check(&IdlTokenKind::Enum) {
                let enum_def = self.parse_enum()?;
                module.add_enum(enum_def);
            } else {
                return Err(self.error("Expected module, struct, enum, or constant module"));
            }
        }

        // Expect '}'
        self.expect_token(&IdlTokenKind::RBrace)?;

        // Expect ';'
        self.expect_token(&IdlTokenKind::Semicolon)?;

        Ok(module)
    }

    /// Check if the next tokens form a struct definition (with possible annotations).
    fn peek_ahead_for_struct(&self) -> bool {
        let mut pos = self.position;

        // Skip annotations
        while pos < self.tokens.len() {
            if let IdlTokenKind::Annotation(_) = &self.tokens[pos].kind {
                pos += 1;
                // Skip annotation parameters: @annotation(param=value)
                if pos < self.tokens.len() && matches!(self.tokens[pos].kind, IdlTokenKind::LParen)
                {
                    // Skip until we find the matching )
                    let mut depth = 0;
                    while pos < self.tokens.len() {
                        match self.tokens[pos].kind {
                            IdlTokenKind::LParen => depth += 1,
                            IdlTokenKind::RParen => {
                                depth -= 1;
                                if depth == 0 {
                                    pos += 1;
                                    break;
                                }
                            }
                            _ => {}
                        }
                        pos += 1;
                    }
                }
            } else {
                break;
            }
        }

        // Now check for 'struct' keyword
        pos < self.tokens.len() && matches!(self.tokens[pos].kind, IdlTokenKind::Struct)
    }

    /// Check if the next tokens form a constant module.
    fn peek_ahead_for_constant_module(&self) -> bool {
        // Constant module: module Name { const ... };
        // Distinguish from regular module by looking ahead
        if self.position + 3 < self.tokens.len() {
            matches!(self.tokens[self.position].kind, IdlTokenKind::Module)
                && matches!(
                    self.tokens[self.position + 1].kind,
                    IdlTokenKind::Identifier(_)
                )
                && matches!(self.tokens[self.position + 2].kind, IdlTokenKind::LBrace)
                && (self.position + 3 >= self.tokens.len()
                    || matches!(self.tokens[self.position + 3].kind, IdlTokenKind::Const))
        } else {
            false
        }
    }

    /// Parse a struct definition.
    fn parse_struct(&mut self) -> ParseResult<IdlStruct> {
        // Parse annotations
        let annotations = self.parse_annotations()?;

        // Expect 'struct' keyword
        self.expect_keyword(IdlTokenKind::Struct)?;

        // Parse struct name
        let name = self.expect_identifier()?;

        // Expect '{'
        self.expect_token(&IdlTokenKind::LBrace)?;

        let mut struct_def = IdlStruct::new(name);
        for annotation in annotations {
            struct_def.add_annotation(annotation);
        }

        // Parse struct members
        while !self.check(&IdlTokenKind::RBrace) && !self.is_at_end() {
            let members = self.parse_struct_members()?;
            for member in members {
                struct_def.add_member(member);
            }
        }

        // Expect '}'
        self.expect_token(&IdlTokenKind::RBrace)?;

        // Expect ';'
        self.expect_token(&IdlTokenKind::Semicolon)?;

        Ok(struct_def)
    }

    /// Parse struct members (can have multiple members per line).
    fn parse_struct_members(&mut self) -> ParseResult<Vec<StructMember>> {
        // Parse annotations for the member(s)
        let annotations = self.parse_annotations()?;

        // Parse member type
        let member_type = self.parse_type()?;

        // Parse member names (comma-separated)
        let mut members = Vec::new();
        loop {
            let name = self.expect_identifier()?;

            // Check for array syntax: name[size]
            let final_type = if self.check(&IdlTokenKind::LBracket) {
                self.advance();

                // Parse array size
                let size_token = self.current()?;
                let size = if let IdlTokenKind::IntegerLiteral(n) = size_token.kind {
                    self.advance();
                    n as usize
                } else {
                    return Err(self.error("Expected integer for array size"));
                };

                self.expect_token(&IdlTokenKind::RBracket)?;

                // Wrap type in Array (tuple variant: Box<IdlType>, Vec<usize>)
                IdlType::Array(Box::new(member_type.clone()), vec![size])
            } else {
                member_type.clone()
            };

            let mut member = StructMember::new(name, final_type);
            for annotation in &annotations {
                member.add_annotation(annotation.clone());
            }
            members.push(member);

            // Check for comma (more members with same type)
            if self.check(&IdlTokenKind::Comma) {
                self.advance();
            } else {
                break;
            }
        }

        // Expect ';'
        self.expect_token(&IdlTokenKind::Semicolon)?;

        Ok(members)
    }

    /// Parse a type.
    fn parse_type(&mut self) -> ParseResult<IdlType> {
        let current = self.current()?;

        match &current.kind {
            // Primitive types
            IdlTokenKind::Short | IdlTokenKind::Int16 => {
                self.advance();
                Ok(IdlType::Primitive(IdlPrimitiveType::Short))
            }
            IdlTokenKind::Long => {
                self.advance();
                // Check for 'long long'
                if self.check(&IdlTokenKind::Long) {
                    self.advance();
                    Ok(IdlType::Primitive(IdlPrimitiveType::LongLong))
                } else if self.check(&IdlTokenKind::Double) {
                    self.advance();
                    Ok(IdlType::Primitive(IdlPrimitiveType::LongDouble))
                } else {
                    Ok(IdlType::Primitive(IdlPrimitiveType::Long))
                }
            }
            IdlTokenKind::Unsigned => {
                self.advance();
                // Parse the following type
                let next = self.current()?;
                match &next.kind {
                    IdlTokenKind::Short => {
                        self.advance();
                        Ok(IdlType::Primitive(IdlPrimitiveType::UnsignedShort))
                    }
                    IdlTokenKind::Long => {
                        self.advance();
                        // Check for 'unsigned long long'
                        if self.check(&IdlTokenKind::Long) {
                            self.advance();
                            Ok(IdlType::Primitive(IdlPrimitiveType::UnsignedLongLong))
                        } else {
                            Ok(IdlType::Primitive(IdlPrimitiveType::UnsignedLong))
                        }
                    }
                    _ => Err(self.error("Expected type after 'unsigned'")),
                }
            }
            IdlTokenKind::Int8 => {
                self.advance();
                Ok(IdlType::Primitive(IdlPrimitiveType::Int8))
            }
            IdlTokenKind::Int32 => {
                self.advance();
                Ok(IdlType::Primitive(IdlPrimitiveType::Int32))
            }
            IdlTokenKind::Int64 => {
                self.advance();
                Ok(IdlType::Primitive(IdlPrimitiveType::Int64))
            }
            IdlTokenKind::Uint8 => {
                self.advance();
                Ok(IdlType::Primitive(IdlPrimitiveType::Uint8))
            }
            IdlTokenKind::Uint16 => {
                self.advance();
                Ok(IdlType::Primitive(IdlPrimitiveType::Uint16))
            }
            IdlTokenKind::Uint32 => {
                self.advance();
                Ok(IdlType::Primitive(IdlPrimitiveType::Uint32))
            }
            IdlTokenKind::Uint64 => {
                self.advance();
                Ok(IdlType::Primitive(IdlPrimitiveType::Uint64))
            }
            IdlTokenKind::Float => {
                self.advance();
                Ok(IdlType::Primitive(IdlPrimitiveType::Float))
            }
            IdlTokenKind::Double => {
                self.advance();
                Ok(IdlType::Primitive(IdlPrimitiveType::Double))
            }
            IdlTokenKind::Char => {
                self.advance();
                Ok(IdlType::Primitive(IdlPrimitiveType::Char))
            }
            IdlTokenKind::WChar => {
                self.advance();
                Ok(IdlType::Primitive(IdlPrimitiveType::Wchar))
            }
            IdlTokenKind::Boolean => {
                self.advance();
                Ok(IdlType::Primitive(IdlPrimitiveType::Boolean))
            }
            IdlTokenKind::Octet => {
                self.advance();
                Ok(IdlType::Primitive(IdlPrimitiveType::Octet))
            }
            // String types
            IdlTokenKind::String => {
                self.advance();
                let bound = self.parse_optional_bound()?;
                Ok(IdlType::String(bound))
            }
            IdlTokenKind::WString => {
                self.advance();
                let bound = self.parse_optional_bound()?;
                Ok(IdlType::WString(bound))
            }
            // Sequence type
            IdlTokenKind::Sequence => {
                self.advance();
                self.expect_token(&IdlTokenKind::LAngle)?;
                let element_type = Box::new(self.parse_type()?);

                // Check for bound
                let bound = if self.check(&IdlTokenKind::Comma) {
                    self.advance();
                    let bound_value = self.expect_integer()?;
                    Some(bound_value as usize)
                } else {
                    None
                };

                self.expect_token(&IdlTokenKind::RAngle)?;
                Ok(IdlType::Sequence(element_type, bound))
            }
            // User-defined type or scoped name
            IdlTokenKind::Identifier(name) => {
                let type_name = name.clone();
                self.advance();

                // Check for scoped name (::)
                if self.check(&IdlTokenKind::ScopeResolution) {
                    let mut path = vec![type_name];
                    while self.check(&IdlTokenKind::ScopeResolution) {
                        self.advance();
                        path.push(self.expect_identifier()?);
                    }
                    Ok(IdlType::Scoped(path))
                } else {
                    Ok(IdlType::UserDefined(type_name))
                }
            }
            _ => Err(self.error(&format!("Expected type, got {:?}", current.kind))),
        }
    }

    /// Parse optional bound for strings/sequences: <N>
    fn parse_optional_bound(&mut self) -> ParseResult<Option<usize>> {
        if self.check(&IdlTokenKind::LAngle) {
            self.advance();
            let bound = self.expect_integer()? as usize;
            self.expect_token(&IdlTokenKind::RAngle)?;
            Ok(Some(bound))
        } else {
            Ok(None)
        }
    }

    /// Parse a constant module.
    fn parse_constant_module(&mut self) -> ParseResult<ConstantModule> {
        // Expect 'module' keyword
        self.expect_keyword(IdlTokenKind::Module)?;

        // Parse module name
        let name = self.expect_identifier()?;

        // Expect '{'
        self.expect_token(&IdlTokenKind::LBrace)?;

        let mut const_mod = ConstantModule::new(name);

        // Parse constants
        while !self.check(&IdlTokenKind::RBrace) && !self.is_at_end() {
            let constant = self.parse_constant()?;
            const_mod.add_constant(constant);
        }

        // Expect '}'
        self.expect_token(&IdlTokenKind::RBrace)?;

        // Expect ';'
        self.expect_token(&IdlTokenKind::Semicolon)?;

        Ok(const_mod)
    }

    /// Parse a constant definition.
    fn parse_constant(&mut self) -> ParseResult<IdlConstant> {
        // Expect 'const' keyword
        self.expect_keyword(IdlTokenKind::Const)?;

        // Parse constant type
        let const_type = self.parse_type()?;

        // Parse constant name
        let name = self.expect_identifier()?;

        // Expect '='
        self.expect_token(&IdlTokenKind::Equal)?;

        // Parse constant value
        let value = self.parse_constant_value(&const_type)?;

        // Expect ';'
        self.expect_token(&IdlTokenKind::Semicolon)?;

        Ok(IdlConstant {
            name,
            const_type,
            value,
        })
    }

    /// Parse a constant value.
    fn parse_constant_value(&mut self, const_type: &IdlType) -> ParseResult<ConstantValue> {
        let current = self.current()?;

        match &current.kind {
            IdlTokenKind::IntegerLiteral(i) => {
                let value = *i;
                self.advance();
                Ok(ConstantValue::Integer(value))
            }
            IdlTokenKind::FloatLiteral(f) => {
                let value = *f;
                self.advance();
                Ok(ConstantValue::Float(value))
            }
            IdlTokenKind::True => {
                self.advance();
                Ok(ConstantValue::Boolean(true))
            }
            IdlTokenKind::False => {
                self.advance();
                Ok(ConstantValue::Boolean(false))
            }
            IdlTokenKind::StringLiteral(s) => {
                let value = s.clone();
                self.advance();
                if const_type.is_wide_string() {
                    Ok(ConstantValue::WString(value))
                } else {
                    Ok(ConstantValue::String(value))
                }
            }
            _ => Err(self.error(&format!("Expected constant value, got {:?}", current.kind))),
        }
    }

    /// Parse an enum definition.
    fn parse_enum(&mut self) -> ParseResult<EnumDef> {
        // Expect 'enum' keyword
        self.expect_keyword(IdlTokenKind::Enum)?;

        // Parse enum name
        let name = self.expect_identifier()?;

        // Expect '{'
        self.expect_token(&IdlTokenKind::LBrace)?;

        let mut enum_def = EnumDef::new(name);

        // Parse enum variants
        while !self.check(&IdlTokenKind::RBrace) && !self.is_at_end() {
            let variant_name = self.expect_identifier()?;

            // Check for explicit value
            let value = if self.check(&IdlTokenKind::Equal) {
                self.advance();
                Some(self.expect_integer()?)
            } else {
                None
            };

            enum_def.add_variant(EnumVariant {
                name: variant_name,
                value,
            });

            // Comma is optional for last variant
            if self.check(&IdlTokenKind::Comma) {
                self.advance();
            }
        }

        // Expect '}'
        self.expect_token(&IdlTokenKind::RBrace)?;

        // Expect ';'
        self.expect_token(&IdlTokenKind::Semicolon)?;

        Ok(enum_def)
    }

    /// Parse annotations.
    fn parse_annotations(&mut self) -> ParseResult<Vec<Annotation>> {
        let mut annotations = Vec::new();

        while let Ok(current) = self.current() {
            if let IdlTokenKind::Annotation(name) = &current.kind {
                let annotation_name = name.clone();
                self.advance();

                let mut annotation = Annotation::new(annotation_name);

                // Check for annotation parameters
                if self.check(&IdlTokenKind::LParen) {
                    self.advance();

                    // Parse parameters (can be key=value pairs or single values)
                    let mut param_index = 0;
                    while !self.check(&IdlTokenKind::RParen) && !self.is_at_end() {
                        // Try to parse as key=value or as a single value
                        if let Ok(current) = self.current() {
                            if let IdlTokenKind::Identifier(id) = &current.kind {
                                let id_value = id.clone();
                                self.advance();

                                // Check if next token is '=' (key=value pair)
                                if self.check(&IdlTokenKind::Equal) {
                                    self.advance();
                                    let value = self.parse_annotation_value()?;
                                    annotation.add_param(id_value, value);
                                } else {
                                    // Single identifier value (use index as key)
                                    let key = format!("_{}", param_index);
                                    annotation
                                        .add_param(key, AnnotationValue::Identifier(id_value));
                                    param_index += 1;
                                }
                            } else {
                                // Not an identifier, parse as a value with index key
                                let value = self.parse_annotation_value()?;
                                let key = format!("_{}", param_index);
                                annotation.add_param(key, value);
                                param_index += 1;
                            }
                        }

                        // Comma is optional for last parameter
                        if self.check(&IdlTokenKind::Comma) {
                            self.advance();
                        }
                    }

                    self.expect_token(&IdlTokenKind::RParen)?;
                }

                annotations.push(annotation);
            } else {
                break;
            }
        }

        Ok(annotations)
    }

    /// Parse an annotation value.
    fn parse_annotation_value(&mut self) -> ParseResult<AnnotationValue> {
        let current = self.current()?;

        match &current.kind {
            IdlTokenKind::IntegerLiteral(i) => {
                let value = *i;
                self.advance();
                Ok(AnnotationValue::Integer(value))
            }
            IdlTokenKind::FloatLiteral(f) => {
                let value = *f;
                self.advance();
                Ok(AnnotationValue::Float(value))
            }
            IdlTokenKind::True => {
                self.advance();
                Ok(AnnotationValue::Boolean(true))
            }
            IdlTokenKind::False => {
                self.advance();
                Ok(AnnotationValue::Boolean(false))
            }
            IdlTokenKind::StringLiteral(s) => {
                // Handle adjacent string literals (concatenate them)
                let mut value = s.clone();
                self.advance();

                // Keep concatenating if we find more adjacent string literals
                while self.position < self.tokens.len() {
                    if let IdlTokenKind::StringLiteral(next_s) = &self.tokens[self.position].kind {
                        value.push_str(next_s);
                        self.advance();
                    } else {
                        break;
                    }
                }

                Ok(AnnotationValue::String(value))
            }
            IdlTokenKind::Identifier(id) => {
                let value = id.clone();
                self.advance();
                Ok(AnnotationValue::Identifier(value))
            }
            _ => Err(self.error(&format!(
                "Expected annotation value, got {:?}",
                current.kind
            ))),
        }
    }

    // Helper methods

    fn current(&self) -> ParseResult<&IdlToken> {
        self.tokens.get(self.position).ok_or_else(|| IdlParseError {
            message: "Unexpected end of input".to_string(),
            line: 0,
            column: 0,
        })
    }

    fn check(&self, kind: &IdlTokenKind) -> bool {
        if let Ok(current) = self.current() {
            std::mem::discriminant(&current.kind) == std::mem::discriminant(kind)
        } else {
            false
        }
    }

    fn advance(&mut self) {
        if self.position < self.tokens.len() {
            self.position += 1;
        }
    }

    fn is_at_end(&self) -> bool {
        self.position >= self.tokens.len()
            || matches!(
                self.current().ok().map(|t| &t.kind),
                Some(IdlTokenKind::Eof)
            )
    }

    fn expect_keyword(&mut self, kind: IdlTokenKind) -> ParseResult<()> {
        let current = self.current()?;
        if std::mem::discriminant(&current.kind) == std::mem::discriminant(&kind) {
            self.advance();
            Ok(())
        } else {
            Err(IdlParseError {
                message: format!("Expected {:?}, got {:?}", kind, current.kind),
                line: current.span.line,
                column: current.span.column,
            })
        }
    }

    fn expect_token(&mut self, kind: &IdlTokenKind) -> ParseResult<()> {
        let current = self.current()?;
        if std::mem::discriminant(&current.kind) == std::mem::discriminant(kind) {
            self.advance();
            Ok(())
        } else {
            Err(IdlParseError {
                message: format!("Expected {:?}, got {:?}", kind, current.kind),
                line: current.span.line,
                column: current.span.column,
            })
        }
    }

    fn expect_identifier(&mut self) -> ParseResult<String> {
        let current = self.current()?;
        if let IdlTokenKind::Identifier(name) = &current.kind {
            let name = name.clone();
            self.advance();
            Ok(name)
        } else {
            Err(IdlParseError {
                message: format!("Expected identifier, got {:?}", current.kind),
                line: current.span.line,
                column: current.span.column,
            })
        }
    }

    fn expect_integer(&mut self) -> ParseResult<i64> {
        let current = self.current()?;
        if let IdlTokenKind::IntegerLiteral(i) = &current.kind {
            let value = *i;
            self.advance();
            Ok(value)
        } else {
            Err(IdlParseError {
                message: format!("Expected integer literal, got {:?}", current.kind),
                line: current.span.line,
                column: current.span.column,
            })
        }
    }

    fn error(&self, message: &str) -> IdlParseError {
        if let Ok(current) = self.current() {
            IdlParseError {
                message: message.to_string(),
                line: current.span.line,
                column: current.span.column,
            }
        } else {
            IdlParseError {
                message: message.to_string(),
                line: 0,
                column: 0,
            }
        }
    }
}

/// Parse an IDL file from a string.
pub fn parse_idl_file(input: &str) -> ParseResult<IdlFile> {
    IdlParser::parse(input)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_struct() {
        let input = r#"
            module test_package {
                module msg {
                    struct SimpleMessage {
                        short value;
                    };
                };
            };
        "#;

        let result = parse_idl_file(input);
        assert!(result.is_ok());
        let idl_file = result.unwrap();
        assert_eq!(idl_file.module.name, "test_package");
    }

    #[test]
    fn test_parse_constant_module() {
        let input = r#"
            module test_package {
                module msg {
                    module MyMessage_Constants {
                        const short CONST_VALUE = 42;
                    };
                };
            };
        "#;

        let result = parse_idl_file(input);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_annotations() {
        let input = r#"
            module test_package {
                module msg {
                    @verbatim(language="comment", text="Test message")
                    struct MyMessage {
                        @key
                        long id;
                        @default(value=123)
                        unsigned short count;
                    };
                };
            };
        "#;

        let result = parse_idl_file(input);
        assert!(result.is_ok());
    }
}
