use logos::Logos;

#[derive(Logos, Debug, Clone, PartialEq)]
#[logos(skip r"[ \t\r\n\f]+")] // Skip whitespace
#[logos(skip r"#[^\n]*")] // Skip line comments starting with #
pub enum TokenKind {
    // Primitive types
    #[token("bool")]
    #[token("boolean")]
    Bool,

    #[token("byte")]
    #[token("octet")]
    Byte,

    #[token("char")]
    Char,

    #[token("int8")]
    Int8,

    #[token("uint8")]
    UInt8,

    #[token("int16")]
    #[token("short")]
    Int16,

    #[token("uint16")]
    UInt16,

    #[token("int32")]
    #[token("long")]
    Int32,

    #[token("uint32")]
    UInt32,

    #[token("int64")]
    Int64,

    #[token("uint64")]
    UInt64,

    #[token("float32")]
    #[token("float")]
    Float32,

    #[token("float64")]
    #[token("double")]
    Float64,

    #[token("string")]
    String,

    #[token("wstring")]
    WString,

    // Symbols
    #[token("[")]
    LBracket,

    #[token("]")]
    RBracket,

    #[token("<")]
    LAngle,

    #[token(">")]
    RAngle,

    #[token("=")]
    Equals,

    #[token("/")]
    Slash,

    #[token("---")]
    TripleDash,

    #[token("<=")]
    LessEqual,

    #[token("-")]
    Minus,

    #[token(",")]
    Comma,

    // Identifiers (lowercase_with_underscores or UpperCamelCase)
    #[regex(r"[a-zA-Z][a-zA-Z0-9_]*")]
    Identifier,

    // Integer literals - hexadecimal (0x or 0X prefix)
    #[regex(r"0[xX][0-9a-fA-F]+")]
    HexInteger,

    // Integer literals - binary (0b or 0B prefix)
    #[regex(r"0[bB][01]+")]
    BinaryInteger,

    // Integer literals - octal (0o or 0O prefix)
    #[regex(r"0[oO][0-7]+")]
    OctalInteger,

    // Integer literals - decimal
    #[regex(r"[0-9]+")]
    DecimalInteger,

    // Float literals
    #[regex(r"[0-9]+\.[0-9]+([eE][+-]?[0-9]+)?")]
    #[regex(r"[0-9]+[eE][+-]?[0-9]+")]
    Float,

    // String literals
    #[regex(r#""([^"\\]|\\.)*""#)]
    #[regex(r#"'([^'\\]|\\.)*'"#)]
    StringLiteral,

    // Boolean literals
    #[token("true")]
    #[token("TRUE")]
    #[token("True")]
    True,

    #[token("false")]
    #[token("FALSE")]
    #[token("False")]
    False,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub text: String,
    pub span: std::ops::Range<usize>,
}

pub fn lex(input: &str) -> Result<Vec<Token>, String> {
    let mut lexer = TokenKind::lexer(input);
    let mut tokens = Vec::new();

    while let Some(kind) = lexer.next() {
        match kind {
            Ok(kind) => {
                tokens.push(Token {
                    kind,
                    text: lexer.slice().to_string(),
                    span: lexer.span(),
                });
            }
            Err(_) => {
                return Err(format!(
                    "Unexpected character at position {}: '{}'",
                    lexer.span().start,
                    lexer.slice()
                ));
            }
        }
    }

    Ok(tokens)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lex_primitive_types() {
        let input = "int32 uint8 float64 bool string";
        let tokens = lex(input).unwrap();
        assert_eq!(tokens.len(), 5);
        assert_eq!(tokens[0].kind, TokenKind::Int32);
        assert_eq!(tokens[1].kind, TokenKind::UInt8);
        assert_eq!(tokens[2].kind, TokenKind::Float64);
        assert_eq!(tokens[3].kind, TokenKind::Bool);
        assert_eq!(tokens[4].kind, TokenKind::String);
    }

    #[test]
    fn lex_field_declaration() {
        let input = "int32 x";
        let tokens = lex(input).unwrap();
        assert_eq!(tokens.len(), 2);
        assert_eq!(tokens[0].kind, TokenKind::Int32);
        assert_eq!(tokens[1].kind, TokenKind::Identifier);
        assert_eq!(tokens[1].text, "x");
    }

    #[test]
    fn lex_array_syntax() {
        let input = "int32[5] fixed\nint32[] unbounded\nint32[<=10] bounded";
        let tokens = lex(input).unwrap();
        assert!(tokens.iter().any(|t| t.kind == TokenKind::LBracket));
        assert!(tokens.iter().any(|t| t.kind == TokenKind::RBracket));
        assert!(tokens.iter().any(|t| t.kind == TokenKind::LessEqual));
    }

    #[test]
    fn lex_constant() {
        let input = "int32 MAX_SIZE=100";
        let tokens = lex(input).unwrap();
        assert!(tokens.iter().any(|t| t.kind == TokenKind::Equals));
        assert!(tokens.iter().any(|t| t.kind == TokenKind::DecimalInteger));
    }

    #[test]
    fn lex_hex_integer() {
        let input = "int32 HEX=0xFF";
        let tokens = lex(input).unwrap();
        assert!(tokens.iter().any(|t| t.kind == TokenKind::HexInteger));
        assert_eq!(
            tokens
                .iter()
                .find(|t| t.kind == TokenKind::HexInteger)
                .unwrap()
                .text,
            "0xFF"
        );
    }

    #[test]
    fn lex_binary_integer() {
        let input = "int32 BIN=0b1010";
        let tokens = lex(input).unwrap();
        assert!(tokens.iter().any(|t| t.kind == TokenKind::BinaryInteger));
    }

    #[test]
    fn lex_comments() {
        let input = "# This is a comment\nint32 x # inline comment\n";
        let tokens = lex(input).unwrap();
        // Comments should be skipped
        assert_eq!(tokens.len(), 2); // Only int32 and x
        assert_eq!(tokens[0].kind, TokenKind::Int32);
    }

    #[test]
    fn lex_service_separator() {
        let input = "int64 a\n---\nint64 sum";
        let tokens = lex(input).unwrap();
        assert!(tokens.iter().any(|t| t.kind == TokenKind::TripleDash));
    }

    #[test]
    fn lex_namespaced_type() {
        let input = "geometry_msgs/Point position";
        let tokens = lex(input).unwrap();
        assert!(tokens.iter().any(|t| t.kind == TokenKind::Slash));
        assert_eq!(tokens.len(), 4); // geometry_msgs / Point position
    }

    #[test]
    fn lex_bounded_string() {
        let input = "string<=256 name";
        let tokens = lex(input).unwrap();
        assert!(tokens.iter().any(|t| t.kind == TokenKind::String));
        assert!(tokens.iter().any(|t| t.kind == TokenKind::LessEqual));
        assert!(tokens.iter().any(|t| t.kind == TokenKind::DecimalInteger));
    }

    #[test]
    fn lex_negative_constant() {
        let input = "int8 NEGATIVE=-42";
        let tokens = lex(input).unwrap();
        assert!(tokens.iter().any(|t| t.kind == TokenKind::Minus));
        assert!(tokens.iter().any(|t| t.kind == TokenKind::DecimalInteger));
        assert!(tokens.iter().any(|t| t.kind == TokenKind::Equals));
    }

    #[test]
    fn lex_default_value_without_equals() {
        let input = "float64 x 0";
        let tokens = lex(input).unwrap();
        assert_eq!(tokens.len(), 3); // float64, x, 0
        assert_eq!(tokens[0].kind, TokenKind::Float64);
        assert_eq!(tokens[1].kind, TokenKind::Identifier);
        assert_eq!(tokens[2].kind, TokenKind::DecimalInteger);
        // Should NOT have equals
        assert!(!tokens.iter().any(|t| t.kind == TokenKind::Equals));
    }

    #[test]
    fn lex_triple_dash_vs_minus() {
        let input = "int8 a\n---\nint8 b -1";
        let tokens = lex(input).unwrap();
        // Should have TripleDash for separator
        assert!(tokens.iter().any(|t| t.kind == TokenKind::TripleDash));
        // Should also have Minus for -1
        assert!(tokens.iter().any(|t| t.kind == TokenKind::Minus));
        // TripleDash should come before Minus
        let triple_dash_pos = tokens
            .iter()
            .position(|t| t.kind == TokenKind::TripleDash)
            .unwrap();
        let minus_pos = tokens
            .iter()
            .position(|t| t.kind == TokenKind::Minus)
            .unwrap();
        assert!(triple_dash_pos < minus_pos);
    }

    #[test]
    fn lex_capitalized_boolean_literals() {
        // Test True/False (capitalized) - common in ROS 2 action files
        let input = "bool use_dock_id True\nbool navigate False";
        let tokens = lex(input).unwrap();
        assert_eq!(tokens.len(), 6); // bool, use_dock_id, True, bool, navigate, False
        assert_eq!(tokens[0].kind, TokenKind::Bool);
        assert_eq!(tokens[1].kind, TokenKind::Identifier);
        assert_eq!(tokens[2].kind, TokenKind::True);
        assert_eq!(tokens[3].kind, TokenKind::Bool);
        assert_eq!(tokens[4].kind, TokenKind::Identifier);
        assert_eq!(tokens[5].kind, TokenKind::False);
    }
}
