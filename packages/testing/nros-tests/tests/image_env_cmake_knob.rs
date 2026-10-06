//! Issue 1712 — a C/C++ leaf's `[image.<id>] env` (RFC-0049's APP rung)
//! reaches the cargo builds the CMAKE road drives.
//!
//! The cargo road writes the rows into `nros-cargo.toml` `[env]`; the cmake
//! road parsed them and dropped them, so a C image could override its board
//! only through whatever shell ran `cmake --build`. It now hands them to every
//! cargo command as a `--config` file (cargo `[env]`, no `force`), which keeps
//! them BELOW an exported variable.
//!
//! The fixture (`bins/image-env-probe-c`, row `image-env-probe-c`) states two
//! knobs read by two different crates' build scripts — nros-node's
//! `NROS_EXECUTOR_MAX_CBS`, which nros-c prints into the C header as
//! `NROS_EXECUTOR_MAX_HANDLES`, and nros-log's `NROS_LOG_MAX_LEVEL` — at values
//! that are neither builtin. Both are READ from its `system.toml` here rather
//! than restated, and the binary's own report must match them. No compilation
//! here: the fixture is built by `build-test-fixtures`.

use std::process::Command;

use nros_tests::{
    fixtures::{self, RequireFixture, Rmw},
    output, project_root,
};

const LEAF: &str = "packages/testing/nros-tests/bins/image-env-probe-c";

/// nros-node's builtin `NROS_EXECUTOR_MAX_CBS`.
const BUILTIN_MAX_CBS: usize = 4;

/// `[image.native] env` of the probe, as authored.
fn image_env() -> toml::Table {
    let path = project_root().join(LEAF).join("system.toml");
    let text =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    let v: toml::Value =
        toml::from_str(&text).unwrap_or_else(|e| panic!("parse {}: {e}", path.display()));
    v["image"]["native"]["env"]
        .as_table()
        .expect("the probe image states `[image.native] env`")
        .clone()
}

#[test]
fn a_c_leaf_image_env_reaches_the_cargo_builds_cmake_drives() {
    let env = image_env();
    let max_cbs: usize = env["NROS_EXECUTOR_MAX_CBS"]
        .as_str()
        .expect("the probe states NROS_EXECUTOR_MAX_CBS")
        .parse()
        .expect("a count");
    assert_ne!(
        max_cbs, BUILTIN_MAX_CBS,
        "the image states the builtin, which hides a dropped rung"
    );
    assert_eq!(
        env["NROS_LOG_MAX_LEVEL"].as_str(),
        Some("warn"),
        "the probe's line asserts a `warn` ceiling (INFO filtered, WARN kept)"
    );

    let bin = fixtures::build_cmake_leaf_rmw(LEAF, "image_env_probe_c", Rmw::Zenoh)
        .require("prebuilt image-env-probe-c");
    let out = Command::new(&bin)
        .output()
        .unwrap_or_else(|e| panic!("run {}: {e}", bin.display()));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "probe exited {:?}: {stdout}",
        out.status
    );
    let line = stdout
        .lines()
        .find_map(|l| l.strip_prefix(output::IMAGE_ENV_PROBE_LINE))
        .unwrap_or_else(|| panic!("no `{}` line in {stdout:?}", output::IMAGE_ENV_PROBE_LINE));
    let field = |k: &str| -> usize {
        line.split_whitespace()
            .find_map(|kv| kv.strip_prefix(&format!("{k}=")))
            .and_then(|v| v.parse().ok())
            .unwrap_or_else(|| panic!("no `{k}=` in {line:?}"))
    };

    assert_eq!(
        field("max_handles"),
        max_cbs,
        "nros-c's header was built without the image's NROS_EXECUTOR_MAX_CBS={max_cbs} \
         ({BUILTIN_MAX_CBS} is the builtin: the cmake road dropped the APP rung, issue 1712)"
    );
    assert_eq!(
        field("warn_enabled"),
        1,
        "WARN filtered too: logging is off altogether, so `info_enabled` says nothing"
    );
    assert_eq!(
        field("info_enabled"),
        0,
        "INFO passed a logger set to DEBUG, so nros-log was built without the image's \
         NROS_LOG_MAX_LEVEL=warn (issue 1712)"
    );
}
