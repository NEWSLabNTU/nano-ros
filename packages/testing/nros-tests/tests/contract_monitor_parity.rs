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

//!
//! ## The C++ twin (phase-462 W1)
//!
//! `contract-monitor-cpp` is `contract-monitor-pub` written against the C
//! ABI a generated C++ entry uses: the same row installed through
//! `nros_cpp_install_monitors`, the stock `Publisher<Header>` whose facade
//! bumps the row's cell, `nros_cpp_executor_drain_violations` for the drain.
//! Its two cases are the W1 "done-when": the violating twin reports
//! `rate-hierarchy-runtime` from the same row the Rust twin declares, and the
//! `CM_CONTRACT=0` twin installs zero rows and stays silent -- RFC-0052's
//! claim that an uncontracted image carries no monitor.

use std::{path::PathBuf, process::Command, time::Duration};

use nros_tests::{
    TestResult,
    fixtures::{
        ManagedProcess, RequireFixture, Rmw, ZenohRouter, build_cmake_leaf_rmw,
        build_contract_monitor_diagsink, build_contract_monitor_pub, build_contract_monitor_sub,
        build_monitor_capacity, require_zenohd, zenohd_unique,
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
        Err(e) => panic!("contract-monitor-pub fixture not built: {e}"),
    };
    let sub_bin = build_contract_monitor_sub().require("contract-monitor-sub");
    let diagsink_bin = build_contract_monitor_diagsink().require("contract-monitor-diagsink");
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
    // The timeout error carries what the diagsink actually printed, and that is
    // the whole diagnostic — `unwrap_or_default()` used to drop it, leaving an
    // empty `got:` that cannot distinguish "the rule never fired" from "the
    // observer printed nothing at all".
    //
    // But it must NOT flow into the string the assertions search: the error
    // text NAMES the pattern it was waiting for (`did not print
    // `max-age-runtime``), so folding it into `seen` makes
    // `seen.contains(RULE_MAX_AGE_RUNTIME)` match the complaint about the
    // missing rule and the test passes exactly when it should fail. Evidence
    // goes in the panic MESSAGE; only real output goes in `seen`.
    // issue 0670 — `collect_until_count` is this closure, promoted to
    // `nros_tests::process` so the next test needing it does not re-derive the
    // separation (and does not re-derive it WRONG: both obvious spellings,
    // `unwrap_or_default()` and `unwrap_or_else(|e| e.to_string())`, are the
    // traps described above).
    let mut why = String::new();
    let mut wait_rule = |proc: &mut ManagedProcess, rule: &str, secs: u64| -> String {
        let (out, diag) = proc.collect_until_count(rule, 1, Duration::from_secs(secs));
        if let Some(d) = diag {
            why.push_str(&d);
        }
        out
    };
    let age_out = wait_rule(&mut diagsink, RULE_MAX_AGE_RUNTIME, 14);
    let rate_out = wait_rule(&mut diagsink, RULE_RATE_HIERARCHY_RUNTIME, 18);

    publisher.kill();
    sub.kill();
    diagsink.kill();

    let seen = format!("{age_out}{rate_out}");
    assert!(
        seen.contains(RULE_MAX_AGE_RUNTIME),
        "expected max-age-runtime on /diagnostics (stale stamp), got:\n{seen}{why}"
    );
    assert!(
        seen.contains(RULE_RATE_HIERARCHY_RUNTIME),
        "expected rate-hierarchy-runtime on /diagnostics (slow publish), got:\n{seen}{why}"
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
        Err(e) => panic!("contract-monitor-pub fixture not built: {e}"),
    };
    let sub_bin = build_contract_monitor_sub().require("contract-monitor-sub");
    let diagsink_bin = build_contract_monitor_diagsink().require("contract-monitor-diagsink");
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
    let diag_out = diagsink.collect_until(CONTRACT_MONITOR_DIAG_PREFIX, Duration::from_secs(12));

    publisher.kill();
    sub.kill();
    diagsink.kill();

    assert!(
        !diag_out.contains(CONTRACT_MONITOR_DIAG_PREFIX),
        "compliant pair must not report any contract violation on /diagnostics, got:\n{diag_out}"
    );
}

/// phase-462 W1 -- the C++ twin, a CMake C++ leaf like any other
/// (`examples/fixtures.toml` row `contract-monitor-cpp`, prebuilt by the
/// linux/cpp/zenoh fixture lane).
fn build_contract_monitor_cpp() -> TestResult<PathBuf> {
    build_cmake_leaf_rmw(
        "packages/testing/nros-tests/bins/contract-monitor-cpp",
        "contract_monitor_cpp",
        Rmw::Zenoh,
    )
}

/// The row the Rust twin declares by hand (`pub.rs`'s `MONITORS`), as the
/// C++ twin prints the row it installed. Field for field: same topic, same
/// endpoint ref, same declared minimum, no latency contract.
const CPP_TWIN_ROW: &str = "cm_pub_cpp: row topic=/cm_header fqn=/cm/pub/cm_header min_rate_hz_milli=10000 max_latency_ms=0";

/// C++ twin, violating: one row installed through the C ABI, a publisher
/// held at 2 Hz under a 10 Hz contract, `rate-hierarchy-runtime` on the
/// drain within one check window -- the same rule, from the same row, the
/// Rust twin reports.
#[rstest]
fn contract_monitor_cpp_twin_reports_rate_violation(zenohd_unique: ZenohRouter) {
    if !require_zenohd() {
        nros_tests::skip!("zenohd not found");
    }
    let bin = build_contract_monitor_cpp().require("contract-monitor-cpp");
    let locator = zenohd_unique.locator();

    let mut publisher = spawn(
        &bin,
        "cm-pub-cpp",
        &locator,
        &[
            ("CM_RUN_MS", "28000"),
            ("CM_PERIOD_MS", "500"),
            ("CM_CONTRACT", "1"),
        ],
    );
    // One wait, for the row line: it is printed right after the install
    // line, from the same install, so seeing it is seeing both. (Two waits
    // in a row lose the second line when both arrive in one read.)
    publisher
        .wait_for_output_pattern(CPP_TWIN_ROW, Duration::from_secs(8))
        .expect("C++ twin did not install the Rust twin's row");

    // Rate needs two ~5 s windows to measure (open, then roll); same ceiling
    // as the Rust twin's case. Evidence goes in the panic message, never in
    // the string the assertion searches (see the Rust case for why).
    let (out, why) =
        publisher.collect_until_count(RULE_RATE_HIERARCHY_RUNTIME, 1, Duration::from_secs(18));
    publisher.kill();
    assert!(
        out.contains(RULE_RATE_HIERARCHY_RUNTIME),
        "expected rate-hierarchy-runtime on the C++ twin's drain (2 Hz < 10 Hz declared), got:\n{out}{}",
        why.unwrap_or_default()
    );
    assert!(
        out.contains("fqn=/cm/pub/cm_header declared=10000")
            || out.contains("fqn=/cm/pub/cm_header measured="),
        "the violation must name the installed row's fqn, got:\n{out}"
    );
}

/// C++ twin, uncontracted (`CM_CONTRACT=0`): the same binary, the same slow
/// publisher, zero rows installed -- and nothing on the drain, because there
/// is no row to check. This is RFC-0052's zero-cost claim at the row level.
#[rstest]
fn contract_monitor_cpp_uncontracted_twin_carries_zero_rows(zenohd_unique: ZenohRouter) {
    if !require_zenohd() {
        nros_tests::skip!("zenohd not found");
    }
    let bin = build_contract_monitor_cpp().require("contract-monitor-cpp");
    let locator = zenohd_unique.locator();

    let mut publisher = spawn(
        &bin,
        "cm-pub-cpp-uncontracted",
        &locator,
        &[
            ("CM_RUN_MS", "16000"),
            ("CM_PERIOD_MS", "500"),
            ("CM_CONTRACT", "0"),
        ],
    );
    publisher
        .wait_for_output_pattern(
            "cm_pub_cpp: installed 0 monitor rows",
            Duration::from_secs(8),
        )
        .expect("uncontracted C++ twin did not report its (empty) install");
    // Confirm it is publishing (so silence means "no row", not "no traffic"),
    // then drain across two rate windows: no rule may appear.
    publisher
        .wait_for_output_pattern("cm_pub_cpp: published 4 headers", Duration::from_secs(6))
        .expect("uncontracted C++ twin published nothing");
    let out = publisher.collect_until(CONTRACT_MONITOR_DIAG_PREFIX, Duration::from_secs(12));
    publisher.kill();
    assert!(
        !out.contains(CONTRACT_MONITOR_DIAG_PREFIX),
        "an uncontracted C++ image must report no contract violation, got:\n{out}"
    );
    assert!(
        !out.contains("cm_pub_cpp: row "),
        "an uncontracted C++ image must install no row, got:\n{out}"
    );
}

/// Run the capacity fixture with `MC_ROWS=<rows>` to completion and return
/// what it printed. `examples/fixtures.toml` row `monitor-capacity`, built with
/// `NROS_EXECUTOR_MAX_MONITORS=14` stated.
fn run_monitor_capacity(locator: &str, rows: &str) -> String {
    let bin = build_monitor_capacity().require("monitor-capacity");
    let mut p = spawn(bin, &format!("mc-{rows}"), locator, &[("MC_ROWS", rows)]);
    p.wait_for_all_output(Duration::from_secs(10))
        .unwrap_or_else(|e| panic!("monitor-capacity (MC_ROWS={rows}) did not finish: {e}"))
}

/// phase-467 W1 (issue 1471) -- the Autoware Safety Island's shape: 14
/// contracted rate rows on ONE executor install, where `MAX_MONITORS = 8`
/// refused them (the C++ setup returned -6 before any node existed).
#[rstest]
fn fourteen_monitor_rows_install_on_one_executor(zenohd_unique: ZenohRouter) {
    if !require_zenohd() {
        nros_tests::skip!("zenohd not found");
    }
    let out = run_monitor_capacity(&zenohd_unique.locator(), "14");
    assert!(
        out.contains("mc: installed 14 monitor rows (cap 14)"),
        "14 rows against NROS_EXECUTOR_MAX_MONITORS=14 must install, got:\n{out}"
    );
}

/// ... and a 15th against the same stated 14 is REFUSED, never truncated, with
/// the knob to raise named in the runtime's own words.
#[rstest]
fn a_fifteenth_monitor_row_is_refused_naming_the_knob(zenohd_unique: ZenohRouter) {
    if !require_zenohd() {
        nros_tests::skip!("zenohd not found");
    }
    let out = run_monitor_capacity(&zenohd_unique.locator(), "15");
    assert!(
        out.contains("mc: install refused: monitor table has 15 rows but this executor watches 14"),
        "15 rows against NROS_EXECUTOR_MAX_MONITORS=14 must be refused, got:\n{out}"
    );
    assert!(
        out.contains("raise NROS_EXECUTOR_MAX_MONITORS"),
        "the refusal must name the knob to raise, got:\n{out}"
    );
    assert!(
        !out.contains("mc: installed"),
        "a refused table must not be reported installed:\n{out}"
    );
}
