//! Phase 212.C.7 gate — when the `nros` CLI cannot be resolved via
//! `$NROS_BIN` → PATH → `~/.nros/bin/nros`, `Codegen::run()` MUST hard
//! fail with an install pointer that names BOTH:
//!
//!   * `scripts/install-nros.sh` (the bootstrap entry-point), and
//!   * `https://github.com/NEWSLabNTU/nros-cli` (the upstream source).
//!
//! Companion to `tests/missing_nros.rs` (unit-level coverage of
//! `find_nros_binary`); this test wires the failure through the
//! `Codegen::run()` integration path that downstream `build.rs`
//! callers actually hit.

use nros_build::{BuildError, Codegen, Lang};

struct EnvGuard {
    saved: Vec<(&'static str, Option<std::ffi::OsString>)>,
}

impl EnvGuard {
    fn snapshot(keys: &[&'static str]) -> Self {
        let saved = keys
            .iter()
            .map(|k| (*k, std::env::var_os(k)))
            .collect::<Vec<_>>();
        Self { saved }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        // SAFETY: single-threaded integration test owns its env.
        unsafe {
            for (k, v) in &self.saved {
                match v {
                    Some(val) => std::env::set_var(k, val),
                    None => std::env::remove_var(k),
                }
            }
        }
    }
}

/// Minimal `package.xml` so discovery passes; the test wants the
/// failure to fall on binary resolution, not on parsing.
fn write_min_package_xml(dir: &std::path::Path) -> std::path::PathBuf {
    let p = dir.join("package.xml");
    std::fs::write(
        &p,
        r#"<?xml version="1.0"?>
<package format="3">
  <name>nros_c7_fixture</name>
  <version>0.0.1</version>
  <description>Phase 212.C.7 gate fixture</description>
  <maintainer email="dev@example.com">dev</maintainer>
  <license>Apache-2.0</license>
</package>
"#,
    )
    .unwrap();
    p
}

#[test]
fn missing_nros_binary_through_codegen_run_hard_fails_with_install_pointer() {
    let tmp = tempfile::tempdir().unwrap();
    let out_dir = tmp.path().join("out");
    std::fs::create_dir_all(&out_dir).unwrap();
    let pkg_dir = tmp.path().join("pkg");
    std::fs::create_dir_all(&pkg_dir).unwrap();
    let pkg_xml = write_min_package_xml(&pkg_dir);

    let empty_path = tmp.path().join("empty-path");
    std::fs::create_dir_all(&empty_path).unwrap();

    let _guard = EnvGuard::snapshot(&[
        "OUT_DIR",
        "PATH",
        "HOME",
        "NROS_HOME",
        "NROS_BIN",
        "CARGO_FEATURE_RMW_ZENOH",
    ]);

    // SAFETY: integration-test binary owns its env exclusively.
    unsafe {
        std::env::set_var("OUT_DIR", &out_dir);
        // Disable the C.6 no-op so we reach binary resolution.
        std::env::set_var("CARGO_FEATURE_RMW_ZENOH", "1");
        // Wipe every resolution route: no $NROS_BIN, empty PATH, fresh
        // $HOME with no `.nros/bin/nros`.
        std::env::remove_var("NROS_BIN");
        std::env::set_var("PATH", &empty_path);
        std::env::set_var("HOME", tmp.path());
        std::env::remove_var("NROS_HOME");
    }

    let err = Codegen::new(&pkg_xml, Lang::Rust)
        .feature_gate("RMW")
        .emit_rerun(false)
        .run()
        .expect_err("212.C.7: missing `nros` must hard fail");

    assert!(
        matches!(err, BuildError::MissingBinary(_)),
        "expected BuildError::MissingBinary, got {err:?}"
    );

    let msg = err.to_string();
    assert!(
        msg.contains("install-nros.sh"),
        "install pointer must name `scripts/install-nros.sh`: {msg}"
    );
    assert!(
        msg.contains("github.com/NEWSLabNTU/nros-cli"),
        "install pointer must include the upstream URL \
         `https://github.com/NEWSLabNTU/nros-cli`: {msg}"
    );
    assert!(
        msg.contains("NROS_BIN"),
        "install pointer should mention the `NROS_BIN` env override: {msg}"
    );
}
