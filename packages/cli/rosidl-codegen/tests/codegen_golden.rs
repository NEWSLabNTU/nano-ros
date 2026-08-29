//! Codegen golden test — RFC-0061 / phase-318 W2.
//!
//! Regenerate the fingerprint corpus and diff it against committed expected
//! output. Seconds, no fixture, no toolchain, no QEMU — and it catches the class
//! of regression that otherwise surfaces as a fixture build failure minutes to
//! hours later, on a platform, with a confusing error.
//!
//! Used ad hoc during issues 0344–0346 this exact pattern caught two real
//! regressions that no other test would have:
//!
//! * a macro extraction that swapped the serialize and deserialize bodies
//!   (a substring collision: `replace("SER_BODY")` matched inside `DESER_BODY`);
//! * a trailing newline appended to six templates, which changed the last byte of
//!   every generated file in the tree.
//!
//! It reads [`rosidl_codegen::fingerprint::emit_corpus`] — the SAME map
//! [`rosidl_codegen::codegen_fingerprint`] hashes. That sharing is deliberate: a
//! golden test covering different bytes than the fingerprint could pass while the
//! fingerprint moved (or the reverse), and neither signal would be trustworthy.
//!
//! # Updating
//!
//! An intentional codegen change SHOULD fail this test. Re-record with:
//!
//! ```console
//! $ NROS_UPDATE_GOLDEN=1 cargo test -p rosidl-codegen --test codegen_golden
//! ```
//!
//! then READ the diff before committing — that diff is the review artifact, and
//! it is the only place a reviewer sees what a template edit actually did to
//! emitted code.

use rosidl_codegen::fingerprint::emit_corpus;
use std::{
    fs,
    path::{Path, PathBuf},
};

fn golden_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/fingerprint-corpus/expected")
}

fn update_requested() -> bool {
    std::env::var("NROS_UPDATE_GOLDEN").is_ok_and(|v| v != "0")
}

#[test]
fn generated_output_matches_the_committed_golden() {
    let emitted = emit_corpus();
    let dir = golden_dir();

    if update_requested() {
        let _ = fs::remove_dir_all(&dir);
        for (rel, body) in &emitted {
            let path = dir.join(rel);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, body).unwrap();
        }
        eprintln!(
            "re-recorded {} golden files under {}",
            emitted.len(),
            dir.display()
        );
        return;
    }

    assert!(
        dir.is_dir(),
        "golden dir {} is missing — record it with \
         NROS_UPDATE_GOLDEN=1 cargo test -p rosidl-codegen --test codegen_golden",
        dir.display()
    );

    let mut missing = Vec::new();
    let mut changed = Vec::new();
    for (rel, body) in &emitted {
        let path = dir.join(rel);
        match fs::read_to_string(&path) {
            Err(_) => missing.push(rel.clone()),
            Ok(on_disk) if &on_disk != body => changed.push((rel.clone(), on_disk, body.clone())),
            Ok(_) => {}
        }
    }

    // A golden file with no emitter behind it is stale coverage — it would keep
    // asserting bytes nothing produces any more.
    let mut orphaned = Vec::new();
    if let Ok(entries) = walk(&dir) {
        for p in entries {
            let rel = p
                .strip_prefix(&dir)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            if !emitted.contains_key(&rel) {
                orphaned.push(rel);
            }
        }
    }

    if missing.is_empty() && changed.is_empty() && orphaned.is_empty() {
        return;
    }

    let mut msg = String::from("generated codegen output does not match the committed golden.\n");
    if !missing.is_empty() {
        msg.push_str(&format!(
            "\nNEW output with no golden file ({}):\n",
            missing.len()
        ));
        for m in missing.iter().take(10) {
            msg.push_str(&format!("  + {m}\n"));
        }
    }
    if !orphaned.is_empty() {
        msg.push_str(&format!(
            "\nGolden files nothing emits any more ({}):\n",
            orphaned.len()
        ));
        for o in orphaned.iter().take(10) {
            msg.push_str(&format!("  - {o}\n"));
        }
    }
    for (rel, on_disk, now) in changed.iter().take(3) {
        msg.push_str(&format!("\nCHANGED {rel}:\n"));
        msg.push_str(&first_diff(on_disk, now));
    }
    if changed.len() > 3 {
        msg.push_str(&format!(
            "\n… and {} more changed files\n",
            changed.len() - 3
        ));
    }
    msg.push_str(
        "\nIf the change is intended:\n  \
         NROS_UPDATE_GOLDEN=1 cargo test -p rosidl-codegen --test codegen_golden\n  \
         …then read the resulting diff before committing.\n",
    );
    panic!("{msg}");
}

/// First differing line with a little context — enough to recognise the change
/// without dumping a whole generated file into the failure output.
fn first_diff(a: &str, b: &str) -> String {
    let (al, bl): (Vec<_>, Vec<_>) = (a.lines().collect(), b.lines().collect());
    for (i, (x, y)) in al.iter().zip(bl.iter()).enumerate() {
        if x != y {
            return format!("  line {}:\n    golden: {x}\n    now   : {y}\n", i + 1);
        }
    }
    // One side is a prefix of the other (a pure append/truncate). Show the
    // added or removed lines — "line count 97 -> 99" tells a reviewer nothing
    // about WHAT a template edit did, and reviewability is the point.
    let (label, extra) = if bl.len() > al.len() {
        ("added", &bl[al.len()..])
    } else {
        ("removed", &al[bl.len()..])
    };
    let mut out = format!("  {label} {} line(s) at end:\n", extra.len());
    for line in extra.iter().take(4) {
        out.push_str(&format!("    {label:>7}: {line}\n"));
    }
    out
}

fn walk(dir: &Path) -> std::io::Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in fs::read_dir(&d)? {
            let p = e?.path();
            if p.is_dir() {
                stack.push(p);
            } else {
                out.push(p);
            }
        }
    }
    Ok(out)
}

/// The corpus must actually reach the emitters. A golden test over an empty map
/// would pass forever while asserting nothing — the failure mode that makes a
/// green suite misleading.
#[test]
fn corpus_emits_every_language_and_entity() {
    let emitted = emit_corpus();
    assert!(!emitted.is_empty(), "emit_corpus produced nothing");
    for needle in [
        ".nros.rs",
        ".h",
        ".c",
        ".hpp", // languages
        "Probe.srv",
        "Probe.action",
        "Shapes",
        "Nested", // entities
        "inline/",
        "configured/", // storage-mode variants
    ] {
        assert!(
            emitted.keys().any(|k| k.contains(needle)),
            "corpus emits nothing matching {needle:?}; keys: {:?}",
            emitted.keys().take(8).collect::<Vec<_>>()
        );
    }
    for (k, v) in &emitted {
        assert!(!v.trim().is_empty(), "{k} emitted an empty artifact");
    }
}
