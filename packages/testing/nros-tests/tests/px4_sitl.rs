//! PX4 SITL runtime cells (phase-521 W8 (e), RFC-0113 D9 — PX4 is Tier 1).
//!
//! The `(Px4, Cpp, Uorb, …)` cells: a nano-ros module running inside a real
//! PX4 SITL build, talking uORB. Gated — run by the hosted `px4-sitl` job in
//! `nightly.yml`, which builds SITL with `just px4 build-sitl-example` first.
//! No build happens here (CLAUDE.md: no compilation inside tests); a missing or
//! wrong-root tree fails with the recipe that builds it.
//!
//! Moved from the out-of-workspace `nros-px4-sitl-test` crate, whose own
//! harness needed the `px4-rs` submodule. `nros_tests::px4_sitl` needs only
//! the SITL tree.

use std::time::Duration;

use nros_tests::{
    fixtures::lane::require_platform_in_lane,
    matrix::PlatformId,
    px4_sitl::{Px4Sitl, prebuilt_sitl_dir},
};

/// The example's module; `just px4 build-sitl-example` links it into SITL.
const MODULE: &str = "nros_uorb_demo";

/// The module publishes at 1 Hz and `listener` needs a fresh sample; generous
/// on purpose, because tightening it buys nothing and flakes under load.
const LISTEN_TIMEOUT: Duration = Duration::from_secs(30);

/// Two `stats:` lines are 10 s apart; the first lands 10 s after start.
const STATS_TIMEOUT: Duration = Duration::from_secs(40);

/// Wakes a window may hold beyond its deadlines and arrivals: the `status`
/// read lands a few ms after each `stats:` line, so a tick or arrival can fall
/// between the two. A retired 10 ms slice would be ~1000.
const WAKE_SLACK: u64 = 4;

/// Arrival->dispatch on the real clock: measured 14-29 us over three runs on
/// PX4 v1.17.0 SITL (RFC-0113 coverage). SITL is a Linux process with no
/// real-time priority, so the bound leaves room for a loaded hosted runner.
const LATENCY_BOUND_US: u64 = 5_000;

/// Tick jitter in PX4 time: a sanity bound, not a real-time claim. Under
/// lockstep that time moves in steps, and the step grows with host load:
/// 16 ms on an idle host, 28-64 ms at load average 45 with the same image
/// (measured 2026-11-03, with and without issue 2225's hook). A quarter
/// period still catches a timer that is broken rather than coarse.
const JITTER_BOUND_US: u64 = 250_000;

/// phase-325 W2.4 — a STOCK PX4 consumer reads what nano-ros published, and
/// nano-ros reads what PX4's commander published.
///
/// On uORB the payload IS the PX4 struct (`publisher_publish_raw` hands the
/// bytes to `orb_publish`), so the failure worth catching is a layout
/// disagreement with PX4's `orb_metadata`. A nano-ros peer shares any such bug
/// with the publisher, so the assertion is on PX4's own `listener`, which
/// prints decoded fields (issue 0356).
#[test]
fn stock_px4_listener_reads_a_nano_ros_publication() {
    require_platform_in_lane(&[PlatformId::Px4], "the PX4 SITL uORB example");
    let build_dir = prebuilt_sitl_dir(MODULE, "just px4 build-sitl-example");
    let sitl = Px4Sitl::boot_in(&build_dir);

    sitl.shell(&format!("{MODULE} start"));
    // One publish cycle before asking a consumer to read one.
    std::thread::sleep(Duration::from_secs(2));

    // `listener` prints "TOPIC: debug_key_value" even for an all-zero sample,
    // so assert on the KEY the module writes ("nros", a char[10]): it
    // round-trips only if the struct layout agrees byte for byte.
    let listener_out = sitl.shell("listener debug_key_value 2");
    assert!(
        listener_out.contains("nros"),
        "stock `listener debug_key_value` did not report a nano-ros sample.\n\
         === listener stdout ===\n{listener_out}\n=== SITL log ===\n{}",
        sitl.log_snapshot()
    );

    // The other direction: nano-ros reading what PX4's commander publishes.
    if let Err(e) = sitl.wait_for_log("recv vehicle_status:", LISTEN_TIMEOUT) {
        panic!(
            "nano-ros never received PX4's vehicle_status: {e}\n=== SITL log ===\n{}",
            sitl.log_snapshot()
        );
    }
}

/// Pull `key=<u64>` out of a log line.
fn field(line: &str, key: &str) -> u64 {
    let pat = format!("{key}=");
    let at = line
        .find(&pat)
        .unwrap_or_else(|| panic!("no `{pat}` in `{line}`"))
        + pat.len();
    line[at..]
        .split(|c: char| !c.is_ascii_digit())
        .next()
        .and_then(|n| n.parse().ok())
        .unwrap_or_else(|| panic!("`{pat}` is not a number in `{line}`"))
}

/// Every tier's `runs=` summed, and the largest `wake_max_us=`, from the
/// module's `status` output.
fn sched_task_status(sitl: &Px4Sitl) -> (u64, u64) {
    let out = sitl.shell(&format!("{MODULE} status"));
    let lines: Vec<&str> = out.lines().filter(|l| l.contains(" runs=")).collect();
    assert!(
        !lines.is_empty(),
        "`{MODULE} status` printed no tier `runs=`:\n{out}"
    );
    let runs = lines.iter().map(|l| field(l, "runs")).sum();
    let wake = lines
        .iter()
        .map(|l| field(l, "wake_max_us"))
        .max()
        .unwrap_or(0);
    (runs, wake)
}

/// phase-521 W8 (e), RFC-0113 D2 under PX4's work-queue scheduler.
///
/// The example's one tier has a 1 s timer and a `vehicle_status`
/// subscription, and nothing else to do. Over one 10 s `stats:` window its
/// work item must run at least once per tick (a deadline was met) and at most
/// once per tick plus once per arrival, plus a small slack for the `status`
/// read landing a few ms after the line. A wake that is neither a deadline nor
/// an arrival is the slice the RFC retired: a 10 ms one would add ~1000 here.
///
/// It also bounds arrival->dispatch: the REAL time from the uORB arrival's
/// `notify()` to the work-item run that serves it (`wake_max_us`, on
/// `steady_clock`), and how far a tick strays from its 1 s period in PX4 time.
/// SITL runs lockstep, so PX4's `hrt` is virtual and moves in steps: a
/// publish-to-dispatch delta measured on it is 0 by construction, and the tick
/// jitter it reports is one lockstep step.
#[test]
fn work_queue_sched_task_wakes_only_for_deadlines_and_arrivals() {
    require_platform_in_lane(&[PlatformId::Px4], "the PX4 SITL uORB example");
    let build_dir = prebuilt_sitl_dir(MODULE, "just px4 build-sitl-example");
    let sitl = Px4Sitl::boot_in(&build_dir);
    sitl.shell(&format!("{MODULE} start"));

    let window = |n: usize| -> (String, u64, u64) {
        let lines = sitl
            .wait_for_log_count("stats: ticks=", n, STATS_TIMEOUT)
            .unwrap_or_else(|e| panic!("{e}\n=== SITL log ===\n{}", sitl.log_snapshot()));
        let (runs, wake) = sched_task_status(&sitl);
        (lines[n - 1].clone(), runs, wake)
    };
    let (a, runs_a, _) = window(1);
    let (b, runs_b, wake_max) = window(2);

    let ticks = field(&b, "ticks") - field(&a, "ticks");
    let arrivals = field(&b, "status") - field(&a, "status");
    let runs = runs_b - runs_a;
    let jitter = field(&b, "jitter_max_us");
    eprintln!(
        "px4 work-queue tier, 10 s window: runs={runs} ticks={ticks} arrivals={arrivals} \
         wake_max_us={wake_max} jitter_max_us={jitter}"
    );

    assert!(
        ticks >= 9,
        "the 1 s timer ticked {ticks} times in a 10 s window: {b}"
    );
    assert!(
        arrivals > 0,
        "no vehicle_status arrived in the window, so latency is unmeasured: {b}"
    );
    assert!(
        runs >= ticks,
        "the tier ran {runs} times for {ticks} timer deadlines: a deadline was missed"
    );
    assert!(
        runs <= ticks + arrivals + WAKE_SLACK,
        "the tier ran {runs} times for {ticks} deadlines and {arrivals} arrivals: \
         {} wake(s) answered neither (RFC-0113 D2)",
        runs - ticks - arrivals
    );
    assert!(
        wake_max <= LATENCY_BOUND_US,
        "arrival->dispatch reached {wake_max} us, over the {LATENCY_BOUND_US} us bound"
    );
    assert!(
        jitter <= JITTER_BOUND_US,
        "a 1 s tick strayed {jitter} us, over the {JITTER_BOUND_US} us bound: {b}"
    );
}

/// RFC-0051 §7c — the cases this file runs are exactly the cells
/// `matrix::consumer_of` assigns to it. Fixture-free.
#[test]
fn cases_bound_to_matrix_cells() {
    use nros_tests::matrix::{
        Consumer, Kind as K, Lang as L, PlatformId as P, Rmw as R, Workload as W,
        assert_consumer_cases,
    };
    assert_consumer_cases(
        Consumer::Px4Sitl,
        &[(P::Px4, L::Cpp, R::Uorb, W::Pubsub, K::Example)],
    );
}
