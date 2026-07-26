//! phase-307 W6 lane 2 (issue 0257) — a metadata-derived executor capacity
//! actually BOOTS, and delivers.
//!
//! The `ws-sizing-rust` node registers six timers and no subscription. Launch
//! wiring has no timer entity, so the SystemModel names ZERO callback entities
//! for it while the runtime needs six slots; the executor's table defaults to
//! four. Sized from the model alone `nros::main!` emits no sizing at all and
//! registration dies with `ExecutorFull("burst_pkg")` — which is exactly the
//! shape issue 0257 reported from a real deployment. Sized from the
//! `nros sync`-produced source-metadata sidecar, which records all six timers,
//! it derives eight and boots.
//!
//! So this test's assertion is deliberately end-of-chain rather than clever:
//! the entry cannot reach a clean spin unless the sidecar was produced (W1/W2),
//! found and read by the macro (W4b), merged with the model bound, and applied
//! to `Executor::open_sized`. Every wave in the phase is load-bearing for one
//! process exiting normally.
//!
//! Two lanes:
//!   * boot — the six-timer node registers and spins to completion;
//!   * delivery — its timers publish, and a separate process receives.
//!
//! Run with: `cargo nextest run -p nros-tests --test executor_sizing_e2e`

use nros_tests::fixtures::{
    ManagedProcess, ZenohRouter, build_int32_sink, build_native_workspace_rust_sizing_entry,
    require_zenohd, zenohd_unique,
};
use rstest::rstest;
use std::{process::Command, time::Duration};

/// Spawn the six-timer entry on `locator`, spinning for `spin_ms`.
fn spawn_sizing_entry(locator: &str, spin_ms: u32) -> ManagedProcess {
    let entry = build_native_workspace_rust_sizing_entry()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|e| nros_tests::skip!("sizing workspace entry fixture not built: {e}"));
    let mut cmd = Command::new(entry);
    cmd.env("RUST_LOG", "info")
        .env("NROS_LOCATOR", locator)
        .env("NROS_SESSION_MODE", "client")
        .env("NROS_ENTRY_SPIN_MS", spin_ms.to_string())
        .env("NROS_ENTRY_SPIN_STEP_MS", "10");
    ManagedProcess::spawn_command(cmd, "burst_talker").expect("spawn burst_talker entry")
}

/// Boot lane. A four-slot executor cannot hold six timers, so reaching the
/// hosted-spin-complete marker is proof the capacity came from the recorded
/// entity count and not from the model's blind zero.
#[rstest]
fn six_timer_node_boots_on_a_metadata_derived_executor(zenohd_unique: ZenohRouter) {
    if !require_zenohd() {
        nros_tests::skip!("zenohd not found");
    }
    let mut entry = spawn_sizing_entry(&zenohd_unique.locator(), 3000);

    let out = entry
        .wait_for_output_pattern(
            nros_tests::output::HOSTED_SPIN_COMPLETE_MARKER,
            Duration::from_secs(30),
        )
        .unwrap_or_else(|err| {
            entry.kill();
            panic!(
                "the six-timer entry never completed its spin ({err}). A registration \
                 failure here means the executor was sized from the SystemModel's \
                 timer-blind count (issue 0257): check that `nros sync` produced \
                 src/burst_pkg/metadata/burst_talker.json and that `nros::main!` read it"
            )
        });
    entry.kill();

    // Registration failure is LOUD, never a degraded boot — assert we did not
    // merely survive with fewer callbacks than declared.
    assert!(
        !out.contains("ExecutorFull"),
        "the executor ran out of callback slots — the sidecar-derived sizing did not \
         reach `Executor::open_sized`:\n{out}"
    );
}

/// Delivery lane. Booting is necessary but not sufficient: the timers must
/// actually dispatch and their publishes must reach another process.
#[rstest]
fn six_timer_node_delivers_to_a_separate_process(zenohd_unique: ZenohRouter) {
    if !require_zenohd() {
        nros_tests::skip!("zenohd not found");
    }
    let locator = zenohd_unique.locator();

    let listener = build_int32_sink()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|e| nros_tests::skip!("native listener fixture not built: {e}"));
    let mut cmd = Command::new(listener);
    cmd.env("RUST_LOG", "info")
        .env("NROS_LOCATOR", &locator)
        .env("NROS_SESSION_MODE", "client");
    let mut listener = ManagedProcess::spawn_command(cmd, "listener").expect("spawn listener");
    listener
        .wait_for_output_pattern(
            nros_tests::output::INT32_SINK_READY_MARKER,
            Duration::from_secs(15),
        )
        .expect("listener did not become ready");

    let mut entry = spawn_sizing_entry(&locator, 8000);

    // Six timers at 50–100 ms produce samples quickly; requiring several rules
    // out a single lucky publish from a partially-registered node.
    let seen = listener.wait_for_output_count(
        nros_tests::output::INT32_LISTENER_LOG_PREFIX,
        5,
        Duration::from_secs(25),
    );
    entry.kill();
    listener.kill();
    seen.unwrap_or_else(|_| {
        panic!(
            "the six-timer node booted but delivered nothing — its timer callbacks \
             did not dispatch"
        )
    });
}
