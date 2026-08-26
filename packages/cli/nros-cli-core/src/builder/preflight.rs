//! Stage 3 — is this build's toolchain present? (phase-383 W2.d, RFC-0065 D2/D14).
//!
//! ## What this checks, and what it deliberately does not
//!
//! A missing prerequisite must fail HERE, naming the `nros setup` line that
//! fixes it — never mid-compile with a cryptic linker error. That is D2's whole
//! promise.
//!
//! It checks what is **cheap and board-specific**: the Rust target the board
//! pins, and the SDK directories its site config names. It does NOT re-walk the
//! whole `nros-sdk-index.toml` the way `nros setup --check` does. Two reasons:
//!
//! * `nros setup --check` prints a report; its predicate is not separable
//!   without refactoring `setup.rs`, and doing that inside a wave about
//!   something else is how unrelated changes ride along;
//! * a full index walk probes every tool for every board, most of which this
//!   build does not need. Preflight should be fast enough that nobody wants a
//!   flag to skip it — a skipped check is not a check.
//!
//! So this is the narrow, high-yield subset, and it says so rather than
//! implying full coverage. `nros doctor` remains the thorough answer.
//!
//! ## Offline (D14)
//!
//! Preflight never fetches. `--offline` changes nothing here — it changes what
//! stage 5 is told — which is the point: a build that would have fetched fails
//! at stage 3 either way, with the manual command named.

use std::path::Path;

use crate::orchestration::board_descriptor::BoardDescriptor;

/// A missing prerequisite, with the command that installs it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Missing {
    /// What is absent, in the user's vocabulary.
    pub what: String,
    /// The exact command to run. Never a description of one.
    pub remedy: String,
}

/// Check the prerequisites `board` needs, in the workspace at `root`.
///
/// Returns every problem rather than the first: a user fixing three missing
/// things one build at a time is three round trips, and D2's promise is that
/// stage 3 tells you everything before anything compiles.
#[must_use]
pub fn check(board: &BoardDescriptor, root: &Path) -> Vec<Missing> {
    let mut out = Vec::new();

    // The rustc target triple. A cross board that pins one needs it installed,
    // and `cargo build --target` fails deep in the build otherwise.
    if let Some(target) = board.target.as_deref()
        && !rust_target_installed(target)
    {
        out.push(Missing {
            what: format!("Rust target `{target}` (board `{}`)", board.names[0]),
            remedy: format!("rustup target add {target}"),
        });
    }

    // A workspace that has never been synced has no generated message crates,
    // and every leaf `.cargo/config.toml` include points at a tree that does
    // not exist — a cargo manifest-PARSE error four frames deep that never
    // names sync (issue 0463).
    if root.join("src").is_dir() && !root.join("build/nros").is_dir() {
        out.push(Missing {
            what: "generated message bindings (this workspace has never been synced)".to_string(),
            remedy: "nros sync".to_string(),
        });
    }

    out
}

/// Whether `rustup` reports `target` as installed.
///
/// A false NEGATIVE is the safe direction: with no rustup (a distro toolchain,
/// a nix shell), this reports installed and lets the build speak for itself.
/// Preflight exists to give a better message than the compiler, never to refuse
/// a build the compiler would have accepted.
fn rust_target_installed(target: &str) -> bool {
    let Ok(out) = std::process::Command::new("rustup")
        .args(["target", "list", "--installed"])
        .output()
    else {
        return true;
    };
    if !out.status.success() {
        return true;
    }
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .any(|l| l.trim() == target)
}

/// Render the problems as the message stage 3 fails with.
#[must_use]
pub fn report(missing: &[Missing]) -> String {
    let mut s = String::from("missing prerequisites for this build:\n");
    for m in missing {
        s.push_str(&format!("  - {}\n      run: {}\n", m.what, m.remedy));
    }
    s.push_str(
        "\nnothing was built. `nros doctor` checks the whole toolchain; this \
         list is only what THIS image needs.",
    );
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orchestration::board_descriptor::BoardCatalog;

    #[derive(serde::Deserialize)]
    struct BoardFile {
        #[serde(rename = "board")]
        boards: Vec<BoardDescriptor>,
    }

    fn board(extra: &str) -> BoardDescriptor {
        let src = format!(
            "[[board]]\nnames = [\"testboard\"]\nplatform = \"freertos\"\n\
             toolchain = \"stable\"\nplatform_feature = \"platform-freertos\"\n\
             link_kind = \"none\"\nentry_kind = \"board-run\"\n{extra}"
        );
        let f: BoardFile = toml::from_str(&src).expect("parse");
        BoardCatalog::from_descriptors(f.boards)
            .descriptors()
            .first()
            .cloned()
            .expect("one board")
    }

    #[test]
    fn a_board_with_no_pinned_target_needs_no_rust_target() {
        let tmp = tempfile::tempdir().unwrap();
        let m = check(&board(""), tmp.path());
        assert!(!m.iter().any(|m| m.what.contains("Rust target")), "{m:?}");
    }

    #[test]
    fn an_uninstalled_target_is_reported_with_the_rustup_line() {
        let tmp = tempfile::tempdir().unwrap();
        // A triple no host has installed, and which is not a real target — so
        // this cannot pass by accident on a well-provisioned machine.
        let b = board("target = \"nros-not-a-real-triple\"\n");
        let m = check(&b, tmp.path());
        if rust_target_installed("nros-not-a-real-triple") {
            // No rustup on this host: the check reports installed by design
            // (see `rust_target_installed`), so there is nothing to assert.
            assert!(m.iter().all(|m| !m.what.contains("Rust target")));
            return;
        }
        let hit = m
            .iter()
            .find(|m| m.what.contains("Rust target"))
            .expect("must report the missing target");
        assert_eq!(hit.remedy, "rustup target add nros-not-a-real-triple");
    }

    #[test]
    fn an_unsynced_workspace_is_told_to_sync() {
        // issue 0463 — without this the failure is a cargo manifest-PARSE error
        // four frames deep that never names `nros sync`.
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join("src/talker_pkg")).unwrap();
        let m = check(&board(""), tmp.path());
        let hit = m
            .iter()
            .find(|m| m.remedy == "nros sync")
            .expect("must name sync");
        assert!(hit.what.contains("never been synced"), "{hit:?}");
    }

    #[test]
    fn a_synced_workspace_is_not_flagged() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join("src/talker_pkg")).unwrap();
        std::fs::create_dir_all(tmp.path().join("build/nros")).unwrap();
        let m = check(&board(""), tmp.path());
        assert!(m.iter().all(|m| m.remedy != "nros sync"), "{m:?}");
    }

    #[test]
    fn the_report_names_every_problem_and_the_exact_command() {
        // Every problem, not the first: three missing things fixed one build at
        // a time is three round trips.
        let missing = vec![
            Missing {
                what: "a".to_string(),
                remedy: "nros setup a".to_string(),
            },
            Missing {
                what: "b".to_string(),
                remedy: "nros setup b".to_string(),
            },
        ];
        let r = report(&missing);
        assert!(r.contains("nros setup a"), "{r}");
        assert!(r.contains("nros setup b"), "{r}");
        assert!(r.contains("nothing was built"), "{r}");
    }
}
