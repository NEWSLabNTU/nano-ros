//! Phase 212.L.5 — top-level init API tests.
//!
//! Exercises the application-pkg pattern of the canonical init surface:
//!
//! - `nros::init()` — reads `ROS_DOMAIN_ID` / `NROS_LOCATOR` etc. into a
//!   [`Context`].
//! - The deprecated `nros::init_with_launch_auto()` /
//!   `nros::init_with_launch(path)` — the same environment read (and, for the
//!   path form, an existence check), kept working until they are removed.
//!   They read no launch file and report `ContextSource::Env` (issue 1917).
//!
//! Pattern 1 (`nros::node!`) is covered by the existing component
//! macro tests in Phase 172.W.3 (`nros-macros` unit tests +
//! `component_runtime_*` integration suites) — not re-asserted here.
//!
//! These tests mutate process env vars. To avoid cross-test leakage
//! they run sequentially through a process-wide mutex (`cargo test`
//! runs `#[test]`s in parallel by default within a single binary).

use std::{
    env,
    sync::{Mutex, OnceLock},
};

fn env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

/// RAII guard that restores an env var to its previous value on drop.
struct EnvGuard {
    key: &'static str,
    prev: Option<std::ffi::OsString>,
}

impl EnvGuard {
    fn set(key: &'static str, value: &str) -> Self {
        let prev = env::var_os(key);
        // SAFETY: tests serialise env access through `env_lock()`.
        unsafe { env::set_var(key, value) };
        Self { key, prev }
    }

    fn unset(key: &'static str) -> Self {
        let prev = env::var_os(key);
        // SAFETY: tests serialise env access through `env_lock()`.
        unsafe { env::remove_var(key) };
        Self { key, prev }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        // SAFETY: tests serialise env access through `env_lock()`.
        unsafe {
            match &self.prev {
                Some(v) => env::set_var(self.key, v),
                None => env::remove_var(self.key),
            }
        }
    }
}

#[test]
fn nros_init_returns_context() {
    let _g = env_lock().lock().unwrap();
    // Scrub the relevant env so we see the defaults.
    let _g1 = EnvGuard::unset("ROS_DOMAIN_ID");
    let _g2 = EnvGuard::unset("NROS_LOCATOR");
    let _g4 = EnvGuard::unset("NROS_SESSION_MODE");
    let _g6 = EnvGuard::unset("NROS_RMW");
    let _g7 = EnvGuard::unset("RMW_IMPLEMENTATION");

    let ctx = nros::init().expect("nros::init returned Err on a clean env");
    assert_eq!(ctx.domain_id, 0, "default domain id should be 0");
    // Issue 0330 — `nros` is RMW-agnostic: unset env leaves the locator EMPTY
    // and the linked backend substitutes its own default (zenoh's is the
    // loopback; cyclonedds ignores the locator entirely).
    assert_eq!(
        ctx.locator, "",
        "default locator should be empty (backend substitutes its default)"
    );
    assert_eq!(
        ctx.source,
        nros::ContextSource::Env,
        "init() context should be sourced from env"
    );

    // Pattern 3 hook — Context must materialise an ExecutorConfig the
    // existing `Executor::open` API consumes (we don't actually open a
    // session here; that would require a backend).
    let cfg = ctx.config("phase212_l5_node");
    assert_eq!(cfg.node_name, "phase212_l5_node");
    assert_eq!(cfg.domain_id, 0);
}

#[test]
#[allow(deprecated)] // the deprecated spelling must keep working until removed
fn nros_init_with_launch_auto_applies_env_defaults() {
    let _g = env_lock().lock().unwrap();
    let _g1 = EnvGuard::set("ROS_DOMAIN_ID", "42");
    let _g2 = EnvGuard::set("NROS_LOCATOR", "tcp/10.0.0.1:7777");
    let _g3 = EnvGuard::set("RMW_IMPLEMENTATION", "rmw_zenoh_cpp");
    let _g5 = EnvGuard::unset("NROS_RMW");

    let ctx = nros::init_with_launch_auto().expect("init_with_launch_auto failed");
    assert_eq!(
        ctx.domain_id, 42,
        "ROS_DOMAIN_ID=42 must propagate through the deprecated spelling too"
    );
    assert_eq!(ctx.locator, "tcp/10.0.0.1:7777");
    assert_eq!(ctx.rmw, "rmw_zenoh_cpp");
    assert_eq!(
        ctx.source,
        nros::ContextSource::Env,
        "init_with_launch_auto reads the environment, so the context says Env"
    );
}

/// phase-484 W8c — the retired `ZENOH_LOCATOR` alias is REFUSED by
/// `nros::init`, naming `NROS_LOCATOR`; an ignored locator would dial the
/// default router and read as "no delivery".
#[test]
fn nros_init_refuses_retired_locator_alias() {
    let _g = env_lock().lock().unwrap();
    let _g1 = EnvGuard::unset("ROS_DOMAIN_ID");
    let _g2 = EnvGuard::unset("NROS_LOCATOR");
    let retired = nros::RETIRED_ENV_VARS[0].0;
    let _g3 = EnvGuard::set(retired, "tcp/127.0.0.1:7447");
    let err = nros::init().expect_err("a set retired alias must refuse");
    assert_eq!(
        err,
        nros::InitError::RetiredEnvVar {
            name: retired,
            replacement: "NROS_LOCATOR"
        }
    );
    assert!(err.to_string().contains("NROS_LOCATOR"), "{err}");
}

#[test]
#[allow(deprecated)] // the deprecated spelling must keep working until removed
fn nros_init_with_launch_path_resolves_relative() {
    let _g = env_lock().lock().unwrap();
    let _g1 = EnvGuard::set("ROS_DOMAIN_ID", "7");
    let _g2 = EnvGuard::unset("NROS_LOCATOR");

    // Stage a launch file. Its contents are not read; only its existence
    // is checked.
    let tmp = tempfile::tempdir().expect("tempdir");
    let path = tmp.path().join("system.launch.xml");
    std::fs::write(
        &path,
        r#"<launch><node pkg="phase212_l5_pkg" exec="phase212_l5_exec"/></launch>"#,
    )
    .expect("write launch file");

    let ctx = nros::init_with_launch(&path).expect("init_with_launch w/ valid path");
    assert_eq!(ctx.domain_id, 7);
    assert_eq!(ctx.source, nros::ContextSource::Env);

    // Missing path → LaunchFileNotFound (so misspelled paths fail fast).
    let missing = tmp.path().join("does-not-exist.launch.xml");
    let err = nros::init_with_launch(&missing).expect_err("missing path should error");
    assert_eq!(err, nros::InitError::LaunchFileNotFound);
}
