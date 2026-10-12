//! phase-486 W7 — the capability switches live in the nano-ros overlay
//! (`nros.toml`) and nowhere else, for every tracked `system.toml`.
//!
//! Until rlm `v0.1.55` this test was W1's safety net: the overlay had to state
//! exactly what rlm projected into `execution.features`, because W3 moved the
//! readers from one to the other. From `v0.1.55` rlm projects nothing there
//! (its W6), so the test now asserts that boundary against rlm's OWN parser
//! and projection (`parse_system_config` + `apply_to_launch`): the model
//! carries no switch, and the overlay still carries every bringup's. The
//! lifecycle default is compared with rlm's `lifecycle_autostart()` while that
//! accessor exists (deleted at W8).

use std::path::{Path, PathBuf};

// Issue 1659 -- every git the CLI tree spawns clears the inherited git env.
include!("../../../tooling/nros-build-paths/src/git_env.rs");

use nros_orchestration_ir::overlay::Overlay;
use ros_launch_manifest_model::{Autostart, Execution, system_config::parse_system_config};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .expect("repo root")
        .to_path_buf()
}

/// Tracked `system.toml` files — `git ls-files`, not a walk (issue 1565: a
/// walk counts other worktrees' and submodules' files).
fn tracked_system_tomls(root: &Path) -> Vec<PathBuf> {
    let out = nros_git_command("git")
        .arg("-C")
        .arg(root)
        .args(["ls-files", "-z", "--", "*system.toml"])
        .output()
        .expect("git ls-files");
    assert!(out.status.success(), "git ls-files failed");
    out.stdout
        .split(|b| *b == 0)
        .filter(|s| !s.is_empty())
        .map(|s| root.join(String::from_utf8_lossy(s).as_ref()))
        .collect()
}

fn autostart_name(a: Autostart) -> &'static str {
    match a {
        Autostart::None => "none",
        Autostart::Configure => "configure",
        Autostart::Active => "active",
    }
}

#[test]
fn the_model_carries_no_switch_and_the_overlay_carries_every_bringups() {
    let root = repo_root();
    let files = tracked_system_tomls(&root);
    assert!(
        files.len() > 100,
        "found only {} system.toml files",
        files.len()
    );
    let mut compared = 0usize;
    let mut stating = 0usize;
    let mut disagreements = Vec::new();
    for f in &files {
        let text = std::fs::read_to_string(f).unwrap();
        // A file rlm cannot parse is not a model input either, so there is
        // nothing to agree with; the strict parser reports it elsewhere.
        let Ok(cfg) = parse_system_config(&text) else {
            continue;
        };
        let mut exec = Execution::default();
        // Placement may refuse with no node list; only the features half is
        // compared, and `apply_to_launch` sets it before any placement.
        let _ = cfg.apply_to_launch(&mut exec, &[], None);
        let model_lifecycle = cfg.lifecycle_autostart().map(autostart_name);
        let overlay = Overlay::from_system_toml_str(&text, f).unwrap();
        compared += 1;
        if !overlay.is_empty() {
            stating += 1;
        }
        if !exec.features.is_empty() || overlay.lifecycle_autostart.as_deref() != model_lifecycle {
            disagreements.push(format!(
                "{}: model features {:?} / lifecycle {:?}, overlay {:?} / {:?}",
                f.strip_prefix(&root).unwrap_or(f).display(),
                exec.features,
                model_lifecycle,
                overlay.features,
                overlay.lifecycle_autostart
            ));
        }
    }
    assert!(
        disagreements.is_empty(),
        "the model carries a nano-ros switch, or the lifecycle default disagrees:\n{}",
        disagreements.join("\n")
    );
    // The bringups the W0 census names (features, managed, safety) state
    // something; if none did, this test would compare only empty sets.
    assert!(
        stating >= 3,
        "only {stating} of {compared} state an overlay"
    );
}
