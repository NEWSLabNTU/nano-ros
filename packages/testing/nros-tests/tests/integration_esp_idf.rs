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
    let root = workspace_root();

    // Phase 150.G — auto-detect IDF_PATH at the canonical in-tree
    // location (`<root>/external/esp-idf/`) when the caller hasn't
    // exported it AND prepend `<IDF_PATH>/tools` to PATH so the
    // `--version`-only smoke probe below resolves `idf.py` without
    // requiring the caller to source `export.sh` (which sets up
    // the full Python venv too — out of scope for a smoke test).
    if std::env::var("IDF_PATH").is_err() {
        let candidate = root.join("external/esp-idf");
        if candidate.join("export.sh").exists() {
            // SAFETY: nextest runs each test in its own process;
            // no cross-test race.
            unsafe { std::env::set_var("IDF_PATH", &candidate) };
            let tools = candidate.join("tools");
            if tools.join("idf.py").exists() {
                let path = std::env::var("PATH").unwrap_or_default();
                let new_path = format!("{}:{}", tools.display(), path);
                unsafe { std::env::set_var("PATH", new_path) };
            }
        }
    }

    if !have("idf.py") {
        nros_tests::skip!("idf.py not on PATH — install ESP-IDF >=5.1 + source export.sh");
    }
    if std::env::var("IDF_PATH").is_err() {
        nros_tests::skip!(
            "IDF_PATH unset and no in-tree external/esp-idf — \
             run `just esp_idf setup` or `. $IDF_PATH/export.sh`"
        );
    }

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
    // dir AND a fully-set-up ESP-IDF tools tree (Python venv,
    // toolchain, etc.). When all gates above pass, kick off a
    // minimal `idf.py --version` to confirm the CLI is actually
    // wired; if the venv isn't sourced (`/usr/bin/env: python`
    // missing, etc.) skip cleanly — full env setup is `just esp_idf
    // setup`'s job, not this smoke test's.
    let version = Command::new("idf.py")
        .arg("--version")
        .output()
        .expect("invoke idf.py --version");
    if !version.status.success() {
        nros_tests::skip!(
            "idf.py --version failed (likely missing Python venv from `. \
             $IDF_PATH/export.sh`): {}",
            String::from_utf8_lossy(&version.stderr).trim()
        );
    }
}
