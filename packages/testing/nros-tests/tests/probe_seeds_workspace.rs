//! Issue 2640 (phase-566) -- the C/C++ metadata probe constructs a component
//! with its LAUNCH values, in a store that holds every declaration.
//!
//! `fixtures/probe_seeds_ws` holds one C and one C++ component. Each reads
//! `ctrl_period_ms` with NO default in code (C: `rclc_add_parameter`, C++:
//! `declare_parameter<int64_t>(name)`) and declares 40 more parameters, more
//! than a default parameter store holds (32). The launch file gives each its
//! period. Probed without the launch values, the C component refuses to
//! configure and the C++ one creates no timer; probed with them, each sidecar
//! records its timer at the launch period and all 41 parameters.
//!
//! Issue 2901: the C++ component is launched twice, agreeing on the period
//! and differing on `gain_03` (which has a default). The probe seeds the
//! agreed period and leaves only `gain_03` on its default, so the timer is
//! still recorded.
//!
//! The probe runs in the build stage (`workspace-probe-seeds` row): this test
//! reads the sidecars that build left beside each package.

use std::path::PathBuf;

use nros_tests::{
    fixtures::{self, RequireFixture},
    project_root,
};

const WS: &str = "packages/testing/nros-tests/fixtures/probe_seeds_ws";
/// Parameters each component declares: the period and 40 gains.
const DECLARED: usize = 41;
/// A parameter store at its build default holds this many.
const DEFAULT_STORE: usize = 32;
// The premise of the test: more parameters than a default store holds.
const _: () = assert!(DECLARED > DEFAULT_STORE);

fn sidecar(pkg: &str, component: &str) -> serde_json::Value {
    // The built image is the evidence the probe ran on this tree; the sidecar
    // is what it left.
    fixtures::build_workspace_cmake_generated_entry("workspace-probe-seeds", "linux_entry")
        .require("prebuilt probe_seeds_ws linux_entry");
    let path: PathBuf = project_root()
        .join(WS)
        .join("src")
        .join(pkg)
        .join("metadata")
        .join(format!("{component}.json"));
    let text =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("parse {}: {e}", path.display()))
}

/// `ctrl_period_ms` of node `name` in the launch file, read rather than
/// restated so the test cannot pass against a fixture that changed under it.
fn launch_period(name: &str) -> u64 {
    let path = project_root()
        .join(WS)
        .join("src/seeds_bringup/launch/system.launch.xml");
    let text = std::fs::read_to_string(&path).unwrap();
    let node = text
        .split("<node ")
        .find(|n| n.contains(&format!("name=\"{name}\"")))
        .unwrap_or_else(|| panic!("no node `{name}` in {}", path.display()));
    let value = node
        .split("name=\"ctrl_period_ms\" value=\"")
        .nth(1)
        .and_then(|rest| rest.split('"').next())
        .unwrap_or_else(|| panic!("node `{name}` sets no ctrl_period_ms"));
    value.parse().expect("an integer period")
}

/// The timer at the launch period and every declared parameter: what a probe
/// that was handed `ctrl_period_ms` records. Returns the sidecar.
fn assert_probed_with_launch_period(
    pkg: &str,
    component: &str,
    language: &str,
) -> serde_json::Value {
    let doc = sidecar(pkg, component);
    let period = launch_period(component);
    let timers: Vec<u64> = doc["nodes"][0]["timers"]
        .as_array()
        .expect("timers")
        .iter()
        .map(|t| t["period_ms"].as_u64().expect("period_ms"))
        .collect();
    assert_eq!(
        timers,
        vec![period],
        "the {language} component reads its period from the launch file only; a probe that \
         did not hand it the launch value records no timer (or the C component fails to \
         configure)"
    );
    let params = doc["parameters"].as_array().expect("parameters").len();
    assert_eq!(
        params, DECLARED,
        "the {language} component declares {DECLARED} parameters; the probe's store must hold \
         every one of them"
    );
    doc
}

fn assert_probed_with_launch_values(pkg: &str, component: &str, language: &str) {
    let doc = assert_probed_with_launch_period(pkg, component, language);
    let seeds = &doc["probe"]["seeds"];
    assert_eq!(
        seeds["instances"],
        serde_json::json!([format!("/{component}")]),
        "the sidecar names the launch instance it was seeded from: {doc:#}"
    );
    assert!(
        seeds["digest"]
            .as_str()
            .is_some_and(|d| d.starts_with("sha256:")),
        "{seeds:#}"
    );
}

#[test]
fn a_c_component_is_probed_with_its_launch_values() {
    assert_probed_with_launch_values("seeds_c_pkg", "seeds_c", "C");
}

// Issue 2901.
/// Two launch instances that disagree on one defaulted key still seed every
/// key they agree on: the period reaches the probe, so the timer is recorded,
/// and only the disagreeing key is named as left on its default.
#[test]
fn a_cpp_component_whose_instances_disagree_on_one_key_is_seeded_with_the_rest() {
    let doc = assert_probed_with_launch_period("seeds_cpp_pkg", "seeds_cpp", "C++");
    assert_eq!(
        launch_period("seeds_cpp_b"),
        launch_period("seeds_cpp"),
        "the fixture's premise: both instances set the same period"
    );
    let partial = &doc["probe"]["partial_seeds"];
    assert_eq!(
        partial["instances"],
        serde_json::json!(["/seeds_cpp", "/seeds_cpp_b"]),
        "the sidecar names both instances: {doc:#}"
    );
    assert_eq!(
        partial["defaulted"],
        serde_json::json!(["gain_03"]),
        "only the key the instances disagree on runs on its default: {doc:#}"
    );
    let seeded: Vec<&str> = partial["seeded"]
        .as_array()
        .expect("seeded")
        .iter()
        .filter_map(|k| k.as_str())
        .collect();
    assert_eq!(seeded, ["ctrl_period_ms"], "{doc:#}");
    assert!(
        partial["digest"]
            .as_str()
            .is_some_and(|d| d.starts_with("sha256:")),
        "{partial:#}"
    );
}
