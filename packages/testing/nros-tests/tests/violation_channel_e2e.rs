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
//! 6 (110 ms). Asserted on the console: exactly one drain-hook line
//! `contract violation #<n>: max-latency-runtime /handler/state measured=>=110
//! declared=50`, after the RUN line, and the arming line's suppressed count.
//! Release-jitter verdicts are not asserted: the overrun delays the spin as
//! well, and on native_sim's 1 ms cadence a wake reads exactly one period late
//! now and then (`measured=1000 declared=1000`). The safety island's S32K344 run is the acceptance; this is the CI guard.
//!
//! phase10-B3 adds the route rule on the same image: each tick publishes
//! `/t4/avail`, which the handler takes itself, and `/t4/answer`, the output of
//! the input-triggered `react` path (180 ms from that take). Asserted: exactly
//! two `route-latency-runtime /handler/answer ... declared=180` verdicts after
//! RUN, tick 6's (the take waits for tick 6, which busy-waits 110 ms) and tick
//! 10's (the take of tick 9's sample busy-waits 250 ms, so tick 10 is late and
//! short); the on-time ticks record none, and `/t4/answer`'s callback-level
//! 180 ms (`max-latency-runtime`) records nothing.
//!
//! phase-544 W2 (issue 2273) adds the QEMU leg, the matrix cell this binary
//! answers: (ZephyrQemuCortexM, Cpp, Zenoh, ViolationChannel, Workspace). The
//! same workspace, image `zephyr_mps2`, on mps2/an385 under the board
//! descriptor's `-icount shift=auto`, so the arming and the 50 ms bound hold on
//! the guest's own timer rather than the host's clock native_sim runs on. It
//! asserts that the provoked overrun is the one post-arming verdict at or over
//! the 110 ms it busy-waits, that nothing was stored before arming, and the
//! arming line's suppressed count. It does NOT assert that no other tick
//! crosses 50 ms there: a tick that only publishes three samples and prints
//! one line measures ~37..63 ms of guest time on that board (the publishes go
//! through the emulated network stack and SLIRP), so the 50 ms bound sits
//! inside the board's own dispatch floor and an unprovoked verdict is a fact
//! about the board, not about the violation channel (issue 2508 measured
//! the same overhead from the other side). The native_sim test stays as the leg the
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

/// The busy-wait tick 6 does inside its dispatch (`Handler.cpp`,
/// `OVERRUN_AT_TICK`). An unprovoked tick (three publishes and a printf)
/// measures less on every board that runs the fixture, so this is what tells
/// the provoked verdict apart.
///
/// The provoked dispatch can never read below this, so the `>=` line has no
/// rounding allowance. Both clocks involved count the same timer, and the
/// conversion to whole milliseconds is exact:
/// - The wait is `k_busy_wait(110000)`. It spins until 110 ms have passed on
///   the kernel's cycle counter: SysTick on mps2/an385, and native_sim's
///   simulated clock, which the busy-wait advances by exactly that amount.
/// - The executor stamps `now_us` before `dispatch_one` and again after it, so
///   the callback, and therefore the whole wait, lies between the two stamps.
/// - `now_us` is `nros_platform_clock_ns() / 1000`. On mps2/an385
///   (`CONFIG_SYS_CLOCK_HW_CYCLES_PER_SEC=25000000`, no 64-bit counter) that is
///   the tick clock at `CONFIG_SYS_CLOCK_TICKS_PER_SEC=1000`, read off the same
///   SysTick, so both stamps are whole milliseconds. If the callback covers
///   real time `[a, b]` with `b - a >= 110 ms`, then
///   `floor(b) - floor(a) >= 110`. On native_sim the 1 MHz cycle counter
///   gives whole microseconds, and the elapsed time is at least 110000 us.
/// - `check_latency` reports `elapsed_us / 1000`, which is at least 110 in
///   both cases.
///
/// So 110 is a floor, not a typical value, and four of six measured runs read
/// exactly 110. A tick period that does not divide 110 ms would break the
/// second-to-last step. `the_overrun_line_is_inclusive_at_110` pins the
/// boundary.
const OVERRUN_MS: u32 = 110;

/// The handler's line just before its provoked overrun.
const OVERRUN_LINE: &str = "[handler] overrun at tick 6";

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

/// Which verdicts besides the provoked one a leg may store.
#[derive(Clone, Copy)]
enum Floor {
    /// native_sim: an unprovoked dispatch is far inside 50 ms, so the provoked
    /// overrun is the ONLY latency verdict of the whole run.
    InsideBound,
    /// mps2/an385 under icount: an unprovoked dispatch measures ~37..63 ms of
    /// guest time, so it may cross the 50 ms bound. Such a verdict is allowed
    /// only after arming and only below [`OVERRUN_MS`]: one at or over it
    /// could not be told apart from the provoked overrun.
    StraddlesBound,
}

/// What [`check_provoked_overrun`] found on a console that passed it.
struct Provoked<'a> {
    /// The RUN line's offset.
    run_at: usize,
    /// The provoked overrun's verdict line.
    line: &'a str,
    /// The other `/handler/state` verdicts a [`Floor::StraddlesBound`] leg
    /// stored (each under [`OVERRUN_MS`], after arming).
    floor: Vec<&'a str>,
}

/// The claim both legs share, as a pure check so its refusals are tested
/// without a fixture: the handler reached RUN and kept ticking; nothing was
/// stored before the RUN line; exactly one `/handler/state` verdict measured
/// at least the [`OVERRUN_MS`] overrun, after the overrun line, naming its
/// 50 ms bound; and the arming line counts INIT's suppressed verdicts.
/// `floor` says what else the leg may store.
///
/// "Exactly one at or over [`OVERRUN_MS`]" is what keeps the identification
/// honest: if an ordinary tick ever reached the overrun's size, two verdicts
/// would qualify and the check refuses rather than pick one.
fn check_provoked_overrun(console: &str, floor: Floor) -> Result<Provoked<'_>, String> {
    let run_at = console
        .find("[handler] RUN at tick 3")
        .ok_or("the handler never reached RUN")?;
    let overrun_at = console
        .find(OVERRUN_LINE)
        .ok_or("the handler never reached its overrun")?;
    if !console.contains(LAST_TICK) {
        return Err("the handler stopped ticking".into());
    }
    // Release-jitter verdicts share the ring (the overrun delays the spin
    // too), so the claim is on the latency rule's lines.
    let latency = verdicts(console, RULE_MAX_LATENCY_RUNTIME);
    if let Some((at, l)) = latency.iter().find(|(at, _)| *at < run_at) {
        return Err(format!(
            "a max-latency-runtime verdict was stored before arming (RUN @{run_at}, verdict \
             @{at}: `{l}`); INIT's overrun must be suppressed"
        ));
    }
    let provoked: Vec<_> = latency
        .iter()
        .filter(|(_, l)| l.contains("/handler/state ") && measured_ms(l) >= OVERRUN_MS)
        .collect();
    let &&(at, line) = match provoked.as_slice() {
        [one] => one,
        many => {
            return Err(format!(
                "expected exactly one /handler/state verdict at or over the {OVERRUN_MS} ms \
                 overrun, found {}: an ordinary dispatch that reaches the overrun's size makes \
                 the provoked verdict ambiguous",
                many.len()
            ));
        }
    };
    if !(line.contains("max-latency-runtime /handler/state measured=")
        && line.contains("declared=50 "))
    {
        return Err(format!(
            "the verdict does not name the contracted path and its 50 ms bound: `{line}`"
        ));
    }
    if at < overrun_at {
        return Err(format!(
            "the verdict at or over {OVERRUN_MS} ms is not the provoked overrun's (overrun \
             line @{overrun_at}, verdict @{at}: `{line}`)"
        ));
    }
    let others: Vec<&str> = latency
        .iter()
        .filter(|(a, _)| *a != at)
        .map(|(_, l)| *l)
        .collect();
    match floor {
        Floor::InsideBound => {
            if let Some(l) = others.first() {
                return Err(format!(
                    "the provoked overrun must be the only max-latency-runtime verdict: `{l}`"
                ));
            }
        }
        Floor::StraddlesBound => {
            // Under OVERRUN_MS by construction of `provoked`. Only the 50 ms
            // row may be crossed by the floor: `/t4/answer`'s 180 ms row is
            // above every dispatch here (issue 2508), so a verdict on it is a
            // defect on this leg too.
            if let Some(l) = others
                .iter()
                .find(|l| !l.contains("max-latency-runtime /handler/state measured="))
            {
                return Err(format!(
                    "only the 50 ms row may be crossed by an unprovoked dispatch: `{l}`"
                ));
            }
        }
    }
    if !console.contains(MONITORS_ARMED_SUPPRESSED_MARKER) {
        return Err("the arming line does not count INIT's suppressed verdicts".into());
    }
    Ok(Provoked {
        run_at,
        line,
        floor: others,
    })
}

/// [`check_provoked_overrun`] on a live console: a refusal panics with the
/// whole console, and a straddling leg's floor verdicts are printed.
fn assert_provoked_overrun_after_arming<'a>(
    console: &'a str,
    floor: Floor,
    excerpt: &dyn Fn() -> String,
) -> (usize, &'a str) {
    let p = check_provoked_overrun(console, floor)
        .unwrap_or_else(|why| panic!("{why}:\n{}", excerpt()));
    for l in &p.floor {
        eprintln!("dispatch floor over the 50 ms bound: {l}");
    }
    (p.run_at, p.line)
}

/// What a failed assertion prints: every drain-hook line with its offset,
/// then the WHOLE captured console, labelled. A 60-line tail used to stand
/// here; it could not show the verdict lines of a boot that printed more than
/// that after them, so a count miss (1 in 55 runs on mps2/an385) left nothing
/// to read. The console is bounded by the leg's budget and the handler's
/// twelve ticks, so all of it fits in a test report.
fn console_dump(console: &str) -> String {
    let lines: Vec<String> = console
        .match_indices(CONTRACT_VIOLATION_PREFIX)
        .map(|(at, _)| format!("  @{at}: {}", console[at..].lines().next().unwrap_or("")))
        .collect();
    format!(
        "--- every contract-violation line ({n}, @byte offset) ---\n{verdicts}\n\
         --- full captured console ({lc} lines, {bytes} bytes) ---\n{console}\n\
         --- end of captured console ---",
        n = lines.len(),
        verdicts = lines.join("\n"),
        lc = console.lines().count(),
        bytes = console.len(),
    )
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

    let excerpt = || console_dump(&console);
    let (run_at, _) = assert_provoked_overrun_after_arming(&console, Floor::InsideBound, &excerpt);
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

    let excerpt = || console_dump(&console);
    // The cell's subject is the provoked overrun and the arming, so the
    // verdict is identified by what provoked it, over the whole run. A count
    // of every latency verdict cannot be the claim on this board: a tick that
    // only publishes and prints measures ~37..63 ms of guest time, which
    // straddles the 50 ms bound. That read 60 ms on the late tick 10 in 1 of
    // 6 runs (2026-11-09), so a window once ended at tick 9; on 2026-11-25
    // (tier 2, cffdca5c9b) the unprovoked tick 5 read 63 ms, inside any
    // window that still holds tick 6.
    let (_, line) = assert_provoked_overrun_after_arming(&console, Floor::StraddlesBound, &excerpt);
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

/// The refusals of [`check_provoked_overrun`], on synthetic consoles shaped
/// like the handler's. Fixture-free.
mod provoked_overrun_check {
    use super::{Floor, check_provoked_overrun};

    const ARMED: &str = "[handler] RUN at tick 3: arming the monitors\n\
                         contract monitors armed; 1 verdict(s) before arming suppressed\n";
    const OVERRUN: &str = "[handler] overrun at tick 6: 110 ms\n";
    const LAST: &str = "[handler] tick=12\n";

    fn state(seq: u32, ms: u32) -> String {
        format!(
            "contract violation #{seq}: max-latency-runtime /handler/state measured={ms} declared=50 (total={seq} dropped=0)\n"
        )
    }

    fn console(parts: &[&str]) -> String {
        parts.concat()
    }

    #[test]
    fn the_provoked_verdict_alone_passes_on_both_legs() {
        let c = console(&[ARMED, OVERRUN, &state(1, 152), LAST]);
        for floor in [Floor::InsideBound, Floor::StraddlesBound] {
            let p = check_provoked_overrun(&c, floor).expect("the provoked verdict alone");
            assert!(p.line.contains("measured=152"));
        }
    }

    #[test]
    fn a_floor_verdict_is_refused_on_native_sim_and_allowed_on_qemu() {
        let c = console(&[ARMED, &state(1, 63), OVERRUN, &state(2, 152), LAST]);
        let err = check_provoked_overrun(&c, Floor::InsideBound)
            .err()
            .expect("native_sim");
        assert!(err.contains("only max-latency-runtime verdict"), "{err}");
        let p = check_provoked_overrun(&c, Floor::StraddlesBound).expect("QEMU");
        assert_eq!(p.floor.len(), 1);
        assert!(p.floor[0].contains("measured=63"));
    }

    #[test]
    fn two_verdicts_at_the_overrun_size_are_ambiguous_and_refused() {
        let c = console(&[ARMED, &state(1, 110), OVERRUN, &state(2, 152), LAST]);
        for floor in [Floor::InsideBound, Floor::StraddlesBound] {
            let err = check_provoked_overrun(&c, floor)
                .err()
                .expect("two at >= 110");
            assert!(err.contains("found 2"), "{err}");
        }
    }

    #[test]
    fn no_verdict_at_the_overrun_size_is_refused() {
        let c = console(&[ARMED, OVERRUN, &state(1, 63), LAST]);
        let err = check_provoked_overrun(&c, Floor::StraddlesBound)
            .err()
            .expect("none >= 110");
        assert!(err.contains("found 0"), "{err}");
    }

    /// The provoked dispatch reads exactly `OVERRUN_MS` on an idle board
    /// (4 of 6 measured QEMU runs), so the line must include it, and one
    /// below it is an unprovoked verdict, not the provoked one.
    #[test]
    fn the_overrun_line_is_inclusive_at_110() {
        let at = console(&[ARMED, OVERRUN, &state(1, 110), LAST]);
        for floor in [Floor::InsideBound, Floor::StraddlesBound] {
            let p = check_provoked_overrun(&at, floor).expect("110 is the provoked verdict");
            assert!(p.line.contains("measured=110"));
        }
        let below = console(&[ARMED, OVERRUN, &state(1, 109), LAST]);
        let err = check_provoked_overrun(&below, Floor::StraddlesBound)
            .err()
            .expect("109 is not the provoked verdict");
        assert!(err.contains("found 0"), "{err}");
    }

    #[test]
    fn a_verdict_before_arming_is_refused() {
        let c = console(&[&state(1, 152), ARMED, OVERRUN, LAST]);
        let err = check_provoked_overrun(&c, Floor::StraddlesBound)
            .err()
            .expect("before RUN");
        assert!(err.contains("before arming"), "{err}");
    }

    #[test]
    fn a_verdict_on_the_answer_row_is_refused_on_qemu() {
        let answer = "contract violation #2: max-latency-runtime /handler/answer measured=189 declared=180 (total=2 dropped=0)\n";
        let c = console(&[ARMED, OVERRUN, &state(1, 152), answer, LAST]);
        let err = check_provoked_overrun(&c, Floor::StraddlesBound)
            .err()
            .expect("/t4/answer");
        assert!(err.contains("only the 50 ms row"), "{err}");
    }
}
