//! Phase 264 W4c — runtime E2E for the declarative in-callback parameter read
//! (nros↔nros, no ROS 2 required).
//!
//! The `ws-params-rust` workspace node (`param_talker`) reads its parameter LIVE every
//! tick via `ctx.parameter::<i64>("publish_period_ms")` and publishes that value on
//! `/chatter`. The model resolves it to 120, so a correctly-wired W4c read makes a
//! cross-process nros subscriber observe `Received: 120` — proving the whole chain:
//! `[param_services]` seeds the volatile store → the component cell captures the store
//! pointer at registration → `dispatch_into_cell` threads it onto `CallbackCtx` →
//! `ctx.parameter` reads the live value → it reaches the wire.
//!
//! The `ros2 param set` reconfig half (which needs a wire-matched `rmw_zenoh_cpp`
//! overlay) lives in `tests/params.rs` (the ROS 2 interop lane).
//!
//! Run with: `cargo nextest run -p nros-tests --test param_live_read_e2e`

use nros_tests::fixtures::{
    ManagedProcess, ZenohRouter, build_native_workspace_rust_params_entry, require_zenohd,
    zenohd_unique,
};
use rstest::rstest;
use std::{process::Command, time::Duration};

// The resolved value and the two wrong ones, each naming the rule it breaks.
// Shared with `tests/params.rs` (the `ros2 param set` half) — they used to carry
// their own numbers and disagreed about which one was correct (issue 0409).
use nros_tests::output::param_talker::{ORDERING_LOST, RESOLVED, SPECIFICITY_LOST};

/// Spawn the `param_talker` workspace entry on `locator`, spinning for `spin_ms`.
fn spawn_param_entry(locator: &str, spin_ms: u32) -> ManagedProcess {
    let entry = build_native_workspace_rust_params_entry()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|e| nros_tests::skip!("params workspace entry fixture not built: {e}"));
    let mut cmd = Command::new(entry);
    cmd.env("RUST_LOG", "info")
        .env("NROS_LOCATOR", locator)
        .env("NROS_SESSION_MODE", "client")
        .env("NROS_ENTRY_SPIN_MS", spin_ms.to_string())
        .env("NROS_ENTRY_SPIN_STEP_MS", "10");
    ManagedProcess::spawn_command(cmd, "param_talker").expect("spawn param_talker entry")
}

/// W4c + the parameter-resolution rules, end to end on a binary that ran.
///
/// The node reads its parameter LIVE in its callback via `ctx.parameter::<i64>`
/// and publishes the value, so ONE number on the wire adjudicates the whole
/// chain: model file → `NodeInstance::resolved_params` → `nros::main!` bake →
/// running binary. The fixture model declares that parameter through two
/// ORDERED sources whose file has two sections, which makes each failure mode
/// distinguishable rather than just "no messages":
///
/// | on the wire | meaning |
/// |---|---|
/// | 120 | correct |
/// | 250 | source ORDERING lost — an inline value beat a later file (the pre-phase-54 bug) |
/// | 999 | section SPECIFICITY lost — `/**` beat the node's own block for being written later |
///
/// Unit tests cover each rule in isolation; this is the one place they are
/// asserted against a real process.
#[rstest]
fn param_live_read_publishes_resolved_value(zenohd_unique: ZenohRouter) {
    if !require_zenohd() {
        nros_tests::skip!("zenohd not found");
    }
    let locator = zenohd_unique.locator();

    let mut listener = nros_tests::fixtures::spawn_int32_sink(None, &locator);
    let mut entry = spawn_param_entry(&locator, 8000);

    // The published value IS the live param read. Wait on the PREFIX, not on the
    // expected number: on a wrong value the wait would otherwise time out with
    // `TestError::Timeout`, which carries no output — and the whole point of
    // this fixture is that the wrong value NAMES the rule that broke.
    let out = listener
        .wait_for_output_count(
            nros_tests::output::INT32_LISTENER_LOG_PREFIX,
            3,
            Duration::from_secs(15),
        )
        .unwrap_or_else(|e| {
            entry.kill();
            listener.kill();
            panic!("subscriber saw no /chatter value at all — the live param read never reached the wire ({e})")
        });

    entry.kill();
    listener.kill();

    let saw = |v: i64| {
        nros_tests::count_pattern(&out, nros_tests::output::int32_listener_line(v).as_str())
    };
    if saw(RESOLVED) < 3 {
        // Name the specific rule that broke rather than just the number.
        let diagnosis = if saw(ORDERING_LOST) > 0 {
            "saw 250 — parameter SOURCE ORDERING was lost: an inline value beat a LATER param \
             file, but ROS applies sources in list order (play_launch issue 0007)"
        } else if saw(SPECIFICITY_LOST) > 0 {
            "saw 999 — within-file section SPECIFICITY was lost: the `/**` block beat the \
             node's own block because it is written later in the file"
        } else {
            "saw some other value entirely"
        };
        panic!(
            "expected at least 3 publishes of the resolved value {RESOLVED} on /chatter; \
             {diagnosis}. Output:\n{out}"
        );
    }
}
