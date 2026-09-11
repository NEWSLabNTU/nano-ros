//! Phase 269 W1 — E2E for C/C++ in-callback live parameter read.
//!
//! The `ws-params-c` / `ws-params-cpp` workspace entries boot a single node that:
//! 1. Gets `publish_period_ms = 250` seeded into the executor's volatile store by the
//!    generated `nros_cpp_declare_param` call in `__nros_entry_setup` (emit_c/cpp.rs W1).
//! 2. Reads that value LIVE each tick via `nros_cpp_get_param_integer` (both C and C++)
//!    and publishes it on `/chatter`.
//!
//! A cross-process nros listener must see `Received: 250` ≥3 times — proving the full
//! chain: emit seeds param → store holds it → component reads live → reaches the wire.
//!
//! phase-426 W5 — a THIRD arm, and the one this file was missing. The two above
//! are single-language images: each reads a value the launch file seeded, and
//! neither ever asks whether the OTHER language is looking at the same store.
//! W5's stated acceptance was "a mixed C/C++ workspace fixture declares from one
//! language and reads from the other", and no such fixture existed.
//! `mixed_param_talker_pkg` is it — one node, a C++ component and a C
//! translation unit, each declaring one parameter and reading the other's.
//!
//! `ros2 param set` reconfig half lives in the ROS 2 interop lane (needs rmw_zenoh_cpp).
//!
//! Run with: `cargo nextest run -p nros-tests --test cpp_c_param_live_read_e2e`

use nros_tests::fixtures::{
    ManagedProcess, ZenohRouter, build_native_workspace_c_params_entry,
    build_native_workspace_cpp_params_entry, build_native_workspace_mixed_params_entry,
    require_zenohd, zenohd_unique,
};
use rstest::rstest;
use std::{path::PathBuf, process::Command, time::Duration};

fn spawn_entry(path: PathBuf, label: &str, locator: &str, spin_ms: u32) -> ManagedProcess {
    let mut cmd = Command::new(path);
    cmd.env("RUST_LOG", "info")
        .env("NROS_LOCATOR", locator)
        .env("NROS_SESSION_MODE", "client")
        .env("NROS_ENTRY_SPIN_MS", spin_ms.to_string())
        .env("NROS_ENTRY_SPIN_STEP_MS", "10");
    ManagedProcess::spawn_command(cmd, label).expect("spawn entry")
}

/// C component reads the launch-baked initial (250) LIVE via nros_cpp_get_param_integer.
#[rstest]
fn c_param_live_read_publishes_baked_initial(zenohd_unique: ZenohRouter) {
    if !require_zenohd() {
        nros_tests::skip!("zenohd not found");
    }
    let path = build_native_workspace_c_params_entry()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|e| nros_tests::skip!("ws-params-c entry fixture not built: {e}"));

    let locator = zenohd_unique.locator();
    let mut listener = nros_tests::fixtures::spawn_int32_sink(None, &locator);
    // Entry must keep publishing for at least the listener's wait window
    // (20 s below): the hosted spin self-terminates after NROS_ENTRY_SPIN_MS,
    // and under concurrent-test load the session open + subscriber discovery
    // can eat several seconds before the first delivery. An 8 s entry could die
    // before the sink accumulated 3 samples — a load-only flake (issue 0387),
    // NOT a param-read bug (the chain passes alone/serial). Outlive the wait.
    let mut entry = spawn_entry(path, "c_param_talker", &locator, 25000);

    let out = listener
        .wait_for_output_count(
            nros_tests::output::int32_listener_line(250).as_str(),
            3,
            Duration::from_secs(20),
        )
        .unwrap_or_else(|_| {
            entry.kill();
            listener.kill();
            panic!(
                "C component never published live-read baked param (250) on /chatter — \
                 nros_cpp_get_param_integer did not reach the callback"
            )
        });

    entry.kill();
    listener.kill();

    let n = nros_tests::count_pattern(&out, nros_tests::output::int32_listener_line(250).as_str());
    assert!(n >= 3, "expected ≥3 live-read publishes of 250, got {n}");
}

/// C++ component reads the launch-baked initial (250) LIVE via nros_cpp_get_param_integer
/// on the executor handle saved from node.executor_handle() at configure time.
#[rstest]
fn cpp_param_live_read_publishes_baked_initial(zenohd_unique: ZenohRouter) {
    if !require_zenohd() {
        nros_tests::skip!("zenohd not found");
    }
    let path = build_native_workspace_cpp_params_entry()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|e| nros_tests::skip!("ws-params-cpp entry fixture not built: {e}"));

    let locator = zenohd_unique.locator();
    let mut listener = nros_tests::fixtures::spawn_int32_sink(None, &locator);
    // Outlive the 20 s listener wait — see the C arm for why 8 s flaked
    // under concurrent load (issue 0387).
    let mut entry = spawn_entry(path, "cpp_param_talker", &locator, 25000);

    let out = listener
        .wait_for_output_count(
            nros_tests::output::int32_listener_line(250).as_str(),
            3,
            Duration::from_secs(20),
        )
        .unwrap_or_else(|_| {
            entry.kill();
            listener.kill();
            panic!(
                "C++ component never published live-read baked param (250) on /chatter — \
                 nros_cpp_get_param_integer on executor_handle did not reach the callback"
            )
        });

    entry.kill();
    listener.kill();

    let n = nros_tests::count_pattern(&out, nros_tests::output::int32_listener_line(250).as_str());
    assert!(n >= 3, "expected ≥3 live-read publishes of 250, got {n}");
}

/// phase-426 W5 — C and C++ declare and read ACROSS each other on one node.
///
/// The image publishes `publish_period_ms * scale`. `publish_period_ms` is
/// declared in C++ (adopting the launch seed, 250) and read in C on every tick;
/// `scale` is declared in C (3.0) and read in C++ at configure. So `750` on the
/// wire is the conjunction of both crossings, and it is a number neither
/// language writes down: a store that does not cross publishes `-1 * 0`, `0`,
/// or nothing at all, and the boot-time check in `configure` fails loudly
/// first.
#[rstest]
fn mixed_c_cpp_param_declare_and_read_cross_languages(zenohd_unique: ZenohRouter) {
    if !require_zenohd() {
        nros_tests::skip!("zenohd not found");
    }
    let path = build_native_workspace_mixed_params_entry()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|e| nros_tests::skip!("mixed-params entry fixture not built: {e}"));

    let locator = zenohd_unique.locator();
    let mut listener = nros_tests::fixtures::spawn_int32_sink(None, &locator);
    // Outlive the 20 s listener wait — see the C arm for why 8 s flaked
    // under concurrent load (issue 0387).
    let mut entry = spawn_entry(path, "mixed_param_talker", &locator, 25000);

    const CROSSED: i32 = 250 * 3;
    let out = listener
        .wait_for_output_count(
            nros_tests::output::int32_listener_line(CROSSED).as_str(),
            3,
            Duration::from_secs(20),
        )
        .unwrap_or_else(|_| {
            entry.kill();
            listener.kill();
            panic!(
                "mixed image never published {CROSSED} on /chatter — C and C++ did not \
                 reach the same parameter store (C++ declares `publish_period_ms` and \
                 reads `scale`; C declares `scale` and reads `publish_period_ms`)"
            )
        });

    entry.kill();
    listener.kill();

    let n = nros_tests::count_pattern(
        &out,
        nros_tests::output::int32_listener_line(CROSSED).as_str(),
    );
    assert!(
        n >= 3,
        "expected >=3 cross-language publishes of {CROSSED}, got {n}"
    );
}
