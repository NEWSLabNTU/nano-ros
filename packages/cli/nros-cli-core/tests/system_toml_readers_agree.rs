//! phase-486 W8a — the two readers of a `system.toml` agree on every key they
//! share, and only nano-ros's carries nano-ros's facts.
//!
//! Two readers parse the same file:
//! - rlm's `parse_system_config`, which the launch-model resolver uses. It is
//!   generic and LAX: it declares the shared tables and ignores the rest by
//!   design.
//! - nano-ros's `SystemToml`, which is STRICT over the whole file.
//!
//! Issue 0293 was the two drifting: a key one reader dropped meant the other
//! placed nodes differently. The W8 revision replaced the planned double
//! parse (shared keys through rlm's struct) with this test, because rlm's
//! struct is rlm's view of the file and not nano-ros's schema. So for every
//! tracked `system.toml`, each SHARED key must mean the same to both:
//! `[system] rmw / domain_id / locator`, `[[component]]` identity, tier
//! binding and parameters, `[[bridge]]`, `[host.*]`, `[tiers.*]`.
//!
//! It also keeps W7's boundary: rlm's own projection (`apply_to_launch`) puts
//! no capability switch into the model, and the overlay carries every
//! bringup's. That half is asked of the SERIALISED execution layer, so it
//! holds whether or not rlm still declares the retired `features` field.

use std::path::{Path, PathBuf};

// Issue 1659 -- every git the CLI tree spawns clears the inherited git env.
include!("../../../tooling/nros-build-paths/src/git_env.rs");

use nros_cli_core::orchestration::cargo_metadata_schema::SystemToml;
use nros_orchestration_ir::overlay::{Overlay, legacy_projected_features};
use ros_launch_manifest_model::{Execution, system_config::parse_system_config};

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

/// One reader's view of the shared keys, in a form both can produce.
#[derive(Debug, PartialEq)]
struct Shared {
    rmw: Option<String>,
    domain_id: Option<u32>,
    locator: Option<String>,
    /// name, pkg, class, group_tiers, params, params_files
    components: Vec<String>,
    bridges: Vec<String>,
    hosts: Vec<String>,
    /// name, class, deadline_policy, then spin_period / period / budget /
    /// deadline in µs
    tiers: Vec<String>,
}

fn rlm_view(text: &str) -> Result<Shared, String> {
    let c = parse_system_config(text).map_err(|e| e.to_string())?;
    let us = |d: Option<ros_launch_manifest_types::duration::Duration>| d.map(|d| d.as_micros());
    Ok(Shared {
        rmw: c.system.rmw.clone(),
        domain_id: c.system.domain_id,
        locator: c.system.locator.clone(),
        components: c
            .components
            .iter()
            .map(|b| {
                format!(
                    "{:?} {:?} {:?} {:?} {:?} {:?}",
                    b.name, b.pkg, b.class, b.group_tiers, b.params, b.params_files
                )
            })
            .collect(),
        bridges: c
            .bridges
            .iter()
            .map(|b| {
                format!(
                    "{} {} {} {:?} {}",
                    b.name, b.from, b.to, b.topics, b.bidirectional
                )
            })
            .collect(),
        hosts: c.host.iter().map(|(k, h)| format!("{k} {h:?}")).collect(),
        tiers: c
            .tiers
            .iter()
            .map(|(k, t)| {
                format!(
                    "{k} {:?} {:?} {:?} {:?} {:?} {:?}",
                    t.class,
                    t.deadline_policy,
                    us(t.spin_period),
                    us(t.period),
                    us(t.budget),
                    us(t.deadline)
                )
            })
            .collect(),
    })
}

fn nros_view(text: &str) -> Result<Shared, String> {
    let s: SystemToml = toml::from_str(text).map_err(|e| e.to_string())?;
    Ok(Shared {
        // nano-ros requires `rmw` and `domain_id`; rlm reads them as optional.
        // An empty `rmw` is nano-ros's "not stated".
        rmw: Some(s.system.rmw.clone()).filter(|r| !r.is_empty()),
        domain_id: Some(s.system.domain_id),
        locator: s.system.locator.clone(),
        components: s
            .components
            .iter()
            .map(|b| {
                format!(
                    "{:?} {:?} {:?} {:?} {:?} {:?}",
                    Some(&b.name),
                    Some(&b.pkg),
                    Some(&b.class),
                    b.group_tiers,
                    b.params,
                    b.params_files
                )
            })
            .collect(),
        bridges: s
            .bridges
            .iter()
            .map(|b| {
                format!(
                    "{} {} {} {:?} {}",
                    b.name, b.from, b.to, b.topics, b.bidirectional
                )
            })
            .collect(),
        hosts: s.host.iter().map(|(k, h)| format!("{k} {h:?}")).collect(),
        tiers: s
            .tiers
            .iter()
            .map(|(k, t)| {
                format!(
                    "{k} {:?} {:?} {:?} {:?} {:?} {:?}",
                    t.class,
                    t.deadline_policy,
                    t.spin_period_us,
                    t.period_us,
                    t.budget_us,
                    t.deadline_us
                )
            })
            .collect(),
    })
}

#[test]
fn both_readers_mean_the_same_by_every_shared_key() {
    let root = repo_root();
    let files = tracked_system_tomls(&root);
    assert!(
        files.len() > 100,
        "found only {} system.toml files",
        files.len()
    );
    let mut compared = 0usize;
    let mut with_tiers = 0usize;
    let mut with_components = 0usize;
    let mut disagreements = Vec::new();
    for f in &files {
        let text = std::fs::read_to_string(f).unwrap();
        let rel = f.strip_prefix(&root).unwrap_or(f).display().to_string();
        // A file nano-ros refuses is reported by its own parse elsewhere; a
        // file only nano-ros parses is a reader disagreement.
        let Ok(nros) = nros_view(&text) else {
            continue;
        };
        let rlm = match rlm_view(&text) {
            Ok(v) => v,
            Err(e) => {
                disagreements.push(format!("{rel}: nano-ros parses it, rlm refuses: {e}"));
                continue;
            }
        };
        compared += 1;
        with_tiers += usize::from(!nros.tiers.is_empty());
        with_components += usize::from(!nros.components.is_empty());
        if nros != rlm {
            disagreements.push(format!("{rel}:\n  nano-ros {nros:?}\n  rlm      {rlm:?}"));
        }
    }
    assert!(
        disagreements.is_empty(),
        "the two system.toml readers disagree on a shared key:\n{}",
        disagreements.join("\n")
    );
    // Not a comparison of empty sets: the tree exercises the tables compared.
    assert!(compared > 100, "compared only {compared} files");
    assert!(with_tiers >= 5, "only {with_tiers} files declare tiers");
    assert!(
        with_components >= 50,
        "only {with_components} files declare components"
    );
}

/// The comparison sees a drift of the kind issue 0293 was: a shared key that
/// reaches one reader and not the other.
#[test]
fn a_shared_key_one_reader_drops_is_a_disagreement() {
    let base = "[system]\nname = \"s\"\nrmw = \"zenoh\"\ndomain_id = 3\n\
                [[component]]\npkg = \"p\"\nclass = \"p::C\"\nname = \"c\"\n\
                params = { rate = 5 }\n";
    assert_eq!(nros_view(base).unwrap(), rlm_view(base).unwrap());
    let mut dropped = nros_view(base).unwrap();
    dropped.components[0] = dropped.components[0].replace("\"rate\"", "\"r\"");
    assert_ne!(dropped, rlm_view(base).unwrap());
}

#[test]
fn the_model_carries_no_switch_and_the_overlay_carries_every_bringups() {
    let root = repo_root();
    let mut stating = 0usize;
    let mut carriers = Vec::new();
    for f in &tracked_system_tomls(&root) {
        let text = std::fs::read_to_string(f).unwrap();
        let Ok(cfg) = parse_system_config(&text) else {
            continue;
        };
        let mut exec = Execution::default();
        // Placement may refuse with no node list; what is asked here is only
        // whether the projection wrote a switch.
        let _ = cfg.apply_to_launch(&mut exec, &[], None);
        let execution: String = serde_yaml_ng::to_string(&exec)
            .unwrap()
            .lines()
            .map(|l| format!("  {l}\n"))
            .collect();
        if !legacy_projected_features(&format!("execution:\n{execution}")).is_empty() {
            carriers.push(f.strip_prefix(&root).unwrap_or(f).display().to_string());
        }
        if !Overlay::from_system_toml_str(&text, f).unwrap().is_empty() {
            stating += 1;
        }
    }
    assert!(
        carriers.is_empty(),
        "rlm projected a capability switch into the model for:\n{}",
        carriers.join("\n")
    );
    // The bringups the W0 census names (features, managed, safety) state
    // something; if none did, this test would compare only empty sets.
    assert!(stating >= 3, "only {stating} bringups state an overlay");
}
