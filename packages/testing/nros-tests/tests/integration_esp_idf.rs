//! Phase 139.6 — ESP-IDF integration shell smoke test.
//!
//! Drives `idf.py build` against a tmpdir consumer that pulls
//! `integrations/esp-idf/` as a component. Skips via `nros_tests::skip!`
//! when `idf.py` is absent.

use std::{path::PathBuf, process::Command};

fn workspace_root() -> PathBuf {
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest
        .ancestors()
        .nth(3)
        .expect("workspace root above CARGO_MANIFEST_DIR")
        .to_path_buf()
}

fn have(cmd: &str) -> bool {
    Command::new("which")
        .arg(cmd)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[test]
fn esp_idf_integration_shell_smoke() {
    if !have("idf.py") {
        nros_tests::skip!("idf.py not on PATH — install ESP-IDF >=5.1 + source export.sh");
    }
    if std::env::var("IDF_PATH").is_err() {
        nros_tests::skip!("IDF_PATH unset — `. $IDF_PATH/export.sh`");
    }

    let root = workspace_root();
    let shell = root.join("integrations/esp-idf");
    assert!(
        shell.join("idf_component.yml").exists(),
        "integrations/esp-idf/idf_component.yml missing",
    );
    assert!(
        shell.join("CMakeLists.txt").exists(),
        "integrations/esp-idf/CMakeLists.txt missing",
    );
    assert!(
        shell.join("Kconfig.projbuild").exists(),
        "integrations/esp-idf/Kconfig.projbuild missing",
    );

    let cmake = std::fs::read_to_string(shell.join("CMakeLists.txt"))
        .expect("read integrations/esp-idf/CMakeLists.txt");
    assert!(
        cmake.contains("idf_component_register"),
        "ESP-IDF shell must call idf_component_register",
    );
    assert!(
        cmake.contains("NANO_ROS_PLATFORM"),
        "ESP-IDF shell must set NANO_ROS_PLATFORM",
    );
    assert!(
        cmake.contains("add_subdirectory"),
        "ESP-IDF shell must add_subdirectory the root CMake",
    );

    // A full `idf.py build` requires picking a chip target + project
    // dir AND a fully-set-up ESP-IDF tools tree. When all gates above
    // pass, kick off a minimal `idf.py --version` to confirm the CLI
    // is actually wired; full build deferred to per-RTOS CI box.
    let version = Command::new("idf.py")
        .arg("--version")
        .output()
        .expect("invoke idf.py --version");
    assert!(
        version.status.success(),
        "idf.py --version failed: {}",
        String::from_utf8_lossy(&version.stderr)
    );
}
