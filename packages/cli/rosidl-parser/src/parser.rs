use crate::{
    ast::*,
    lexer::{Token, TokenKind},
};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ParseError {
    #[error("Unexpected token: expected {expected}, got {got}")]
    UnexpectedToken { expected: String, got: String },

    #[error("Unexpected end of input")]
    UnexpectedEOF,

    #[error("Invalid integer literal: {0}")]
    InvalidInteger(String),

    #[error("Invalid float literal: {0}")]
    InvalidFloat(String),

    #[error("Unknown type: {0}")]
    UnknownType(String),

    #[error("Lexer error: {0}")]
    LexerError(String),
}

pub type ParseResult<T> = Result<T, ParseError>;

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

impl Parser {
    fn new(tokens: Vec<Token>) -> Self {
        Self { tokens, pos: 0 }
    }

    fn current(&self) -> Option<&Token> {
        self.tokens.get(self.pos)
    }

    fn advance(&mut self) -> Option<&Token> {
        if self.pos < self.tokens.len() {
            let token = &self.tokens[self.pos];
            self.pos += 1;
            Some(token)
        } else {
            None
        }
    }

    fn expect(&mut self, kind: TokenKind) -> ParseResult<String> {
        match self.advance() {
            Some(token) if token.kind == kind => Ok(token.text.clone()),
            Some(token) => Err(ParseError::UnexpectedToken {
                expected: format!("{:?}", kind),
                got: token.text.clone(),
            }),
            None => Err(ParseError::UnexpectedEOF),
        }
    }

    fn parse_integer(&self, text: &str, kind: &TokenKind, is_negative: bool) -> ParseResult<i64> {
        // For negative numbers, prepend minus sign to handle i64::MIN correctly
        let text_with_sign = if is_negative {
            format!("-{}", text)
        } else {
            text.to_string()
        };

        let result = match kind {
            TokenKind::HexInteger if is_negative => {
                // Parse without sign, then negate
                i64::from_str_radix(&text[2..], 16).map(|v| -v)
            }
            TokenKind::HexInteger => i64::from_str_radix(&text[2..], 16),
            TokenKind::BinaryInteger if is_negative => {
                i64::from_str_radix(&text[2..], 2).map(|v| -v)
            }
            TokenKind::BinaryInteger => i64::from_str_radix(&text[2..], 2),
            TokenKind::OctalInteger if is_negative => {
                i64::from_str_radix(&text[2..], 8).map(|v| -v)
            }
            TokenKind::OctalInteger => i64::from_str_radix(&text[2..], 8),
            TokenKind::DecimalInteger => text_with_sign.parse(),
            _ => return Err(ParseError::InvalidInteger(text.to_string())),
        };

        result.map_err(|_| ParseError::InvalidInteger(text_with_sign))
    }

    fn parse_field_type(&mut self) -> ParseResult<FieldType> {
        let token = self.advance().ok_or(ParseError::UnexpectedEOF)?;

        let base_type = match &token.kind {
            // Primitive types
            TokenKind::Bool => FieldType::Primitive(PrimitiveType::Bool),
            TokenKind::Byte => FieldType::Primitive(PrimitiveType::Byte),
            TokenKind::Char => FieldType::Primitive(PrimitiveType::Char),
            TokenKind::Int8 => FieldType::Primitive(PrimitiveType::Int8),
            TokenKind::UInt8 => FieldType::Primitive(PrimitiveType::UInt8),
            TokenKind::Int16 => FieldType::Primitive(PrimitiveType::Int16),
            TokenKind::UInt16 => FieldType::Primitive(PrimitiveType::UInt16),
            TokenKind::Int32 => FieldType::Primitive(PrimitiveType::Int32),
            TokenKind::UInt32 => FieldType::Primitive(PrimitiveType::UInt32),
            TokenKind::Int64 => FieldType::Primitive(PrimitiveType::Int64),
            TokenKind::UInt64 => FieldType::Primitive(PrimitiveType::UInt64),
            TokenKind::Float32 => FieldType::Primitive(PrimitiveType::Float32),
            TokenKind::Float64 => FieldType::Primitive(PrimitiveType::Float64),

            // String types
            TokenKind::String => {
                // Check for bounded string (string<=N)
                if matches!(self.current().map(|t| &t.kind), Some(TokenKind::LessEqual)) {
                    self.advance(); // consume <=
                    let size_token = self.advance().ok_or(ParseError::UnexpectedEOF)?;
                    let text = size_token.text.clone();
                    let kind = size_token.kind.clone();
                    let size = self.parse_integer(&text, &kind, false)?;
                    FieldType::BoundedString(size as usize)
                } else {
                    FieldType::String
                }
            }

            TokenKind::WString => {
                if matches!(self.current().map(|t| &t.kind), Some(TokenKind::LessEqual)) {
                    self.advance();
                    let size_token = self.advance().ok_or(ParseError::UnexpectedEOF)?;
                    let text = size_token.text.clone();
                    let kind = size_token.kind.clone();
                    let size = self.parse_integer(&text, &kind, false)?;
                    FieldType::BoundedWString(size as usize)
                } else {
                    FieldType::WString
                }
            }

            // Namespaced types (package/Type or Type)
            TokenKind::Identifier => {
                let name = token.text.clone();
                // Check for namespace separator
                if matches!(self.current().map(|t| &t.kind), Some(TokenKind::Slash)) {
                    self.advance(); // consume /
                    let type_name = self.expect(TokenKind::Identifier)?;
                    FieldType::NamespacedType {
                        package: Some(name),
                        name: type_name,
                    }
                } else {
                    FieldType::NamespacedType {
                        package: None,
                        name,
                    }
                }
            }

            _ => return Err(ParseError::UnknownType(token.text.clone())),
        };

        // Check for array/sequence specifiers
        if matches!(self.current().map(|t| &t.kind), Some(TokenKind::LBracket)) {
            self.advance(); // consume [

            match self.current().map(|t| &t.kind) {
                Some(TokenKind::RBracket) => {
                    // Unbounded sequence: type[]
                    self.advance();
                    Ok(FieldType::Sequence {
                        element_type: Box::new(base_type),
                    })
                }
                Some(TokenKind::LessEqual) => {
                    // Bounded sequence: type[<=N]
                    self.advance();
                    let size_token = self.advance().ok_or(ParseError::UnexpectedEOF)?;
                    let text = size_token.text.clone();
                    let kind = size_token.kind.clone();
                    let size = self.parse_integer(&text, &kind, false)?;
                    self.expect(TokenKind::RBracket)?;
                    Ok(FieldType::BoundedSequence {
                        element_type: Box::new(base_type),
                        max_size: size as usize,
                    })
                }
                Some(
                    TokenKind::DecimalInteger
                    | TokenKind::HexInteger
                    | TokenKind::BinaryInteger
                    | TokenKind::OctalInteger,
                ) => {
                    // Fixed array: type[N]
                    let size_token = self.advance().ok_or(ParseError::UnexpectedEOF)?;
                    let text = size_token.text.clone();
                    let kind = size_token.kind.clone();
                    let size = self.parse_integer(&text, &kind, false)?;
                    self.expect(TokenKind::RBracket)?;
                    Ok(FieldType::Array {
                        element_type: Box::new(base_type),
                        size: size as usize,
                    })
                }
                _ => Err(ParseError::UnexpectedToken {
                    expected: "array size or ]".to_string(),
                    got: self.current().map(|t| t.text.clone()).unwrap_or_default(),
                }),
            }
        } else {
            Ok(base_type)
        }
    }

    fn parse_constant_value(&mut self, type_: &FieldType) -> ParseResult<ConstantValue> {
        // Check for negative sign
        let is_negative = if matches!(self.current().map(|t| &t.kind), Some(TokenKind::Minus)) {
            self.advance(); // consume -
            true
        } else {
            false
        };

        let token = self.advance().ok_or(ParseError::UnexpectedEOF)?;
        let text = token.text.clone();
        let kind = token.kind.clone();

        match &kind {
            TokenKind::DecimalInteger
            | TokenKind::HexInteger
            | TokenKind::BinaryInteger
            | TokenKind::OctalInteger => {
                // Try parsing as i64 first
                match self.parse_integer(&text, &kind, is_negative) {
                    Ok(value) => Ok(ConstantValue::Integer(value)),
                    Err(_) if !is_negative => {
                        // If parsing as i64 failed and value is not negative,
                        // try parsing as u64 for values > i64::MAX
                        match text.parse::<u64>() {
                            Ok(uvalue) => Ok(ConstantValue::UInteger(uvalue)),
                            Err(_) => Err(ParseError::InvalidInteger(text)),
                        }
                    }
                    Err(e) => Err(e),
                }
            }
            TokenKind::Float => {
                let mut value = text
                    .parse::<f64>()
                    .map_err(|_| ParseError::InvalidFloat(text.clone()))?;
                if is_negative {
                    value = -value;
                }
                Ok(ConstantValue::Float(value))
            }
            TokenKind::True => {
                if is_negative {
                    return Err(ParseError::UnexpectedToken {
                        expected: "numeric value".to_string(),
                        got: "true".to_string(),
                    });
                }
                Ok(ConstantValue::Bool(true))
            }
            TokenKind::False => {
                if is_negative {
                    return Err(ParseError::UnexpectedToken {
                        expected: "numeric value".to_string(),
                        got: "false".to_string(),
                    });
                }
                Ok(ConstantValue::Bool(false))
            }
            TokenKind::StringLiteral => {
                if is_negative {
                    return Err(ParseError::UnexpectedToken {
                        expected: "numeric value".to_string(),
                        got: text,
                    });
                }
                // Remove quotes
                let s = text.trim_matches(|c| c == '"' || c == '\'');
                Ok(ConstantValue::String(s.to_string()))
            }
            // Handle unquoted strings for string/wstring types
            // This allows constants like: string PARAM=/path/to/file
            _ if matches!(
                type_,
                FieldType::String
                    | FieldType::WString
                    | FieldType::BoundedString(_)
                    | FieldType::BoundedWString(_)
            ) =>
            {
                if is_negative {
                    return Err(ParseError::UnexpectedToken {
                        expected: "string value".to_string(),
                        got: format!("-{}", text),
                    });
                }
                // For unquoted strings, consume tokens until we hit end of line or a meaningful separator
                // Start with the current token
                let mut parts = vec![text];

                // Continue consuming tokens that could be part of an unquoted string
                while let Some(next_token) = self.current() {
                    match &next_token.kind {
                        // These tokens can be part of an unquoted string constant
                        TokenKind::Identifier
                        | TokenKind::Slash
                        | TokenKind::Minus
                        | TokenKind::DecimalInteger
                        | TokenKind::HexInteger
                        | TokenKind::BinaryInteger
                        | TokenKind::OctalInteger => {
                            parts.push(self.advance().unwrap().text.clone());
                        }
                        // Stop at these tokens (they mark end of constant)
                        _ => break,
                    }
                }

                Ok(ConstantValue::String(parts.join("")))
            }
            _ => Err(ParseError::UnexpectedToken {
                expected: "constant value".to_string(),
                got: text,
            }),
        }
    }

    fn parse_field_or_constant(&mut self) -> ParseResult<(Option<Field>, Option<Constant>)> {
        let field_type = self.parse_field_type()?;
        let name = self.expect(TokenKind::Identifier)?;

        // Check if this is a constant (has = sign followed by value)
        // Constants have explicit = sign and are typically UPPER_CASE
        if matches!(self.current().map(|t| &t.kind), Some(TokenKind::Equals)) {
            self.advance(); // consume =
            let value = self.parse_constant_value(&field_type)?;
            Ok((
                None,
                Some(Constant {
                    constant_type: field_type,
                    name,
                    value,
                }),
            ))
        } else {
            // It's a field, check for default value (with or without =)
            let default_value = self.try_parse_default_value(&field_type)?;

            Ok((
                Some(Field {
                    field_type,
                    name,
                    default_value,
                }),
                None,
            ))
        }
    }

    fn parse_array_literal(&mut self, _field_type: &FieldType) -> ParseResult<ConstantValue> {
        self.expect(TokenKind::LBracket)?; // consume [

        let mut values = Vec::new();

        // Handle empty array []
        if matches!(self.current().map(|t| &t.kind), Some(TokenKind::RBracket)) {
            self.advance(); // consume ]
            return Ok(ConstantValue::Array(values));
        }

        // Parse first value
        // For array literals, we need to determine element type from context
        // Use a dummy primitive type for parsing individual values
        let dummy_type = FieldType::Primitive(PrimitiveType::Int32);
        values.push(self.parse_constant_value(&dummy_type)?);

        // Parse remaining values separated by commas
        while matches!(self.current().map(|t| &t.kind), Some(TokenKind::Comma)) {
            self.advance(); // consume ,
            values.push(self.parse_constant_value(&dummy_type)?);
        }

        self.expect(TokenKind::RBracket)?; // consume ]

        Ok(ConstantValue::Array(values))
    }

    fn try_parse_default_value(
        &mut self,
        field_type: &FieldType,
    ) -> ParseResult<Option<ConstantValue>> {
        // Check if next token is a literal value (default value without =)
        // or if there's an = sign (default value with =)
        match self.current().map(|t| &t.kind) {
            Some(TokenKind::Equals) => {
                self.advance(); // consume =
                Ok(Some(self.parse_constant_value(field_type)?))
            }
            // Check for array literal [...]
            Some(TokenKind::LBracket) => Ok(Some(self.parse_array_literal(field_type)?)),
            // Check for literal values (default value without =)
            Some(
                TokenKind::DecimalInteger
                | TokenKind::HexInteger
                | TokenKind::BinaryInteger
                | TokenKind::OctalInteger
                | TokenKind::Float
                | TokenKind::True
                | TokenKind::False
                | TokenKind::StringLiteral
                | TokenKind::Minus, // For negative numbers
            ) => {
                // Parse as default value
                Ok(Some(self.parse_constant_value(field_type)?))
            }
            _ => Ok(None), // No default value
        }
    }

    fn parse_message_impl(&mut self) -> ParseResult<Message> {
        let mut message = Message::new();

        while self.current().is_some() {
            // Stop at triple dash (service/action separator)
            if matches!(self.current().map(|t| &t.kind), Some(TokenKind::TripleDash)) {
                break;
            }

            let (field, constant) = self.parse_field_or_constant()?;

            if let Some(field) = field {
                message.fields.push(field);
            }
            if let Some(constant) = constant {
                message.constants.push(constant);
            }
        }

        Ok(message)
    }
}

pub fn parse_message(input: &str) -> ParseResult<Message> {
    let tokens = crate::lexer::lex(input).map_err(ParseError::LexerError)?;
    let mut parser = Parser::new(tokens);
    parser.parse_message_impl()
}

pub fn parse_service(input: &str) -> ParseResult<Service> {
    let tokens = crate::lexer::lex(input).map_err(ParseError::LexerError)?;
    let mut parser = Parser::new(tokens);

    let request = parser.parse_message_impl()?;

    // Expect separator
    parser.expect(TokenKind::TripleDash)?;

    let response = parser.parse_message_impl()?;

    Ok(Service { request, response })
}

pub fn parse_action(input: &str) -> ParseResult<Action> {
    let tokens = crate::lexer::lex(input).map_err(ParseError::LexerError)?;
    let mut parser = Parser::new(tokens);

    let goal = parser.parse_message_impl()?;
    parser.expect(TokenKind::TripleDash)?;

    let result = parser.parse_message_impl()?;
    parser.expect(TokenKind::TripleDash)?;

    let feedback = parser.parse_message_impl()?;

    Ok(Action {
        spec: ActionSpec {
            goal,
            result,
            feedback,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_primitive_field() {
        let msg = parse_message("int32 x\nuint8 y\nfloat64 z\n").unwrap();
        assert_eq!(msg.fields.len(), 3);
        assert_eq!(msg.fields[0].name, "x");
    }

    #[test]
    fn parse_string_field() {
        let msg = parse_message("string name\n").unwrap();
        assert_eq!(msg.fields.len(), 1);
        assert!(matches!(msg.fields[0].field_type, FieldType::String));
    }

    #[test]
    fn parse_bounded_string() {
        let msg = parse_message("string<=256 name\n").unwrap();
        assert_eq!(msg.fields.len(), 1);
        assert!(matches!(
            msg.fields[0].field_type,
            FieldType::BoundedString(256)
        ));
    }

    #[test]
    fn parse_fixed_array() {
        let msg = parse_message("int32[5] data\n").unwrap();
        assert_eq!(msg.fields.len(), 1);
        assert!(matches!(msg.fields[0].field_type, FieldType::Array { .. }));
    }

    #[test]
    fn parse_unbounded_sequence() {
        let msg = parse_message("int32[] data\n").unwrap();
        assert_eq!(msg.fields.len(), 1);
        assert!(matches!(
            msg.fields[0].field_type,
            FieldType::Sequence { .. }
        ));
    }

    #[test]
    fn parse_bounded_sequence() {
        let msg = parse_message("int32[<=100] data\n").unwrap();
        assert_eq!(msg.fields.len(), 1);
        assert!(matches!(
            msg.fields[0].field_type,
            FieldType::BoundedSequence { .. }
        ));
    }

    #[test]
    fn parse_constant() {
        let msg = parse_message("int32 MAX_SIZE=100\n").unwrap();
        assert_eq!(msg.constants.len(), 1);
        assert_eq!(msg.constants[0].name, "MAX_SIZE");
        assert!(matches!(
            msg.constants[0].value,
            ConstantValue::Integer(100)
        ));
    }

    #[test]
    fn parse_hex_constant() {
        let msg = parse_message("int32 HEX=0xFF\n").unwrap();
        assert_eq!(msg.constants.len(), 1);
        assert!(matches!(
            msg.constants[0].value,
            ConstantValue::Integer(255)
        ));
    }

    #[test]
    fn parse_namespaced_type() {
        let msg = parse_message("geometry_msgs/Point position\n").unwrap();
        assert_eq!(msg.fields.len(), 1);
        if let FieldType::NamespacedType { package, name } = &msg.fields[0].field_type {
            assert_eq!(package.as_ref().unwrap(), "geometry_msgs");
            assert_eq!(name, "Point");
        } else {
            panic!("Expected NamespacedType");
        }
    }

    #[test]
    fn parse_simple_service() {
        let srv = parse_service("int64 a\nint64 b\n---\nint64 sum\n").unwrap();
        assert_eq!(srv.request.fields.len(), 2);
        assert_eq!(srv.response.fields.len(), 1);
    }

    #[test]
    fn parse_simple_action() {
        let act =
            parse_action("int32 order\n---\nint32[] sequence\n---\nint32[] sequence\n").unwrap();
        assert_eq!(act.spec.goal.fields.len(), 1);
        assert_eq!(act.spec.result.fields.len(), 1);
        assert_eq!(act.spec.feedback.fields.len(), 1);
    }

    #[test]
    fn parse_negative_constant() {
        let msg = parse_message("int8 STATUS_UNKNOWN=-2\nint8 STATUS_NO_FIX=-1\n").unwrap();
        assert_eq!(msg.constants.len(), 2);
        assert_eq!(msg.constants[0].name, "STATUS_UNKNOWN");
        assert!(matches!(msg.constants[0].value, ConstantValue::Integer(-2)));
        assert_eq!(msg.constants[1].name, "STATUS_NO_FIX");
        assert!(matches!(msg.constants[1].value, ConstantValue::Integer(-1)));
    }

    #[test]
    fn parse_negative_float_constant() {
        let msg = parse_message("float64 NEGATIVE_PI=-3.14159\n").unwrap();
        assert_eq!(msg.constants.len(), 1);
        assert!(
            matches!(msg.constants[0].value, ConstantValue::Float(v) if (v + std::f64::consts::PI).abs() < 0.001)
        );
    }

    #[test]
    fn parse_default_value_without_equals() {
        let msg = parse_message("float64 x 0\nfloat64 y 0\nfloat64 z 0\nfloat64 w 1\n").unwrap();
        assert_eq!(msg.fields.len(), 4);

        // All should be fields, not constants
        assert_eq!(msg.constants.len(), 0);

        // Check default values
        assert!(matches!(
            msg.fields[0].default_value,
            Some(ConstantValue::Integer(0))
        ));
        assert!(matches!(
            msg.fields[1].default_value,
            Some(ConstantValue::Integer(0))
        ));
        assert!(matches!(
            msg.fields[2].default_value,
            Some(ConstantValue::Integer(0))
        ));
        assert!(matches!(
            msg.fields[3].default_value,
            Some(ConstantValue::Integer(1))
        ));
    }

    #[test]
    fn parse_field_with_negative_default() {
        let msg = parse_message("int8 status -2\n").unwrap();
        assert_eq!(msg.fields.len(), 1);
        assert_eq!(msg.constants.len(), 0);
        assert_eq!(msg.fields[0].name, "status");
        assert!(matches!(
            msg.fields[0].default_value,
            Some(ConstantValue::Integer(-2))
        ));
    }

    #[test]
    fn parse_mixed_constants_and_defaults() {
        let msg =
            parse_message("int8 STATUS_UNKNOWN=-2\nint8 STATUS_FIX=0\nint8 status -2\n").unwrap();
        assert_eq!(msg.constants.len(), 2);
        assert_eq!(msg.fields.len(), 1);

        // Constants
        assert_eq!(msg.constants[0].name, "STATUS_UNKNOWN");
        assert!(matches!(msg.constants[0].value, ConstantValue::Integer(-2)));
        assert_eq!(msg.constants[1].name, "STATUS_FIX");
        assert!(matches!(msg.constants[1].value, ConstantValue::Integer(0)));

        // Field with default
        assert_eq!(msg.fields[0].name, "status");
        assert!(matches!(
            msg.fields[0].default_value,
            Some(ConstantValue::Integer(-2))
        ));
    }

    #[test]
    fn parse_quaternion_message() {
        // Test the actual Quaternion.msg structure
        let input = "# This represents an orientation in free space in quaternion form.\n\nfloat64 x 0\nfloat64 y 0\nfloat64 z 0\nfloat64 w 1\n";
        let msg = parse_message(input).unwrap();

        assert_eq!(msg.fields.len(), 4);
        assert_eq!(msg.constants.len(), 0);

        assert_eq!(msg.fields[0].name, "x");
        assert_eq!(msg.fields[1].name, "y");
        assert_eq!(msg.fields[2].name, "z");
        assert_eq!(msg.fields[3].name, "w");

        // Check default values
        assert!(matches!(
            msg.fields[0].default_value,
            Some(ConstantValue::Integer(0))
        ));
        assert!(matches!(
            msg.fields[1].default_value,
            Some(ConstantValue::Integer(0))
        ));
        assert!(matches!(
            msg.fields[2].default_value,
            Some(ConstantValue::Integer(0))
        ));
        assert!(matches!(
            msg.fields[3].default_value,
            Some(ConstantValue::Integer(1))
        ));
    }

    #[test]
    fn parse_unquoted_string_constant() {
        // Test unquoted string constants (from bond/Constants.msg)
        let msg = parse_message(
            "string DISABLE_HEARTBEAT_TIMEOUT_PARAM=/bond_disable_heartbeat_timeout\n",
        )
        .unwrap();
        assert_eq!(msg.constants.len(), 1);
        assert_eq!(msg.constants[0].name, "DISABLE_HEARTBEAT_TIMEOUT_PARAM");
        assert!(matches!(
            &msg.constants[0].value,
            ConstantValue::String(s) if s == "/bond_disable_heartbeat_timeout"
        ));
    }

    #[test]
    fn parse_bond_constants_message() {
        // Test the actual bond/Constants.msg file
        let input = r#"float32 DEAD_PUBLISH_PERIOD = 0.05
float32 DEFAULT_CONNECT_TIMEOUT = 10.0
float32 DEFAULT_HEARTBEAT_TIMEOUT = 4.0
float32 DEFAULT_DISCONNECT_TIMEOUT = 2.0
float32 DEFAULT_HEARTBEAT_PERIOD = 1.0

string DISABLE_HEARTBEAT_TIMEOUT_PARAM=/bond_disable_heartbeat_timeout
"#;
        let msg = parse_message(input).unwrap();
        assert_eq!(msg.constants.len(), 6);

        // Check the problematic string constant
        let string_const = msg
            .constants
            .iter()
            .find(|c| c.name == "DISABLE_HEARTBEAT_TIMEOUT_PARAM")
            .unwrap();
        assert!(matches!(
            &string_const.value,
            ConstantValue::String(s) if s == "/bond_disable_heartbeat_timeout"
        ));
    }
}
