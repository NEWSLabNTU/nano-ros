//! Every tracked `system.toml`'s `[tiers.*]` must parse through the REAL
//! schema type — being valid TOML is not the property that matters.
//!
//! `[tiers.*]` has TWO parsers: `nros_orchestration_ir::TierDef` (this one,
//! `deny_unknown_fields`) and the resolver's
//! `ros_launch_manifest_sched::TierPlatformSpec`. They are separate mirrors of
//! one block, and when they disagree the NARROWER one silently defines what a
//! user may write — the failure mode issue 0380 was filed for, and the one the
//! rlm v0.1.11 key rename reproduced: rlm renamed `spin_period_us` →
//! `spin_period` (keeping the old name as a serde alias so it can retire it),
//! the resolver accepted the documented new spelling, and this parser rejected
//! it with `unknown field`. Nothing caught that until a fixture was migrated,
//! because every other test either constructs the structs directly or reads
//! keys off a `toml::Value` — neither of which exercises this schema.
//!
//! Buildless and cheap: it reads tracked files and deserializes them.

use std::{collections::BTreeMap, path::PathBuf, process::Command};

#[test]
fn every_tracked_system_toml_tiers_parse() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .expect("repo root above packages/testing/nros-tests")
        .to_path_buf();

    let out = Command::new("git")
        .arg("ls-files")
        .arg("*system.toml")
        .current_dir(&root)
        .output()
        .expect("git ls-files must run");
    assert!(
        out.status.success(),
        "git ls-files failed in {}",
        root.display()
    );
    let files: Vec<String> = String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(|s| s.to_string())
        .collect();
    // Precondition, not decoration: a bad root would list zero files and this
    // test would "pass" having checked nothing.
    assert!(
        files.len() >= 30,
        "expected the tracked system.toml set under {}, got {}",
        root.display(),
        files.len()
    );

    let mut bad = Vec::new();
    let mut with_tiers = 0usize;
    for f in &files {
        let text = std::fs::read_to_string(root.join(f)).expect("tracked file readable");
        let doc: toml::Value = match toml::from_str(&text) {
            Ok(v) => v,
            Err(e) => {
                bad.push(format!("{f}: not valid TOML: {e}"));
                continue;
            }
        };
        let Some(tiers) = doc.get("tiers") else {
            continue;
        };
        with_tiers += 1;
        if let Err(e) = tiers
            .clone()
            .try_into::<BTreeMap<String, nros_orchestration_ir::TierDef>>()
        {
            bad.push(format!(
                "{f}: [tiers.*] rejected by nros_orchestration_ir::TierDef: {e}"
            ));
        }
    }

    // Same reason as the file-count floor: if `[tiers.*]` stopped being written
    // the loop body would never run and this would report success.
    assert!(
        with_tiers >= 7,
        "expected system.toml files carrying [tiers.*], saw {with_tiers}"
    );
    assert!(
        bad.is_empty(),
        "{} system.toml file(s) failed to parse:\n{}",
        bad.len(),
        bad.join("\n")
    );
}
