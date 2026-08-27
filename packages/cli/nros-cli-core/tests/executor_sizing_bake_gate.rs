//! phase-307 W6 lane 1 — over-capacity is caught at BAKE, not at boot.
//!
//! Issue 0257's failure mode: a bringup registers more callback entities than
//! the compiled `NROS_EXECUTOR_MAX_CBS` table holds, the image builds fine, and
//! the first over-capacity `create_*` dies at boot with `code=-6 Full`. The
//! bake-time check exists to move that discovery from the board to the build.
//!
//! What this file adds over the unit tests in `model_ingest`: the whole path,
//! end to end through the real `codegen-system` verb — sidecar on disk →
//! `Workspace` discovery → slot counts → `max(model, recorded)` → the refusal.
//! The unit tests take a hand-built `BTreeMap`; only this proves the sidecar is
//! actually FOUND and READ by the bake.
//!
//! And it is specifically the case the pre-307 bake could not see: a node whose
//! MODELLED entity count fits the table comfortably, but which registers timers
//! the launch wiring has no entity for. The model said 2 and passed; the truth
//! was 7. That gap is the whole reason issue 0257 stayed open.

mod common;

use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use nros_cli_core::cmd::codegen_system::{self, Args};

fn temp_root(tag: &str) -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir =
        std::env::temp_dir().join(format!("nros-307-w6-{tag}-{}-{stamp}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// A bringup whose model wires ONE subscriber onto `/listener` — two callback
/// entities in total, which fits the default four-slot table with room to
/// spare. `deploy` targets a firmware board, which drops per-entry sizing and
/// opens at the build-time `NROS_EXECUTOR_MAX_CBS`.
fn write_fixture(dir: &Path) {
    fs::write(
        dir.join("Cargo.toml"),
        "[workspace]\nresolver = \"2\"\nmembers = [\"talker_pkg\", \"listener_pkg\", \
         \"demo_bringup\"]\n\n[workspace.metadata.nros]\ndefault_system = \"demo_bringup\"\n",
    )
    .unwrap();

    for pkg in ["talker_pkg", "listener_pkg"] {
        fs::create_dir_all(dir.join(pkg).join("src")).unwrap();
        fs::write(
            dir.join(pkg).join("Cargo.toml"),
            format!(
                "[package]\nname = \"{pkg}\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n\
                 [lib]\npath = \"src/lib.rs\"\n\n[package.metadata.nros.component]\n\
                 default_namespace = \"/demo\"\n"
            ),
        )
        .unwrap();
        fs::write(dir.join(pkg).join("src/lib.rs"), "").unwrap();
    }

    fs::create_dir_all(dir.join("demo_bringup/launch")).unwrap();
    fs::create_dir_all(dir.join("demo_bringup/src")).unwrap();
    fs::create_dir_all(dir.join("demo_bringup/config")).unwrap();
    fs::write(
        dir.join("demo_bringup/Cargo.toml"),
        "[package]\nname = \"demo_bringup\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n\
         [lib]\npath = \"src/lib.rs\"\n",
    )
    .unwrap();
    fs::write(dir.join("demo_bringup/src/lib.rs"), "").unwrap();
    fs::write(
        dir.join("demo_bringup/system.toml"),
        r#"
[system]
name = "demo"
rmw = "zenoh"
domain_id = 7
locator = "tcp/127.0.0.1:7447"
default_launch = "system.launch.xml"

[[component]]
pkg = "talker_pkg"
class = "talker_pkg::TalkerNode"
name = "talker"

[[component]]
pkg = "listener_pkg"
class = "listener_pkg::ListenerNode"
name = "listener"

[deploy.qemu_freertos]
kind = "freertos"
target = "thumbv7m-none-eabi"
board = "mps2-an385"
launch = "freertos.launch.xml"
"#,
    )
    .unwrap();
    // One topic, one subscriber: the modelled count is 1 and fits easily.
    fs::write(
        dir.join("demo_bringup/config/system_model.yaml"),
        r#"
meta:
  version: 1
structure:
  scopes:
    /: {}
  nodes:
    /listener:
      scope: /
      pkg: listener_pkg
      exec: listener
    /talker:
      scope: /
      pkg: talker_pkg
      exec: talker
  topics:
    /chatter:
      type: std_msgs/msg/Int32
      publishers: ["/talker/chatter"]
      subscribers: ["/listener/chatter"]
"#,
    )
    .unwrap();
    for launch in ["system.launch.xml", "freertos.launch.xml"] {
        fs::write(
            dir.join("demo_bringup/launch").join(launch),
            "<launch></launch>\n",
        )
        .unwrap();
    }
}

/// A sidecar for `(listener_pkg, listener)` recording `timers` timers on top of
/// the one modelled subscription. Written where `Workspace` discovery looks:
/// a package root's `metadata/` dir.
fn write_sidecar(dir: &Path, timers: usize) {
    fs::write(
        dir.join("package.xml"),
        "<package format=\"3\"><name>demo</name><version>0.1.0</version>\
         <description>d</description><maintainer email=\"a@b.c\">a</maintainer>\
         <license>Apache-2.0</license></package>\n",
    )
    .unwrap();
    fs::create_dir_all(dir.join("metadata")).unwrap();
    let timer_rows: Vec<String> = (0..timers)
        .map(|i| {
            format!(
                "{{\"id\":\"tick{i}\",\"period_ms\":100,\"callback\":\"tick{i}\",\
                 \"callback_slot\":{i}}}"
            )
        })
        .collect();
    let json = format!(
        r#"{{
  "version": 1,
  "package": "listener_pkg",
  "component": "listener",
  "language": "rust",
  "executable": "listener",
  "exported_symbol": "nros_component_listener",
  "nodes": [
    {{
      "id": "listener",
      "unresolved_name": {{ "value": "listener", "kind": "relative" }},
      "namespace": null,
      "publishers": [],
      "subscribers": [
        {{
          "id": "on_message",
          "unresolved_topic": {{ "value": "/chatter", "kind": "absolute" }},
          "interface": {{ "package": "std_msgs", "name": "msg/Int32", "kind": "message" }},
          "qos": {{ "reliability": "reliable", "durability": "volatile",
                   "history": "keep_last", "depth": 10, "deadline_ms": null,
                   "lifespan_ms": null, "liveliness": "automatic",
                   "liveliness_lease_duration_ms": null, "extensions": {{}} }},
          "callback": "on_message",
          "callback_slot": 0
        }}
      ],
      "timers": [{timers_json}],
      "services": [],
      "actions": []
    }}
  ],
  "callbacks": [],
  "parameters": [],
  "trace": {{ "generator": "phase-307-w6-fixture", "package_manifest": "package.xml",
             "source_artifacts": [] }}
}}
"#,
        timers_json = timer_rows.join(",")
    );
    fs::write(dir.join("metadata/listener.json"), json).unwrap();
}

fn args(ws: &Path, out: &Path) -> Args {
    Args {
        workspace: Some(ws.to_path_buf()),
        bringup: None,
        target: Some("freertos".to_string()),
        out: Some(out.to_path_buf()),
        ahead_of_vendor: None,
        file: None,
        exec: None,
        rmw: None,
        model: None,
    }
}

/// Control: the model alone. One subscription fits the default table, the bake
/// succeeds — and it must keep succeeding, or every existing build regresses.
#[test]
fn modelled_count_alone_fits_and_the_bake_succeeds() {
    common::isolate_model_discovery();
    let dir = temp_root("model_only");
    write_fixture(&dir);
    let out = dir.join("build");
    codegen_system::run(args(&dir, &out)).expect("1 modelled entity fits the default 4 slots");
}

/// The 0257 regression test, at last. Same model, same board, same table — the
/// only difference is a sidecar recording six timers the launch wiring cannot
/// see. Pre-307 this bake passed and the image died at boot with `code=-6
/// Full`; now it fails at the bake and names the real count.
#[test]
fn recorded_timers_over_capacity_fail_the_bake_with_the_count() {
    common::isolate_model_discovery();
    let dir = temp_root("timers_over");
    write_fixture(&dir);
    write_sidecar(&dir, 6);
    let out = dir.join("build");
    let err = codegen_system::run(args(&dir, &out))
        .expect_err("1 sub + 6 recorded timers do not fit the default 4 slots")
        .to_string();
    // The count must be NAMED: "it doesn't fit" without a number leaves the
    // user guessing what to raise the knob to.
    assert!(err.contains("7 callback entities"), "got: {err}");
    assert!(err.contains("holds 4"), "got: {err}");
    assert!(err.contains("NROS_EXECUTOR_MAX_CBS"), "got: {err}");
    assert!(err.contains("0257"), "got: {err}");
}

/// A sidecar that records FEWER entities than the model wires must not shrink
/// the count — the rule is a per-node max, not a substitution. (The recorder
/// does not record service/action clients, which the model does name.)
#[test]
fn a_thin_sidecar_never_lowers_the_bake_count() {
    common::isolate_model_discovery();
    let dir = temp_root("thin_sidecar");
    write_fixture(&dir);
    write_sidecar(&dir, 0);
    let out = dir.join("build");
    codegen_system::run(args(&dir, &out)).expect("still 1 entity; still fits");
}
