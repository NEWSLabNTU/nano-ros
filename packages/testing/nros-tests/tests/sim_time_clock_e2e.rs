//! phase-425 W5 — simulated time, over a real transport, between two processes.
//!
//! Every other sim-time test in this tree installs a ROS time by calling
//! `Clock::set_ros_time_override`. That proves the timer arithmetic and says
//! nothing about the wire: not that `rosgraph_msgs/msg/Clock` serialises, not
//! that a subscription on `/clock` matches a publisher, and not that
//! `use_sim_time` alone is enough to attach the source. This is the test that
//! does.
//!
//! The pair is `bins/sim-clock-publisher` (a stand-in simulator) and
//! `bins/sim-clock-listener` (a node with `use_sim_time` true, a ROS-time timer
//! and a wall timer of the SAME period). The wall timer is the control: two
//! counts from one executor make the ratio evidence rather than a measurement
//! of how loaded the machine is.
//!
//! CYCLONE, not zenoh. Zenoh needs `rmw_zenohd`, which ships with ROS
//! (RFC-0075), so on a host without ROS the pair cannot open a session and this
//! could only skip. The claim under test is about clocks.

use std::{path::Path, process::Command, time::Duration};

use nros_tests::{
    fixtures::{build_sim_clock_listener, build_sim_clock_publisher},
    output::{SIMCLOCK_PUB_STOPPED, SIMCLOCK_REPORT_PREFIX},
    process::ManagedProcess,
};
use rstest::rstest;

/// The simulator advances time this many times faster than the wall.
const RATE: u64 = 10;
/// Both timers run at this period.
const PERIOD_MS: u64 = 100;

/// The publisher's REAL interval between `/clock` samples.
///
/// It has to satisfy `STEP_MS * RATE <= PERIOD_MS`, and that is a property of
/// the system rather than a tuning knob: each sample advances simulated time by
/// `STEP_MS * RATE`, and `TimerOverrunPolicy::Skip` — the default — coalesces a
/// jump worth several periods into ONE activation and counts the rest as
/// overruns. Publish coarsely and a 10x simulator produces a 1x tick rate while
/// the CLOCK still advances at 10x, which is correct behaviour and not what
/// this test is trying to observe.
///
/// Measured, on the way to getting this right: `STEP_MS = PERIOD_MS` gives
/// ros=10 wall=10 per second — indistinguishable from no simulation — while
/// `ros_now_ms` climbs by 10 000 ms per real second. The tick rate and the
/// clock rate are different questions, and only one of them survives Skip.
const STEP_MS: u64 = PERIOD_MS / RATE;

/// One `SIMCLOCK t=…` line.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Report {
    t: u64,
    ros: u64,
    wall: u64,
    ros_now_ms: u64,
}

fn field(line: &str, key: &str) -> Option<u64> {
    line.split_whitespace()
        .find_map(|tok| tok.strip_prefix(key)?.parse().ok())
}

fn parse_reports(log: &str) -> Vec<Report> {
    log.lines()
        .filter(|l| l.contains(SIMCLOCK_REPORT_PREFIX))
        .filter_map(|l| {
            Some(Report {
                t: field(l, "t=")?,
                ros: field(l, "ros=")?,
                wall: field(l, "wall=")?,
                ros_now_ms: field(l, "ros_now_ms=")?,
            })
        })
        .collect()
}

fn spawn_publisher(bin: &Path, domain: u8, run_ms: u64) -> ManagedProcess {
    let mut cmd = Command::new(bin);
    cmd.env("ROS_DOMAIN_ID", domain.to_string())
        .env("RUST_LOG", "info")
        .env("NROS_SIM_RATE", RATE.to_string())
        .env("NROS_SIM_STEP_MS", STEP_MS.to_string())
        .env("NROS_SIM_RUN_MS", run_ms.to_string())
        // Idles with the session OPEN after it stops publishing. Exiting would
        // remove the participant, and a listener that stopped ticking because
        // its peer vanished would prove nothing.
        .env("NROS_SIM_IDLE_MS", "6000");
    ManagedProcess::spawn_command(cmd, "sim-clock-publisher").expect("spawn sim clock publisher")
}

fn spawn_listener(bin: &Path, domain: u8, observe_ms: u64) -> ManagedProcess {
    let mut cmd = Command::new(bin);
    cmd.env("ROS_DOMAIN_ID", domain.to_string())
        .env("RUST_LOG", "info")
        .env("NROS_SIM_PERIOD_MS", PERIOD_MS.to_string())
        .env("NROS_SIM_OBSERVE_MS", observe_ms.to_string());
    ManagedProcess::spawn_command(cmd, "sim-clock-listener").expect("spawn sim clock listener")
}

/// A node with `use_sim_time` true follows a `/clock` publisher, and stops when
/// it stops — while a wall timer on the same executor does neither.
///
/// Three claims in one run, because they need the same two processes:
///
/// 1. the source ATTACHES from the parameter alone, with no explicit
///    `install_ros_time_source()` call anywhere in the fixture;
/// 2. a `TimerClockSource::Ros` timer runs at the SIMULATOR's rate, not the
///    wall's;
/// 3. when `/clock` stops, that timer stops dead, while the wall timer keeps
///    its cadence.
#[rstest]
fn use_sim_time_makes_a_ros_timer_follow_the_clock_publisher() {
    let publisher_bin = build_sim_clock_publisher()
        .map(Path::to_path_buf)
        .unwrap_or_else(|e| {
            nros_tests::skip!(
                "sim-clock-publisher cyclonedds fixture not built \
                 (run `just build-test-fixtures`): {e:?}"
            )
        });
    let listener_bin = build_sim_clock_listener()
        .map(Path::to_path_buf)
        .unwrap_or_else(|e| {
            nros_tests::skip!(
                "sim-clock-listener cyclonedds fixture not built \
                 (run `just build-test-fixtures`): {e:?}"
            )
        });

    let domain = nros_tests::unique_ros_domain_id();
    let run_ms = 3000;
    // Long enough to cover discovery, the publishing window, and a clear silent
    // stretch after it.
    let observe_ms = 8000;

    let mut publisher = spawn_publisher(&publisher_bin, domain, run_ms);
    let mut listener = spawn_listener(&listener_bin, domain, observe_ms);

    // `wait_for_output_pattern` and not `wait_for_output`: the fixtures log
    // through `env_logger`, which writes to STDERR, and only the pattern form
    // reads both streams. The plain form returned an empty string here and the
    // failure read as "the listener printed nothing" — which was true of the
    // stream it was looking at, and false of the process.
    let log = listener
        .wait_for_output_pattern("SIMCLOCK_LISTENER_DONE", Duration::from_secs(30))
        .expect("the listener never finished its observation window");

    // Then confirm the silence was DELIBERATE. Without this, a publisher that
    // crashed at 3 s and a publisher that stopped on purpose look identical
    // from the listener's side, and claim (3) would pass on a dead peer.
    let pub_log = publisher
        .wait_for_output_pattern(SIMCLOCK_PUB_STOPPED, Duration::from_secs(15))
        .unwrap_or_else(|e| {
            panic!(
                "the publisher never reached its stop marker, so the silent \
                 half of this test measured a CRASH rather than a paused \
                 simulator: {e:?}"
            )
        });
    let _ = &pub_log;
    let reports = parse_reports(&log);
    assert!(
        reports.len() >= 5,
        "expected at least five per-second reports in an {observe_ms} ms window, got {}:\n{log}",
        reports.len()
    );

    // (1) ATTACHED. `attached=true` on the last line, and it got there without
    // the fixture asking: the only request is
    // `declare_parameter(\"use_sim_time\", Bool(true))`.
    let last = log
        .lines()
        .rfind(|l| l.contains(SIMCLOCK_REPORT_PREFIX))
        .unwrap_or_default()
        .to_string();
    assert!(
        last.contains("attached=true") && last.contains("active=true"),
        "`use_sim_time` did not attach the /clock source, or no sample ever \
         landed. The parameter is the ONLY request this fixture makes — an \
         explicit install_ros_time_source() call appears nowhere in it.\n\
         last report: {last}\n{log}"
    );

    // (2) RATE. Per-second deltas: while the simulator runs, the ROS timer must
    // outpace the wall timer by close to the replay rate. A conservative bound
    // (half the rate) because the first interval overlaps discovery and the
    // last one may straddle the stop.
    // Intervals with a real second in them. The listener emits one final report
    // the instant its loop ends, so the last window is a few milliseconds wide
    // and BOTH deltas are zero — which would read as "the wall timer stopped
    // too" and sink claim (3) for a reason that is an artifact of when the last
    // line was printed. The wall timer is the elapsed-time proxy, so it is also
    // the right thing to filter on: fewer than 5 of its 100 ms ticks is not a
    // second.
    let deltas: Vec<(u64, u64)> = reports
        .windows(2)
        .map(|w| (w[1].ros - w[0].ros, w[1].wall - w[0].wall))
        .filter(|(_, wall)| *wall >= 5)
        .collect();
    assert!(
        deltas.len() >= 4,
        "fewer than four full-second intervals to reason about: {deltas:?}\n{log}"
    );
    let fastest = deltas
        .iter()
        .max_by_key(|(ros, wall)| ros / (*wall).max(1))
        .copied()
        .unwrap_or((0, 0));
    assert!(
        fastest.0 >= fastest.1.max(1) * (RATE / 2),
        "no one-second interval saw the ROS-time timer outpace the wall timer \
         by at least {}x on a {RATE}x simulator. Best was ros={} wall={}. Both \
         timers are on the SAME executor at the SAME period, so this ratio is \
         the simulated clock and not machine load.\n{log}",
        RATE / 2,
        fastest.0,
        fastest.1
    );

    // (3) STOPPED. After the publisher goes silent the ROS timer must stop
    // COMPLETELY — not merely slow — while the wall timer keeps going. The last
    // two intervals are safely inside the silent window (publishing ends at
    // 3 s, observation runs to 8 s).
    let tail = &deltas[deltas.len().saturating_sub(2)..];
    assert!(
        tail.iter().all(|(ros, _)| *ros == 0),
        "the ROS-time timer kept firing after /clock went silent: tail deltas \
         {tail:?}. A timer that keeps its cadence when the simulator pauses is \
         a WALL timer wearing the other name.\n{log}"
    );
    assert!(
        tail.iter().all(|(_, wall)| *wall >= 5),
        "the wall timer also stopped, so the previous assertion proves nothing \
         — the process was starved, not the clock. tail deltas {tail:?}\n{log}"
    );
    assert!(
        fastest.0 > tail.iter().map(|(ros, _)| *ros).sum::<u64>(),
        "the busiest interval was no busier than the silent ones, so this run \
         never observed a RUNNING simulator at all: fastest {fastest:?}, tail \
         {tail:?}\n{log}"
    );

    // (4) And the clock itself froze where the last sample left it, which is
    // the difference between \"no samples\" and \"samples that stopped moving\".
    let frozen = reports.last().unwrap().ros_now_ms;
    assert!(
        frozen > 0,
        "ROS time never advanced at all: {frozen} ms\n{log}"
    );
}
