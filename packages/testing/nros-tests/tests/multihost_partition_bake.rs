//! Phase 211.F — per-host partition bake: one multi-host launch
//! (`<node machine="…">`) bakes a per-host Rust Entry, each carrying only its
//! host's nodes (plus any unhosted/shared node).
//!
//! Drives `nros codegen entry --lang rust --host <id>` over the
//! `examples/workspaces/rust` workspace + `demo_bringup/launch/multihost.launch.xml`
//! (talker on `robot1`, listener on `robot2`) and asserts the emitted `main.rs`
//! source registers ONLY that host's node. Complements the unit-level
//! `Plan::for_host` test (nros-cli-core) by exercising the full CLI pipeline:
//! launch parse (`machine=` attr) → `PlanNode.host` → `for_host` filter →
//! `emit_rust`.
//!
//! Cross-process *delivery* between hosts is already proven by
//! `deployed_native_system_e2e` (a planned deploy publishes to the ROS graph; a
//! separate process receives) — this seals the remaining piece, the bake
//! partition.

use std::process::Command;

fn codegen_entry_host(host: &str, out: &std::path::Path) -> String {
    let nros = nros_tests::nros_cli_bin_path().expect("nros CLI (require_nros_cli gated this)");
    let workspace = nros_tests::project_root().join("examples/workspaces/rust");
    let status = Command::new(&nros)
        .args(["codegen", "entry", "--lang", "rust"])
        .arg("--workspace")
        .arg(&workspace)
        .args(["--launch", "demo_bringup:multihost.launch.xml"])
        .args(["--host", host])
        .arg("--out")
        .arg(out)
        .output()
        .expect("spawn nros codegen entry");
    assert!(
        status.status.success(),
        "`nros codegen entry --host {host}` failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&status.stdout),
        String::from_utf8_lossy(&status.stderr),
    );
    std::fs::read_to_string(out).expect("read generated entry source")
}

#[test]
fn multihost_launch_bakes_per_host_entries() {
    if !nros_tests::require_nros_cli() {
        nros_tests::skip!("nros CLI not found");
    }
    // Older CLI without the Phase-211.F `--host` flag → skip rather than misreport.
    let nros = nros_tests::nros_cli_bin_path().unwrap();
    let help = Command::new(&nros)
        .args(["codegen", "entry", "--help"])
        .output()
        .expect("nros codegen entry --help");
    if !String::from_utf8_lossy(&help.stdout).contains("--host") {
        nros_tests::skip!("installed nros lacks `codegen entry --host` (Phase 211.F) — rebuild CLI");
    }

    let tmp = tempfile::tempdir().expect("tempdir");

    // robot1 → talker only.
    let robot1 = codegen_entry_host("robot1", &tmp.path().join("robot1_main.rs"));
    assert!(
        robot1.contains("talker_pkg::register"),
        "robot1 entry missing talker:\n{robot1}"
    );
    assert!(
        !robot1.contains("listener_pkg::register"),
        "robot1 entry wrongly includes listener (machine=robot2):\n{robot1}"
    );

    // robot2 → listener only.
    let robot2 = codegen_entry_host("robot2", &tmp.path().join("robot2_main.rs"));
    assert!(
        robot2.contains("listener_pkg::register"),
        "robot2 entry missing listener:\n{robot2}"
    );
    assert!(
        !robot2.contains("talker_pkg::register"),
        "robot2 entry wrongly includes talker (machine=robot1):\n{robot2}"
    );
}

/// Phase 211.F — the bringup `system.toml` declares a `[deploy.<id>]` target
/// (RFC-0004 §4 home — NOT a root `nros.toml`, see issue #51) for each host the
/// multi-host launch bakes. The deploy-target id == the launch `machine=` id, so
/// `nros codegen entry --host <id>` maps onto `[deploy.<id>]` by name. This ties
/// the per-host bake (above) to the per-host deploy SSOT.
#[test]
fn multihost_deploy_targets_match_baked_hosts() {
    let system_toml = nros_tests::project_root()
        .join("examples/workspaces/rust/src/demo_bringup/system.toml");
    let raw = std::fs::read_to_string(&system_toml)
        .unwrap_or_else(|e| panic!("read {}: {e}", system_toml.display()));

    // The hosts the multi-host launch partitions into (mirrors the bake above).
    for host in ["robot1", "robot2"] {
        assert!(
            raw.contains(&format!("[deploy.{host}]")),
            "system.toml has no `[deploy.{host}]` target for the multi-host launch \
             machine `{host}` — per-host bake (`--host {host}`) has no deploy SSOT \
             to map onto:\n{raw}"
        );
    }
    // Both per-host targets point at the multi-host launch.
    assert!(
        raw.matches("multihost.launch.xml").count() >= 2,
        "per-host `[deploy.robotN]` targets must bind to multihost.launch.xml:\n{raw}"
    );
}
