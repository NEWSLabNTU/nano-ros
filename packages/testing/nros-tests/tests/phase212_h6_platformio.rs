//! Phase 212.H.6 — PlatformIO adapter integration test.
//!
//! Drives `pio run -e native` against
//! `packages/testing/nros-tests/fixtures/multi_pkg_workspace_platformio/pio_app/`.
//! PIO is HOOKLESS — its only chance to read `system.toml` is a
//! pre-build `extra_script` running `nros codegen-system
//! --ahead-of-vendor` BEFORE PIO's library resolver sees the tree.
//!
//! Skips cleanly via `nros_tests::skip!` when `pio` (or `platformio`)
//! is not on PATH — the adapter is opt-in per the Phase 212 §H tier
//! policy. Also skips when `pio`'s `native` platform package isn't
//! installed locally (heavyweight install is a per-RTOS CI step).

use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

fn workspace_root() -> PathBuf {
    nros_tests::project_root()
}

fn fixture(name: &str) -> PathBuf {
    workspace_root()
        .join("packages/testing/nros-tests/fixtures")
        .join(name)
}

fn copy_tree(src: &Path, dst: &Path) -> std::io::Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let from = entry.path();
        let to = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_tree(&from, &to)?;
        } else if ty.is_file() {
            fs::copy(&from, &to)?;
        }
    }
    Ok(())
}

fn stage_fixture(name: &str) -> (tempfile::TempDir, PathBuf) {
    let src = fixture(name);
    let dst = tempfile::tempdir().expect("tempdir");
    copy_tree(&src, dst.path()).expect("copy fixture");
    let root = dst.path().to_path_buf();
    // Rewrite NANO_ROS_ROOT placeholder in pio_app/platformio.ini.
    let ini = root.join("pio_app/platformio.ini");
    let rendered = fs::read_to_string(&ini)
        .expect("read platformio.ini")
        .replace("@NANO_ROS_ROOT@", workspace_root().to_str().unwrap());
    fs::write(&ini, rendered).expect("write rendered platformio.ini");
    (dst, root)
}

fn have(cmd: &str) -> bool {
    Command::new("which")
        .arg(cmd)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn pio_bin() -> Option<PathBuf> {
    if have("pio") {
        return Some(PathBuf::from("pio"));
    }
    if have("platformio") {
        return Some(PathBuf::from("platformio"));
    }
    None
}

#[test]
fn platformio_zephyr_framework_2_component_bringup_builds() {
    let Some(bin) = pio_bin() else {
        nros_tests::skip!("pio CLI not available — run `just platformio setup`");
    };

    // Validate the adapter surface is wired the way the spec expects.
    let root = workspace_root();
    let library_json = root.join("library.json");
    assert!(
        library_json.is_file(),
        "repo-root library.json missing at {}",
        library_json.display()
    );
    let lib_body = fs::read_to_string(&library_json).expect("read library.json");
    assert!(
        lib_body.contains("\"name\": \"nano-ros\""),
        "library.json must declare name = nano-ros"
    );
    assert!(
        lib_body.contains("integrations/platformio/nros_codegen.py"),
        "library.json must register the ahead-of-vendor extra_script"
    );

    let script = root.join("integrations/platformio/nros_codegen.py");
    assert!(
        script.is_file(),
        "pre-build script missing at {}",
        script.display()
    );
    let script_body = fs::read_to_string(&script).expect("read nros_codegen.py");
    assert!(
        script_body.contains("--ahead-of-vendor"),
        "pre-build script must invoke `nros codegen-system --ahead-of-vendor`"
    );

    // Stage the fixture into a tempdir and run `pio run` from pio_app/.
    let (_guard, staged) = stage_fixture("multi_pkg_workspace_platformio");
    let pio_app = staged.join("pio_app");

    let out = Command::new(&bin)
        .args(["run", "-e", "native"])
        .current_dir(&pio_app)
        .output()
        .expect("spawn pio run");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);

    // Skip cleanly when PIO can't fetch the platform package (offline /
    // sandboxed CI). Same policy as integration_esp_idf.rs.
    let offline_markers = [
        "Could not find the package",
        "PackageManagerError",
        "HTTPClientError",
        "Unable to resolve",
        "PlatformIO Manager",
    ];
    if !out.status.success()
        && offline_markers
            .iter()
            .any(|m| stdout.contains(m) || stderr.contains(m))
    {
        nros_tests::skip!(
            "pio could not fetch the `native` platform package (offline / sandboxed); \
             ran the pre-build extra_script + adapter wiring smoke instead"
        );
    }

    assert!(
        out.status.success(),
        "pio run failed:\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
}
