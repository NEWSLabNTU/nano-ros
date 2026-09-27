//! The artifact-naming formats have exactly ONE producer, `generator::naming`.
//!
//! Six authored copies of "what is this artifact called?" existed across
//! `msg.rs`, `srv.rs`, `action.rs` and three arms of `cpp.rs`, differing only in
//! a kind word and an extension, plus three copies of the constant prefix and
//! two of the intra-package include path. No gate covered the class, which is
//! why it reached six: a fourth language surface adds three more copies unless
//! something refuses them.
//!
//! This lives in the crate rather than in `just/check.just` on purpose — it is
//! one file read per source file, it needs no repo layout knowledge, and
//! `check-cli-tests` (on the required pull-request context) already runs it. A
//! surface is added by adding a `Surface` row in `generator::naming`, not by
//! re-authoring a format string somewhere else.

use std::{fs, path::Path};

/// Format literals that may appear only in `generator/naming.rs`.
///
/// Uppercase forms catch both the include guard (`…_MSG_{}_H`) and the constant
/// prefix (`…_MSG_{}`), since the first contains the second. Lowercase forms
/// catch the filenames; `_msg_{}.h` also catches `_msg_{}.hpp`.
fn banned_literals() -> Vec<String> {
    let mut out = Vec::new();
    for kind in ["msg", "srv", "action"] {
        out.push(format!("_{}_{{}}", kind.to_uppercase()));
        out.push(format!("_{kind}_{{}}.h"));
        out.push(format!("_{kind}_{{}}.c"));
    }
    out
}

/// Every banned literal `text` contains, with the 1-based line it sits on.
fn findings(text: &str) -> Vec<(usize, String)> {
    let banned = banned_literals();
    let mut hits = Vec::new();
    for (i, line) in text.lines().enumerate() {
        for b in &banned {
            if line.contains(b) {
                hits.push((i + 1, b.clone()));
            }
        }
    }
    hits
}

fn rust_sources(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
    for entry in fs::read_dir(dir).expect("read src dir") {
        let path = entry.expect("dir entry").path();
        if path.is_dir() {
            rust_sources(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

#[test]
fn only_the_naming_module_authors_an_artifact_name_format() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let producer = src.join("generator").join("naming.rs");
    assert!(
        producer.is_file(),
        "the one producer must exist: {producer:?}"
    );

    let mut files = Vec::new();
    rust_sources(&src, &mut files);
    assert!(
        files.len() > 5,
        "expected to scan the crate, found {files:?}"
    );

    let mut offenders = Vec::new();
    for path in files {
        if path == producer {
            continue;
        }
        let text = fs::read_to_string(&path).expect("read source");
        for (line, literal) in findings(&text) {
            offenders.push(format!(
                "{}:{line}: authors `{literal}` — derive it from \
                 `generator::naming::artifact_names(surface, kind, pkg, type)` instead",
                path.strip_prefix(env!("CARGO_MANIFEST_DIR"))
                    .unwrap()
                    .display()
            ));
        }
    }

    assert!(
        offenders.is_empty(),
        "an artifact-name format has more than one producer:\n  {}",
        offenders.join("\n  ")
    );
}

/// Negative control: a gate that can never fail is indistinguishable from a
/// clean tree, so prove the scanner sees each shape it exists to refuse.
#[test]
fn the_scanner_catches_every_shape_it_refuses() {
    let cases = [
        r#"let g = format!("{}_MSG_{}_H", p.to_uppercase(), s.to_uppercase());"#,
        r#"let p = format!("{}_SRV_{}", p.to_uppercase(), s.to_uppercase());"#,
        r#"let c = format!("{}_ACTION_{}", p.to_uppercase(), s.to_uppercase());"#,
        r#"let h = format!("{}_msg_{}.h", pkg, snake);"#,
        r#"let h = format!("{}_srv_{}.hpp", pkg, snake);"#,
        r#"let s = format!("{}_action_{}.c", pkg, snake);"#,
    ];
    for case in cases {
        assert!(
            !findings(case).is_empty(),
            "the scanner must refuse this shape: {case}"
        );
    }
    // And it must not fire on what legitimately survives: generated SYMBOL
    // names are a different family (their own prefixes and suffixes), and a
    // guard spelled out in full carries no `{}` to substitute.
    for ok in [
        r#"let f = format!("nros_cpp_publish_{}_msg_{}", c_pkg_name, msg_snake);"#,
        r##"assert!(pkg.header.contains("#ifndef TEST_MSGS_MSG_POINT_H"));"##,
    ] {
        assert!(
            findings(ok).is_empty(),
            "the scanner must not fire on this: {ok}"
        );
    }
}
