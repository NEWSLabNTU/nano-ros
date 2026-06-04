//! ROS `.msg` parser. Mirrors `rosidl_adapter.parser.parse_message_string`
//! for the common-case subset used by the bundled rcl-interfaces tree.

use crate::types::ConvertError;

/// Primitive type tokens recognised in `.msg` files (mirror of
/// `rosidl_adapter.parser.PRIMITIVE_TYPES`).
const PRIMITIVE_TYPES: &[&str] = &[
    "bool", "byte", "char", "float32", "float64", "int8", "uint8", "int16", "uint16", "int32",
    "uint32", "int64", "uint64", "string", "wstring", "duration", "time",
];

const STRING_UPPER_BOUND_TOKEN: &str = "<=";
const ARRAY_UPPER_BOUND_TOKEN: &str = "<=";

/// A parsed type expression. Mirrors `rosidl_adapter.parser.Type`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RosType {
    /// Package name for non-primitive types (`Some("std_msgs")`)
    /// or `None` for primitives.
    pub pkg: Option<String>,
    /// Bare type name without package prefix or array brackets.
    pub base: String,
    /// `string<=N` upper bound, if set.
    pub string_upper_bound: Option<u32>,
    /// `true` if the type carries `[]`/`[N]`/`[<=N]`.
    pub is_array: bool,
    /// Numeric bound parsed out of the array brackets, if any.
    pub array_size: Option<u32>,
    /// `true` for `[<=N]` (vs `[N]`).
    pub is_upper_bound: bool,
}

impl RosType {
    pub fn is_primitive(&self) -> bool {
        self.pkg.is_none()
    }
    pub fn is_fixed_size_array(&self) -> bool {
        self.is_array && self.array_size.is_some() && !self.is_upper_bound
    }
}

/// A field in a parsed `.msg`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Field {
    pub ty: RosType,
    pub name: String,
    /// Comment lines attached to this field (in order). Mirrors
    /// `rosidl_adapter`'s `annotations['comment']` *after*
    /// `process_comments` dedent / blank-strip.
    pub comments: Vec<String>,
}

/// A parsed `.msg` file. `constants` is reserved — none of the
/// fixture set uses constants and the python action / srv flows
/// don't either, so the port leaves it as a stub.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    pub package: String,
    pub name: String,
    pub comments: Vec<String>,
    pub fields: Vec<Field>,
}

/// Parse a `.msg` file. Mirrors
/// `rosidl_adapter.parser.parse_message_string`.
pub fn parse_msg(package: &str, name: &str, source: &str) -> Result<Message, ConvertError> {
    // Mirror the python `replace('\t', ' ')` and `splitlines()`.
    let normalized = source.replace('\t', " ");
    let raw_lines: Vec<&str> = normalized.split('\n').collect();
    // python `splitlines()` drops a trailing empty line if the
    // string ends in `\n`. Mimic that.
    let mut all_lines: Vec<&str> = raw_lines.clone();
    if let Some(last) = all_lines.last() {
        if last.is_empty() {
            all_lines.pop();
        }
    }

    // 1. Extract file-level (message) comments: every leading line
    //    that starts with `#`, stripping one or more leading `#`.
    let split_idx = all_lines
        .iter()
        .position(|l| !l.starts_with('#'))
        .unwrap_or(all_lines.len());
    let mut message_comments: Vec<String> = all_lines[..split_idx]
        .iter()
        .map(|l| lstrip_chars(l, '#'))
        .collect();
    let body = &all_lines[split_idx..];

    let mut fields: Vec<Field> = Vec::new();
    let mut current_comments: Vec<String> = Vec::new();

    for raw in body {
        // Mirror python `line.rstrip()` — strips ASCII whitespace
        // from the end. `str::trim_end` matches.
        let line = raw.trim_end();
        if line.is_empty() {
            continue;
        }

        // Find the comment delimiter `#`.
        let (code, comment): (&str, Option<String>) = match line.find('#') {
            Some(idx) => {
                let raw_comment = &line[idx..];
                (&line[..idx], Some(lstrip_chars(raw_comment, '#')))
            }
            None => (line, None),
        };

        if let Some(comment_text) = comment {
            // Python: `if line and not line.strip()` — indented
            // comment, attaches to previous field.
            if !code.is_empty() && code.trim().is_empty() {
                if let Some(last) = fields.last_mut() {
                    last.comments.push(comment_text);
                }
                continue;
            }
            // Otherwise it's a free-floating comment ahead of a field.
            current_comments.push(comment_text);
            // And fall through with `code` for the field parse.
        }
        let code = code.trim_end();
        if code.is_empty() {
            continue;
        }

        // Split off `<type> <rest>`.
        let (ty_tok, rest) = match code.split_once(' ') {
            Some((t, r)) => (t, r.trim_start()),
            None => return Err(ConvertError::InvalidFieldDefinition(code.to_string())),
        };
        if rest.is_empty() {
            return Err(ConvertError::InvalidFieldDefinition(code.to_string()));
        }

        // Detect constants (`=` separator). The fixture set has
        // none; reject explicitly so future callers see an error
        // instead of silent skip.
        if rest.contains('=') {
            return Err(ConvertError::Unsupported(format!(
                "constant declarations not yet supported: {code}"
            )));
        }

        // Default values (third whitespace-separated token) — also
        // unsupported by the fixture set; if present, fold into
        // the type token's `rest` and parse only the field name.
        let (field_name, default) = match rest.split_once(' ') {
            Some((n, d)) => (n, Some(d.trim_start().to_string())),
            None => (rest, None),
        };
        if let Some(d) = &default {
            if !d.is_empty() {
                return Err(ConvertError::Unsupported(format!(
                    "default values not yet supported: {code}"
                )));
            }
        }

        let ty = parse_type(ty_tok, package)?;

        let mut field = Field {
            ty,
            name: field_name.to_string(),
            comments: Vec::new(),
        };
        field.comments.extend(current_comments.drain(..));
        fields.push(field);
    }

    // Run the per-comment-list condense pass on the message + every
    // field, mirroring `process_comments`.
    process_comments(&mut message_comments);
    for f in fields.iter_mut() {
        process_comments(&mut f.comments);
    }

    Ok(Message {
        package: package.to_string(),
        name: name.to_string(),
        comments: message_comments,
        fields,
    })
}

/// Mirror of `rosidl_adapter.parser.process_comments` (only the
/// trimming branch — unit-extraction omitted because none of the
/// fixture set carries a `[unit]` annotation).
fn process_comments(lines: &mut Vec<String>) {
    // Strip leading empties.
    while lines.first().map(|s| s.is_empty()).unwrap_or(false) {
        lines.remove(0);
    }
    // Strip trailing empties.
    while lines.last().map(|s| s.is_empty()).unwrap_or(false) {
        lines.pop();
    }
    // Collapse consecutive empties.
    let mut i = 1;
    while i < lines.len() {
        if lines[i].is_empty() && lines[i - 1].is_empty() {
            lines.remove(i);
            continue;
        }
        i += 1;
    }
    if lines.is_empty() {
        return;
    }
    // Apply `textwrap.dedent`. Implemented inline for the ASCII-only
    // comment input set.
    let dedented = textwrap_dedent(lines);
    *lines = dedented;
}

/// Tiny textwrap.dedent: find the longest common leading-whitespace
/// prefix across all non-empty lines and strip it from every line.
fn textwrap_dedent(lines: &[String]) -> Vec<String> {
    let prefixes: Vec<&str> = lines
        .iter()
        .filter(|l| !l.trim().is_empty())
        .map(|l| {
            let end = l.find(|c: char| !c.is_whitespace()).unwrap_or(l.len());
            &l[..end]
        })
        .collect();
    let common = if prefixes.is_empty() {
        ""
    } else {
        let mut common = prefixes[0];
        for p in &prefixes[1..] {
            common = common_prefix(common, p);
            if common.is_empty() {
                break;
            }
        }
        common
    };
    lines
        .iter()
        .map(|l| {
            if l.starts_with(common) {
                l[common.len()..].to_string()
            } else {
                l.clone()
            }
        })
        .collect()
}

fn common_prefix<'a>(a: &'a str, b: &str) -> &'a str {
    let n = a
        .as_bytes()
        .iter()
        .zip(b.as_bytes().iter())
        .take_while(|(x, y)| x == y)
        .count();
    &a[..n]
}

/// Strip one or more leading occurrences of `ch` from `s`. Mirrors
/// python's `str.lstrip(COMMENT_DELIMITER)` (which strips a SET of
/// chars, but only `#` is in the set).
fn lstrip_chars(s: &str, ch: char) -> String {
    let trimmed = s.trim_start_matches(ch);
    trimmed.to_string()
}

/// Parse a single type token (after array / string-bound handling).
/// Mirrors `rosidl_adapter.parser.Type::__init__` + `BaseType::__init__`.
fn parse_type(tok: &str, context_package: &str) -> Result<RosType, ConvertError> {
    let mut is_array = false;
    let mut array_size: Option<u32> = None;
    let mut is_upper_bound = false;
    let mut core_tok = tok;

    if tok.ends_with(']') {
        is_array = true;
        let idx = tok
            .rfind('[')
            .ok_or_else(|| ConvertError::MalformedArray(tok.to_string()))?;
        let bracket_inner = &tok[idx + 1..tok.len() - 1];
        if !bracket_inner.is_empty() {
            let (rest, upper) =
                if let Some(stripped) = bracket_inner.strip_prefix(ARRAY_UPPER_BOUND_TOKEN) {
                    (stripped, true)
                } else {
                    (bracket_inner, false)
                };
            let n: u32 = rest
                .parse()
                .map_err(|_| ConvertError::InvalidBound(tok.to_string()))?;
            if n == 0 {
                return Err(ConvertError::InvalidBound(tok.to_string()));
            }
            array_size = Some(n);
            is_upper_bound = upper;
        }
        core_tok = &tok[..idx];
    }

    // Now `core_tok` is the un-arrayed type, e.g. `int32`, `string<=20`,
    // `std_msgs/Header`.
    let (pkg, base, string_upper_bound) = parse_base_type(core_tok, context_package)?;

    Ok(RosType {
        pkg,
        base,
        string_upper_bound,
        is_array,
        array_size,
        is_upper_bound,
    })
}

fn parse_base_type(
    tok: &str,
    context_package: &str,
) -> Result<(Option<String>, String, Option<u32>), ConvertError> {
    if PRIMITIVE_TYPES.contains(&tok) {
        return Ok((None, tok.to_string(), None));
    }

    // string<=N / wstring<=N
    for prefix in ["string", "wstring"] {
        let bound_prefix = format!("{prefix}{STRING_UPPER_BOUND_TOKEN}");
        if let Some(rest) = tok.strip_prefix(&bound_prefix) {
            let n: u32 = rest
                .parse()
                .map_err(|_| ConvertError::InvalidBound(tok.to_string()))?;
            if n == 0 {
                return Err(ConvertError::InvalidBound(tok.to_string()));
            }
            return Ok((None, prefix.to_string(), Some(n)));
        }
    }

    // `<pkg>/<Type>` or bare `<Type>` (using context_package).
    let (pkg, base) = match tok.split_once('/') {
        Some((p, b)) => (p.to_string(), b.to_string()),
        None => (context_package.to_string(), tok.to_string()),
    };
    if pkg.is_empty() || base.is_empty() {
        return Err(ConvertError::InvalidResourceName(tok.to_string()));
    }

    // No primitive-vs-namespaced disambiguation needed — the
    // primitive check above already covered it.
    Ok((Some(pkg), base, None))
}
