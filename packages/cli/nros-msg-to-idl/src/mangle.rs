//! Cyclone-DDS post-processing pass. Direct port of the python
//! `mangle_idl` + `_mangle_scoped_refs` + `_escape_member` /
//! `SERVICE_HEADER_FIELDS` from `scripts/cyclonedds/msg_to_cyclone_idl.py`.

/// IDL reserved words that legally appear as ROS field names but
/// collide with the grammar. Mirrored verbatim from the python's
/// `_IDL_RESERVED` set so member-escape behaviour is bit-identical.
const IDL_RESERVED: &[&str] = &[
    "sequence",
    "string",
    "wstring",
    "long",
    "short",
    "double",
    "float",
    "char",
    "wchar",
    "boolean",
    "octet",
    "struct",
    "union",
    "enum",
    "module",
    "interface",
    "typedef",
    "const",
    "fixed",
    "native",
    "any",
    "void",
    "in",
    "out",
    "inout",
    "switch",
    "case",
    "default",
    "unsigned",
    // IDL 4.x additions Cyclone 0.10.5's idlc also treats as keywords
    // (phase-306 W2: nav_msgs' {Get,Set,Load}Map srvs carry a `map` field).
    "map",
    "bitmask",
    "bitset",
    "bitfield",
    "int8",
    "uint8",
    "int16",
    "uint16",
    "int32",
    "uint32",
    "int64",
    "uint64",
];

/// Mirror of `SERVICE_HEADER_FIELDS` — Phase 117.X.3 / 117.12.B request
/// header inlined into every `.srv` struct so the wire CDR matches stock
/// `rmw_cyclonedds_cpp`'s `cdds_request_header_t`.
const SERVICE_HEADER_FIELDS: &[&str] = &[
    "unsigned long long rmw_writer_guid;",
    "long long rmw_sequence_number;",
];

/// Post-process a rosidl_adapter-shaped IDL string into the
/// Cyclone-DDS-shaped form (`module dds_ { struct <Name>_ { … } }`).
///
/// Byte-identical to the python `mangle_idl(src,
/// inject_service_header)`.
pub fn mangle_idl(src: &str, inject_service_header: bool) -> String {
    let mut out_lines: Vec<String> = Vec::new();
    let mut nesting: Vec<String> = Vec::new();

    // `splitlines(keepends=False)` semantics — split on `\n` but
    // drop the trailing empty after a final `\n`.
    let mut lines: Vec<&str> = src.split('\n').collect();
    if let Some(last) = lines.last()
        && last.is_empty()
    {
        lines.pop();
    }

    for raw in lines {
        // Scoped-reference rewrite (member fields only — struct lines
        // and wrapper braces carry no `::msg::` triples). The python
        // applies this to every line; we mirror that.
        let line = mangle_scoped_refs(raw);
        // phase-306 W2 (issue 0258): escape reserved member names in the
        // msg/srv path (python does the same in `mangle_idl`) — installed
        // packages hit them (nav_msgs {Get,Set,Load}Map's `map` member).
        // Idempotent: an already-escaped `_map` is not in the reserved set.
        let line = escape_member(&line);

        if let Some((indent_owned, name, rest)) = match_struct(&line) {
            let new_indent = format!("{indent_owned}  ");
            let field_indent = format!("{new_indent}  ");
            out_lines.push(format!("{indent_owned}module dds_ {{"));
            out_lines.push(format!("{new_indent}struct {name}_ {{{rest}"));
            if inject_service_header {
                for hdr in SERVICE_HEADER_FIELDS {
                    out_lines.push(format!("{field_indent}{hdr}"));
                }
            }
            nesting.push(indent_owned);
            continue;
        }

        // Closing `};` for the original struct?
        if let Some(top) = nesting.last() {
            let want = format!("{top}}};");
            if line.trim_end() == want {
                let indent = nesting.pop().unwrap();
                out_lines.push(format!("{indent}  }};"));
                out_lines.push(format!("{indent}}};"));
                continue;
            }
        }

        if !nesting.is_empty() {
            out_lines.push(format!("  {line}"));
        } else {
            out_lines.push(line);
        }
    }

    let mut joined = out_lines.join("\n");
    joined.push('\n');
    joined
}

/// Mirror of the python `_FIELD_DECL_RE` gate + `_escape_member`:
/// a member-declaration line (`<ws><type> <name>;`) whose `<name>` is
/// an `IDL_RESERVED` word gets the IDL escaped-identifier `_` prefix.
/// Any line that is not a field declaration in the python regex's
/// charset (`^\s*[A-Za-z_][\w:<>, ]*\s+[A-Za-z_]\w*;\s*$`) is returned
/// unchanged, keeping the two lowerings bit-identical.
fn escape_member(line: &str) -> String {
    let trimmed_end = line.trim_end();
    let Some(body) = trimmed_end.strip_suffix(';') else {
        return line.to_string();
    };
    // Whole-line charset gate (python `_FIELD_DECL_RE`): leading ws, then
    // an identifier start, then only word/`:<>, `/ws chars up to the `;`.
    let after_ws = body.trim_start();
    let Some(first) = after_ws.chars().next() else {
        return line.to_string();
    };
    if !(first.is_ascii_alphabetic() || first == '_') {
        return line.to_string();
    }
    if !after_ws
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | ':' | '<' | '>' | ',' | ' ' | '\t'))
    {
        return line.to_string();
    }
    // Split `<type> <name>` on the LAST whitespace run (python's greedy
    // `(?P<type>.*\s)(?P<name>[A-Za-z_]\w*)`).
    let Some(name_start) = body.rfind([' ', '\t']).map(|i| i + 1) else {
        return line.to_string();
    };
    let name = &body[name_start..];
    if name.is_empty()
        || !name
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
    {
        return line.to_string();
    }
    if !IDL_RESERVED.contains(&name) {
        return line.to_string();
    }
    // python: f"{type}_{name};" — the trailing `\s*` after `;` is dropped.
    format!("{}_{name};", &trimmed_end[..name_start])
}

/// Detect a `<indent>struct <Name> {` line. Returns
/// `(indent, name, rest)` where `rest` is whatever follows the `{`
/// (e.g. a trailing comment on the same line).
fn match_struct(line: &str) -> Option<(String, String, String)> {
    let mut chars = line.char_indices();
    // Leading indent — tabs or spaces.
    let mut indent_end = 0;
    for (i, c) in chars.by_ref() {
        if c != ' ' && c != '\t' {
            indent_end = i;
            break;
        }
        indent_end = i + 1;
    }
    let rest = &line[indent_end..];
    let after = rest.strip_prefix("struct")?;
    // Require at least one whitespace after `struct`.
    let after_ws_start = 0;
    let mut after_ws_end = 0;
    for (i, c) in after.char_indices() {
        if c == ' ' || c == '\t' {
            after_ws_end = i + 1;
        } else {
            break;
        }
    }
    if after_ws_end == after_ws_start {
        return None;
    }
    let after_ws = &after[after_ws_end..];

    // Capture the identifier.
    let id_end = after_ws
        .char_indices()
        .take_while(|(_, c)| c.is_ascii_alphanumeric() || *c == '_')
        .map(|(i, c)| i + c.len_utf8())
        .last()
        .unwrap_or(0);
    if id_end == 0 {
        return None;
    }
    let name = &after_ws[..id_end];
    // First char of identifier must be alpha or `_`.
    let first = name.chars().next()?;
    if !(first.is_ascii_alphabetic() || first == '_') {
        return None;
    }

    let after_id = &after_ws[id_end..];
    // Skip optional whitespace then require `{`.
    let after_id_trimmed = after_id.trim_start_matches([' ', '\t']);
    let rest_after_brace = after_id_trimmed.strip_prefix('{')?;

    Some((
        line[..indent_end].to_string(),
        name.to_string(),
        rest_after_brace.to_string(),
    ))
}

/// Rewrite `<pkg>::<msg|srv|action>::<Type>` triples to their
/// `<pkg>::<kind>::dds_::<Type>_` form. Mirrors `_SCOPED_REF_RE` +
/// `_mangle_scoped_refs` (idempotent — lines already carrying
/// `::dds_::` are returned unchanged).
fn mangle_scoped_refs(line: &str) -> String {
    if line.contains("::dds_::") {
        return line.to_string();
    }
    let bytes = line.as_bytes();
    let mut out = String::with_capacity(line.len() + 16);
    let mut i = 0;
    while i < bytes.len() {
        // Try to match an identifier at position i.
        let id_start = i;
        let mut j = i;
        while j < bytes.len() && (is_word(bytes[j])) {
            j += 1;
        }
        if j == id_start {
            // Not an identifier char — copy the whole CHARACTER and advance by
            // its length. `bytes[i] as char` would latin-1-decode a single byte
            // and re-encode it as UTF-8, doubling every non-ASCII byte (an em
            // dash `e2 80 94` in an IDL comment came out as `c3 a2 c2 80 c2 94`);
            // see the sibling fix in `cargo_nano_ros::replace_outside_strings`.
            let ch = line[i..]
                .chars()
                .next()
                .expect("byte index is on a char boundary");
            out.push(ch);
            i += ch.len_utf8();
            continue;
        }

        // Word-boundary check: previous char must not be a word char.
        if id_start > 0 && is_word(bytes[id_start - 1]) {
            // Copy literally up to j.
            out.push_str(&line[id_start..j]);
            i = j;
            continue;
        }

        // Look for `<id>::<kind>::<Type>` with `kind in {msg,srv,action}`.
        let id1 = &line[id_start..j];
        if !is_valid_ident_start(id1) {
            out.push_str(id1);
            i = j;
            continue;
        }

        // Expect `::`.
        if !line[j..].starts_with("::") {
            out.push_str(id1);
            i = j;
            continue;
        }
        let kind_start = j + 2;
        let mut kind_end = kind_start;
        while kind_end < bytes.len() && is_word(bytes[kind_end]) {
            kind_end += 1;
        }
        let kind = &line[kind_start..kind_end];
        if kind != "msg" && kind != "srv" && kind != "action" {
            out.push_str(id1);
            i = j;
            continue;
        }

        if !line[kind_end..].starts_with("::") {
            out.push_str(id1);
            i = j;
            continue;
        }
        let ty_start = kind_end + 2;
        let mut ty_end = ty_start;
        while ty_end < bytes.len() && is_word(bytes[ty_end]) {
            ty_end += 1;
        }
        let ty_name = &line[ty_start..ty_end];
        if !is_valid_ident_start(ty_name) {
            out.push_str(id1);
            i = j;
            continue;
        }
        // Trailing word-boundary: next char (if any) must not be word.
        if ty_end < bytes.len() && is_word(bytes[ty_end]) {
            out.push_str(id1);
            i = j;
            continue;
        }

        // Match!
        out.push_str(&format!("{id1}::{kind}::dds_::{ty_name}_"));
        i = ty_end;
    }

    out
}

fn is_word(b: u8) -> bool {
    (b.is_ascii_alphanumeric()) || b == b'_'
}

fn is_valid_ident_start(s: &str) -> bool {
    let mut chars = s.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_alphabetic() || c == '_')
}
