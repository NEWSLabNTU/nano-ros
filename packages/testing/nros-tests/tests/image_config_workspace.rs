//! phase-481 W4 (RFC-0098 D12) — per-image configuration on the WORKSPACE
//! cmake road.
//!
//! `fixtures/image_config_ws` declares two native images on one coordinate:
//! `plain` states nothing, `warn` states `[image.warn] env`. One configure
//! builds `nros-c` / `nros-cpp` once for every image on it, so a configured
//! image must get its OWN configure (`build/<coord>-cfg<hash>/`) — otherwise
//! its knobs are dropped, or leak into the other image. Each binary prints what
//! its runtime build compiled in, and this asserts each saw its own image.

use std::{process::Command, time::Duration};

use nros_tests::{
    fixtures::{self, RequireFixture},
    output,
    process::ManagedProcess,
    project_root,
};

const BRINGUP: &str = "packages/testing/nros-tests/fixtures/image_config_ws/src/probe_bringup";

/// nros-node's builtin `NROS_EXECUTOR_MAX_CBS`.
const BUILTIN_MAX_CBS: usize = 4;

/// `[image.warn] env`, as authored — read rather than restated, so the test
/// cannot pass against a fixture that changed under it.
fn warn_env() -> toml::Table {
    let path = project_root().join(BRINGUP).join("system.toml");
    let text =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    let v: toml::Value =
        toml::from_str(&text).unwrap_or_else(|e| panic!("parse {}: {e}", path.display()));
    assert!(
        v["image"]["plain"].get("env").is_none(),
        "the premise: `plain` states no configuration"
    );
    v["image"]["warn"]["env"]
        .as_table()
        .expect("`warn` states `[image.warn] env`")
        .clone()
}

/// Boot one entry against `locator` and return its probe line.
fn probe_line(bin: &std::path::Path, locator: &str, label: &str) -> String {
    let mut cmd = Command::new(bin);
    cmd.env("NROS_LOCATOR", locator);
    let mut p =
        ManagedProcess::spawn_command(cmd, label).unwrap_or_else(|e| panic!("spawn {label}: {e}"));
    let out = p
        .wait_for_output_pattern(output::IMAGE_CONFIG_PROBE_LINE, Duration::from_secs(20))
        .unwrap_or_else(|e| panic!("{label} never printed its probe line: {e}"));
    out.lines()
        .find(|l| l.contains(output::IMAGE_CONFIG_PROBE_LINE))
        .expect("the line just matched")
        .to_string()
}

fn field(line: &str, key: &str) -> String {
    line.split_whitespace()
        .find_map(|w| w.strip_prefix(&format!("{key}=")))
        .unwrap_or_else(|| panic!("no `{key}=` in {line:?}"))
        .to_string()
}

#[test]
fn each_workspace_image_builds_against_its_own_configuration() {
    let env = warn_env();
    let max_cbs: usize = env["NROS_EXECUTOR_MAX_CBS"]
        .as_str()
        .expect("`warn` states NROS_EXECUTOR_MAX_CBS")
        .parse()
        .expect("a count");
    assert_ne!(
        max_cbs, BUILTIN_MAX_CBS,
        "`warn` states the builtin, which hides a dropped rung"
    );
    assert_eq!(env["NROS_LOG_MAX_LEVEL"].as_str(), Some("warn"));

    let plain = fixtures::build_workspace_cmake_generated_entry(
        "workspace-image-config-plain",
        "plain_entry",
    )
    .require("prebuilt image_config_ws plain_entry");
    let warn = fixtures::build_workspace_cmake_generated_entry(
        "workspace-image-config-warn",
        "warn_entry",
    )
    .require("prebuilt image_config_ws warn_entry");
    assert_ne!(
        plain.parent(),
        warn.parent(),
        "the two images share a configure, so one of them cannot have its own \
         configuration (RFC-0098 D12)"
    );

    let router = fixtures::or_skip(fixtures::ZenohRouter::start_unique());
    let locator = router.locator();

    let p = probe_line(&plain, &locator, "plain_entry");
    assert_eq!(
        field(&p, "max_handles"),
        BUILTIN_MAX_CBS.to_string(),
        "`plain` states nothing, so it must build against the builtins — a \
         configured sibling's value leaked into it: {p}"
    );
    assert_eq!(field(&p, "info_enabled"), "1", "{p}");

    let w = probe_line(&warn, &locator, "warn_entry");
    assert_eq!(
        field(&w, "max_handles"),
        max_cbs.to_string(),
        "`[image.warn] env` NROS_EXECUTOR_MAX_CBS did not reach nros-c: {w}"
    );
    assert_eq!(
        field(&w, "info_enabled"),
        "0",
        "`[image.warn] env` NROS_LOG_MAX_LEVEL=warn did not reach nros-log: {w}"
    );
    assert_eq!(field(&w, "warn_enabled"), "1", "{w}");
}
