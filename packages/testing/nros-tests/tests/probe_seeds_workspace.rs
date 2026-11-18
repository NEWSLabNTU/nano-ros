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

fn assert_probed_with_launch_values(pkg: &str, component: &str, language: &str) {
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
    assert!(
        DECLARED > DEFAULT_STORE,
        "the premise: more than a default store"
    );
    assert_eq!(
        params, DECLARED,
        "the {language} component declares {DECLARED} parameters; the probe's store must hold \
         every one of them"
    );
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

#[test]
fn a_cpp_component_is_probed_with_its_launch_values() {
    assert_probed_with_launch_values("seeds_cpp_pkg", "seeds_cpp", "C++");
}
