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

use std::time::Duration;

use nros_tests::{
    TestResult,
    fixtures::{
        RequireFixture, ZenohRouter, ZephyrPlatform, ZephyrProcess,
        build_zephyr_workspace_cpp_violation_entry,
    },
};

/// The port `examples/fixtures.toml` bakes for this fixture.
const PORT: u16 = 7697;

#[test]
fn zephyr_cpp_violation_channel_reports_one_overrun_after_arming() -> TestResult<()> {
    let entry = build_zephyr_workspace_cpp_violation_entry()
        .require("violation-cpp (NROS_ZEPHYR_FIXTURE_FILTER=violation just zephyr build-fixtures)");
    let _router = ZenohRouter::start_on("127.0.0.1", PORT)
        .unwrap_or_else(|e| nros_tests::unmet!("zenohd failed to start on {PORT}: {e}"));
    let mut guest = ZephyrProcess::start(&entry, ZephyrPlatform::NativeSim)
        .unwrap_or_else(|e| panic!("boot zephyr native_sim: {e}"));
    let console = guest.wait_for_pattern("[handler] tick=12", Duration::from_secs(60));
    guest.kill();

    let excerpt = || {
        let lines: Vec<_> = console.lines().collect();
        lines[lines.len().saturating_sub(60)..].join("\n")
    };
    let run_at = console
        .find("[handler] RUN at tick 3")
        .unwrap_or_else(|| panic!("the handler never reached RUN:\n{}", excerpt()));
    assert!(
        console.contains("[handler] tick=12"),
        "the handler stopped ticking:\n{}",
        excerpt()
    );
    // The drain hook's line: `contract violation #<seq>: <rule> <fqn> measured=..
    // declared=..`. Release-jitter verdicts share the ring (the overrun delays
    // the spin too, and a 1 ms native_sim cadence reads exactly one period
    // late now and then), so the assertion is on the latency rule's lines.
    let latency: Vec<(usize, &str)> = console
        .match_indices("contract violation #")
        .map(|(at, _)| (at, console[at..].lines().next().unwrap_or("")))
        .filter(|(_, line)| line.contains("max-latency-runtime"))
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
    let measured: u32 = line
        .split("measured=")
        .nth(1)
        .and_then(|r| r.split_whitespace().next())
        .and_then(|n| n.parse().ok())
        .unwrap_or(0);
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
        console.contains("before arming suppressed"),
        "the arming line counts INIT's suppressed verdicts:\n{}",
        excerpt()
    );

    // phase10-B3 -- the route rule: two late routes, nothing for the on-time
    // ones, both after RUN.
    let routes: Vec<(usize, &str)> = console
        .match_indices("contract violation #")
        .map(|(at, _)| (at, console[at..].lines().next().unwrap_or("")))
        .filter(|(_, line)| line.contains("route-latency-runtime"))
        .collect();
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
        let measured: u32 = line
            .split("measured=")
            .nth(1)
            .and_then(|r| r.split_whitespace().next())
            .and_then(|n| n.parse().ok())
            .unwrap_or(0);
        let floor = if k == 0 { 181 } else { 250 };
        assert!(measured >= floor, "route {k} measured {measured} ms: `{line}`");
        assert!(*at > run_at, "a route verdict before arming:\n{}", excerpt());
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
