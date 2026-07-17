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
        model: None,
    }
}

/// 212.E spec test — FreeRTOS thumbv7m bake produces the baked header
/// (`system_config.h`) under `<out>/nros-system/`.
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

    // Phase 258 (Track 2, follow-up): the retired system_main.c C-baker is no
    // longer emitted.
    assert!(!bake.join("system_main.c").exists());

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
    let snap: Vec<(String, Vec<u8>)> = ["system_config.h", "Cargo.toml", "nros-plan.json"]
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

/// RFC-0052 / phase-296 W1 — a SystemModel-driven bake produces a
/// byte-identical `nros-plan.json` to the same execution config authored
/// in `system.toml` ([tiers.*] + group_tiers). Same resolver by
/// construction; this pins the conversion seam.
#[test]
fn codegen_system_model_mode_plan_matches_system_toml() {
    let ws = temp_root("model-eq");
    write_fixture(&ws);
    // Author tiers + bindings in system.toml (the baseline).
    let system_toml = ws.join("demo_bringup/system.toml");
    let mut s = fs::read_to_string(&system_toml).unwrap();
    s = s.replace(
        "[[component]]\npkg = \"talker_pkg\"\nclass = \"talker_pkg::TalkerNode\"\nname = \"talker\"",
        "[[component]]\npkg = \"talker_pkg\"\nclass = \"talker_pkg::TalkerNode\"\nname = \"talker\"\ngroup_tiers = { ctrl = \"high\" }",
    );
    s.push_str(
        r#"
[tiers.high]
spin_period_us = 1000
[tiers.high.posix]
priority = 80
sched_class = "SCHED_FIFO"
[tiers.high.freertos]
priority = 5
stack_bytes = 32768
"#,
    );
    fs::write(&system_toml, &s).unwrap();

    let out_toml = temp_root("model-eq-out-toml");
    codegen_system::run(args(&ws, &out_toml)).expect("toml-mode bake");

    // The equivalent model: same tiers, binding expressed the model way.
    let model_path = ws.join("system_model.yaml");
    fs::write(
        &model_path,
        r#"meta:
  version: 1
structure: {}
execution:
  tiers:
    high:
      spin_period_us: 1000
      posix:
        priority: 80
        sched_class: SCHED_FIFO
      freertos:
        priority: 5
        stack_bytes: 32768
  bindings:
    /demo/talker/ctrl: high
"#,
    )
    .unwrap();
    // Strip the authored tiers so ONLY the model supplies them (proves the
    // model replaces, not merges).
    let stripped = fs::read_to_string(&system_toml)
        .unwrap()
        .split("[tiers.high]")
        .next()
        .unwrap()
        .to_string();
    fs::write(&system_toml, stripped).unwrap();

    let out_model = temp_root("model-eq-out-model");
    let mut a = args(&ws, &out_model);
    a.model = Some(model_path);
    codegen_system::run(a).expect("model-mode bake");

    let plan_toml = fs::read_to_string(out_toml.join("nros-system/nros-plan.json")).unwrap();
    let plan_model = fs::read_to_string(out_model.join("nros-system/nros-plan.json")).unwrap();
    assert_eq!(
        plan_toml, plan_model,
        "model-driven plan must be byte-identical to the system.toml-authored plan"
    );
}

/// Fail-loud: a model binding naming a component the bringup doesn't have
/// refuses the bake (never a silent no-op).
#[test]
fn codegen_system_model_mode_unknown_binding_fails() {
    let ws = temp_root("model-badbind");
    write_fixture(&ws);
    let model_path = ws.join("system_model.yaml");
    fs::write(
        &model_path,
        r#"meta:
  version: 1
structure: {}
execution:
  tiers:
    rt:
      posix:
        priority: 20
  bindings:
    /ghost_node: rt
"#,
    )
    .unwrap();
    let out = temp_root("model-badbind-out");
    let mut a = args(&ws, &out);
    a.model = Some(model_path);
    let err = codegen_system::run(a).unwrap_err().to_string();
    assert!(
        err.contains("no component named 'ghost_node'") && err.contains("/ghost_node"),
        "got: {err}"
    );
}

/// R1-N1 — a model with publisher rate contracts bakes the monitor table
/// (`system_monitors.rs` + plan `monitors` section); an uncontracted model
/// bakes neither (legacy byte-identity).
#[test]
fn codegen_system_model_mode_emits_monitor_table() {
    let ws = temp_root("model-mon");
    write_fixture(&ws);
    let model_path = ws.join("system_model.yaml");
    fs::write(
        &model_path,
        r#"meta:
  version: 1
structure:
  topics:
    /demo/chatter:
      type: std_msgs/msg/String
      pub:
        - /demo/talker/chatter
      sub:
        - /demo/listener/chatter
contracts:
  pub_endpoints:
    /demo/talker/chatter:
      min_rate_hz: 10.0
  sub_endpoints:
    /demo/listener/chatter:
      max_age_ms: 150.0
  node_paths:
    /demo/talker/proc:
      output:
        - /demo/talker/chatter
      max_latency_ms: 30.0
execution: {}
"#,
    )
    .unwrap();
    let out = temp_root("model-mon-out");
    let mut a = args(&ws, &out);
    a.model = Some(model_path);
    codegen_system::run(a).expect("bake");

    let mon = fs::read_to_string(out.join("nros-system/system_monitors.rs")).expect("table baked");
    assert!(mon.contains("min_rate_hz_milli: 10000u32"), "{mon}");
    assert!(mon.contains("fqn: \"/demo/talker/chatter\""), "{mon}");
    // W3b.5 — latency budget on the output endpoint + subscriber age row.
    assert!(mon.contains("max_latency_ms: 30u32"), "{mon}");
    assert!(mon.contains("max_age_ms: 150u32"), "{mon}");
    assert!(mon.contains("fqn: \"/demo/listener/chatter\""), "{mon}");
    assert!(mon.contains("set_age_table(NROS_AGE_MONITORS)"), "{mon}");
    let plan = fs::read_to_string(out.join("nros-system/nros-plan.json")).unwrap();
    assert!(plan.contains("\"monitors\""), "{plan}");
    assert!(plan.contains("\"age_monitors\""), "{plan}");

    // Uncontracted model: no table file, no plan section.
    let model2 = ws.join("m2.yaml");
    fs::write(&model2, "meta:\n  version: 1\nstructure: {}\n").unwrap();
    let out2 = temp_root("model-mon-out2");
    let mut a2 = args(&ws, &out2);
    a2.model = Some(model2);
    codegen_system::run(a2).expect("bake2");
    assert!(!out2.join("nros-system/system_monitors.rs").exists());
    assert!(
        !fs::read_to_string(out2.join("nros-system/nros-plan.json"))
            .unwrap()
            .contains("\"monitors\"")
    );
}

/// R1-N3 — a model carrying `execution.transports` rides them into the
/// bake plan (`transports` section); a transport-free model omits the
/// section (legacy byte-identity).
#[test]
fn codegen_system_model_mode_emits_transports() {
    let ws = temp_root("model-tx");
    write_fixture(&ws);
    let model_path = ws.join("system_model.yaml");
    fs::write(
        &model_path,
        r#"meta:
  version: 1
structure: {}
execution:
  transports:
    - kind: ethernet
      id: eth0
      ip: 10.0.2.50/24
      mac: "02:00:00:00:00:01"
      gateway: 10.0.2.2
      domain: 7
"#,
    )
    .unwrap();
    let out = temp_root("model-tx-out");
    let mut a = args(&ws, &out);
    a.model = Some(model_path);
    codegen_system::run(a).expect("bake");

    let plan = fs::read_to_string(out.join("nros-system/nros-plan.json")).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&plan).unwrap();
    let tx = parsed["transports"].as_array().expect("transports section");
    assert_eq!(tx.len(), 1);
    assert_eq!(tx[0]["kind"], "ethernet");
    assert_eq!(tx[0]["ip"], "10.0.2.50/24");
    assert_eq!(tx[0]["mac"], "02:00:00:00:00:01");
    assert_eq!(tx[0]["domain"], 7);

    // Unknown kind: fail-loud, never a silent skip.
    let bad = ws.join("bad.yaml");
    fs::write(
        &bad,
        "meta:\n  version: 1\nstructure: {}\nexecution:\n  transports:\n    - kind: carrier-pigeon\n",
    )
    .unwrap();
    let out2 = temp_root("model-tx-out2");
    let mut a2 = args(&ws, &out2);
    a2.model = Some(bad);
    let err = codegen_system::run(a2).unwrap_err().to_string();
    assert!(err.contains("carrier-pigeon"), "got: {err}");
}

/// R1-N2 — `plan_from_model`: a model with deploy placement, params,
/// bindings, and features produces a Plan sliced per board with all the
/// legacy-path fields populated.
#[test]
fn plan_from_model_slices_and_populates() {
    use nros_cli_core::codegen::entry::plan_from_model;
    let tmp = temp_root("model-plan");
    let model_path = tmp.join("system_model.yaml");
    fs::write(
        &model_path,
        r#"meta:
  version: 1
structure:
  nodes:
    /demo/talker:
      scope: /
      pkg: talker_pkg
      exec: talker
      params:
        rate_hz: 10
        label: fast
    /sensing/imu:
      scope: /
      pkg: imu_pkg
      exec: imu
execution:
  deploy:
    /demo/talker:
      target: linux
    /sensing/imu:
      target: mcu:stm32f4
  tiers:
    high:
      spin_period_us: 1000
      posix:
        priority: 80
  bindings:
    /demo/talker/ctrl: high
  features:
    - safety
    - param_services
"#,
    )
    .unwrap();

    let plan = plan_from_model(&model_path, None).expect("native plan");
    assert_eq!(plan.nodes.len(), 1, "linux slice only");
    let n = &plan.nodes[0];
    assert_eq!(n.pkg, "talker_pkg");
    assert_eq!(n.exec, "talker");
    assert_eq!(n.name.as_deref(), Some("talker"));
    assert_eq!(n.namespace.as_deref(), Some("/demo"));
    assert!(
        n.params
            .contains(&("rate_hz".to_string(), "10".to_string()))
    );
    assert_eq!(n.group_tiers.get("ctrl").map(String::as_str), Some("high"));
    assert!(plan.param_services);
    assert_eq!(plan.safety, Some(true));
    assert_eq!(plan.tiers["high"].spin_period_us, Some(1000));

    let mcu = plan_from_model(&model_path, Some("stm32f4".to_string())).expect("mcu plan");
    assert_eq!(mcu.nodes.len(), 1);
    assert_eq!(mcu.nodes[0].exec, "imu");

    let err = plan_from_model(&model_path, Some("zephyr".to_string())).unwrap_err();
    assert!(err.to_string().contains("places no nodes"), "{err}");
}

/// W4.3 — an Mcu deploy naming a CONCRETE board ("fvp-aemv8r-smp") still
/// slices under the entry codegen's board-FAMILY key ("zephyr") via the
/// integrator's `[deploy.<t>] kind` (carried in `extra.kind`).
#[test]
fn plan_from_model_matches_deploy_kind_family() {
    use nros_cli_core::codegen::entry::plan_from_model;
    let tmp = temp_root("model-plan-kind");
    let model_path = tmp.join("system_model.yaml");
    fs::write(
        &model_path,
        r#"meta:
  version: 1
structure:
  nodes:
    /controller:
      scope: /
      pkg: controller_pkg
      exec: controller
execution:
  deploy:
    /controller:
      target: mcu:fvp-aemv8r-smp
      rmw: cyclonedds
      extra:
        kind: zephyr
        deploy_name: fvp
"#,
    )
    .unwrap();

    let plan = plan_from_model(&model_path, Some("zephyr".to_string())).expect("family slice");
    assert_eq!(plan.nodes.len(), 1);
    assert_eq!(plan.nodes[0].exec, "controller");
    // Exact concrete key still works too.
    let exact =
        plan_from_model(&model_path, Some("fvp-aemv8r-smp".to_string())).expect("exact slice");
    assert_eq!(exact.nodes.len(), 1);
    // Unrelated family: empty slice refuses.
    let err = plan_from_model(&model_path, Some("freertos".to_string())).unwrap_err();
    assert!(err.to_string().contains("places no nodes"), "{err}");
}
