//! Issue 2978 -- every WRITE_DATA this backend sends goes through one of the
//! four checked writers in `nros-rmw-xrce/src/session.c`.
//!
//! A WRITE_DATA submessage's length is a 16-bit wire field, and the client
//! narrows the length to it without checking: past 65,535 bytes a sample was
//! lost, or delivered cut to its size modulo 65,536, while the publish returned
//! success. The writers (`xrce_write_topic`, `xrce_write_request`,
//! `xrce_write_reply`, `xrce_prepare_topic_stream`) refuse such a body before
//! anything is written. A send path that calls the client's writer directly
//! would skip that refusal, so this test refuses the direct call anywhere but
//! inside the writers.

use std::path::Path;

/// The client calls that buffer a WRITE_DATA submessage.
const CLIENT_WRITERS: [&str; 4] = [
    "uxr_buffer_topic",
    "uxr_buffer_request",
    "uxr_buffer_reply",
    "uxr_prepare_output_stream",
];

/// `src` without its `//` and `/* */` comments, so a writer NAMED in prose is
/// not a call.
fn strip_comments(src: &str) -> String {
    let mut out = String::with_capacity(src.len());
    let mut rest = src;
    while !rest.is_empty() {
        if let Some(r) = rest.strip_prefix("//") {
            rest = r.find('\n').map_or("", |i| &r[i..]);
        } else if let Some(r) = rest.strip_prefix("/*") {
            rest = r.find("*/").map_or("", |i| &r[i + 2..]);
        } else {
            let c = rest.chars().next().unwrap();
            out.push(c);
            rest = &rest[c.len_utf8()..];
        }
    }
    out
}

/// The calls to `name` in `code`: the name, not followed by another
/// identifier character, then optional space and `(`.
fn calls(code: &str, name: &str) -> usize {
    code.match_indices(name)
        .filter(|(i, _)| {
            let before = code[..*i].chars().next_back();
            let after = &code[i + name.len()..];
            !before.is_some_and(|c| c.is_ascii_alphanumeric() || c == '_')
                && after.trim_start().starts_with('(')
        })
        .count()
}

#[test]
fn every_write_data_goes_through_a_checked_writer() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("../nros-rmw-xrce/src");
    let mut direct = Vec::new();
    let mut in_session = [0usize; 4];
    for entry in std::fs::read_dir(&src).expect("reading nros-rmw-xrce/src") {
        let path = entry.expect("a directory entry").path();
        if path.extension().and_then(|e| e.to_str()) != Some("c") {
            continue;
        }
        let code = strip_comments(&std::fs::read_to_string(&path).expect("reading a source"));
        let file = path.file_name().unwrap().to_string_lossy().into_owned();
        for (k, name) in CLIENT_WRITERS.iter().enumerate() {
            let n = calls(&code, name);
            if file == "session.c" {
                in_session[k] = n;
            } else if n > 0 {
                direct.push(format!("{file}: {n} call(s) to {name}"));
            }
        }
    }
    assert!(
        direct.is_empty(),
        "a send path calls the client's WRITE_DATA writer directly, so nothing checks the \
         16-bit length; call the checked writer in session.c instead:\n  {}",
        direct.join("\n  ")
    );
    assert_eq!(
        in_session,
        [1, 1, 1, 1],
        "session.c should call each client writer exactly once, inside its checked writer \
         ({CLIENT_WRITERS:?})"
    );
}

#[test]
fn the_scan_finds_a_call_and_ignores_prose() {
    let code = strip_comments(
        "/* `uxr_buffer_topic` refuses */ x = uxr_buffer_topic (a);\n\
         // uxr_buffer_topic(b)\n\
         nros_uxr_buffer_topic(c); uxr_buffer_topic_2(d);",
    );
    assert_eq!(calls(&code, "uxr_buffer_topic"), 1);
}
