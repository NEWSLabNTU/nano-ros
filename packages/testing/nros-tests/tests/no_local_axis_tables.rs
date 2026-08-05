//! phase-329 W6 — grep-gate against a SECOND matrix-axis table.
//!
//! RFC-0051 declares ONE generated matrix: `matrix::CELLS` (+ `SCHED_CELLS`) and
//! `interop::CELLS`. The pre-295 regression was files hand-defining their OWN
//! `const CELLS` axis table over the same platform×lang×rmw vocabulary, so adding
//! a coordinate in one place silently missed the others (`platform_header_matrix`
//! was the last such second spelling — converted in W5). This gate keeps it from
//! regrowing: no `const`/`static` axis TABLE may live outside the two SSoT files.
//!
//! What it forbids, outside `src/matrix.rs` / `src/interop.rs`:
//!   * a `const`/`static` binding whose NAME contains `CELLS`
//!     (`const FOO_CELLS: …`), and
//!   * a `const`/`static` binding TYPED as an array of a `*Cell` element
//!     (`const CASES: &[FooCell] = …`) — the axis-table shape even under a
//!     different name.
//!
//! What it ALLOWS: a local `struct Cell { … }` used as per-case EXECUTION DATA
//! built dynamically (the W1 `Exec`/`exec_for` pattern; `interop_e2e` and
//! `zephyr` do this) — that is a consumer's private data, not a coordinate table.

use std::path::{Path, PathBuf};

fn crate_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// The two files that ARE the sanctioned axis tables.
const SSOT: &[&str] = &["matrix.rs", "interop.rs"];

fn is_ssot(path: &Path) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .map(|n| SSOT.contains(&n))
        .unwrap_or(false)
}

/// Scan one `.rs` file; return offending `(line_no, line)` for any axis-table decl.
fn offenders_in(text: &str) -> Vec<(usize, String)> {
    let mut out = Vec::new();
    for (i, raw) in text.lines().enumerate() {
        let t = raw.trim_start();
        if t.starts_with("//") {
            continue;
        }
        let decl = t
            .trim_start_matches("pub ")
            .trim_start_matches("pub(crate) ");
        let is_const_static = decl.starts_with("const ") || decl.starts_with("static ");
        if !is_const_static {
            continue;
        }
        // Split name : type at the first ':'.
        let Some(colon) = decl.find(':') else {
            continue;
        };
        let name = &decl[..colon];
        let ty = &decl[colon + 1..];
        // (a) name ends/contains CELLS.
        if name.contains("CELLS") {
            out.push((i + 1, raw.trim().to_string()));
            continue;
        }
        // (b) typed as an array of a `*Cell` element.
        if ty.contains("&[") && ty.contains("Cell") {
            out.push((i + 1, raw.trim().to_string()));
        }
    }
    out
}

/// Recursively collect `.rs` files under a dir.
fn rs_files(dir: &Path, acc: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            if p.file_name().and_then(|n| n.to_str()) != Some("target") {
                rs_files(&p, acc);
            }
        } else if p.extension().and_then(|x| x.to_str()) == Some("rs") {
            acc.push(p);
        }
    }
}

#[test]
fn no_matrix_axis_table_outside_matrix_and_interop() {
    let root = crate_root();
    let mut files = Vec::new();
    rs_files(&root.join("src"), &mut files);
    rs_files(&root.join("tests"), &mut files);

    let mut violations = Vec::new();
    for f in &files {
        // The SSoT files and THIS gate (which names the patterns in strings) are exempt.
        if is_ssot(f) || f.file_name().and_then(|n| n.to_str()) == Some("no_local_axis_tables.rs") {
            continue;
        }
        for (line_no, line) in offenders_in(&std::fs::read_to_string(f).unwrap_or_default()) {
            let rel = f.strip_prefix(&root).unwrap_or(f);
            violations.push(format!("  {}:{}  {}", rel.display(), line_no, line));
        }
    }

    assert!(
        violations.is_empty(),
        "phase-329 W6: {} matrix-axis table(s) defined OUTSIDE src/matrix.rs / \
         src/interop.rs — the RFC-0051 single-matrix rule forbids a second spelling. \
         Move the coordinates into `matrix::CELLS`/`SCHED_CELLS`/`interop::CELLS` and \
         derive the case list from there (a per-case exec struct built dynamically is \
         fine; a `const`/`static` coordinate table is not):\n{}",
        violations.len(),
        violations.join("\n"),
    );
}
