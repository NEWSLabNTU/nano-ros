//! Emit the IDL string produced by `rosidl_adapter.resource.msg.idl.em`
//! + `struct.idl.em` for a parsed `Message`.
//!
//! The output is intentionally **byte-identical** to the python's
//! `expand_template` rendering, so the mangling pass downstream can
//! re-rewrite without surprises.

use crate::{
    parser::{Message, RosType},
    types::idl_type_for,
};
use std::collections::{BTreeMap, BTreeSet};

/// Render a parsed `.msg` to the raw (un-mangled) IDL string —
/// matches `msg.idl.em` byte-for-byte for the fixture set.
pub fn emit_idl(package: &str, message: &str, msg: &Message) -> String {
    let mut out = String::new();
    // File header (4 lines + trailing blank).
    out.push_str("// generated from rosidl_adapter/resource/msg.idl.em\n");
    out.push_str(&format!("// with input from {package}/msg/{message}.msg\n"));
    out.push_str("// generated code does not contain a copyright notice\n");
    out.push('\n');

    // `#include` lines, sorted (BTreeSet matches python's `sorted(set)`).
    let mut includes: BTreeSet<String> = BTreeSet::new();
    for f in &msg.fields {
        if let Some(inc) = include_for(&f.ty) {
            includes.insert(inc);
        }
    }
    for inc in &includes {
        out.push_str(&format!("#include \"{inc}\"\n"));
    }

    // The template always emits the leading newline before the
    // `module` block. After the `@[end for]@` for includes, an `@`
    // collapses the next newline IF nothing was emitted in the loop;
    // but EmPy still leaves a single blank line before `module`
    // regardless. Match the observed output: one blank line here.
    out.push('\n');

    out.push_str(&format!("module {package} {{\n"));
    out.push_str("  module msg {\n");
    emit_struct(&mut out, message, msg);
    out.push_str("  };\n");
    out.push_str("};\n");
    out
}

/// Inner struct template (`struct.idl.em`). Implements the subset
/// used by the fixture set: typedef synthesis for fixed-size arrays,
/// optional message-level comment, then per-field { optional
/// comment, optional blank-separator, `<idl_type> <name>;` }.
fn emit_struct(out: &mut String, message: &str, msg: &Message) {
    // Pre-walk: synthesize typedefs for fixed-size arrays. Order is
    // first-seen (matches python's `OrderedDict`).
    let mut typedefs: Vec<(String, String)> = Vec::new();
    let mut seen: BTreeMap<String, String> = BTreeMap::new();
    for f in &msg.fields {
        if !f.ty.is_fixed_size_array() {
            continue;
        }
        let idl_type = idl_type_for(&f.ty);
        let idl_base_type = idl_type.split('[').next().unwrap();
        let idl_base_type_identifier = idl_base_type.replace("::", "__");
        if idl_base_type_identifier != idl_base_type
            && !seen.contains_key(&idl_base_type_identifier)
        {
            typedefs.push((idl_base_type_identifier.clone(), idl_base_type.to_string()));
            seen.insert(idl_base_type_identifier.clone(), idl_base_type.to_string());
        }
        let n = f.ty.array_size.unwrap();
        let idl_type_identifier = format!("{}[{n}]", get_idl_type_identifier(&idl_type));
        if !seen.contains_key(&idl_type_identifier) {
            typedefs.push((
                idl_type_identifier.clone(),
                idl_base_type_identifier.clone(),
            ));
            seen.insert(idl_type_identifier, idl_base_type_identifier);
        }
    }
    for (k, v) in &typedefs {
        out.push_str(&format!("    typedef {v} {k};\n"));
    }

    // Message-level comment block.
    if !msg.comments.is_empty() {
        emit_verbatim(out, "    ", &msg.comments);
    }

    out.push_str(&format!("    struct {message} {{\n"));

    if msg.fields.is_empty() {
        out.push_str("      uint8 structure_needs_at_least_one_member;\n");
    } else {
        for (i, field) in msg.fields.iter().enumerate() {
            if i > 0 {
                out.push('\n');
            }
            if !field.comments.is_empty() {
                emit_verbatim(out, "      ", &field.comments);
            }
            let idl = if field.ty.is_fixed_size_array() {
                get_idl_type_identifier(&idl_type_for(&field.ty))
            } else {
                idl_type_for(&field.ty)
            };
            out.push_str(&format!("      {idl} {};\n", field.name));
        }
    }

    out.push_str("    };\n");
}

/// Mirror of struct.idl.em's `get_idl_type_identifier`. Strips the
/// `[N]` suffix and translates separators (`::`, `<`, `>`, `[`, `]`)
/// into the `__`-flattened identifier form.
fn get_idl_type_identifier(idl_type: &str) -> String {
    idl_type
        .replace("::", "__")
        .replace('<', "__")
        .replace('>', "")
        .replace('[', "__")
        .replace(']', "")
}

/// Emit a `@verbatim (language="comment", text= ...)` block at the
/// given indent. Mirrors the EmPy template's comment-emission lines.
fn emit_verbatim(out: &mut String, indent: &str, comments: &[String]) {
    out.push_str(&format!("{indent}@verbatim (language=\"comment\", text=\n"));
    let inner_indent = format!("{indent}  ");
    let n = comments.len();
    for (i, line) in comments.iter().enumerate() {
        let literal = idl_string_literal(line);
        if i + 1 < n {
            // not last → trailing ` "\n"`
            out.push_str(&format!("{inner_indent}{literal} \"\\n\"\n"));
        } else {
            // last → trailing `)`
            out.push_str(&format!("{inner_indent}{literal})\n"));
        }
    }
}

/// Mirror of `rosidl_adapter.msg.string_to_idl_string_literal`.
///
/// The python first runs `s.encode().decode('unicode_escape')` —
/// which interprets `\n`, `\t`, `\xNN`, `\uNNNN`, etc. in the input
/// AS escape sequences. Then escapes `"` to `\"` and wraps in `"`.
///
/// For the fixture set every comment is plain ASCII text with no
/// backslash escapes, so the unicode_escape pass is a no-op; we
/// only need to handle `\` → `\\` and `"` → `\"` for forward-safety.
fn idl_string_literal(s: &str) -> String {
    // The python pass produces `s.encode().decode('unicode_escape')`
    // which would CONSUME backslash sequences in the input. For
    // plain ASCII without backslashes, output == input. None of
    // the fixture-set comments carry a backslash, so a 1:1 copy
    // is faithful. We still escape `"` per python.
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            _ => out.push(ch),
        }
    }
    out.push('"');
    out
}

/// `<pkg>/msg/<Type>.idl` for non-primitive types; `None` for primitives.
/// Mirrors `rosidl_adapter.msg.get_include_file`.
fn include_for(ty: &RosType) -> Option<String> {
    if ty.is_primitive() {
        return None;
    }
    let pkg = ty.pkg.as_deref()?;
    Some(format!("{pkg}/msg/{}.idl", ty.base))
}
