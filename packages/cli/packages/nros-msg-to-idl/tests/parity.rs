//! Phase 212.O.7 — parity test: the Rust `nros-msg-to-idl` port must
//! produce IDL byte-identical to the retired python
//! `scripts/cyclonedds/msg_to_cyclone_idl.py` for the supported `.msg`
//! subset. Pins the port across future edits to the Rust crate.
//!
//! Method (α — saved expected files): every `<pkg>__<Name>.msg` fixture
//! under `tests/fixtures/parity/` has a sibling `<pkg>__<Name>.idl`
//! captured from the python script (rosidl_adapter Humble +
//! `msg_to_cyclone_idl.py`). The test reads both, runs the Rust port,
//! and `assert_eq!`s the output.
//!
//! No normalisation is applied — the Rust port targets byte-identical
//! output and the python script is deterministic, so any divergence is
//! a real port-level bug.
//!
//! Known divergences flagged by Phase 212.O.7 (NOT fixed here):
//!
//! - **Non-ASCII comment bytes** (`KNOWN_DIVERGENCE_NON_ASCII`): the
//!   python passes each `# … <comment>` through
//!   `s.encode().decode('unicode_escape')`, which re-interprets the
//!   UTF-8 bytes of any non-ASCII character as separate Latin-1
//!   codepoints (e.g. `—` U+2014 → `â\x80\x94`). The Rust port's
//!   `idl_string_literal` documents this as a noted limitation and
//!   passes the comment through unchanged — output divergence is real
//!   for `.msg` files containing non-ASCII comment bytes. The corpus
//!   fixtures avoid non-ASCII to keep the parity test clean; a fixture
//!   that exercises the divergence is left for a future task that
//!   chooses how to reconcile (port `unicode_escape` into Rust, or
//!   drop the python pass and assert ASCII-only comments upstream).
//!
//! Out-of-scope (documented divergences, see `srv_and_action_*` tests):
//!
//! - `.srv` files: the python script emits a single combined IDL with
//!   two `module dds_` blocks under one `module srv`. The Rust port
//!   has no `.srv` emitter — callers split on `---` and run the
//!   per-half through `Converter::with_service_header(true)`. The
//!   `.srv` fixtures here keep python's combined output as a contract
//!   reference for a future srv-shape emitter and check that each half
//!   parses through the per-half Converter without error.
//! - `.action` files: the python script synthesises the eight
//!   SendGoal / GetResult / FeedbackMessage structs from the three
//!   action sections (no rosidl `action2idl` exists). The Rust port
//!   has no action synthesiser. The `.action` fixture keeps python's
//!   synthesised output as a contract reference and checks that each
//!   section parses through the per-section Converter without error.

use std::{fs, path::PathBuf};

use nros_msg_to_idl::Converter;

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/parity")
}

#[derive(Debug)]
struct MsgFixture {
    package: String,
    message: String,
    msg_path: PathBuf,
    idl_path: PathBuf,
}

fn collect_msg_fixtures() -> Vec<MsgFixture> {
    let fixtures_dir = fixtures_dir();
    let mut out = Vec::new();
    for entry in fs::read_dir(&fixtures_dir).expect("fixtures dir present") {
        let entry = entry.expect("readdir entry");
        let path = entry.path();
        let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        if path.extension().and_then(|s| s.to_str()) != Some("msg") {
            continue;
        }
        let (package, message) = stem
            .split_once("__")
            .expect("fixture name must follow <pkg>__<Msg> shape");
        let idl_path = fixtures_dir.join(format!("{stem}.idl"));
        assert!(
            idl_path.is_file(),
            "missing expected python output {}",
            idl_path.display()
        );
        out.push(MsgFixture {
            package: package.to_string(),
            message: message.to_string(),
            msg_path: path,
            idl_path,
        });
    }
    out.sort_by(|a, b| a.msg_path.cmp(&b.msg_path));
    out
}

#[test]
fn at_least_eight_msg_fixtures_present() {
    let n = collect_msg_fixtures().len();
    assert!(
        n >= 8,
        "task requires ≥8 .msg parity fixtures under tests/fixtures/parity/, found {n}"
    );
}

#[test]
fn msg_to_cyclone_idl_rust_port_matches_python_output() {
    let fixtures = collect_msg_fixtures();
    assert!(!fixtures.is_empty(), "no parity fixtures found");

    let mut failures: Vec<String> = Vec::new();
    let mut passes = 0;
    for fx in &fixtures {
        let src = fs::read_to_string(&fx.msg_path).expect("read .msg");
        let expected = fs::read_to_string(&fx.idl_path).expect("read .idl");
        let actual = match Converter::new(&fx.package, &fx.message).convert(&src) {
            Ok(s) => s,
            Err(e) => {
                failures.push(format!("{}/{}: convert error: {e}", fx.package, fx.message));
                continue;
            }
        };
        if actual != expected {
            failures.push(diff_summary(&fx.package, &fx.message, &expected, &actual));
        } else {
            passes += 1;
        }
    }

    if !failures.is_empty() {
        panic!(
            "{} of {} fixtures matched; failures:\n{}",
            passes,
            fixtures.len(),
            failures.join("\n--------\n")
        );
    }
    eprintln!("[parity] {passes}/{} msg fixtures matched", fixtures.len());
}

/// `.srv` parity reference. The Rust port has no `.srv` emitter — see
/// the module-level doc comment. This test confirms each half parses
/// through `Converter::with_service_header(true)` without error and
/// that the saved python `<stem>.srv.expected.idl` exists as the
/// contract reference for a future srv-shape emitter.
#[test]
fn srv_per_half_converts_and_python_reference_present() {
    let dir = fixtures_dir();
    let mut count = 0;
    for entry in fs::read_dir(&dir).expect("fixtures dir present") {
        let path = entry.expect("entry").path();
        if path.extension().and_then(|s| s.to_str()) != Some("srv") {
            continue;
        }
        count += 1;
        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap();
        let (package, message) = stem.split_once("__").expect("<pkg>__<Name> shape");

        let expected = dir.join(format!("{stem}.srv.expected.idl"));
        assert!(
            expected.is_file(),
            "missing python reference {}",
            expected.display()
        );

        let src = fs::read_to_string(&path).expect("read .srv");
        let (req_body, res_body) = src
            .split_once("\n---\n")
            .expect("srv must contain `---` separator");

        // Per-half conversion sanity-check — the public API the
        // Rust port supports for a `.srv`.
        let req_msg = format!("{}_Request", message);
        let res_msg = format!("{}_Response", message);
        Converter::new(package, &req_msg)
            .with_service_header(true)
            .convert(req_body)
            .unwrap_or_else(|e| panic!("{}/{} request convert: {e}", package, req_msg));
        Converter::new(package, &res_msg)
            .with_service_header(true)
            .convert(res_body)
            .unwrap_or_else(|e| panic!("{}/{} response convert: {e}", package, res_msg));
    }
    assert!(count >= 1, "expected ≥1 .srv parity fixture");
}

/// `.action` parity reference. The Rust port has no action
/// synthesiser — see the module-level doc comment. This test confirms
/// each section parses through the per-section Converter without
/// error and that the saved python `<stem>.action.expected.idl`
/// exists as the contract reference for a future synthesiser.
#[test]
fn action_per_section_converts_and_python_reference_present() {
    let dir = fixtures_dir();
    let mut count = 0;
    for entry in fs::read_dir(&dir).expect("fixtures dir present") {
        let path = entry.expect("entry").path();
        if path.extension().and_then(|s| s.to_str()) != Some("action") {
            continue;
        }
        count += 1;
        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap();
        let (package, message) = stem.split_once("__").expect("<pkg>__<Name> shape");

        let expected = dir.join(format!("{stem}.action.expected.idl"));
        assert!(
            expected.is_file(),
            "missing python reference {}",
            expected.display()
        );

        let src = fs::read_to_string(&path).expect("read .action");
        let sections: Vec<&str> = src.split("\n---\n").collect();
        assert_eq!(
            sections.len(),
            3,
            "action must have 3 sections (goal/result/feedback)"
        );
        for (section, suffix) in sections.iter().zip(["_Goal", "_Result", "_Feedback"]) {
            let msg = format!("{message}{suffix}");
            Converter::new(package, &msg)
                .convert(section)
                .unwrap_or_else(|e| panic!("{}/{} section convert: {e}", package, msg));
        }
    }
    assert!(count >= 1, "expected ≥1 .action parity fixture");
}

fn diff_summary(pkg: &str, msg: &str, expected: &str, actual: &str) -> String {
    let mut out = format!("{pkg}/{msg}: byte-mismatch.\n");
    let e_lines: Vec<&str> = expected.split('\n').collect();
    let a_lines: Vec<&str> = actual.split('\n').collect();
    let n = e_lines.len().max(a_lines.len());
    for i in 0..n {
        let el = e_lines.get(i).copied().unwrap_or("<eof>");
        let al = a_lines.get(i).copied().unwrap_or("<eof>");
        if el != al {
            out.push_str(&format!(
                "  line {}:\n    expected: {el:?}\n    actual:   {al:?}\n",
                i + 1
            ));
        }
    }
    out
}
