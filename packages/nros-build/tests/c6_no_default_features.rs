//! Phase 212.C.6 gate — `nros-build` degrades to a warn-only no-op when
//! no RMW Cargo feature is selected (matches the Phase 118.B probe
//! hazard: `cargo check --no-default-features` must not break).
//!
//! The gate fires INSIDE `Codegen::run()` BEFORE binary resolution and
//! BEFORE interface discovery, so the fixture does NOT need a real
//! `package.xml` or a real `nros` binary — that is the entire point of
//! the no-op path.
//!
//! NOTE: tests in this file mutate process-wide env. They must run
//! sequentially. We serialize on a static `Mutex` rather than splitting
//! into two integration-test binaries (one per gate direction would
//! double the build time for ~zero coverage gain).

use nros_build::{Codegen, Lang, RunOutcome};
use std::sync::Mutex;

static ENV_LOCK: Mutex<()> = Mutex::new(());

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
        // SAFETY: single-threaded section (held under ENV_LOCK).
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

#[test]
fn no_rmw_feature_degrades_to_warn_only_no_op() {
    let _serial = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());

    let tmp = tempfile::tempdir().unwrap();
    let out_dir = tmp.path().join("out");
    std::fs::create_dir_all(&out_dir).unwrap();
    let pkg_dir = tmp.path().join("pkg");
    std::fs::create_dir_all(&pkg_dir).unwrap();
    let pkg_xml = pkg_dir.join("package.xml");
    // Empty file is fine — gate fires before discovery reads it.

    let _guard = EnvGuard::snapshot(&[
        "OUT_DIR",
        "CARGO_FEATURE_RMW_ZENOH",
        "CARGO_FEATURE_RMW_XRCE",
        "CARGO_FEATURE_RMW_CYCLONEDDS",
        "CARGO_FEATURE_RMW",
    ]);

    // SAFETY: integration-test binary holds ENV_LOCK.
    unsafe {
        std::env::set_var("OUT_DIR", &out_dir);
        std::env::remove_var("CARGO_FEATURE_RMW_ZENOH");
        std::env::remove_var("CARGO_FEATURE_RMW_XRCE");
        std::env::remove_var("CARGO_FEATURE_RMW_CYCLONEDDS");
        std::env::remove_var("CARGO_FEATURE_RMW");
    }

    let outcome = Codegen::new(&pkg_xml, Lang::Rust)
        .feature_gate("RMW")
        .emit_rerun(false)
        .run()
        .expect("212.C.6 gate: no-RMW must be a no-op, not an error");

    assert!(
        matches!(outcome, RunOutcome::SkippedNoFeature),
        "expected RunOutcome::SkippedNoFeature, got {outcome:?}"
    );

    // No-op contract: `$OUT_DIR/nros-gen/.stamp` MUST NOT exist (codegen
    // never ran, so no digest was committed).
    let stamp = out_dir.join("nros-gen").join(".stamp");
    assert!(
        !stamp.exists(),
        "no-op path must not write a stamp file: {}",
        stamp.display()
    );
}

#[test]
fn one_rmw_feature_set_skips_the_no_op_path() {
    let _serial = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());

    // Inverse gate: when ANY `CARGO_FEATURE_RMW*` is set, the no-op path
    // must NOT fire. We assert this indirectly: with the gate disabled,
    // `Codegen::run()` proceeds to discovery, which fails on a missing
    // `package.xml` with `BuildError::Io` (NOT `RunOutcome::SkippedNoFeature`).
    let tmp = tempfile::tempdir().unwrap();
    let out_dir = tmp.path().join("out");
    std::fs::create_dir_all(&out_dir).unwrap();

    let _guard = EnvGuard::snapshot(&[
        "OUT_DIR",
        "CARGO_FEATURE_RMW_ZENOH",
        "CARGO_FEATURE_RMW_XRCE",
        "CARGO_FEATURE_RMW_CYCLONEDDS",
    ]);

    // SAFETY: integration-test binary holds ENV_LOCK.
    unsafe {
        std::env::set_var("OUT_DIR", &out_dir);
        std::env::set_var("CARGO_FEATURE_RMW_ZENOH", "1");
        std::env::remove_var("CARGO_FEATURE_RMW_XRCE");
        std::env::remove_var("CARGO_FEATURE_RMW_CYCLONEDDS");
    }

    let err = Codegen::new("/nonexistent/package.xml", Lang::Rust)
        .feature_gate("RMW")
        .emit_rerun(false)
        .run()
        .expect_err("with RMW feature set, gate must not no-op; discovery must run");

    let msg = err.to_string();
    assert!(
        !msg.contains("no `RMW` Cargo feature"),
        "with RMW set, the no-RMW warning must not appear: {msg}"
    );
}
