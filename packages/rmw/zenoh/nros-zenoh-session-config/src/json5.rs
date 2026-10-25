//! A json5 reader into `serde_json::Value` — the subset zenoh's session
//! configs are written in.
//!
//! Written here rather than taken from the `json5` crate because this crate is
//! reached by the zenoh backend, which sits in every zenoh image's lockfile:
//! the `json5` crate brings `pest` and four more packages into each.
//! The language is small and fixed (<https://spec.json5.org>): `//` and `/* */`
//! comments, unquoted identifier keys, single- or double-quoted strings,
//! trailing commas, hexadecimal and signed numbers, `Infinity` / `NaN`.

use alloc::{format, string::String, vec::Vec};
use serde_json::{Map, Number, Value};

/// Parse `src`.
///
/// # Errors
///
/// The first syntax error, with its line and column.
pub fn parse(src: &str) -> Result<Value, String> {
    let mut p = Parser {
        s: src.as_bytes(),
        i: 0,
    };
    p.ws()?;
    let v = p.value()?;
    p.ws()?;
    if p.i != p.s.len() {
        return Err(p.err("trailing characters after the value"));
    }
    Ok(v)
}

struct Parser<'a> {
    s: &'a [u8],
    i: usize,
}

impl Parser<'_> {
    fn err(&self, what: &str) -> String {
        let before = &self.s[..self.i.min(self.s.len())];
        let line = before.iter().filter(|b| **b == b'\n').count() + 1;
        let col = before.iter().rev().take_while(|b| **b != b'\n').count() + 1;
        format!("{what} at line {line}, column {col}")
    }

    fn peek(&self) -> Option<u8> {
        self.s.get(self.i).copied()
    }

    fn eat(&mut self, b: u8) -> bool {
        if self.peek() == Some(b) {
            self.i += 1;
            true
        } else {
            false
        }
    }

    /// Whitespace and comments.
    fn ws(&mut self) -> Result<(), String> {
        loop {
            match self.peek() {
                Some(b' ' | b'\t' | b'\n' | b'\r') => self.i += 1,
                Some(b'/') if self.s.get(self.i + 1) == Some(&b'/') => {
                    while !matches!(self.peek(), None | Some(b'\n')) {
                        self.i += 1;
                    }
                }
                Some(b'/') if self.s.get(self.i + 1) == Some(&b'*') => {
                    self.i += 2;
                    loop {
                        match self.peek() {
                            None => return Err(self.err("unterminated /* comment")),
                            Some(b'*') if self.s.get(self.i + 1) == Some(&b'/') => {
                                self.i += 2;
                                break;
                            }
                            _ => self.i += 1,
                        }
                    }
                }
                _ => return Ok(()),
            }
        }
    }

    fn value(&mut self) -> Result<Value, String> {
        match self.peek() {
            Some(b'{') => self.object(),
            Some(b'[') => self.array(),
            Some(b'"' | b'\'') => self.string().map(Value::String),
            Some(b'-' | b'+' | b'.' | b'0'..=b'9') => self.number(),
            Some(c) if is_ident_start(c) => {
                let w = self.ident();
                match w.as_str() {
                    "true" => Ok(Value::Bool(true)),
                    "false" => Ok(Value::Bool(false)),
                    "null" => Ok(Value::Null),
                    "Infinity" | "NaN" => Err(self.err("Infinity / NaN has no JSON value")),
                    _ => Err(self.err(&format!("unexpected word `{w}`"))),
                }
            }
            Some(_) => Err(self.err("unexpected character")),
            None => Err(self.err("unexpected end of input")),
        }
    }

    fn object(&mut self) -> Result<Value, String> {
        self.i += 1; // `{`
        let mut map = Map::new();
        loop {
            self.ws()?;
            if self.eat(b'}') {
                return Ok(Value::Object(map));
            }
            let key = match self.peek() {
                Some(b'"' | b'\'') => self.string()?,
                Some(c) if is_ident_start(c) => self.ident(),
                _ => return Err(self.err("expected a key")),
            };
            self.ws()?;
            if !self.eat(b':') {
                return Err(self.err("expected `:` after a key"));
            }
            self.ws()?;
            let v = self.value()?;
            if map.insert(key.clone(), v).is_some() {
                return Err(self.err(&format!("key `{key}` stated twice")));
            }
            self.ws()?;
            if self.eat(b',') {
                continue;
            }
            self.ws()?;
            if self.eat(b'}') {
                return Ok(Value::Object(map));
            }
            return Err(self.err("expected `,` or `}`"));
        }
    }

    fn array(&mut self) -> Result<Value, String> {
        self.i += 1; // `[`
        let mut out = Vec::new();
        loop {
            self.ws()?;
            if self.eat(b']') {
                return Ok(Value::Array(out));
            }
            out.push(self.value()?);
            self.ws()?;
            if self.eat(b',') {
                continue;
            }
            self.ws()?;
            if self.eat(b']') {
                return Ok(Value::Array(out));
            }
            return Err(self.err("expected `,` or `]`"));
        }
    }

    fn ident(&mut self) -> String {
        let start = self.i;
        while self
            .peek()
            .is_some_and(|c| is_ident_start(c) || c.is_ascii_digit())
        {
            self.i += 1;
        }
        String::from_utf8_lossy(&self.s[start..self.i]).into_owned()
    }

    fn string(&mut self) -> Result<String, String> {
        let q = self.s[self.i];
        self.i += 1;
        let mut out: Vec<u8> = Vec::new();
        loop {
            let Some(c) = self.peek() else {
                return Err(self.err("unterminated string"));
            };
            self.i += 1;
            match c {
                c if c == q => break,
                b'\n' => return Err(self.err("a newline inside a string")),
                b'\\' => {
                    let Some(e) = self.peek() else {
                        return Err(self.err("unterminated escape"));
                    };
                    self.i += 1;
                    match e {
                        b'n' => out.push(b'\n'),
                        b't' => out.push(b'\t'),
                        b'r' => out.push(b'\r'),
                        b'b' => out.push(0x08),
                        b'f' => out.push(0x0c),
                        b'v' => out.push(0x0b),
                        b'0' => out.push(0),
                        b'\n' => {} // line continuation
                        b'u' => {
                            let hex = self
                                .s
                                .get(self.i..self.i + 4)
                                .and_then(|h| core::str::from_utf8(h).ok())
                                .and_then(|h| u32::from_str_radix(h, 16).ok())
                                .ok_or_else(|| self.err("a \\u escape needs four hex digits"))?;
                            self.i += 4;
                            let ch = char::from_u32(hex).ok_or_else(|| {
                                self.err("a \\u escape outside the BMP's scalars")
                            })?;
                            let mut b = [0u8; 4];
                            out.extend_from_slice(ch.encode_utf8(&mut b).as_bytes());
                        }
                        other => out.push(other),
                    }
                }
                other => out.push(other),
            }
        }
        String::from_utf8(out).map_err(|_| self.err("a string that is not UTF-8"))
    }

    fn number(&mut self) -> Result<Value, String> {
        let start = self.i;
        let neg = self.eat(b'-');
        if !neg {
            self.eat(b'+');
        }
        if self.s[self.i..].starts_with(b"0x") || self.s[self.i..].starts_with(b"0X") {
            self.i += 2;
            let h0 = self.i;
            while self.peek().is_some_and(|c| c.is_ascii_hexdigit()) {
                self.i += 1;
            }
            let v = u64::from_str_radix(core::str::from_utf8(&self.s[h0..self.i]).unwrap(), 16)
                .map_err(|_| self.err("a malformed hexadecimal number"))?;
            return Ok(if neg {
                Value::Number(Number::from(-(v as i128) as i64))
            } else {
                Value::Number(Number::from(v))
            });
        }
        while self
            .peek()
            .is_some_and(|c| c.is_ascii_digit() || matches!(c, b'.' | b'e' | b'E' | b'+' | b'-'))
        {
            self.i += 1;
        }
        let text = core::str::from_utf8(&self.s[start..self.i])
            .unwrap()
            .trim_start_matches('+');
        if let Ok(n) = text.parse::<i64>() {
            return Ok(Value::Number(Number::from(n)));
        }
        if let Ok(n) = text.parse::<u64>() {
            return Ok(Value::Number(Number::from(n)));
        }
        text.parse::<f64>()
            .ok()
            .and_then(Number::from_f64)
            .map(Value::Number)
            .ok_or_else(|| self.err(&format!("`{text}` is not a number")))
    }
}

fn is_ident_start(c: u8) -> bool {
    c.is_ascii_alphabetic() || c == b'_' || c == b'$'
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_json5_zenoh_configs_are_written_in_parses() {
        let v = parse(
            r#"
            // a comment
            {
              mode: 'peer', /* block */
              "listen": { endpoints: ["tcp/0.0.0.0:7448",], },
              scouting: { timeout: 3000, multicast: { enabled: false } },
              n: -0x10, f: 1.5e3, s: "a\"bA", t: true, z: null,
            }
            "#,
        )
        .unwrap();
        assert_eq!(v["mode"], "peer");
        assert_eq!(v["listen"]["endpoints"][0], "tcp/0.0.0.0:7448");
        assert_eq!(v["scouting"]["timeout"], 3000);
        assert_eq!(v["n"], -16);
        assert_eq!(v["f"], 1500.0);
        assert_eq!(v["s"], "a\"bA");
        assert_eq!(v["t"], true);
        assert!(v["z"].is_null());
        // Plain JSON is json5.
        assert_eq!(parse(r#"{"a": [1, 2]}"#).unwrap()["a"][1], 2);
    }

    #[test]
    fn syntax_errors_say_where() {
        for (src, want) in [
            ("{ a: 1", "expected `,` or `}`"),
            ("{ a 1 }", "expected `:`"),
            ("{ a: 1, a: 2 }", "stated twice"),
            ("{ a: 'x }", "unterminated string"),
            ("{ a: /* x }", "unterminated /*"),
            ("{ a: nope }", "unexpected word"),
            ("{} x", "trailing"),
            ("{ a: Infinity }", "no JSON value"),
        ] {
            let e = parse(src).unwrap_err();
            assert!(e.contains(want) && e.contains("line 1"), "{src}\n-> {e}");
        }
    }
}
