//! RFC-0052 / phase-296 W3b.4/.5 — the cross-runtime contract-monitor parity
//! e2e (the W3b.4 + W3b.5 "done-when").
//!
//! A native three-process topology exercises the on-target contract monitors
//! over a real zenoh graph:
//!
//! * `contract-monitor-pub` bakes a `min_rate_hz` publisher contract on
//!   `/cm_header` and publishes a `std_msgs/Header` whose stamp is aged by
//!   `CM_STALE_MS`.
//! * `contract-monitor-sub` bakes a `max_age_ms` subscriber contract on the
//!   same topic.
//! * both drain their executor's violation ring through the `nros-diagnostics`
//!   reporter and publish `DiagnosticArray` on `/diagnostics`; the
//!   `contract-monitor-diagsink` observer prints one `DIAG rule=<id>` line per
//!   status.
//!
//! Violating config (slow 2 Hz publish < the 10 Hz declared minimum + 2 s
//! stale stamps) must surface BOTH `rate-hierarchy-runtime` (pub side) and
//! `max-age-runtime` (sub side) on `/diagnostics`; the compliant twin (20 Hz,
//! fresh stamps) stays silent while still delivering. The rule ids are the
//! play_launch runtime-enforcement vocabulary (RFC-0050), so the SAME contract
//! reports in the SAME words on the Linux runtime — the cross-runtime parity.
//!
//! ## Why cross-process
//!
//! zenoh-pico does not deliver in-process (see `deployed_native_system_e2e`),
//! and the age monitor can only fire on a message it RECEIVES from another
//! process. So the pub, sub, and diagsink are three separate processes on one
//! zenohd router.

use std::{process::Command, time::Duration};

use nros_tests::{
    fixtures::{
        ManagedProcess, ZenohRouter, build_contract_monitor_diagsink, build_contract_monitor_pub,
        build_contract_monitor_sub, require_zenohd, zenohd_unique,
    },
    output::{
        CONTRACT_MONITOR_DIAG_PREFIX, CONTRACT_MONITOR_DIAGSINK_READY_MARKER, RULE_MAX_AGE_RUNTIME,
        RULE_RATE_HIERARCHY_RUNTIME,
    },
};
use rstest::rstest;

/// Spawn one contract-monitor bin wired to the shared router.
fn spawn(
    bin: &std::path::Path,
    name: &str,
    locator: &str,
    envs: &[(&str, &str)],
) -> ManagedProcess {
    let mut cmd = Command::new(bin);
    cmd.env("RUST_LOG", "info")
        .env("NROS_LOCATOR", locator)
        .env("NROS_SESSION_MODE", "client");
    for (k, v) in envs {
        cmd.env(k, v);
    }
    ManagedProcess::spawn_command(cmd, name.to_string()).expect("spawn contract-monitor bin")
}

/// Violating pair: the slow + stale publisher trips the rate contract and the
/// sub's age contract; both rules land on `/diagnostics`.
#[rstest]
fn contract_monitor_violations_report_on_diagnostics(zenohd_unique: ZenohRouter) {
    if !require_zenohd() {
        nros_tests::skip!("zenohd not found");
    }
    let pub_bin = match build_contract_monitor_pub() {
        Ok(p) => p,
        Err(e) => nros_tests::skip!("contract-monitor-pub fixture not built: {e}"),
    };
    let sub_bin = build_contract_monitor_sub().expect("contract-monitor-sub fixture");
    let diagsink_bin =
        build_contract_monitor_diagsink().expect("contract-monitor-diagsink fixture");
    let locator = zenohd_unique.locator();

    // Observer first, so its /diagnostics subscription is live before either
    // monitor publishes. Long window so it outlives the ~13 s rate warm-up.
    let mut diagsink = spawn(
        diagsink_bin,
        "cm-diagsink",
        &locator,
        &[("CM_RUN_MS", "32000")],
    );
    diagsink
        .wait_for_output_pattern(
            CONTRACT_MONITOR_DIAGSINK_READY_MARKER,
            Duration::from_secs(8),
        )
        .expect("diagsink did not become ready");

    let mut sub = spawn(sub_bin, "cm-sub", &locator, &[("CM_RUN_MS", "30000")]);
    sub.wait_for_output_pattern("subscribed", Duration::from_secs(8))
        .expect("sub did not become ready");

    // Slow (2 Hz < 10 Hz declared) + stale (2 s > 200 ms declared).
    let mut publisher = spawn(
        pub_bin,
        "cm-pub",
        &locator,
        &[
            ("CM_RUN_MS", "28000"),
            ("CM_PERIOD_MS", "500"),
            ("CM_STALE_MS", "2000"),
        ],
    );

    // Age fires as soon as a stale message is taken (fast); rate needs two
    // ~5 s windows to measure, so give it a generous ceiling.
    let age_out = diagsink
        .wait_for_output_count(RULE_MAX_AGE_RUNTIME, 1, Duration::from_secs(14))
        .unwrap_or_default();
    let rate_out = diagsink
        .wait_for_output_count(RULE_RATE_HIERARCHY_RUNTIME, 1, Duration::from_secs(18))
        .unwrap_or_default();

    publisher.kill();
    sub.kill();
    diagsink.kill();

    let seen = format!("{age_out}{rate_out}");
    assert!(
        seen.contains(RULE_MAX_AGE_RUNTIME),
        "expected max-age-runtime on /diagnostics (stale stamp), got:\n{seen}"
    );
    assert!(
        seen.contains(RULE_RATE_HIERARCHY_RUNTIME),
        "expected rate-hierarchy-runtime on /diagnostics (slow publish), got:\n{seen}"
    );
}

/// Compliant twin: a fast (20 Hz) publisher with fresh stamps meets both
/// contracts, so `/diagnostics` stays silent while messages still flow.
#[rstest]
fn contract_monitor_compliant_pair_stays_silent(zenohd_unique: ZenohRouter) {
    if !require_zenohd() {
        nros_tests::skip!("zenohd not found");
    }
    let pub_bin = match build_contract_monitor_pub() {
        Ok(p) => p,
        Err(e) => nros_tests::skip!("contract-monitor-pub fixture not built: {e}"),
    };
    let sub_bin = build_contract_monitor_sub().expect("contract-monitor-sub fixture");
    let diagsink_bin =
        build_contract_monitor_diagsink().expect("contract-monitor-diagsink fixture");
    let locator = zenohd_unique.locator();

    let mut diagsink = spawn(
        diagsink_bin,
        "cm-diagsink-ok",
        &locator,
        &[("CM_RUN_MS", "18000")],
    );
    diagsink
        .wait_for_output_pattern(
            CONTRACT_MONITOR_DIAGSINK_READY_MARKER,
            Duration::from_secs(8),
        )
        .expect("diagsink did not become ready");

    let mut sub = spawn(sub_bin, "cm-sub-ok", &locator, &[("CM_RUN_MS", "16000")]);
    sub.wait_for_output_pattern("subscribed", Duration::from_secs(8))
        .expect("sub did not become ready");

    // Fast (20 Hz) + fresh (0 ms stale) meets min_rate_hz AND max_age_ms.
    let mut publisher = spawn(
        pub_bin,
        "cm-pub-ok",
        &locator,
        &[
            ("CM_RUN_MS", "14000"),
            ("CM_PERIOD_MS", "50"),
            ("CM_STALE_MS", "0"),
        ],
    );

    // Confirm the graph is alive (the sub is receiving) so silence means
    // "no violation", not "no traffic".
    sub.wait_for_output_count("received header", 5, Duration::from_secs(10))
        .expect("compliant sub received no messages — graph not alive");

    // Drain the observer across two rate windows (~12 s): no rule may appear.
    let diag_out = diagsink
        .wait_for_output_pattern(CONTRACT_MONITOR_DIAG_PREFIX, Duration::from_secs(12))
        .unwrap_or_default();

    publisher.kill();
    sub.kill();
    diagsink.kill();

    assert!(
        !diag_out.contains(CONTRACT_MONITOR_DIAG_PREFIX),
        "compliant pair must not report any contract violation on /diagnostics, got:\n{diag_out}"
    );
}
