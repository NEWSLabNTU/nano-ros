//! Phase 212.L.5 — top-level init API tests.
//!
//! Exercises Pattern 2 / Pattern 3 of the canonical init surface:
//!
//! - Pattern 3 `nros::init()` — raw, launch-ignoring init. Reads
//!   `ROS_DOMAIN_ID` / `NROS_LOCATOR` etc. into a [`Context`].
//! - Pattern 2 `nros::init_with_launch_auto()` — launch-aware init.
//!   Today the env vars are the active overlay channel (the launcher
//!   projects them into the child env before exec). Real launch XML
//!   parsing lands with the runtime-overlay follow-up wave.
//! - Pattern 2 `nros::init_with_launch(path)` — explicit-path variant.
//!   Verifies path existence; XML parse is the same follow-up.
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
    let _g3 = EnvGuard::unset("ZENOH_LOCATOR");
    let _g4 = EnvGuard::unset("NROS_SESSION_MODE");
    let _g5 = EnvGuard::unset("ZENOH_MODE");
    let _g6 = EnvGuard::unset("NROS_RMW");
    let _g7 = EnvGuard::unset("RMW_IMPLEMENTATION");

    let ctx = nros::init().expect("nros::init returned Err on a clean env");
    assert_eq!(ctx.domain_id, 0, "default domain id should be 0");
    assert_eq!(
        ctx.locator, "tcp/127.0.0.1:7447",
        "default locator should be the zenoh loopback"
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
fn nros_init_with_launch_auto_applies_env_defaults() {
    let _g = env_lock().lock().unwrap();
    let _g1 = EnvGuard::set("ROS_DOMAIN_ID", "42");
    let _g2 = EnvGuard::set("NROS_LOCATOR", "tcp/10.0.0.1:7777");
    let _g3 = EnvGuard::set("RMW_IMPLEMENTATION", "rmw_zenoh_cpp");
    // Scrub the legacy aliases so they don't shadow the new vars.
    let _g4 = EnvGuard::unset("ZENOH_LOCATOR");
    let _g5 = EnvGuard::unset("NROS_RMW");

    let ctx = nros::init_with_launch_auto().expect("init_with_launch_auto failed");
    assert_eq!(
        ctx.domain_id, 42,
        "ROS_DOMAIN_ID=42 must propagate through the launch overlay path"
    );
    assert_eq!(ctx.locator, "tcp/10.0.0.1:7777");
    assert_eq!(ctx.rmw, "rmw_zenoh_cpp");
    assert_eq!(
        ctx.source,
        nros::ContextSource::Launch,
        "init_with_launch_auto must tag the context as Launch-sourced"
    );
}

#[test]
fn nros_init_with_launch_path_resolves_relative() {
    let _g = env_lock().lock().unwrap();
    let _g1 = EnvGuard::set("ROS_DOMAIN_ID", "7");
    let _g2 = EnvGuard::unset("NROS_LOCATOR");
    let _g3 = EnvGuard::unset("ZENOH_LOCATOR");

    // Stage a placeholder launch file (XML parsing isn't wired this
    // wave — see module docs; the runtime only verifies the path
    // exists today).
    let tmp = tempfile::tempdir().expect("tempdir");
    let path = tmp.path().join("system.launch.xml");
    std::fs::write(
        &path,
        r#"<launch><node pkg="phase212_l5_pkg" exec="phase212_l5_exec"/></launch>"#,
    )
    .expect("write launch file");

    let ctx = nros::init_with_launch(&path).expect("init_with_launch w/ valid path");
    assert_eq!(ctx.domain_id, 7);
    assert_eq!(ctx.source, nros::ContextSource::Launch);

    // Missing path → LaunchFileNotFound (so misspelled paths fail fast).
    let missing = tmp.path().join("does-not-exist.launch.xml");
    let err = nros::init_with_launch(&missing).expect_err("missing path should error");
    assert_eq!(err, nros::InitError::LaunchFileNotFound);
}

/// Follow-up — real launch.xml param/remap overlay (Option A — JSON
/// sidecar emitted by `nros launch --emit-runtime-overlay`). Not yet
/// wired; see `init_with_launch_auto` doc comment.
#[test]
#[ignore = "Phase 212.L.5 follow-up: structured launch overlay (NROS_RUNTIME_OVERLAY JSON sidecar)"]
fn nros_init_with_launch_auto_applies_xml_params() {
    // Placeholder: will assert that `<param name=\"start_value\"
    // value=\"99\"/>` from the launch XML lands in the Context's
    // parameter overlay once Option A wires up.
}
