//! phase-474 T4 -- the violation channel end to end on a Zephyr image.
//!
//! The host executor test (`t4_an_overrun_after_arming_is_the_one_stored_violation`,
//! nros-node) drives the rule on a mock session; this boots the real thing:
//! `examples/workspaces/violation-cpp` on native_sim, built with
//! `CONFIG_NROS_MONITOR_ARM_ON_CALL=y` and `CONFIG_NROS_VIOLATION_DRAIN_REPORT=y`,
//! whose generated entry installs a `max-latency-runtime` row (50 ms) on the
//! handler's `/t4/state` publisher from the bringup's contract.
//!
//! The handler overruns in INIT (tick 1, 80 ms, before arming: counted, not
//! stored), arms on tick 3 (`nros::arm_monitors()`), and overruns once on tick
//! 6 (150 ms). Asserted on the console: exactly one drain-hook line
//! `contract violation #<n>: max-latency-runtime /handler/state measured=>=150
//! declared=50`, after the RUN line, and the arming line's suppressed count.
//! Release-jitter verdicts are not asserted: the overrun delays the spin as
//! well, and on native_sim's 1 ms cadence a wake reads exactly one period late
//! now and then (`measured=1000 declared=1000`). The safety island's S32K344 run is the acceptance; this is the CI guard.
//!
//! phase10-B3 adds the route rule on the same image: each tick publishes
//! `/t4/avail`, which the handler takes itself, and `/t4/answer`, the output of
//! the input-triggered `react` path (180 ms from that take). Asserted: exactly
//! two `route-latency-runtime /handler/answer ... declared=180` verdicts after
//! RUN, tick 6's (the take waits for tick 6, which busy-waits 150 ms) and tick
//! 10's (the take of tick 9's sample busy-waits 250 ms, so tick 10 is late and
//! short); the on-time ticks record none, and `/t4/answer`'s callback-level
//! 180 ms (`max-latency-runtime`) records nothing.
//!
//! phase-544 W2 (issue 2273) adds the QEMU leg, the matrix cell this binary
//! answers: (ZephyrQemuCortexM, Cpp, Zenoh, ViolationChannel, Workspace). The
//! same workspace, image `zephyr_mps2`, on mps2/an385 under the board
//! descriptor's `-icount shift=auto`, so the arming and the 50 ms bound hold on
//! the guest's own timer rather than the host's clock native_sim runs on. It
//! asserts the single post-arming `max-latency-runtime` verdict and the
//! arming line's suppressed count. The native_sim test stays as the leg the
//! matrix does not model as a cell (its coordinate is a tier-1 platform, and a
//! cell there would put every `zephyr,cpp,zenoh` fixture into tier 1); it also
//! carries the route assertions, which the QEMU cell does not take on.

use std::time::Duration;

use nros_tests::{
    TestResult,
    alloc::port_of,
    fixtures::{
        QemuProcess, RequireFixture, ZenohRouter, ZephyrPlatform, ZephyrProcess,
        baked_locator::assert_row_dials_port, build_zephyr_cortex_m_workspace_cpp_violation_entry,
        build_zephyr_workspace_cpp_violation_entry, is_qemu_available, is_zenohd_available,
    },
    matrix::{Lang, PlatformId, Workload},
    output::{
        CONTRACT_VIOLATION_PREFIX, MONITORS_ARMED_SUPPRESSED_MARKER, RULE_MAX_LATENCY_RUNTIME,
        RULE_ROUTE_LATENCY_RUNTIME,
    },
};

/// The port `examples/fixtures.toml` bakes for the native_sim fixture.
const PORT: u16 = 7697;

/// The QEMU row's id, held to its cell's allocator port (issue 1581).
const QEMU_ROW: &str = "workspace-zephyr-cortex-m-cpp-violation";

/// The handler's line after its last asserted tick; both legs wait for it.
const LAST_TICK: &str = "[handler] tick=12";

/// The QEMU guest's budget: boot, the in-kernel stack's static address, a TCP
/// session through SLIRP, then twelve 100 ms ticks plus 480 ms of busy-wait.
/// A healthy run finishes in seconds; this is the cortex-m lane's boot budget.
const QEMU_BUDGET: Duration = Duration::from_secs(90);

/// Every drain-hook line (`contract violation #<seq>: <rule> ..`) of `rule`,
/// with its offset in `console`.
fn verdicts<'a>(console: &'a str, rule: &str) -> Vec<(usize, &'a str)> {
    console
        .match_indices(CONTRACT_VIOLATION_PREFIX)
        .map(|(at, _)| (at, console[at..].lines().next().unwrap_or("")))
        .filter(|(_, line)| line.split_whitespace().nth(3) == Some(rule))
        .collect()
}

/// The `measured=<ms>` field of a verdict line, or 0.
fn measured_ms(line: &str) -> u32 {
    line.split("measured=")
        .nth(1)
        .and_then(|r| r.split_whitespace().next())
        .and_then(|n| n.parse().ok())
        .unwrap_or(0)
}

/// The assertion both legs share: the handler reached RUN and kept ticking,
/// exactly one `max-latency-runtime` verdict was stored, on the contracted
/// path with its 50 ms bound, measured at least the 150 ms overrun, AFTER the
/// RUN line, and the arming line counts INIT's suppressed verdicts. Returns
/// the RUN line's offset and the verdict line.
///
/// `window_end` bounds the verdicts counted (an offset into `console`): the
/// native_sim leg counts the whole run, the QEMU leg counts up to the late
/// tick the route scenario provokes (see its test).
fn assert_one_latency_verdict_after_arming<'a>(
    console: &'a str,
    window_end: usize,
    excerpt: &dyn Fn() -> String,
) -> (usize, &'a str) {
    let run_at = console
        .find("[handler] RUN at tick 3")
        .unwrap_or_else(|| panic!("the handler never reached RUN:\n{}", excerpt()));
    assert!(
        console.contains(LAST_TICK),
        "the handler stopped ticking:\n{}",
        excerpt()
    );
    // Release-jitter verdicts share the ring (the overrun delays the spin
    // too), so the assertion is on the latency rule's lines.
    let latency: Vec<_> = verdicts(console, RULE_MAX_LATENCY_RUNTIME)
        .into_iter()
        .filter(|(at, _)| *at < window_end)
        .collect();
    assert_eq!(
        latency.len(),
        1,
        "exactly one max-latency-runtime verdict (INIT's overrun suppressed, RUN's stored):\n{}",
        excerpt()
    );
    let (at, line) = latency[0];
    assert!(
        line.contains("max-latency-runtime /handler/state measured=")
            && line.contains("declared=50 "),
        "the verdict names the contracted path and its 50 ms bound: `{line}`"
    );
    let measured = measured_ms(line);
    assert!(
        measured >= 150,
        "measured {measured} ms for a 150 ms overrun: `{line}`"
    );
    assert!(
        at > run_at,
        "the verdict was stored before arming (INIT's overrun must be suppressed):\n{}",
        excerpt()
    );
    assert!(
        console.contains(MONITORS_ARMED_SUPPRESSED_MARKER),
        "the arming line counts INIT's suppressed verdicts:\n{}",
        excerpt()
    );
    (run_at, line)
}

fn tail(console: &str) -> String {
    let lines: Vec<_> = console.lines().collect();
    lines[lines.len().saturating_sub(60)..].join("\n")
}

#[test]
fn zephyr_cpp_violation_channel_reports_one_overrun_after_arming() -> TestResult<()> {
    let entry = build_zephyr_workspace_cpp_violation_entry()
        .require("violation-cpp (NROS_ZEPHYR_FIXTURE_FILTER=violation just zephyr build-fixtures)");
    let _router = ZenohRouter::start_on("127.0.0.1", PORT)
        .unwrap_or_else(|e| nros_tests::unmet!("zenohd failed to start on {PORT}: {e}"));
    let mut guest = ZephyrProcess::start(&entry, ZephyrPlatform::NativeSim)
        .unwrap_or_else(|e| panic!("boot zephyr native_sim: {e}"));
    let console = guest.wait_for_pattern(LAST_TICK, Duration::from_secs(60));
    guest.kill();

    let excerpt = || tail(&console);
    let (run_at, _) = assert_one_latency_verdict_after_arming(&console, console.len(), &excerpt);
    let latency = verdicts(&console, RULE_MAX_LATENCY_RUNTIME);

    // phase10-B3 -- the route rule: two late routes, nothing for the on-time
    // ones, both after RUN.
    let routes = verdicts(&console, RULE_ROUTE_LATENCY_RUNTIME);
    assert_eq!(
        routes.len(),
        2,
        "two late routes (tick 6's overrun, tick 10's late tick), none on time:\n{}",
        excerpt()
    );
    for (_, line) in &routes {
        eprintln!("route verdict: {line}");
    }
    let late_tick = console
        .find("[handler] took avail 9")
        .unwrap_or_else(|| panic!("the handler never delayed tick 10:\n{}", excerpt()));
    for (k, (at, line)) in routes.iter().enumerate() {
        assert!(
            line.contains("route-latency-runtime /handler/answer measured=")
                && line.contains("declared=180 "),
            "the verdict names the route's output and its 180 ms: `{line}`"
        );
        let measured = measured_ms(line);
        let floor = if k == 0 { 181 } else { 250 };
        assert!(
            measured >= floor,
            "route {k} measured {measured} ms: `{line}`"
        );
        assert!(
            *at > run_at,
            "a route verdict before arming:\n{}",
            excerpt()
        );
    }
    assert!(
        routes[1].0 > late_tick,
        "the second verdict is the late tick's:\n{}",
        excerpt()
    );
    assert!(
        !latency.iter().any(|(_, l)| l.contains("/handler/answer")),
        "every dispatch that published /t4/answer was inside its 180 ms:\n{}",
        excerpt()
    );
    Ok(())
}

/// phase-544 W2 (issue 2273) -- the matrix cell: the same workspace on the
/// Zephyr Cortex-M QEMU board, one stored `max-latency-runtime` verdict after
/// arming, measured on the guest's own timer.
#[test]
fn zephyr_cortex_m_cpp_violation_channel_reports_one_overrun_after_arming() {
    // The lane first: a lane that selects no Cortex-M coordinate deselects
    // this before any emulator or router probe (issue 1900).
    nros_tests::fixtures::lane::require_platform_in_lane(
        &[PlatformId::ZephyrQemuCortexM],
        "the Zephyr Cortex-M violation-channel cell",
    );
    if !is_qemu_available() {
        nros_tests::unmet!("qemu-system-arm not found");
    }
    if !is_zenohd_available() {
        nros_tests::unmet!("zenohd not found -- run `just setup` / source ./activate.sh");
    }
    let entry = build_zephyr_cortex_m_workspace_cpp_violation_entry().require(
        "violation-cpp for mps2/an385 (NROS_ZEPHYR_FIXTURE_FILTER=violation just zephyr build-fixtures)",
    );
    // The image bakes its router locator; the router listens on the cell's
    // allocator port, and the row is held to the same number (issue 1581).
    let port = port_of(
        PlatformId::ZephyrQemuCortexM,
        Lang::Cpp,
        Workload::ViolationChannel,
    );
    assert_row_dials_port(QEMU_ROW, port);
    let _router = ZenohRouter::start_slirp(port)
        .unwrap_or_else(|e| panic!("failed to start zenohd on {port} for the SLIRP guest: {e:?}"));
    // The board descriptor's `[board.run.qemu]` line, `-icount shift=auto`
    // included, plus the SLIRP NIC.
    let mut qemu = QemuProcess::start_mps2_an385_networked(&entry)
        .unwrap_or_else(|e| panic!("boot the mps2/an385 violation image: {e:?}"));
    let console = qemu.collect_until(LAST_TICK, QEMU_BUDGET);
    qemu.kill();

    let excerpt = || tail(&console);
    // The window ends at the route scenario's late tick (phase10-B3): tick 9's
    // take busy-waits 250 ms, so tick 10 is released late, and on the
    // emulated core its timer path read 60 ms against the 50 ms bound in 1 of
    // 6 runs (measured 2026-11-09; never on native_sim). That verdict is
    // correct and is the route scenario's, not the overrun's; the cell's
    // subject is the overrun and the arming, which all happen before it.
    let late_tick = console
        .find("[handler] took avail 9")
        .unwrap_or_else(|| panic!("the handler never delayed tick 10:\n{}", excerpt()));
    let (_, line) = assert_one_latency_verdict_after_arming(&console, late_tick, &excerpt);
    eprintln!("mps2/an385 verdict: {line}");
}

/// RFC-0051 section 7c (phase-510 W3) -- the cells this file answers are
/// exactly the Runtime cells `matrix::consumer_of` assigns to it.
/// Fixture-free.
#[test]
fn cases_bound_to_matrix_cells() {
    use nros_tests::matrix::{
        Consumer, Kind as K, Lang as L, PlatformId as P, Rmw as R, Workload as W,
        assert_consumer_cases,
    };
    assert_consumer_cases(
        Consumer::ViolationChannel,
        &[(
            P::ZephyrQemuCortexM,
            L::Cpp,
            R::Zenoh,
            W::ViolationChannel,
            K::Workspace,
        )],
    );
}
