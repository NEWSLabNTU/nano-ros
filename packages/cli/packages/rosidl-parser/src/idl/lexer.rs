//! IDL lexer (tokenizer) implementation.

use std::fmt;

/// Token produced by the IDL lexer.
#[derive(Debug, Clone, PartialEq)]
pub struct IdlToken {
    pub kind: IdlTokenKind,
    pub span: Span,
}

/// Span information for error reporting.
#[derive(Debug, Clone, PartialEq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
    pub line: usize,
    pub column: usize,
}

/// IDL token kinds.
#[derive(Debug, Clone, PartialEq)]
pub enum IdlTokenKind {
    // Keywords
    Module,
    Struct,
    Const,
    Enum,
    Sequence,
    String,
    WString,
    Boolean,
    True,
    False,

    // Primitive types
    Short,
    UnsignedShort,
    Long,
    UnsignedLong,
    LongLong,
    UnsignedLongLong,
    Float,
    Double,
    LongDouble,
    Char,
    WChar,
    Octet,
    Unsigned, // For parsing "unsigned short", etc.

    // ROS 2 fixed-size types
    Int8,
    Int16,
    Int32,
    Int64,
    Uint8,
    Uint16,
    Uint32,
    Uint64,

    // Literals
    IntegerLiteral(i64),
    FloatLiteral(f64),
    StringLiteral(String),
    WStringLiteral(String),
    BooleanLiteral(bool),

    // Identifiers
    Identifier(String),

    // Annotations
    Annotation(String), // @key, @default, etc.

    // Delimiters
    LBrace,    // {
    RBrace,    // }
    LParen,    // (
    RParen,    // )
    LBracket,  // [
    RBracket,  // ]
    LAngle,    // <
    RAngle,    // >
    Semicolon, // ;
    Comma,     // ,
    Colon,     // :
    Equal,     // =

    // Operators
    ScopeResolution, // ::

    // Comments (usually skipped)
    Comment(String),

    // Special
    Eof,
    Whitespace,
}

impl fmt::Display for IdlTokenKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            IdlTokenKind::Module => write!(f, "module"),
            IdlTokenKind::Struct => write!(f, "struct"),
            IdlTokenKind::Const => write!(f, "const"),
            IdlTokenKind::Enum => write!(f, "enum"),
            IdlTokenKind::Sequence => write!(f, "sequence"),
            IdlTokenKind::String => write!(f, "string"),
            IdlTokenKind::WString => write!(f, "wstring"),
            IdlTokenKind::Boolean => write!(f, "boolean"),
            IdlTokenKind::True => write!(f, "TRUE"),
            IdlTokenKind::False => write!(f, "FALSE"),
            IdlTokenKind::Short => write!(f, "short"),
            IdlTokenKind::UnsignedShort => write!(f, "unsigned short"),
            IdlTokenKind::Long => write!(f, "long"),
            IdlTokenKind::UnsignedLong => write!(f, "unsigned long"),
            IdlTokenKind::LongLong => write!(f, "long long"),
            IdlTokenKind::UnsignedLongLong => write!(f, "unsigned long long"),
            IdlTokenKind::Float => write!(f, "float"),
            IdlTokenKind::Double => write!(f, "double"),
            IdlTokenKind::LongDouble => write!(f, "long double"),
            IdlTokenKind::Char => write!(f, "char"),
            IdlTokenKind::WChar => write!(f, "wchar"),
            IdlTokenKind::Octet => write!(f, "octet"),
            IdlTokenKind::Unsigned => write!(f, "unsigned"),
            IdlTokenKind::Int8 => write!(f, "int8"),
            IdlTokenKind::Int16 => write!(f, "int16"),
            IdlTokenKind::Int32 => write!(f, "int32"),
            IdlTokenKind::Int64 => write!(f, "int64"),
            IdlTokenKind::Uint8 => write!(f, "uint8"),
            IdlTokenKind::Uint16 => write!(f, "uint16"),
            IdlTokenKind::Uint32 => write!(f, "uint32"),
            IdlTokenKind::Uint64 => write!(f, "uint64"),
            IdlTokenKind::IntegerLiteral(i) => write!(f, "{}", i),
            IdlTokenKind::FloatLiteral(fl) => write!(f, "{}", fl),
            IdlTokenKind::StringLiteral(s) => write!(f, "\"{}\"", s),
            IdlTokenKind::WStringLiteral(s) => write!(f, "\"{}\"", s),
            IdlTokenKind::BooleanLiteral(b) => write!(f, "{}", b),
            IdlTokenKind::Identifier(id) => write!(f, "{}", id),
            IdlTokenKind::Annotation(name) => write!(f, "@{}", name),
            IdlTokenKind::LBrace => write!(f, "{{"),
            IdlTokenKind::RBrace => write!(f, "}}"),
            IdlTokenKind::LParen => write!(f, "("),
            IdlTokenKind::RParen => write!(f, ")"),
            IdlTokenKind::LBracket => write!(f, "["),
            IdlTokenKind::RBracket => write!(f, "]"),
            IdlTokenKind::LAngle => write!(f, "<"),
            IdlTokenKind::RAngle => write!(f, ">"),
            IdlTokenKind::Semicolon => write!(f, ";"),
            IdlTokenKind::Comma => write!(f, ","),
            IdlTokenKind::Colon => write!(f, ":"),
            IdlTokenKind::Equal => write!(f, "="),
            IdlTokenKind::ScopeResolution => write!(f, "::"),
            IdlTokenKind::Comment(c) => write!(f, "// {}", c),
            IdlTokenKind::Eof => write!(f, "EOF"),
            IdlTokenKind::Whitespace => write!(f, " "),
        }
    }
}

/// IDL lexer.
pub struct IdlLexer {
    input: Vec<char>,
    position: usize,
    line: usize,
    column: usize,
}

impl IdlLexer {
    /// Create a new lexer for the given input.
    pub fn new(input: &str) -> Self {
        Self {
            input: input.chars().collect(),
            position: 0,
            line: 1,
            column: 1,
        }
    }

    /// Tokenize the entire input.
    pub fn tokenize(&mut self) -> Result<Vec<IdlToken>, String> {
        let mut tokens = Vec::new();

        loop {
            let token = self.next_token()?;
            let is_eof = token.kind == IdlTokenKind::Eof;

            // Skip whitespace, comments, and EOF
            match &token.kind {
                IdlTokenKind::Whitespace | IdlTokenKind::Comment(_) | IdlTokenKind::Eof => {}
                _ => tokens.push(token),
            }

            if is_eof {
                break;
            }
        }

        Ok(tokens)
    }

    /// Get the next token.
    fn next_token(&mut self) -> Result<IdlToken, String> {
        self.skip_whitespace();

        let start_pos = self.position;
        let start_line = self.line;
        let start_col = self.column;

        if self.is_at_end() {
            return Ok(IdlToken {
                kind: IdlTokenKind::Eof,
                span: Span {
                    start: start_pos,
                    end: start_pos,
                    line: start_line,
                    column: start_col,
                },
            });
        }

        let ch = self.current_char();

        // Single-character tokens
        let kind = match ch {
            '{' => {
                self.advance();
                IdlTokenKind::LBrace
            }
            '}' => {
                self.advance();
                IdlTokenKind::RBrace
            }
            '(' => {
                self.advance();
                IdlTokenKind::LParen
            }
            ')' => {
                self.advance();
                IdlTokenKind::RParen
            }
            '[' => {
                self.advance();
                IdlTokenKind::LBracket
            }
            ']' => {
                self.advance();
                IdlTokenKind::RBracket
            }
            '<' => {
                self.advance();
                IdlTokenKind::LAngle
            }
            '>' => {
                self.advance();
                IdlTokenKind::RAngle
            }
            ';' => {
                self.advance();
                IdlTokenKind::Semicolon
            }
            ',' => {
                self.advance();
                IdlTokenKind::Comma
            }
            '=' => {
                self.advance();
                IdlTokenKind::Equal
            }
            ':' => {
                self.advance();
                if !self.is_at_end() && self.current_char() == ':' {
                    self.advance();
                    IdlTokenKind::ScopeResolution
                } else {
                    IdlTokenKind::Colon
                }
            }
            '@' => {
                self.advance();
                let name = self.read_identifier();
                IdlTokenKind::Annotation(name)
            }
            '/' => {
                if self.peek() == Some('/') {
                    self.read_line_comment()
                } else if self.peek() == Some('*') {
                    self.read_block_comment()?
                } else {
                    return Err(format!("Unexpected character: {}", ch));
                }
            }
            '"' => self.read_string_literal()?,
            _ if ch.is_ascii_digit()
                || (ch == '-' && self.peek().is_some_and(|c| c.is_ascii_digit()))
                || (ch == '.' && self.peek().is_some_and(|c| c.is_ascii_digit())) =>
            {
                self.read_number_literal()?
            }
            _ if ch.is_alphabetic() || ch == '_' => {
                let ident = self.read_identifier();
                self.identifier_to_token(ident)
            }
            _ => {
                return Err(format!("Unexpected character: {}", ch));
            }
        };

        Ok(IdlToken {
            kind,
            span: Span {
                start: start_pos,
                end: self.position,
                line: start_line,
                column: start_col,
            },
        })
    }

    /// Skip whitespace characters.
    fn skip_whitespace(&mut self) {
        while !self.is_at_end() && self.current_char().is_whitespace() {
            if self.current_char() == '\n' {
                self.line += 1;
                self.column = 1;
            } else {
                self.column += 1;
            }
            self.position += 1;
        }
    }

    /// Read an identifier.
    fn read_identifier(&mut self) -> String {
        let mut ident = String::new();
        while !self.is_at_end() {
            let ch = self.current_char();
            if ch.is_alphanumeric() || ch == '_' {
                ident.push(ch);
                self.advance();
            } else {
                break;
            }
        }
        ident
    }

    /// Read a string literal.
    fn read_string_literal(&mut self) -> Result<IdlTokenKind, String> {
        self.advance(); // Skip opening quote
        let mut string = String::new();

        while !self.is_at_end() && self.current_char() != '"' {
            let ch = self.current_char();
            if ch == '\\' {
                self.advance();
                if !self.is_at_end() {
                    let escaped = match self.current_char() {
                        'n' => '\n',
                        't' => '\t',
                        'r' => '\r',
                        '\\' => '\\',
                        '"' => '"',
                        c => c,
                    };
                    string.push(escaped);
                    self.advance();
                }
            } else {
                string.push(ch);
                self.advance();
            }
        }

        if self.is_at_end() {
            return Err("Unterminated string literal".to_string());
        }

        self.advance(); // Skip closing quote
        Ok(IdlTokenKind::StringLiteral(string))
    }

    /// Read a number literal (integer or float).
    fn read_number_literal(&mut self) -> Result<IdlTokenKind, String> {
        let mut number = String::new();
        let mut is_float = false;

        // Handle negative sign
        if self.current_char() == '-' {
            number.push('-');
            self.advance();
        }

        // Check if starting with decimal point (e.g., .1, .3d)
        if self.current_char() == '.' {
            is_float = true;
            number.push('0'); // Add leading zero for parsing
            number.push('.');
            self.advance();

            // Read fractional part
            while !self.is_at_end() && self.current_char().is_ascii_digit() {
                number.push(self.current_char());
                self.advance();
            }
        } else {
            // Read integer part
            while !self.is_at_end() && self.current_char().is_ascii_digit() {
                number.push(self.current_char());
                self.advance();
            }

            // Check for decimal point
            if !self.is_at_end() && self.current_char() == '.' {
                is_float = true;
                number.push('.');
                self.advance();

                // Read fractional part
                while !self.is_at_end() && self.current_char().is_ascii_digit() {
                    number.push(self.current_char());
                    self.advance();
                }
            }
        }

        // Check for scientific notation (e, E) or fixed-point (d, D)
        if !self.is_at_end() {
            let ch = self.current_char();
            if ch == 'e' || ch == 'E' || ch == 'd' || ch == 'D' {
                is_float = true;
                let is_fixed_point = ch == 'd' || ch == 'D';

                // For e/E, add it; for d/D, only add 'e' if there's an exponent following
                if !is_fixed_point {
                    number.push(ch);
                }
                self.advance();

                // Peek ahead to see if there's an exponent
                let has_exponent = !self.is_at_end()
                    && (self.current_char() == '+'
                        || self.current_char() == '-'
                        || self.current_char().is_ascii_digit());

                if has_exponent {
                    // For fixed-point with exponent, add 'e' now
                    if is_fixed_point {
                        number.push('e');
                    }

                    // Handle optional +/- in exponent
                    if self.current_char() == '+' || self.current_char() == '-' {
                        number.push(self.current_char());
                        self.advance();
                    }

                    // Read exponent digits
                    while !self.is_at_end() && self.current_char().is_ascii_digit() {
                        number.push(self.current_char());
                        self.advance();
                    }
                }
                // If fixed-point without exponent (e.g., "8.7d"), just consume the 'd' and parse as-is
            }
        }

        if is_float {
            number
                .parse::<f64>()
                .map(IdlTokenKind::FloatLiteral)
                .map_err(|e| format!("Invalid float literal: {}", e))
        } else {
            number
                .parse::<i64>()
                .map(IdlTokenKind::IntegerLiteral)
                .map_err(|e| format!("Invalid integer literal: {}", e))
        }
    }

    /// Read a line comment.
    fn read_line_comment(&mut self) -> IdlTokenKind {
        self.advance(); // Skip first /
        self.advance(); // Skip second /

        let mut comment = String::new();
        while !self.is_at_end() && self.current_char() != '\n' {
            comment.push(self.current_char());
            self.advance();
        }

        IdlTokenKind::Comment(comment.trim().to_string())
    }

    /// Read a block comment.
    fn read_block_comment(&mut self) -> Result<IdlTokenKind, String> {
        self.advance(); // Skip /
        self.advance(); // Skip *

        let mut comment = String::new();
        while !self.is_at_end() {
            if self.current_char() == '*' && self.peek() == Some('/') {
                self.advance(); // Skip *
                self.advance(); // Skip /
                return Ok(IdlTokenKind::Comment(comment.trim().to_string()));
            }
            if self.current_char() == '\n' {
                self.line += 1;
                self.column = 1;
            }
            comment.push(self.current_char());
            self.advance();
        }

        Err("Unterminated block comment".to_string())
    }

    /// Convert an identifier string to the appropriate token kind.
    fn identifier_to_token(&self, ident: String) -> IdlTokenKind {
        match ident.as_str() {
            // Keywords
            "module" => IdlTokenKind::Module,
            "struct" => IdlTokenKind::Struct,
            "const" => IdlTokenKind::Const,
            "enum" => IdlTokenKind::Enum,
            "sequence" => IdlTokenKind::Sequence,
            "string" => IdlTokenKind::String,
            "wstring" => IdlTokenKind::WString,
            "boolean" => IdlTokenKind::Boolean,
            "TRUE" => IdlTokenKind::True,
            "FALSE" => IdlTokenKind::False,

            // Primitive types
            "short" => IdlTokenKind::Short,
            "long" => IdlTokenKind::Long,
            "float" => IdlTokenKind::Float,
            "double" => IdlTokenKind::Double,
            "char" => IdlTokenKind::Char,
            "wchar" => IdlTokenKind::WChar,
            "octet" => IdlTokenKind::Octet,
            "unsigned" => IdlTokenKind::Unsigned,

            // ROS 2 fixed-size types
            "int8" => IdlTokenKind::Int8,
            "int16" => IdlTokenKind::Int16,
            "int32" => IdlTokenKind::Int32,
            "int64" => IdlTokenKind::Int64,
            "uint8" => IdlTokenKind::Uint8,
            "uint16" => IdlTokenKind::Uint16,
            "uint32" => IdlTokenKind::Uint32,
            "uint64" => IdlTokenKind::Uint64,

            // Default to identifier
            _ => IdlTokenKind::Identifier(ident),
        }
    }

    /// Check if we're at the end of input.
    fn is_at_end(&self) -> bool {
        self.position >= self.input.len()
    }

    /// Get the current character.
    fn current_char(&self) -> char {
        self.input[self.position]
    }

    /// Peek at the next character without consuming it.
    fn peek(&self) -> Option<char> {
        if self.position + 1 < self.input.len() {
            Some(self.input[self.position + 1])
        } else {
            None
        }
    }

    /// Advance to the next character.
    fn advance(&mut self) {
        if !self.is_at_end() {
            if self.current_char() == '\n' {
                self.line += 1;
                self.column = 1;
            } else {
                self.column += 1;
            }
            self.position += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_keywords() {
        let mut lexer = IdlLexer::new("module struct const enum sequence");
        let tokens = lexer.tokenize().unwrap();
        assert_eq!(tokens.len(), 5);
        assert!(matches!(tokens[0].kind, IdlTokenKind::Module));
        assert!(matches!(tokens[1].kind, IdlTokenKind::Struct));
        assert!(matches!(tokens[2].kind, IdlTokenKind::Const));
        assert!(matches!(tokens[3].kind, IdlTokenKind::Enum));
        assert!(matches!(tokens[4].kind, IdlTokenKind::Sequence));
    }

    #[test]
    fn test_primitives() {
        let mut lexer = IdlLexer::new("short long float double boolean");
        let tokens = lexer.tokenize().unwrap();
        assert_eq!(tokens.len(), 5);
        assert!(matches!(tokens[0].kind, IdlTokenKind::Short));
        assert!(matches!(tokens[1].kind, IdlTokenKind::Long));
        assert!(matches!(tokens[2].kind, IdlTokenKind::Float));
        assert!(matches!(tokens[3].kind, IdlTokenKind::Double));
        assert!(matches!(tokens[4].kind, IdlTokenKind::Boolean));
    }

    #[test]
    fn test_integer_literal() {
        let mut lexer = IdlLexer::new("42 -23 0");
        let tokens = lexer.tokenize().unwrap();
        assert_eq!(tokens.len(), 3);
        assert_eq!(tokens[0].kind, IdlTokenKind::IntegerLiteral(42));
        assert_eq!(tokens[1].kind, IdlTokenKind::IntegerLiteral(-23));
        assert_eq!(tokens[2].kind, IdlTokenKind::IntegerLiteral(0));
    }

    #[test]
    fn test_float_literal() {
        let mut lexer = IdlLexer::new("3.14 -2.5 1.9e10 8.7d");
        let tokens = lexer.tokenize().unwrap();
        assert_eq!(tokens.len(), 4);
        assert!(matches!(tokens[0].kind, IdlTokenKind::FloatLiteral(_)));
        assert!(matches!(tokens[1].kind, IdlTokenKind::FloatLiteral(_)));
        assert!(matches!(tokens[2].kind, IdlTokenKind::FloatLiteral(_)));
        assert!(matches!(tokens[3].kind, IdlTokenKind::FloatLiteral(_)));
    }

    #[test]
    fn test_string_literal() {
        let mut lexer = IdlLexer::new(r#""hello world" "test\nvalue""#);
        let tokens = lexer.tokenize().unwrap();
        assert_eq!(tokens.len(), 2);
        assert_eq!(
            tokens[0].kind,
            IdlTokenKind::StringLiteral("hello world".to_string())
        );
        assert_eq!(
            tokens[1].kind,
            IdlTokenKind::StringLiteral("test\nvalue".to_string())
        );
    }

    #[test]
    fn test_annotation() {
        let mut lexer = IdlLexer::new("@key @default @verbatim");
        let tokens = lexer.tokenize().unwrap();
        assert_eq!(tokens.len(), 3);
        assert_eq!(tokens[0].kind, IdlTokenKind::Annotation("key".to_string()));
        assert_eq!(
            tokens[1].kind,
            IdlTokenKind::Annotation("default".to_string())
        );
        assert_eq!(
            tokens[2].kind,
            IdlTokenKind::Annotation("verbatim".to_string())
        );
    }

    #[test]
    fn test_delimiters() {
        let mut lexer = IdlLexer::new("{}()<>[];,:");
        let tokens = lexer.tokenize().unwrap();
        assert_eq!(tokens.len(), 11);
    }

    #[test]
    fn test_line_comment() {
        let mut lexer = IdlLexer::new("// This is a comment\nshort");
        let tokens = lexer.tokenize().unwrap();
        assert_eq!(tokens.len(), 1);
        assert!(matches!(tokens[0].kind, IdlTokenKind::Short));
    }

    #[test]
    fn test_block_comment() {
        let mut lexer = IdlLexer::new("/* Block comment */\nshort");
        let tokens = lexer.tokenize().unwrap();
        assert_eq!(tokens.len(), 1);
        assert!(matches!(tokens[0].kind, IdlTokenKind::Short));
    }
}
