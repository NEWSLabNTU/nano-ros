//! Multi-host partition — RESOLVE-time (phase-326, issue 0364).
//!
//! `<node machine="…">` was ROS 1 roslaunch syntax; ROS 2 rejects it, so the
//! bake-time partition (`Plan::for_host`, `nros codegen entry --host`,
//! `nros::main!(host = …)`) is gone. The partition now happens when the
//! launch file is RESOLVED: `multihost.launch.xml` declares
//! `<arg name="host" default="all"/>` and gates each node with an
//! `if=$(eval …)` condition, so resolving with `host:=robot1` produces a
//! SystemModel that only CONTAINS robot1's nodes, and the ordinary
//! `codegen entry --model` bake needs no partition step.
//!
//! Three seams, three tests:
//! 1. the LIVE resolve drops the other host's nodes
//!    (`resolving_with_host_arg_partitions_the_model`);
//! 2. the COMMITTED per-host models carry their own binding (`meta.args`)
//!    and only their host's nodes, in all four example workspaces
//!    (`committed_per_host_models_carry_their_binding`) — `nros sync`
//!    replays `meta.args` on refresh, so a model whose binding went missing
//!    would silently re-resolve as the default (`all`) configuration;
//! 3. `nros codegen entry --model <per-host model>` emits an entry
//!    registering only that host's node
//!    (`multihost_bake_emits_only_the_hosts_node`).
//!
//! Cross-process *delivery* between hosts is proven by `multihost_e2e`; this
//! file seals the source-level story.

use std::process::Command;

use nros_tests::launch_resolver_bin as launch_resolver;

/// Resolve the rust workspace's multihost launch with `host:=<id>` into a
/// temp file and return the model YAML.
/// Resolve `<ws>`'s multihost launch for `host` into `out` and return the model
/// text. phase-330 W4 made the SystemModel a build artifact, so a test that
/// wants one RESOLVES it — it does not open a committed file (issue 0414).
fn resolve_ws_with_host(ws: &str, host: &str, out: &std::path::Path) -> String {
    let resolver = launch_resolver().expect("caller gated on launch_resolver()");
    let bringup =
        nros_tests::project_root().join(format!("examples/workspaces/{ws}/src/demo_bringup"));
    let output = Command::new(&resolver)
        .arg(bringup.join("launch/multihost.launch.xml"))
        .arg(format!("host:={host}"))
        .arg("--bringup-root")
        .arg(&bringup)
        .arg("--system")
        .arg(bringup.join("system.toml"))
        .arg("-o")
        .arg(out)
        .output()
        .expect("spawn nros-launch-resolve");
    assert!(
        output.status.success(),
        "nros-launch-resolve host:={host} failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    std::fs::read_to_string(out).expect("read resolved model")
}

fn resolve_with_host(host: &str, out: &std::path::Path) -> String {
    resolve_ws_with_host("rust", host, out)
}

#[test]
fn resolving_with_host_arg_partitions_the_model() {
    if launch_resolver().is_none() {
        nros_tests::skip!("nros-launch-resolve not built (run `just setup-launch-resolve`)");
    }
    if !nros_tests::host_python_available() {
        // Issue 0914's residue: `$(eval …)` needs an interpreter, and without
        // one this failed rather than skipping — "no Python here" and "the
        // shipped pair is broken" produce the same parse error.
        nros_tests::skip!("no usable python3 on this host");
    }
    let tmp = tempfile::tempdir().expect("tempdir");

    // robot1 → the talker only.
    let robot1 = resolve_with_host("robot1", &tmp.path().join("r1.yaml"));
    assert!(
        robot1.contains("/talker:"),
        "robot1 model lost the talker:\n{robot1}"
    );
    assert!(
        !robot1.contains("/listener:"),
        "robot1 model wrongly contains the listener — the `if=` condition \
         did not drop it at resolve time:\n{robot1}"
    );

    // robot2 → the listener only.
    let robot2 = resolve_with_host("robot2", &tmp.path().join("r2.yaml"));
    assert!(
        robot2.contains("/listener:"),
        "robot2 model lost the listener:\n{robot2}"
    );
    assert!(
        !robot2.contains("/talker:"),
        "robot2 model wrongly contains the talker:\n{robot2}"
    );

    // The default (`all`) keeps both — a node with no `if=` would be shared.
    let all = resolve_with_host("all", &tmp.path().join("all.yaml"));
    assert!(
        all.contains("/talker:") && all.contains("/listener:"),
        "host:=all must keep the whole topology:\n{all}"
    );
}

/// Each workspace's multihost bringup, resolved per host: the model records the
/// binding it was resolved from (`meta.args: host: robotN`) and contains ONLY
/// that host's nodes, and `system.toml` still names the host in a `[deploy.*]`
/// block.
///
/// This RESOLVES each model rather than reading a committed one. phase-330 W4
/// made the SystemModel a pure build artifact — regenerated into the active
/// build's output dir and no longer committed — so opening
/// `config/multihost_robot1_model.yaml` failed on `os error 2` and proved
/// nothing about the partition (issue 0414). Resolving is also the stronger
/// assertion: it exercises the resolver on every run instead of trusting a file
/// somebody generated once.
#[test]
fn per_host_resolves_partition_and_carry_their_binding() {
    if launch_resolver().is_none() {
        nros_tests::skip!("nros-launch-resolve not built (run `just setup-launch-resolve`)");
    }
    if !nros_tests::host_python_available() {
        // Issue 0914's residue: `$(eval …)` needs an interpreter, and without
        // one this failed rather than skipping — "no Python here" and "the
        // shipped pair is broken" produce the same parse error.
        nros_tests::skip!("no usable python3 on this host");
    }
    let tmp = tempfile::tempdir().expect("tempdir");
    // (workspace, host, must-contain node keys, must-NOT-contain node keys)
    let cells: &[(&str, &str, &[&str], &[&str])] = &[
        ("rust", "robot1", &["/talker:"], &["/listener:"]),
        ("rust", "robot2", &["/listener:"], &["/talker:"]),
        ("c", "robot1", &["/talker:"], &["/listener:"]),
        ("c", "robot2", &["/listener:"], &["/talker:"]),
        ("cpp", "robot1", &["/talker:"], &["/listener:"]),
        ("cpp", "robot2", &["/listener:"], &["/talker:"]),
        (
            "mixed",
            "robot1",
            &["/talker:", "/heartbeat:"],
            &["/listener:"],
        ),
        (
            "mixed",
            "robot2",
            &["/listener:"],
            &["/talker:", "/heartbeat:"],
        ),
    ];
    for (ws, host, contains, absent) in cells {
        let out = tmp.path().join(format!("{ws}_{host}.yaml"));
        let raw = resolve_ws_with_host(ws, host, &out);
        assert!(
            raw.contains(&format!("host: {host}")),
            "[{ws}/{host}] resolved model records no `meta.args` binding — a \
             refresh would re-resolve it as the default (all-hosts) configuration"
        );
        for key in *contains {
            assert!(
                raw.contains(key),
                "[{ws}/{host}] model lost its own node {key}"
            );
        }
        for key in *absent {
            assert!(
                !raw.contains(key),
                "[{ws}/{host}] model contains the OTHER host's node {key} — \
                 the per-host partition did not hold"
            );
        }
        // The deploy SSOT still names this host: `[deploy.<host>]` with an
        // explicit `nodes = [..]` placement (with `machine=` gone there is no
        // launch-derived placement fact).
        let system_toml = nros_tests::project_root().join(format!(
            "examples/workspaces/{ws}/src/demo_bringup/system.toml"
        ));
        let toml_raw = std::fs::read_to_string(&system_toml)
            .unwrap_or_else(|e| panic!("read {}: {e}", system_toml.display()));
        assert!(
            toml_raw.contains(&format!("[deploy.{host}]")),
            "[{ws}] system.toml lost `[deploy.{host}]`:\n{toml_raw}"
        );
    }
}

fn codegen_entry_model(model: &std::path::Path, out: &std::path::Path) -> String {
    let nros = nros_tests::nros_cli_bin_path().expect("require_nros_cli gated this");
    let workspace = nros_tests::project_root().join("examples/workspaces/rust");
    let status = Command::new(&nros)
        .args(["codegen", "entry", "--lang", "rust"])
        .arg("--workspace")
        .arg(&workspace)
        .arg("--model")
        .arg(model)
        .arg("--out")
        .arg(out)
        .output()
        .expect("spawn nros codegen entry");
    assert!(
        status.status.success(),
        "`nros codegen entry --model {}` failed:\nstdout:\n{}\nstderr:\n{}",
        model.display(),
        String::from_utf8_lossy(&status.stdout),
        String::from_utf8_lossy(&status.stderr),
    );
    std::fs::read_to_string(out).expect("read generated entry source")
}

#[test]
fn multihost_bake_emits_only_the_hosts_node() {
    if !nros_tests::require_nros_cli() {
        nros_tests::skip!("nros CLI not found");
    }
    if launch_resolver().is_none() {
        nros_tests::skip!("nros-launch-resolve not built (run `just setup-launch-resolve`)");
    }
    if !nros_tests::host_python_available() {
        // Issue 0914's residue: `$(eval …)` needs an interpreter, and without
        // one this failed rather than skipping — "no Python here" and "the
        // shipped pair is broken" produce the same parse error.
        nros_tests::skip!("no usable python3 on this host");
    }
    let tmp = tempfile::tempdir().expect("tempdir");

    // Resolve both host models first. They used to be read from
    // `config/multihost_robot<N>_model.yaml`; phase-330 W4 deleted the committed
    // models (issue 0414), and the bake's input is a model the BUILD produces —
    // so produce one, then bake from it. What is under test is the bake, not the
    // provenance of the yaml.
    let robot1_model = tmp.path().join("robot1_model.yaml");
    resolve_ws_with_host("rust", "robot1", &robot1_model);
    let robot2_model = tmp.path().join("robot2_model.yaml");
    resolve_ws_with_host("rust", "robot2", &robot2_model);

    // robot1 model → talker only.
    let robot1 = codegen_entry_model(&robot1_model, &tmp.path().join("robot1_main.rs"));
    assert!(
        robot1.contains("talker_pkg::register"),
        "robot1 entry missing talker:\n{robot1}"
    );
    assert!(
        !robot1.contains("listener_pkg::register"),
        "robot1 entry wrongly includes listener:\n{robot1}"
    );

    // robot2 model → listener only.
    let robot2 = codegen_entry_model(&robot2_model, &tmp.path().join("robot2_main.rs"));
    assert!(
        robot2.contains("listener_pkg::register"),
        "robot2 entry missing listener:\n{robot2}"
    );
    assert!(
        !robot2.contains("talker_pkg::register"),
        "robot2 entry wrongly includes talker:\n{robot2}"
    );
}
