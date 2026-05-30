//! PX4 SITL end-to-end test for nano-ros uORB modules.
//!
//! Builds PX4 SITL with `examples/px4/rust/uorb/{talker,listener}`
//! linked in via `EXTERNAL_MODULES_LOCATION`, boots the simulator
//! using [`px4_sitl_tests::Px4Sitl::boot_in`], starts both modules,
//! and asserts the listener observes at least one matching `recv:`
//! line within a fixed time budget.
//!
//! ## Preconditions
//!
//! - px4-rs SITL test dependency populated (run `just px4 setup`).
//! - PX4 SITL build prerequisites installed (cmake, ninja, gcc, py3).
//! - `PX4_AUTOPILOT_DIR` env var pointing at a PX4-Autopilot checkout
//!   (provided by `just px4 test-sitl`, `just test-all`, or `.envrc`).
//!
//! Per CLAUDE.md's "no silent skip" rule, this test PANICS (does not
//! report PASS via a [SKIPPED] line) when preconditions are unmet.
//! The whole test suite is gated behind the `px4-sitl` Cargo feature
//! so plain `just ci` does not enable it; you opt in via
//! `just px4 test-sitl`.
//!
//! ## Reuse strategy
//!
//! Heavy lifting (subprocess spawn, stdout drainer threads, line-tail
//! with regex matching, SIGTERM-then-SIGKILL process-group cleanup on
//! Drop) comes from `px4-sitl-tests`'s [`Px4Sitl`] fixture. This test
//! only writes the build invocation that points
//! `EXTERNAL_MODULES_LOCATION` at nano-ros's example modules — then
//! hands the resulting build directory to [`Px4Sitl::boot_in`].
//!
//! See `docs/roadmap/phase-98-px4-autopilot-integration.md` for the
//! design rationale.
//!
//! Phase 208.D.3 — lives in its own crate (`nros-px4-sitl-test`) with
//! an empty `[workspace]` table so the nano-ros root workspace doesn't
//! try to resolve the `px4-sitl-tests` path dep when the `px4-rs`
//! submodule is absent (the audit blocker P3). Run via
//! `just px4 test-sitl`.

use std::{env, path::PathBuf, process::Command, time::Duration};

use px4_sitl_tests::Px4Sitl;

const RECV_TIMEOUT: Duration = Duration::from_secs(15);

/// Project root, computed from this test's `CARGO_MANIFEST_DIR`.
fn project_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .expect("canonicalize project root")
}

/// Per CLAUDE.md no-silent-skip rule, panics with an actionable
/// message if the configured path is invalid.
fn ensure_px4_autopilot_dir() -> PathBuf {
    let dir = env::var("PX4_AUTOPILOT_DIR").expect(
        "PX4_AUTOPILOT_DIR unset. Run via `just px4 test-sitl`, \
         `just test-all`, load `.envrc`, or set it to a PX4-Autopilot checkout.",
    );
    let path = PathBuf::from(&dir);
    assert!(
        path.join("Tools").is_dir(),
        "PX4_AUTOPILOT_DIR={dir} does not look like a PX4 checkout (missing Tools/). \
         Run `just px4 setup` or set PX4_AUTOPILOT_DIR."
    );
    path
}

/// Invoke `make px4_sitl_default EXTERNAL_MODULES_LOCATION=…` to build
/// PX4 SITL with our nano-ros example modules linked in. Returns the
/// path to the build directory containing `bin/px4`.
fn build_sitl_with_nros_externals() -> PathBuf {
    let px4 = ensure_px4_autopilot_dir();
    let externals = project_root().join("examples/px4/rust/uorb");
    assert!(
        externals.join("talker/Cargo.toml").is_file()
            && externals.join("listener/Cargo.toml").is_file(),
        "examples/px4/rust/uorb/{{talker,listener}} not found at {}",
        externals.display()
    );

    eprintln!(
        "Building PX4 SITL: make -C {} px4_sitl_default EXTERNAL_MODULES_LOCATION={}",
        px4.display(),
        externals.display()
    );
    let status = Command::new("make")
        .current_dir(&px4)
        .arg("px4_sitl_default")
        .arg(format!("EXTERNAL_MODULES_LOCATION={}", externals.display()))
        .status()
        .expect("invoke make");
    assert!(
        status.success(),
        "PX4 SITL build failed (exit {:?})",
        status.code()
    );

    let build_dir = px4.join("build/px4_sitl_default");
    let bin = build_dir.join("bin/px4");
    assert!(
        bin.is_file(),
        "expected {} after build, but it is missing",
        bin.display()
    );
    build_dir
}

#[test]
fn px4_sitl_talker_listener_round_trip() {
    let build_dir = build_sitl_with_nros_externals();

    // Boot via px4-sitl-tests fixture: subprocess + drainer threads +
    // SIGTERM-process-group cleanup on Drop, all reused.
    let sitl = Px4Sitl::boot_in(&build_dir).expect("Px4Sitl::boot_in");

    sitl.shell("nros_listener start")
        .expect("start nros_listener");
    // Brief gap so the listener subscription is in place before the
    // talker's first publish.
    std::thread::sleep(Duration::from_millis(500));
    sitl.shell("nros_talker start").expect("start nros_talker");

    // px4-sitl-tests' wait_for_log takes a &str pattern (compiled
    // internally to a Regex). Match the px4-log output shape from
    // examples/px4/rust/uorb/listener/src/lib.rs.
    // px4-sitl-tests wait_for_log uses substring match, NOT regex.
    // Match a literal prefix that the listener prints once per delivered
    // message (see examples/px4/rust/uorb/listener/src/lib.rs).
    let recv_pat = "recv: ts=";
    let line = match sitl.wait_for_log(recv_pat, RECV_TIMEOUT) {
        Ok(line) => line,
        Err(e) => {
            // Dump full daemon log so we can see what actually happened
            // (modules-started? recv'd 0 messages? other errors?).
            let snapshot = sitl.log_snapshot();
            panic!(
                "wait_for_log timed out: {e:?}\n=== daemon log snapshot ===\n{snapshot}\n=== end snapshot ==="
            );
        }
    };
    assert!(
        line.contains("recv:"),
        "matched line did not contain 'recv:': {line}"
    );

    eprintln!("Observed recv line: {line}");
    // Drop(sitl) -> SIGTERM process group, 3 s grace, SIGKILL.
}
