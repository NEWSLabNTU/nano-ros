//! Phase 139.6 — PX4 integration template smoke test.
//!
//! Validates the `integrations/px4/module-template/` shape (PX4
//! requires an exact `src/CMakeLists.txt` +
//! `src/modules/<name>/CMakeLists.txt` layout) and runs
//! `make px4_sitl_default` with `EXTERNAL_MODULES_LOCATION=`
//! pointing at the template when PX4 is available.
//!
//! Uses `PX4_AUTOPILOT_DIR`, whose default is provided by `justfile`
//! and `.envrc`.

use std::{path::PathBuf, process::Command};

fn workspace_root() -> PathBuf {
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest
        .ancestors()
        .nth(3)
        .expect("workspace root above CARGO_MANIFEST_DIR")
        .to_path_buf()
}

#[test]
fn px4_integration_template_smoke() {
    let root = workspace_root();
    let template = root.join("integrations/px4/module-template");

    // Shape gates — these always run, even without PX4 installed.
    assert!(
        template.join("CMakeLists.txt").exists(),
        "integrations/px4/module-template/CMakeLists.txt missing",
    );
    assert!(
        template.join("src/CMakeLists.txt").exists(),
        "integrations/px4/module-template/src/CMakeLists.txt missing",
    );
    let module_dir = template.join("src/modules/nano_ros_app");
    assert!(
        module_dir.join("CMakeLists.txt").exists(),
        "module CMakeLists missing",
    );
    assert!(
        module_dir.join("nano_ros_app.cpp").exists(),
        "module source missing",
    );

    let src_cmake = std::fs::read_to_string(template.join("src/CMakeLists.txt"))
        .expect("read src/CMakeLists.txt");
    assert!(
        src_cmake.contains("config_module_list_external"),
        "PX4 EXTERNAL_MODULES_LOCATION entry must set config_module_list_external",
    );

    let mod_cmake = std::fs::read_to_string(module_dir.join("CMakeLists.txt"))
        .expect("read module CMakeLists.txt");
    assert!(
        mod_cmake.contains("px4_add_module"),
        "PX4 module CMakeLists must call px4_add_module",
    );

    // Heavy gate: the SITL build itself.
    let px4_dir = match std::env::var("PX4_AUTOPILOT_DIR") {
        Ok(d) => PathBuf::from(d),
        Err(_) => nros_tests::skip!(
            "PX4_AUTOPILOT_DIR unset — run via `just test-all`, load `.envrc`, \
             or set it to a PX4-Autopilot checkout"
        ),
    };
    if !px4_dir.join("Makefile").exists() {
        nros_tests::skip!(
            "PX4_AUTOPILOT_DIR={} does not look like a PX4 checkout (no Makefile) — \
             run `just px4 setup` or set PX4_AUTOPILOT_DIR",
            px4_dir.display()
        );
    }

    // A full SITL build can take 5-10 minutes; here we just verify
    // PX4's Make can list targets under the EXTERNAL_MODULES_LOCATION
    // we point it at. Anything heavier belongs in per-RTOS CI.
    let listing = Command::new("make")
        .arg("-C")
        .arg(&px4_dir)
        .env("EXTERNAL_MODULES_LOCATION", &template)
        .args(["px4_sitl_default", "--just-print", "-n"])
        .output();
    match listing {
        Ok(out) if out.status.success() => {}
        Ok(out) => panic!(
            "make px4_sitl_default --just-print failed: stdout={}\nstderr={}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr),
        ),
        Err(e) => panic!("invoking make failed: {}", e),
    }
}
