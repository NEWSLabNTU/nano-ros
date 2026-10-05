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
//! 3. each language's BUILT per-host entry registers only that host's nodes,
//!    read back from the image through a census run
//!    (`multihost_bake_emits_only_the_hosts_node`, issue 1692).
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
        // The placement SSOT still names this host — `[host.<host>]` since
        // issue 0951, with an explicit `nodes = [..]` (with `machine=` gone
        // there is no launch-derived placement fact). It was `[deploy.<host>]`
        // until the machine half moved out of that table.
        let system_toml = nros_tests::project_root().join(format!(
            "examples/workspaces/{ws}/src/demo_bringup/system.toml"
        ));
        let toml_raw = std::fs::read_to_string(&system_toml)
            .unwrap_or_else(|e| panic!("read {}: {e}", system_toml.display()));
        assert!(
            toml_raw.contains(&format!("[host.{host}]")),
            "[{ws}] system.toml lost `[host.{host}]`:\n{toml_raw}"
        );
    }
}

/// Each language's per-host entry, as the BUILD baked it, registers only its
/// host's nodes — read from the image itself, through a census run.
///
/// Issue 1692. This used to bake a Rust entry with `nros codegen entry --lang
/// rust --model <per-host model>` and grep the emitted `main.rs` for
/// `talker_pkg::register`. That verb was retired in phase-432 W2.4 (a Rust
/// entry is the `nros::main!()` expansion, at compile time, with no source
/// artifact), and since phase-460 W1 the verb's model door also refused a
/// model the bare resolver had stamped with its own crate version instead of
/// the `play_launch` pin `nros sync` writes. Nothing ran the test between the
/// retirement and 2026-10-05 (issue 1651), so it asserted a surface that no
/// longer existed.
///
/// The surface that exists for EVERY language is the built image, and the
/// cheapest question to ask it is a census (`$NROS_CENSUS_OUT`, phase-463):
/// the entry constructs every component its bake registered, writes the nodes
/// the recorder saw and exits — no router, no spin. So this asserts the bake's
/// OUTPUT on the multihost fixtures `multihost_e2e` boots, which were baked
/// from `nros sync`'s per-host models (`[[model]] args = { host = … }`) through
/// each language's real road: `nros::main!` for Rust, `nano_ros_entry` →
/// `nros codegen entry --typed` for C, C++ and mixed. `multihost_e2e` proves
/// robot1 reaches robot2; only this proves robot1 carries no listener.
#[test]
fn multihost_bake_emits_only_the_hosts_node() {
    use nros_tests::fixtures::{
        RequireFixture, build_native_workspace_c_entry_robot1,
        build_native_workspace_c_entry_robot2, build_native_workspace_cpp_entry_robot1,
        build_native_workspace_cpp_entry_robot2, build_native_workspace_mixed_entry_robot1,
        build_native_workspace_mixed_entry_robot2, build_native_workspace_rust_entry_robot1,
        build_native_workspace_rust_entry_robot2,
    };
    type Resolver = fn() -> nros_tests::TestResult<&'static std::path::Path>;
    // (entry, resolver, nodes it must register, nodes it must NOT register)
    let cells: &[(&str, Resolver, &[&str], &[&str])] = &[
        (
            "rust robot1",
            build_native_workspace_rust_entry_robot1,
            &["talker"],
            &["listener"],
        ),
        (
            "rust robot2",
            build_native_workspace_rust_entry_robot2,
            &["listener"],
            &["talker"],
        ),
        (
            "c robot1",
            build_native_workspace_c_entry_robot1,
            &["talker"],
            &["listener"],
        ),
        (
            "c robot2",
            build_native_workspace_c_entry_robot2,
            &["listener"],
            &["talker"],
        ),
        (
            "cpp robot1",
            build_native_workspace_cpp_entry_robot1,
            &["talker"],
            &["listener"],
        ),
        (
            "cpp robot2",
            build_native_workspace_cpp_entry_robot2,
            &["listener"],
            &["talker"],
        ),
        (
            "mixed robot1",
            build_native_workspace_mixed_entry_robot1,
            &["talker", "heartbeat"],
            &["listener"],
        ),
        (
            "mixed robot2",
            build_native_workspace_mixed_entry_robot2,
            &["listener"],
            &["talker", "heartbeat"],
        ),
    ];
    for (what, resolve, registers, absent) in cells {
        let entry = resolve().require(&format!("native {what} multihost entry"));
        let census = nros_tests::census::take(entry, std::time::Duration::from_secs(20));
        assert!(
            census.status.success(),
            "[{what}] census run of {} exited {}: {}",
            entry.display(),
            census.status,
            census.raw
        );
        let nodes = census.node_ids();
        for node in *registers {
            assert!(
                nodes.iter().any(|n| n == node),
                "[{what}] {} does not register its own host's node `{node}` (census \
                 nodes: {nodes:?})",
                entry.display()
            );
        }
        for node in *absent {
            assert!(
                !nodes.iter().any(|n| n == node),
                "[{what}] {} registers the OTHER host's node `{node}` -- the per-host \
                 partition did not reach the bake (census nodes: {nodes:?})",
                entry.display()
            );
        }
    }
}
