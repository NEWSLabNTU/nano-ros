//! Phase 212 — Non-Goals CI grep.
//!
//! Phase doc §Non-Goals lists user-surfaces that re-create the
//! colcon-shaped anti-pattern. CI grep guards three rejected verbs
//! that have never been (or were retracted from) the top-level `nros`
//! command tree:
//!
//! * `nros emit` — top-level emit verb retracted in 212.G
//!   (`01d2ddff8`); users hand-write `package.xml`, and bringup
//!   `<exec_depend>` drift is caught by `nros check --bringup`.
//! * `nros sign` — never existed; signing is out-of-scope.
//! * `nros flash` — never existed at the top level; flashing happens
//!   inside `nros run` / `nros deploy` orchestration.
//!
//! ## Reconciliation note (build / run / monitor / deploy gray area)
//!
//! Phase doc §Non-Goals also lists `nros build`, `nros test`, and
//! `nros monitor`. Those bullets reject the **build-system
//! orchestrator** shape (colcon-style: owns stdout, swallows
//! root-cause errors, parallel build system to learn). They do NOT
//! reject the Phase 172 WP-A orchestrator surface — `nros build`,
//! `nros run`, `nros monitor`, `nros deploy` already exist as
//! workspace-/deploy-plan-driven verbs gated on `nros.toml`. This
//! test deliberately does NOT grep against `build` / `run` / `monitor`
//! / `deploy` — they are the existing Phase 172 surface, not the
//! rejected build-system surface. Preference: false-negative (allow
//! the gray area through) over false-positive (block legitimate verbs).
//!
//! `nros test` is also not asserted against — it's not present today
//! and the Non-Goals bullet covers it, but the same orchestrator-vs-
//! build-system ambiguity applies if Phase 172 ever grows a `test`
//! verb. Re-tighten if/when policy clarifies.

use std::{path::PathBuf, process::Command};

fn nros_bin() -> Option<PathBuf> {
    // Phase 218: prefer the in-tree CLI built by `just setup-cli`. Fall
    // back to `~/.nros/bin/nros` for users still on the transitional path.
    let in_tree = nros_tests::project_root().join("packages/cli/target/release/nros");
    if in_tree.is_file() {
        return Some(in_tree);
    }
    let home = std::env::var_os("HOME")?;
    let bin = PathBuf::from(home).join(".nros/bin/nros");
    if bin.is_file() { Some(bin) } else { None }
}

fn nros_help() -> String {
    let bin = nros_bin().expect("nros binary checked by caller");
    let out = Command::new(&bin)
        .arg("--help")
        .output()
        .expect("invoke nros --help");
    assert!(
        out.status.success(),
        "nros --help exited non-zero: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).expect("nros --help stdout utf8")
}

/// Extract the lines under the `Commands:` block of `nros --help`.
/// Each command line is `  <verb>   <description...>` (2-space indent
/// from clap). The block ends at the next blank line / `Options:`
/// header.
fn commands_block(help: &str) -> Vec<String> {
    let mut in_block = false;
    let mut out = Vec::new();
    for line in help.lines() {
        if !in_block {
            if line.trim_start().starts_with("Commands:") {
                in_block = true;
            }
            continue;
        }
        if line.trim().is_empty() {
            break;
        }
        if !line.starts_with(' ') {
            break;
        }
        out.push(line.to_string());
    }
    assert!(
        !out.is_empty(),
        "failed to parse Commands: block from nros --help"
    );
    out
}

/// Return the leading verb on a `Commands:` line (first whitespace-
/// separated token of the trimmed line).
fn line_verb(line: &str) -> &str {
    line.trim().split_whitespace().next().unwrap_or("")
}

fn assert_verb_absent(verb: &str) {
    let help = nros_help();
    let block = commands_block(&help);
    let hits: Vec<&String> = block.iter().filter(|l| line_verb(l) == verb).collect();
    assert!(
        hits.is_empty(),
        "Phase 212 §Non-Goals: `nros {verb}` must NOT appear in \
         `nros --help` Commands: block. Found:\n{}",
        hits.iter()
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    );
}

#[test]
fn nros_help_lacks_emit_verb() {
    if nros_bin().is_none() {
        nros_tests::skip!("nros binary missing — run `just setup-cli` + `source ./activate.sh`");
    }
    assert_verb_absent("emit");
}

#[test]
fn nros_help_lacks_sign_verb() {
    if nros_bin().is_none() {
        nros_tests::skip!("nros binary missing — run `just setup-cli` + `source ./activate.sh`");
    }
    assert_verb_absent("sign");
}

#[test]
fn nros_help_lacks_flash_verb() {
    if nros_bin().is_none() {
        nros_tests::skip!("nros binary missing — run `just setup-cli` + `source ./activate.sh`");
    }
    assert_verb_absent("flash");
}

/// Phase 212.A was retracted: the `cargo-nros` cargo subcommand shell
/// added no functional value over the bare `nros` verb (every
/// `cargo nros <verb>` produced byte-identical output to
/// `nros <verb>`), so it was dropped. Guard against accidental
/// re-installation under `~/.nros/bin/cargo-nros`.
#[test]
fn cargo_nros_binary_absent() {
    let Some(home) = std::env::var_os("HOME") else {
        nros_tests::skip!("$HOME unset — cannot probe ~/.nros/bin/");
    };
    let bin = PathBuf::from(home).join(".nros/bin/cargo-nros");
    assert!(
        !bin.exists(),
        "Phase 212.A retracted: cargo-nros must NOT be installed at \
         {}. Drop it from any local install scripts or release packaging.",
        bin.display()
    );
}

#[test]
fn phase_doc_non_goals_lists_emit() {
    let root = nros_tests::project_root();
    // Phase 212 completed → archived. The §Non-Goals lock travels with it.
    let doc =
        root.join("docs/roadmap/archived/phase-212-ux-cargo-native-and-file-consolidation.md");
    let body = std::fs::read_to_string(&doc).expect("read phase 212 doc");
    let (_, after) = body
        .split_once("## Non-Goals")
        .expect("phase 212 doc missing ## Non-Goals section");
    let (non_goals, _) = after
        .split_once("\n## ")
        .expect("phase 212 doc missing terminator after Non-Goals");
    assert!(
        non_goals.contains("nros emit package-xml"),
        "Phase 212 §Non-Goals must list `nros emit package-xml` (212.G \
         retraction). Locks the doc entry against accidental removal."
    );
}
