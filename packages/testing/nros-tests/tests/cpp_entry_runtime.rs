//! Native C++ Entry-pkg **runtime** E2E (Phase 235.A.4).
//!
//! Phase 235.A replaced `NativeBoard`'s recording NodeContextOps with the real
//! `NativeNodeRuntime` (`packages/core/nros-cpp/include/nros/main.hpp`): a
//! timer-driven `Publishes` effect synthesizes a monotonic `std_msgs/Int32`
//! counter; a `Reads` effect drains its subscription.
//!
//! External-observer proof (RFC-0032 §8): boot the prebuilt
//! `multi-node-workspace-cpp` Entry binary for a bounded window and confirm a
//! stock native Rust `listener` — a separate process subscribing to `/chatter`
//! over the same zenohd router — actually receives the talker node's samples.
//!
//! The cmake build runs in the **build stage** — the `cpp_robot_entry` cmake
//! fixture (`compile-check-fixtures.sh`, run by `build-test-fixtures`) builds
//! `robot_entry` into `build/cmake-fixtures/cpp_robot_entry/`. This test runs
//! the prebuilt binary rather than running cmake at run time (issue 0034 /
//! AGENTS.md "No compilation inside tests"). Shares the build with
//! `cpp_multi_node_entry`.

use std::{process::Command, time::Duration};

use nros_tests::{
    fixtures::{
        ManagedProcess, ZenohRouter, build_native_listener, require_cmake_fixture, require_zenohd,
        zenohd_unique,
    },
    output::parse_listener,
};
use rstest::rstest;

#[rstest]
fn cpp_entry_runtime_publishes_live_samples(zenohd_unique: ZenohRouter) {
    if !require_zenohd() {
        panic!("zenohd not found; build it via `just zenohd setup` to run this test");
    }

    // The prebuilt C++ Entry binary (build stage). Absent → tier-aware
    // skip/fail via the resolver (run `just build-test-fixtures`).
    let robot_entry = require_cmake_fixture("cpp_robot_entry", "src/robot_entry/robot_entry")
        .expect("cpp_robot_entry fixture (run `just build-test-fixtures`)");
    let listener_bin = build_native_listener().expect("native listener fixture");

    let locator = zenohd_unique.locator();

    // External observer first: a stock native listener on /chatter.
    let mut listener_cmd = Command::new(listener_bin);
    listener_cmd
        .env("RUST_LOG", "info")
        .env("NROS_LOCATOR", &locator)
        .env("NROS_SESSION_MODE", "client");
    let mut listener =
        ManagedProcess::spawn_command(listener_cmd, "ext-listener").expect("spawn listener");
    listener
        .wait_for_output_pattern("Waiting for", Duration::from_secs(8))
        .expect("listener did not become ready");

    // Boot the C++ Entry pkg for a bounded window — the synthesized talker fires
    // a 1 Hz Int32 counter through the live runtime.
    let mut entry_cmd = Command::new(&robot_entry);
    entry_cmd
        .env("RUST_LOG", "info")
        .env("NROS_LOCATOR", &locator)
        .env("NROS_SESSION_MODE", "client")
        .env("NROS_ENTRY_SPIN_MS", "8000");
    let mut entry =
        ManagedProcess::spawn_command(entry_cmd, "robot_entry").expect("spawn robot_entry");

    let listener_output = listener
        .wait_for_output_count("Received:", 1, Duration::from_secs(20))
        .unwrap_or_else(|e| {
            entry.kill();
            listener.kill();
            panic!("external listener never received a sample from the C++ Entry runtime: {e}");
        });

    entry.kill();
    listener.kill();

    println!("=== ext-listener output ===\n{listener_output}");
    let parsed = parse_listener(&listener_output);
    assert!(
        !parsed.values.is_empty(),
        "expected ≥1 Int32 sample from the live C++ Entry talker, got none:\n{listener_output}"
    );
    println!(
        "SUCCESS: C++ Entry runtime published {} live sample(s): {:?}",
        parsed.values.len(),
        parsed.values
    );
}
