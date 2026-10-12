//! phase-486 W1, revised at W7 -- the nano-ros overlay (`nros.toml`) carries
//! the capability switches the SystemModel no longer does, for every tracked
//! `system.toml`.
//!
//! Until W7 this was W3's safety net: the overlay had to state exactly the
//! `execution.features` rlm projected, so moving the readers to it was
//! byte-identical. rlm v0.1.55 (phase-486 W6) stopped writing
//! `execution.features`, so at W7 the test asserts the hand-over instead: the
//! model's projection is empty for every file, and the switches are still
//! stated, by the overlay. The lifecycle default is still compared with rlm's
//! `lifecycle_autostart()`, which stays until W8. The comparison is against
//! rlm's OWN parser and projection (`parse_system_config` + `apply_to_launch` +
//! `lifecycle_autostart`), never a second spelling of them.

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
fn overlay_agrees_with_the_model_projection_for_every_tracked_system_toml() {
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
        "the model still carries switches, or the lifecycle defaults disagree:\n{}",
        disagreements.join("\n")
    );
    // The bringups the W0 census names (features, managed, safety) state
    // something; if none did, the overlay would have lost the switches the
    // model dropped, and this test would compare only empty sets.
    assert!(
        stating >= 3,
        "only {stating} of {compared} state an overlay"
    );
}
