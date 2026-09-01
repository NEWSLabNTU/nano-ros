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

/// The COMMITTED goldens must agree across languages about one type's bound.
///
/// This is the check whose absence let issue 0896's own regression ship. The C
/// and C++ packs share one derivation (`generator::common::derive_message_bound`)
/// precisely so a bound cannot differ by language — the sizes-header mirror
/// class, 0088 → 0114 → 0122 → 0123 → 0245 → 0268, wearing a language axis. But
/// nothing compared the two GOLDEN FILES:
///
/// * `message_size_bound_parity.rs` cross-checks C, C++ and Rust — by
///   GENERATING headers. It passed, because the generator was right.
/// * `generated_output_matches_the_committed_golden` compares generated against
///   committed, per file. It failed, correctly — but only in `cli-tests`, which
///   is in `check-build` and not on the required set, so the red landed on main
///   and sat there.
///
/// So the C++ goldens were captured before the emitter learned to tell RX from
/// TX and never re-captured: three of them stated `RX == TX` while the C golden
/// beside them said `RX == TX + 3`. `Bounded.msg` exists to make exactly that
/// visible — "the XCDR1/XCDR2 numbers must DIFFER here, so a regression that
/// emits one value for both is visible in the golden diff" — and the golden
/// diff did show it. Nothing was comparing the pair.
///
/// Reading the committed files rather than the emitter is the point: a stale
/// capture is invisible to any check that regenerates first.
#[test]
fn the_committed_c_and_cpp_goldens_state_the_same_bound() {
    /// `..._TX_MAX_SERIALIZED_SIZE 133` in a C header → ("TX", 133).
    fn c_bounds(src: &str) -> Vec<(String, String)> {
        src.lines()
            .filter_map(|l| {
                let (head, val) = l.rsplit_once(' ')?;
                let which = if head.ends_with("_TX_MAX_SERIALIZED_SIZE") {
                    "TX"
                } else if head.ends_with("_RX_MAX_SERIALIZED_SIZE") {
                    "RX"
                } else {
                    return None;
                };
                if !head.starts_with("#define") {
                    return None;
                }
                val.trim().parse::<usize>().ok()?;
                Some((which.to_string(), val.trim().to_string()))
            })
            .collect()
    }

    /// `static constexpr size_t RX_MAX_SERIALIZED_SIZE = 136;` → ("RX", 136).
    fn cpp_bounds(src: &str) -> Vec<(String, String)> {
        src.lines()
            .filter_map(|l| {
                let l = l.trim();
                let rest = l.strip_prefix("static constexpr size_t ")?;
                let (name, val) = rest.split_once(" = ")?;
                let which = match name {
                    "TX_MAX_SERIALIZED_SIZE" => "TX",
                    "RX_MAX_SERIALIZED_SIZE" => "RX",
                    _ => return None,
                };
                let val = val.trim_end_matches(';').trim();
                val.parse::<usize>().ok()?;
                Some((which.to_string(), val.to_string()))
            })
            .collect()
    }

    let mut compared = 0;
    let mut problems = Vec::new();
    for h in walk(&golden_dir()).expect("walk goldens") {
        if h.extension().and_then(|e| e.to_str()) != Some("h") {
            continue;
        }
        let hpp = h.with_extension("hpp");
        if !hpp.exists() {
            continue;
        }
        let (c_src, cpp_src) = (
            std::fs::read_to_string(&h).expect("read C golden"),
            std::fs::read_to_string(&hpp).expect("read C++ golden"),
        );
        let cpp = cpp_bounds(&cpp_src);
        for (which, c_val) in c_bounds(&c_src) {
            let Some((_, cpp_val)) = cpp.iter().find(|(w, _)| *w == which) else {
                problems.push(format!(
                    "{}: C states {which}_MAX_SERIALIZED_SIZE {c_val}, the C++ golden states none",
                    h.file_name().unwrap().to_string_lossy()
                ));
                continue;
            };
            compared += 1;
            if *cpp_val != c_val {
                problems.push(format!(
                    "{}: {which}_MAX_SERIALIZED_SIZE — C golden {c_val}, C++ golden {cpp_val}",
                    h.file_name().unwrap().to_string_lossy()
                ));
            }
        }
    }

    assert!(
        compared > 0,
        "compared no bounds at all — the corpus lost its bounded types, or the \
         patterns stopped matching. A cross-check that silently examines nothing \
         is the vacuous-test shape this repo gates against."
    );
    assert!(
        problems.is_empty(),
        "the committed goldens disagree across languages about a bound derived \
         in ONE place:\n  {}\n\nRegenerate with NROS_UPDATE_GOLDEN=1 and read the \
         diff: if the two languages really do differ, the shared derivation is \
         broken, not the golden.",
        problems.join("\n  ")
    );
}
