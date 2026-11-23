//! A linux image whose wall clock is the BOARD's (phase-582 W1d, RFC-0141 D2):
//! a stub board's hook answering one fixed instant reaches `Clock::system()`
//! and the samples' source stamps, in Rust, C and C++.
//!
//! **Bucket (phase-329 W4): matrix consumer.** The configured cells
//! `(Linux, {Rust, C, Cpp}, Zenoh, Pubsub, Example) + [ClockBoard]` and
//! `(Linux, Rust, {Cyclonedds, Xrce}, Pubsub, Example) + [ClockBoard]` of
//! `matrix::CELLS`.
//!
//! The images are the Rust, C and C++ workspaces' `linux_talker_clock_board`
//! (the pubsub bringup): `[knobs.clock] source = "board"`, and a launch running the
//! ordinary talker beside `clock_board_pkg`'s node. That package is the stub
//! board: it defines `nros_board_wall_clock_ns`, answering
//! [`BOARD_INSTANT_NS`], and its node prints
//! `clock-board: wall_ns=<ns> synced=<0|1> source=<id> steps=<n>` from the
//! language's public wall-clock read and state query. Each case asserts:
//!
//! - the printed wall clock sits in `[instant, instant + 60 s]`: the board's
//!   instant plus the time since the port asked it;
//! - the state is synced, from `board`, with no steps;
//! - the MessageInfo observer's `ts=` (the source stamp the talker's samples
//!   carry, from the same wall clock) is in the same window;
//! - the image defines the hook and links the selection object that
//!   references it.
//!
//! The control is the same workspace's ordinary `linux_talker`: its samples
//! carry host time (between two host clock reads, nowhere near the instant),
//! and its binary neither defines nor references the hook or the selection
//! object, which is what "a board that does not select `board` is unchanged in
//! what it links" can be checked as on a built image. The control runs in C
//! too.
//!
//! The same configuration on Cyclone DDS and XRCE (Rust,
//! `linux_talker_{cyclonedds,xrce}_clock_board`) measures what those backends
//! stamp, observed by the MessageInfo observer's image for the same backend
//! (`linux_cyclonedds`, `linux_xrce`). These rows of
//! `docs/reference/source-timestamps.md` cite
//! `board_clock_source_stamp_per_backend`: Cyclone stamps host time, XRCE
//! delivers no stamp (0), while the talker's own wall clock is the board's.
//!
//! The image's knob gives it its own configure, and that configure builds the
//! image only (issue 2841). It used to also build every TYPED C component's
//! native carrier executable against the same `board`-selected runtime, and
//! those carriers, having no hook, failed to link.
//!
//! ## Prerequisites
//!
//! - the rows `workspace-{rust,c,cpp}-native-talker-clock-board`,
//!   `workspace-{rust,c,cpp}-native-talker` and the MessageInfo observer (`just build-test-fixtures lane=linux`)
//! - for the per-backend case, the rows
//!   `workspace-rust-native-talker-{cyclonedds,xrce}-clock-board` and
//!   `linux-rust-{cyclonedds,xrce}-message-info-observer`
//! - `rmw_zenohd` (the router), the XRCE Agent, and `nm`
//!
//! Run with: `cargo nextest run -p nros-tests --test linux_board_clock`

use nros_tests::{
    fixtures::{
        RequireFixture, ZenohRouter, build_message_info_observer, or_unmet,
        workspace_image::{self, PairRole},
    },
    matrix::{Config, Lang, PlatformId, Rmw},
    output::{
        CLOCK_BOARD_PREFIX, ClockBoardReading, LISTENER_READY_MARKER, MESSAGE_INFO_LOG_PREFIX,
        clock_board_readings,
    },
    process::ManagedProcess,
};
use rstest::rstest;
use std::{
    path::{Path, PathBuf},
    process::Command,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

/// The instant `clock_board_pkg`'s hook answers, in every language:
/// 2023-11-14T22:13:20Z.
const BOARD_INSTANT_NS: i64 = 1_700_000_000_000_000_000;

/// How far past the instant a reading may be: the board clock advances on the
/// monotonic clock from the moment the port asked the hook, and no case runs
/// this long.
const BOARD_WINDOW_NS: i64 = 60 * 1_000_000_000;

/// The board's hook, and the function of the selection object that references
/// it (`nros_clock_board__set_by__knobs_clock_source.c`).
const HOOK_SYMBOL: &str = "nros_board_wall_clock_ns";
const SELECTION_SYMBOL: &str = "nros_clock_board_try__set_by__knobs_clock_source";

/// `NROS_WALL_CLOCK_SOURCE_BOARD`.
const SOURCE_BOARD: u8 = 3;

/// How many samples the observer must report.
const SAMPLES: usize = 3;

const WAIT: Duration = Duration::from_secs(30);

fn host_now_ns() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("the host clock is after 1970")
        .as_nanos() as i64
}

fn in_board_window(ns: i64) -> bool {
    (BOARD_INSTANT_NS..=BOARD_INSTANT_NS + BOARD_WINDOW_NS).contains(&ns)
}

fn talker(lang: Lang, config: &[Config]) -> PathBuf {
    talker_rmw(lang, Rmw::Zenoh, config)
}

fn talker_rmw(lang: Lang, rmw: Rmw, config: &[Config]) -> PathBuf {
    nros_tests::fixtures::lane::require_platform_in_lane(
        &[PlatformId::Linux],
        "the linux board-clock cells",
    );
    let resolved = if config.is_empty() {
        workspace_image::resolve_coord(PlatformId::Linux, lang, rmw, PairRole::Talker)
    } else {
        workspace_image::resolve_configured(PlatformId::Linux, lang, rmw, PairRole::Talker, config)
    };
    resolved.unwrap_or_else(|e| {
        nros_tests::unmet!(
            "linux {lang:?} {rmw:?} talker image {config:?} not prebuilt or stale \
             (run `just build-test-fixtures lane=linux`): {e:?}"
        )
    })
}

/// The `ts=` of every MessageInfo line in `out`.
fn source_stamps(out: &str) -> Vec<i64> {
    out.lines()
        .filter(|l| l.contains(MESSAGE_INFO_LOG_PREFIX))
        .filter_map(|l| {
            l.split_whitespace()
                .find_map(|f| f.strip_prefix("ts="))
                .and_then(|v| v.parse().ok())
        })
        .collect()
}

/// The defined (`T`/`t`/...) and undefined (`U`) symbols `nm` lists for `bin`
/// that equal `name`.
fn nm_kinds(bin: &Path, name: &str) -> Vec<String> {
    let out = Command::new("nm")
        .arg(bin)
        .output()
        .unwrap_or_else(|e| nros_tests::unmet!("`nm` is not runnable on this host: {e}"));
    assert!(
        out.status.success(),
        "nm {} failed: {}",
        bin.display(),
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|l| {
            let mut f = l.split_whitespace().rev();
            let sym = f.next()?;
            let kind = f.next()?;
            (sym == name).then(|| kind.to_string())
        })
        .collect()
}

/// Runs the observer, then `image`, against one router: the image's output
/// once it printed a clock-board line (empty when `expect_clock_line` is
/// false) and the observer's output once it reported [`SAMPLES`] samples.
fn run_pair(image: &Path, label: &str, expect_clock_line: bool) -> (String, String) {
    let observer = build_message_info_observer().require("the MessageInfo observer");
    let router = or_unmet(ZenohRouter::start_unique());
    let locator = router.locator();

    let mut obs_cmd = Command::new(observer);
    obs_cmd
        .env("NROS_LOCATOR", &locator)
        .env("RUST_LOG", "info");
    let mut obs =
        ManagedProcess::spawn_image(obs_cmd, "message-info-observer").expect("start the observer");
    obs.wait_for_output_pattern(LISTENER_READY_MARKER, WAIT)
        .expect("the observer never created its subscription");

    let mut cmd = Command::new(image);
    cmd.env("NROS_LOCATOR", &locator).env("RUST_LOG", "info");
    let mut talker = ManagedProcess::spawn_image(cmd, label).expect("start the talker image");

    let obs_out = obs
        .wait_for_output_count(MESSAGE_INFO_LOG_PREFIX, SAMPLES, WAIT)
        .unwrap_or_else(|e| panic!("the observer did not report {SAMPLES} samples: {e}"));
    let talker_out = if expect_clock_line {
        talker
            .wait_for_output_pattern(CLOCK_BOARD_PREFIX, WAIT)
            .unwrap_or_else(|e| panic!("{label} never printed its wall clock: {e}"))
    } else {
        String::new()
    };
    talker.kill();
    obs.kill();
    (talker_out, obs_out)
}

#[rstest]
#[case::rust(Lang::Rust)]
#[case::c(Lang::C)]
#[case::cpp(Lang::Cpp)]
fn board_clock_reaches_the_wall_clock_and_the_source_stamp(#[case] lang: Lang) {
    let image = talker(lang, &[Config::ClockBoard]);
    let label = format!("{lang:?} linux_talker_clock_board");

    // The link rule, on the built image: the hook is the image's own, and the
    // selection object that asks it is linked.
    let hook = nm_kinds(&image, HOOK_SYMBOL);
    assert!(
        hook.iter().any(|k| k.eq_ignore_ascii_case("t")),
        "{label} does not define `{HOOK_SYMBOL}` (nm: {hook:?})"
    );
    assert!(
        !nm_kinds(&image, SELECTION_SYMBOL).is_empty(),
        "{label} links no `{SELECTION_SYMBOL}`: the `board` selection did not reach its port"
    );

    let (talker_out, obs_out) = run_pair(&image, &label, true);

    let readings: Vec<ClockBoardReading> = clock_board_readings(&talker_out);
    let first = *readings
        .first()
        .unwrap_or_else(|| panic!("{label} printed no parsable clock-board line:\n{talker_out}"));
    assert!(
        in_board_window(first.wall_ns),
        "{label}: the wall clock read {} ns, outside the board's [{BOARD_INSTANT_NS}, +60 s]; \
         the host clock reads {}. Output:\n{talker_out}",
        first.wall_ns,
        host_now_ns()
    );
    assert!(
        first.synced,
        "{label}: the state says not synced: {first:?}"
    );
    assert_eq!(
        first.source, SOURCE_BOARD,
        "{label}: the state names source {} (3 is board): {first:?}",
        first.source
    );
    assert_eq!(
        first.steps, 0,
        "{label}: the source changed since boot: {first:?}"
    );

    let stamps = source_stamps(&obs_out);
    assert!(
        stamps.len() >= SAMPLES,
        "the observer printed {} `ts=` values, wanted {SAMPLES}:\n{obs_out}",
        stamps.len()
    );
    for ts in &stamps {
        assert!(
            in_board_window(*ts),
            "{label}: a sample's source stamp is {ts} ns, outside the board's window: the \
             zenoh stamp does not follow the selected source. Observer:\n{obs_out}"
        );
    }
    eprintln!(
        "{label}: wall_ns={} (instant + {} ms), stamps {:?}",
        first.wall_ns,
        (first.wall_ns - BOARD_INSTANT_NS) / 1_000_000,
        stamps
    );
}

/// The control: the same workspace's ordinary talker selects no source, so
/// its samples carry host time, and its binary carries nothing of the board
/// selection.
#[rstest]
#[case::rust(Lang::Rust)]
#[case::c(Lang::C)]
#[case::cpp(Lang::Cpp)]
fn ordinary_talker_stamps_host_time_and_links_no_hook(#[case] lang: Lang) {
    let image = talker(lang, &[]);
    let label = format!("{lang:?} linux_talker");

    for sym in [HOOK_SYMBOL, SELECTION_SYMBOL] {
        let kinds = nm_kinds(&image, sym);
        assert!(
            kinds.is_empty(),
            "{label} selects no board source, yet links `{sym}` (nm: {kinds:?})"
        );
    }

    let before = host_now_ns();
    let (_, obs_out) = run_pair(&image, &label, false);
    let after = host_now_ns();
    let stamps = source_stamps(&obs_out);
    assert!(
        stamps.len() >= SAMPLES,
        "the observer printed {} `ts=` values, wanted {SAMPLES}:\n{obs_out}",
        stamps.len()
    );
    for ts in &stamps {
        assert!(
            (before..=after).contains(ts) && !in_board_window(*ts),
            "{label}: a sample's source stamp is {ts} ns, not host time in [{before}, {after}]. \
             Observer:\n{obs_out}"
        );
    }
}

/// What a backend delivers as a sample's source stamp when the publishing
/// image's wall clock is the board's, where that is NOT the selected clock
/// (zenoh's, which is, is the case above).
#[derive(Clone, Copy, Debug)]
enum SourceStamp {
    /// The host's `CLOCK_REALTIME`: every stamp is between two host clock
    /// reads bracketing the run, nowhere near the board's instant.
    HostClock,
    /// No stamp: every sample reports 0.
    Absent,
}

/// The per-backend source-stamp table (`docs/reference/source-timestamps.md`)
/// read back from built images: a Rust talker whose wall clock is the stub
/// board's, observed by the MessageInfo observer built for the same backend.
/// The talker's own clock line proves the board clock IS the image's wall
/// clock, so a stamp outside the board's window is the backend's own choice.
#[rstest]
#[case::cyclonedds(Rmw::Cyclonedds, SourceStamp::HostClock)]
#[case::xrce(Rmw::Xrce, SourceStamp::Absent)]
fn board_clock_source_stamp_per_backend(#[case] rmw: Rmw, #[case] want: SourceStamp) {
    use nros_tests::fixtures::{
        Rmw as FixtureRmw, XrceAgent, build_message_info_observer_rmw, require_xrce_agent,
    };
    let image = talker_rmw(Lang::Rust, rmw, &[Config::ClockBoard]);
    let label = format!("Rust {rmw:?} linux_talker_clock_board");
    let observer = build_message_info_observer_rmw(match rmw {
        Rmw::Cyclonedds => FixtureRmw::Cyclonedds,
        Rmw::Xrce => FixtureRmw::Xrce,
        other => panic!("{other:?} is not measured by this case"),
    })
    .require("the MessageInfo observer for this backend");

    let mut obs_cmd = Command::new(&observer);
    let mut talker_cmd = Command::new(&image);
    let mut _agent: Option<XrceAgent> = None;
    let domain = nros_tests::unique_ros_domain_id().to_string();
    for cmd in [&mut obs_cmd, &mut talker_cmd] {
        cmd.env("RUST_LOG", "info").env("ROS_DOMAIN_ID", &domain);
    }
    match rmw {
        Rmw::Cyclonedds => {}
        Rmw::Xrce => {
            require_xrce_agent();
            let agent = XrceAgent::start_unique()
                .unwrap_or_else(|e| nros_tests::unmet!("XRCE Agent failed to start: {e:?}"));
            let addr = agent.addr();
            for cmd in [&mut obs_cmd, &mut talker_cmd] {
                cmd.env("NROS_LOCATOR", &addr).env("XRCE_AGENT_ADDR", &addr);
            }
            _agent = Some(agent);
        }
        _ => unreachable!(),
    }

    let before = host_now_ns();
    let mut obs =
        ManagedProcess::spawn_image(obs_cmd, "message-info-observer").expect("start the observer");
    obs.wait_for_output_pattern(LISTENER_READY_MARKER, WAIT)
        .expect("the observer never created its subscription");
    // Discovery on both backends is slower than a zenoh router's.
    std::thread::sleep(Duration::from_secs(2));
    let mut talker = ManagedProcess::spawn_image(talker_cmd, &label).expect("start the talker");
    let obs_out = obs
        .wait_for_output_count(MESSAGE_INFO_LOG_PREFIX, SAMPLES, WAIT)
        .unwrap_or_else(|e| panic!("the observer did not report {SAMPLES} samples: {e}"));
    let talker_out = talker
        .wait_for_output_pattern(CLOCK_BOARD_PREFIX, WAIT)
        .unwrap_or_else(|e| panic!("{label} never printed its wall clock: {e}"));
    talker.kill();
    obs.kill();
    let after = host_now_ns();

    let first = *clock_board_readings(&talker_out)
        .first()
        .unwrap_or_else(|| panic!("{label} printed no parsable clock-board line:\n{talker_out}"));
    assert!(
        in_board_window(first.wall_ns) && first.synced && first.source == SOURCE_BOARD,
        "{label}: the image's wall clock is not the board's ({first:?}), so its stamps \
         say nothing about the backend. Output:\n{talker_out}"
    );

    let stamps = source_stamps(&obs_out);
    assert!(
        stamps.len() >= SAMPLES,
        "the observer printed {} `ts=` values, wanted {SAMPLES}:\n{obs_out}",
        stamps.len()
    );
    eprintln!(
        "{label}: wall_ns={} (board), host [{before}, {after}], source stamps {stamps:?}",
        first.wall_ns
    );
    for ts in &stamps {
        let ok = match want {
            SourceStamp::HostClock => (before..=after).contains(ts) && !in_board_window(*ts),
            SourceStamp::Absent => *ts == 0,
        };
        assert!(
            ok,
            "{label}: a sample's source stamp is {ts} ns, which is not {want:?} (board \
             window [{BOARD_INSTANT_NS}, +60 s], host [{before}, {after}]); the table in \
             docs/reference/source-timestamps.md is wrong for this backend. \
             Observer:\n{obs_out}"
        );
    }
}

/// The cases above answer exactly the cells `matrix::consumer_of` routes here.
#[test]
fn cases_bound_to_matrix_cells() {
    use nros_tests::matrix::{
        Consumer, Kind as K, Lang as L, PlatformId as P, Rmw as R, Workload as W,
        assert_consumer_config_cases,
    };
    let cell = |l: L| (P::Linux, l, R::Zenoh, W::Pubsub, K::Example);
    assert_consumer_config_cases(
        Consumer::LinuxBoardClock,
        &[
            (cell(L::Rust), &[Config::ClockBoard]),
            (cell(L::C), &[Config::ClockBoard]),
            (cell(L::Cpp), &[Config::ClockBoard]),
            (
                (P::Linux, L::Rust, R::Cyclonedds, W::Pubsub, K::Example),
                &[Config::ClockBoard],
            ),
            (
                (P::Linux, L::Rust, R::Xrce, W::Pubsub, K::Example),
                &[Config::ClockBoard],
            ),
        ],
    );
}
