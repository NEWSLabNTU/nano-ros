//! Phase 212.E integration tests for `nros codegen-system` per the spec
//! test list. The inline `mod tests` in `src/cmd/codegen_system.rs` covers
//! the renderer-level cases; this file holds the end-to-end fixtures spec'd
//! in `docs/roadmap/phase-212-ux-cargo-native-and-file-consolidation.md`
//! §212.E.

use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use nros_cli_core::cmd::codegen_system::{self, AheadOfVendor, Args};

fn temp_root(tag: &str) -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "codegen-system-{tag}-{}-{stamp}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// Materialise the canonical fixture: a Cargo workspace with two
/// component pkgs (`talker_pkg`, `listener_pkg`) + one bringup pkg
/// (`demo_bringup`) carrying `system.toml` + `launch/system.launch.xml`.
/// Mirrors `tests/fixtures/codegen_system_basic/` per the spec.
fn write_fixture(dir: &Path) {
    fs::write(
        dir.join("Cargo.toml"),
        r#"
[workspace]
resolver = "2"
members = ["talker_pkg", "listener_pkg", "demo_bringup"]

[workspace.metadata.nros]
default_system = "demo_bringup"
"#,
    )
    .unwrap();

    for pkg in ["talker_pkg", "listener_pkg"] {
        fs::create_dir_all(dir.join(pkg).join("src")).unwrap();
        fs::write(
            dir.join(pkg).join("Cargo.toml"),
            format!(
                r#"
[package]
name = "{pkg}"
version = "0.1.0"
edition = "2021"

[lib]
path = "src/lib.rs"

[package.metadata.nros.component]
default_namespace = "/demo"
"#
            ),
        )
        .unwrap();
        fs::write(dir.join(pkg).join("src/lib.rs"), "").unwrap();
    }

    fs::create_dir_all(dir.join("demo_bringup/launch")).unwrap();
    fs::create_dir_all(dir.join("demo_bringup/src")).unwrap();
    fs::write(
        dir.join("demo_bringup/Cargo.toml"),
        r#"
[package]
name = "demo_bringup"
version = "0.1.0"
edition = "2021"

[lib]
path = "src/lib.rs"
"#,
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

[deploy.native]
kind = "self"
target = "x86_64-unknown-linux-gnu"
"#,
    )
    .unwrap();
    fs::write(
        dir.join("demo_bringup/launch/system.launch.xml"),
        "<launch></launch>\n",
    )
    .unwrap();
    fs::write(
        dir.join("demo_bringup/launch/freertos.launch.xml"),
        "<launch></launch>\n",
    )
    .unwrap();
}

fn args(ws: &Path, out: &Path) -> Args {
    Args {
        workspace: Some(ws.to_path_buf()),
        bringup: None,
        target: None,
        out: Some(out.to_path_buf()),
        ahead_of_vendor: None,
        file: None,
        exec: None,
        rmw: None,
    }
}

/// 212.E spec test — FreeRTOS thumbv7m bake produces baked headers
/// (`system_config.h` + `system_main.c`) under `<out>/nros-system/`.
/// The target string is recorded into `nros-plan.json`.
#[test]
fn codegen_system_emits_baked_headers_for_freertos_qemu() {
    let dir = temp_root("baked_freertos_qemu");
    write_fixture(&dir);

    let out = dir.join("build/demo_bringup");
    let mut a = args(&dir, &out);
    a.target = Some("thumbv7m-none-eabi".into());
    codegen_system::run(a).expect("codegen runs for freertos target");

    let bake = out.join("nros-system");
    let header = fs::read_to_string(bake.join("system_config.h")).unwrap();
    assert!(header.contains("#define NROS_SYSTEM_DOMAIN_ID 7u"));
    assert!(header.contains("#define NROS_SYSTEM_RMW \"zenoh\""));
    assert!(header.contains("#define NROS_SYSTEM_RMW_ZENOH"));
    assert!(header.contains("#define NROS_SYSTEM_LOCATOR \"tcp/127.0.0.1:7447\""));
    assert!(header.contains("#define NROS_SYSTEM_COMPONENT_COUNT 2"));

    let main_c = fs::read_to_string(bake.join("system_main.c")).unwrap();
    assert!(main_c.contains("extern int nros_component_talker_register(void);"));
    assert!(main_c.contains("extern int nros_component_listener_register(void);"));
    assert!(main_c.contains("nros_system_spin();"));

    let plan = fs::read_to_string(bake.join("nros-plan.json")).unwrap();
    assert!(plan.contains("\"target\": \"thumbv7m-none-eabi\""));
}

/// 212.E spec test — bake always emits `nros-plan.json` with the
/// resolved bringup + system + components.
#[test]
fn codegen_system_emits_nros_plan_json() {
    let dir = temp_root("emits_plan_json");
    write_fixture(&dir);

    let out = dir.join("build/demo_bringup");
    codegen_system::run(args(&dir, &out)).expect("codegen runs");

    let plan =
        fs::read_to_string(out.join("nros-system/nros-plan.json")).expect("nros-plan.json exists");
    // Parse the JSON to confirm it's well-formed + has the expected keys.
    let parsed: serde_json::Value = serde_json::from_str(&plan).expect("plan parses as JSON");
    assert_eq!(parsed["bringup"], "demo_bringup");
    assert_eq!(parsed["system"], "demo");
    assert_eq!(parsed["rmw"], "zenoh");
    assert_eq!(parsed["domain_id"], 7);
    assert_eq!(parsed["locator"], "tcp/127.0.0.1:7447");
    let comps = parsed["components"].as_array().expect("components array");
    assert_eq!(comps.len(), 2);
    assert_eq!(comps[0]["name"], "talker");
    assert_eq!(comps[0]["pkg"], "talker_pkg");
    assert_eq!(comps[0]["lang"], "rust");
    assert_eq!(comps[1]["name"], "listener");
}

/// 212.E.3 spec test — `--ahead-of-vendor pio` mode emits the
/// `vendor_hint.json` skeleton under the bake tree (in addition to the
/// per-vendor artifacts which other tests cover).
#[test]
fn codegen_system_ahead_of_vendor_emits_hint_file() {
    let dir = temp_root("ahead_of_vendor_hint");
    write_fixture(&dir);

    let out = dir.join("build/demo_bringup");
    let mut a = args(&dir, &out);
    a.ahead_of_vendor = Some(AheadOfVendor::Pio);
    codegen_system::run(a).expect("codegen runs");

    let hint_path = out.join("nros-system/vendor_hint.json");
    assert!(
        hint_path.exists(),
        "vendor_hint.json at {}",
        hint_path.display()
    );
    let body = fs::read_to_string(&hint_path).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&body).expect("hint parses as JSON");
    assert_eq!(parsed["kind"], "platformio");
    assert_eq!(parsed["bringup"], "demo_bringup");
    assert_eq!(parsed["system"], "demo");
    let comps = parsed["components"].as_array().expect("components array");
    assert!(comps.iter().any(|v| v == "talker"));
    assert!(comps.iter().any(|v| v == "listener"));

    // Also verify px4 mode emits the hint with kind="px4".
    let out2 = dir.join("build/demo_bringup_px4");
    let mut a = args(&dir, &out2);
    a.ahead_of_vendor = Some(AheadOfVendor::Px4);
    codegen_system::run(a).expect("codegen runs px4");
    let body = fs::read_to_string(out2.join("nros-system/vendor_hint.json")).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&body).expect("hint parses");
    assert_eq!(parsed["kind"], "px4");
}

/// 212.E spec test — re-running with identical inputs produces
/// byte-identical outputs across every emitted file.
#[test]
fn codegen_system_idempotent_on_unchanged_input() {
    let dir = temp_root("idempotent");
    write_fixture(&dir);

    let out = dir.join("build/demo_bringup");
    codegen_system::run(args(&dir, &out)).expect("first run");

    let bake = out.join("nros-system");
    let snap: Vec<(String, Vec<u8>)> = [
        "system_config.h",
        "system_main.c",
        "Cargo.toml",
        "nros-plan.json",
    ]
    .iter()
    .map(|f| (f.to_string(), fs::read(bake.join(f)).expect("read")))
    .collect();

    codegen_system::run(args(&dir, &out)).expect("second run");

    for (name, before) in snap {
        let after = fs::read(bake.join(&name)).expect("read");
        assert_eq!(before, after, "file `{name}` differs across runs");
    }
}

/// 212.E spec test — invoking with a `--bringup` name that doesn't
/// resolve to a workspace bringup pkg fails with a useful error.
#[test]
fn codegen_system_unknown_pkg_errors() {
    let dir = temp_root("unknown_pkg");
    write_fixture(&dir);

    let out = dir.join("build/unknown");
    let mut a = args(&dir, &out);
    a.bringup = Some("does_not_exist".into());
    let err = codegen_system::run(a).expect_err("must error on unknown bringup");
    let msg = format!("{err:#}");
    assert!(
        msg.contains("does_not_exist") || msg.contains("no bringup package"),
        "unexpected error message: {msg}"
    );
}

/// 212.E spec test — passing `--target <name>` where the bringup has a
/// matching `[deploy.<name>]` block with `launch = "..."` picks that
/// per-target launch override (over `[system].default_launch`). The
/// resolved launch path is recorded into `nros-plan.json::launch_file`.
#[test]
fn codegen_system_picks_deploy_target_overlay() {
    let dir = temp_root("deploy_target_overlay");
    write_fixture(&dir);

    let out = dir.join("build/demo_bringup");
    let mut a = args(&dir, &out);
    a.target = Some("qemu_freertos".into());
    codegen_system::run(a).expect("codegen runs");

    let plan = fs::read_to_string(out.join("nros-system/nros-plan.json")).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&plan).expect("plan parses");
    let launch_file = parsed["launch_file"]
        .as_str()
        .expect("launch_file recorded");
    assert!(
        launch_file.ends_with("freertos.launch.xml"),
        "expected deploy.qemu_freertos.launch override; got {launch_file}"
    );

    // Without --target, default_launch should still apply.
    let out2 = dir.join("build/demo_bringup_default");
    codegen_system::run(args(&dir, &out2)).expect("default target codegen");
    let plan = fs::read_to_string(out2.join("nros-system/nros-plan.json")).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&plan).expect("plan parses");
    let launch_file = parsed["launch_file"]
        .as_str()
        .expect("launch_file recorded");
    assert!(
        launch_file.ends_with("system.launch.xml"),
        "expected default launch; got {launch_file}"
    );
}
