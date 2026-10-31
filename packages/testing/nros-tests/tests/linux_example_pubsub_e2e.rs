//! phase-329 W4 — THE native-example pubsub matrix consumer (RFC-0051).
//!
//! The fold VEHICLE for the native `Example` pubsub cells: it derives its cases
//! from `matrix::CELLS` (`Native` / `Pubsub` / `Example` / `Runtime`) instead of
//! the hand-written `nano2nano.rs` one-off. Built ADDITIVELY (phase-329 W4
//! dispositioning): it runs alongside `nano2nano` until its cases are run-proven,
//! THEN their delivery cases fold in and the duplicate is deleted. Nothing is
//! removed by this commit, so coverage cannot regress.
//!
//! Covers all 9 native pubsub cells (rust/c/cpp × zenoh/cyclone/xrce),
//! run-proven 2026-08-04 on FRESH fixtures. (The rust cyclone/xrce cells were
//! briefly carved when they failed on a 7-day-stale example binary — issue
//! #0413 — resolved once a fresh rebuild delivered.) Each boots a same-language
//! talker + listener and
//! proves the listener sees the `I heard:` marker (the plain example pair is
//! String on both ends in every language), under the isolation + env each RMW
//! needs — faithful to the working `nano2nano`/`linux_api`/`xrce.rs` tests:
//! - **zenoh** — ephemeral `zenohd`; both dial `NROS_LOCATOR`; assert ≥3.
//! - **cyclone** — unique `ROS_DOMAIN_ID` + `LD_LIBRARY_PATH=build/install/lib`
//!   (the cyclone libs load at runtime; without it the backend is dead and
//!   nothing delivers — the first run's root cause). Listener readiness +
//!   settle; assert ≥2.
//! - **xrce** — ephemeral Agent + `XRCE_MSG_COUNT` bounded receive; readiness +
//!   settle; assert ≥1.
//!
//! Also the consumer of the `HostLoop` cell (phase-520 W2): the same delivery,
//! to a listener whose `main` owns its loop (`nros::setup!()`).
//!
//! Run with: `cargo nextest run -p nros-tests --test linux_example_pubsub_e2e`.

use nros_tests::{
    TestResult,
    fixtures::{
        ManagedProcess, Rmw as FixtureRmw, XrceAgent, ZenohRouter, build_native_c_example_rmw,
        build_native_cpp_example_rmw, build_native_rust_example_rmw, require_xrce_agent,
        require_zenohd,
    },
    matrix::{Cell as MCell, Kind as MK, Lang as ML, Rmw as MR, Tier as MT, Workload as MW},
    output::LISTENER_LOG_PREFIX,
    unique_ros_domain_id,
};
use rstest::rstest;
use std::{path::PathBuf, process::Command, time::Duration};

/// Build a native example binary for `(lang, case, rmw)`. The cmake target names
/// are `c_talker`/`cpp_talker`/… ; the Rust `[[bin]]` names are the bare role.
fn fixture_rmw(r: MR) -> FixtureRmw {
    match r {
        MR::Zenoh => FixtureRmw::Zenoh,
        MR::Cyclonedds => FixtureRmw::Cyclonedds,
        MR::Xrce => FixtureRmw::Xrce,
        MR::Uorb => unreachable!("no uorb native example pubsub cell"),
    }
}

fn resolve(lang: ML, case: &str, binary: &str, rmw: MR) -> TestResult<PathBuf> {
    let fr = fixture_rmw(rmw);
    match lang {
        ML::Rust => build_native_rust_example_rmw(case, binary, fr).map(|p| p.to_path_buf()),
        ML::C => build_native_c_example_rmw(case, binary, fr).map(|p| p.to_path_buf()),
        ML::Cpp => build_native_cpp_example_rmw(case, binary, fr).map(|p| p.to_path_buf()),
        ML::Mixed => unreachable!("no mixed native example pubsub cell"),
    }
}

fn talker_bin(l: ML) -> &'static str {
    match l {
        ML::Rust => "talker",
        ML::C => "c_talker",
        ML::Cpp => "cpp_talker",
        ML::Mixed => "?",
    }
}
fn listener_bin(l: ML) -> &'static str {
    match l {
        ML::Rust => "listener",
        ML::C => "c_listener",
        ML::Cpp => "cpp_listener",
        ML::Mixed => "?",
    }
}
/// The listener's delivery marker. Run-proved 2026-08-04: the native talker/
/// listener demos (all three languages) default to the STRING topic and print
/// `I heard:` — the C listener's `Received:` (Int32) arm is only taken under
/// ~~`NROS_SUB_TYPE=int32`~~, which the plain example pair does not set. (The
/// per-lang split this replaced made c/cpp/zenoh falsely fail.)
fn listener_prefix(_l: ML) -> &'static str {
    LISTENER_LOG_PREFIX
}

fn rmw_str(r: MR) -> &'static str {
    match r {
        MR::Zenoh => "zenoh",
        MR::Cyclonedds => "cyclone",
        MR::Xrce => "xrce",
        MR::Uorb => "uorb",
    }
}

/// The cells this file consumes, as a predicate — ONE definition, used by both
/// the per-case lookup and the coverage tripwire so they cannot disagree.
fn is_pubsub_cell(c: &MCell) -> bool {
    matches!(c.platform, nros_tests::matrix::PlatformId::Linux)
        && matches!(c.kind, MK::Example)
        && matches!(c.workload, MW::Pubsub)
        && matches!(c.tier, MT::Runtime)
}

/// The `(lang, rmw)` pairs the `#[case]`s below declare. The tripwire asserts
/// this equals the filtered `matrix::CELLS` set, so a cell added to the matrix
/// without a case here FAILS rather than being silently unrun.
const DECLARED_CASES: &[(ML, MR)] = &[
    (ML::Rust, MR::Zenoh),
    (ML::C, MR::Zenoh),
    (ML::Cpp, MR::Zenoh),
    (ML::Rust, MR::Cyclonedds),
    (ML::C, MR::Cyclonedds),
    (ML::Cpp, MR::Cyclonedds),
    (ML::Rust, MR::Xrce),
    (ML::C, MR::Xrce),
    (ML::Cpp, MR::Xrce),
];

/// THE consumer — ONE TEST PER CELL (phase-342 W1).
///
/// This was a single `#[test]` folding over the nine cells, with a hand-rolled
/// `catch_unwind` that classified skip-vs-fail so one missing fixture could not
/// abort the rest. Two costs came with that shape:
///
///   * **wall clock.** Nine cells ran SERIALLY inside one test at 95.1 s — the
///     single longest test in tier 1, and therefore the floor the whole lane's
///     wall time could never drop below, no matter how parallel nextest is. No
///     scheduler enters a test body.
///   * **attribution.** A failure read `1 of 9 cell(s) FAILED` and took the
///     whole test red; the other eight verdicts were lost. Reconstructing which
///     coordinate actually broke is work issue 0422's triage had to do by hand.
///
/// Per-cell tests fix both, and the classification machinery simply goes away:
/// `run_cell` panics `[UNMET PRECONDITION] …` on an unmet precondition, which
/// is exactly what `nros_tests::unmet!` does everywhere else (issue 1758). The harness now does what the fold hand-rolled.
#[rstest]
#[case::rust_zenoh(ML::Rust, MR::Zenoh)]
#[case::c_zenoh(ML::C, MR::Zenoh)]
#[case::cpp_zenoh(ML::Cpp, MR::Zenoh)]
#[case::rust_cyclone(ML::Rust, MR::Cyclonedds)]
#[case::c_cyclone(ML::C, MR::Cyclonedds)]
#[case::cpp_cyclone(ML::Cpp, MR::Cyclonedds)]
#[case::rust_xrce(ML::Rust, MR::Xrce)]
#[case::c_xrce(ML::C, MR::Xrce)]
#[case::cpp_xrce(ML::Cpp, MR::Xrce)]
fn native_example_pubsub(#[case] lang: ML, #[case] rmw: MR) {
    let cell = nros_tests::matrix::CELLS
        .iter()
        .find(|c| is_pubsub_cell(c) && c.lang == lang && c.rmw == rmw)
        .unwrap_or_else(|| {
            panic!(
                "matrix regression: no Linux/Pubsub/Example/Runtime cell for {}/{}",
                lang.as_str(),
                rmw_str(rmw)
            )
        });
    run_cell(cell);
}

/// Tripwire — the `#[case]` list above is hand-written, so something must keep
/// it bound to the derived truth. Mirrors `interop::assert_test_bound`'s job for
/// the interop consumer.
#[test]
fn pubsub_cases_cover_every_matrix_cell() {
    use std::collections::BTreeSet;
    let from_matrix: BTreeSet<(String, String)> = nros_tests::matrix::CELLS
        .iter()
        .filter(|c| is_pubsub_cell(c))
        .map(|c| (c.lang.as_str().to_string(), rmw_str(c.rmw).to_string()))
        .collect();
    let declared: BTreeSet<(String, String)> = DECLARED_CASES
        .iter()
        .map(|(l, r)| (l.as_str().to_string(), rmw_str(*r).to_string()))
        .collect();

    assert!(
        !from_matrix.is_empty(),
        "matrix regression: no Native/Pubsub/Example runtime cells"
    );
    let missing: Vec<_> = from_matrix.difference(&declared).collect();
    let extra: Vec<_> = declared.difference(&from_matrix).collect();
    assert!(
        missing.is_empty() && extra.is_empty(),
        "pubsub #[case] list has drifted from matrix::CELLS.\n  \
         cells with no case (would never run): {missing:?}\n  \
         cases with no cell (would panic):     {extra:?}"
    );
}

/// Boot a same-language talker + listener under the cell's RMW isolation and
/// prove the listener receives ≥3 samples. Panics `[SKIPPED] …` on an unmet
/// precondition; the caller classifies.
fn run_cell(cell: &MCell) {
    let lang = cell.lang;
    let prefix = listener_prefix(lang);
    let talker = resolve(lang, "talker", talker_bin(lang), cell.rmw)
        .unwrap_or_else(|e| panic!("{} talker fixture not built: {e}", lang.as_str()));
    let listener = resolve(lang, "listener", listener_bin(lang), cell.rmw)
        .unwrap_or_else(|e| panic!("{} listener fixture not built: {e}", lang.as_str()));

    // Per-RMW isolation + env. Keep the router/agent guard alive for the cell.
    let mut talker_cmd = Command::new(&talker);
    let mut listener_cmd = Command::new(&listener);
    talker_cmd.env("RUST_LOG", "info");
    listener_cmd.env("RUST_LOG", "info");

    // Guards live to end of scope (their Drop tears down the router / agent).
    let mut _zenohd: Option<ZenohRouter> = None;
    let mut _agent: Option<XrceAgent> = None;

    // Per-RMW recipe, faithful to the working `linux_api`/`xrce.rs` tests:
    // (min delivered samples to assert, whether the listener needs a
    // ready-marker + stabilisation wait before the talker starts).
    let (min_count, needs_settle) = match cell.rmw {
        MR::Zenoh => {
            require_zenohd();
            let router = ZenohRouter::start_unique()
                .unwrap_or_else(|e| nros_tests::unmet!("zenohd failed to start: {e}"));
            talker_cmd.env("NROS_LOCATOR", router.locator());
            listener_cmd.env("NROS_LOCATOR", router.locator());
            _zenohd = Some(router);
            (3, false)
        }
        MR::Cyclonedds => {
            // The cyclone example binaries load libcyclonedds from
            // `build/install/lib` at runtime; without LD_LIBRARY_PATH the
            // backend never initialises and NOTHING delivers (this was the
            // 2026-08-04 root cause). Mirrors `linux_api::spawn_cyclone_binary`.
            let domain = unique_ros_domain_id().to_string();
            let cyclone_lib = nros_tests::project_root().join("build/install/lib");
            let ldp = match std::env::var_os("LD_LIBRARY_PATH") {
                Some(existing) if !existing.is_empty() => {
                    let mut paths = vec![cyclone_lib];
                    paths.extend(std::env::split_paths(&existing));
                    std::env::join_paths(paths).expect("valid LD_LIBRARY_PATH")
                }
                _ => cyclone_lib.into_os_string(),
            };
            for cmd in [&mut talker_cmd, &mut listener_cmd] {
                cmd.env("ROS_DOMAIN_ID", &domain)
                    .env("LD_LIBRARY_PATH", &ldp);
            }
            (2, true)
        }
        MR::Xrce => {
            require_xrce_agent();
            let agent = XrceAgent::start_unique()
                .unwrap_or_else(|e| nros_tests::unmet!("XRCE Agent failed to start: {e:?}"));
            let addr = agent.addr();
            let domain = unique_ros_domain_id().to_string();
            for cmd in [&mut talker_cmd, &mut listener_cmd] {
                cmd.env("NROS_LOCATOR", &addr)
                    .env("XRCE_AGENT_ADDR", &addr)
                    .env("ROS_DOMAIN_ID", &domain);
            }
            // The XRCE listener runs a bounded receive loop of XRCE_MSG_COUNT
            // (mirrors `xrce.rs::test_xrce_talker_listener_communication`).
            listener_cmd.env("XRCE_MSG_COUNT", "3");
            _agent = Some(agent);
            (1, true)
        }
        MR::Uorb => nros_tests::unmet!("uorb has no native pubsub example cell"),
    };

    // Listener first, so its subscription is live before the talker publishes.
    let mut listener_proc = ManagedProcess::spawn_command(listener_cmd, "native-example-listener")
        .unwrap_or_else(|e| panic!("[{}] spawn listener: {e}", rmw_str(cell.rmw)));
    // Cyclone/xrce discovery is slower — wait for the listener's ready marker,
    // then let the subscription propagate before the talker publishes.
    if needs_settle {
        // phase-342 — role, not string: the harness knows this demo spells
        // readiness differently in rust and C/C++, and it FAILS on timeout.
        listener_proc.expect_ready(
            nros_tests::output::DemoRole::Listener,
            lang,
            Duration::from_secs(30),
        );
        std::thread::sleep(Duration::from_secs(2));
    }
    let mut talker_proc = ManagedProcess::spawn_command(talker_cmd, "native-example-talker")
        .unwrap_or_else(|e| {
            listener_proc.kill();
            panic!("[{}] spawn talker: {e}", rmw_str(cell.rmw))
        });

    let out = listener_proc
        .wait_for_output_count(prefix, min_count, Duration::from_secs(25))
        .unwrap_or_else(|_| {
            talker_proc.kill();
            listener_proc.kill();
            panic!(
                "[{}/{}] listener never saw {min_count} `{prefix}` deliveries — native pubsub \
                 delivery did not work",
                lang.as_str(),
                rmw_str(cell.rmw)
            )
        });
    talker_proc.kill();
    listener_proc.kill();

    let n = nros_tests::count_pattern(&out, prefix);
    assert!(
        n >= min_count,
        "[{}/{}] expected ≥{min_count} deliveries, got {n}",
        lang.as_str(),
        rmw_str(cell.rmw)
    );
}

/// phase-520 W2 — the `HostLoop` cell: the stock rust talker into
/// `examples/linux/rust/listener-host-loop`, whose `main` is `nros::setup!()`'s
/// `nros_boot` + `nros_setup` and a plain `std` loop over `spin_once`
/// (RFC-0112 Tier 0). The node is the `listener` example's, unchanged, so a
/// delivery here proves the `setup!` assembly installs everything
/// `nros::main!` does — the subscription, its callback, and the board's log
/// sink the `I heard:` line goes through.
///
/// Then SIGTERM: the loop polls `nros_stop_requested()`, and the process must
/// EXIT on its own. A loop that ignores the board's guard never returns, and
/// the board never gets to close the session (issue 1732).
#[test]
fn native_example_host_loop_delivers() {
    let cell = nros_tests::matrix::CELLS
        .iter()
        .find(|c| {
            matches!(c.platform, nros_tests::matrix::PlatformId::Linux)
                && matches!(c.kind, MK::Example)
                && matches!(c.workload, MW::HostLoop)
                && matches!(c.tier, MT::Runtime)
        })
        .expect("matrix regression: no Linux/HostLoop/Example/Runtime cell");
    assert!(
        matches!((cell.lang, cell.rmw), (ML::Rust, MR::Zenoh)),
        "the HostLoop cell moved off rust/zenoh: {cell:?}"
    );

    let talker = resolve(ML::Rust, "talker", talker_bin(ML::Rust), MR::Zenoh)
        .unwrap_or_else(|e| panic!("rust talker fixture not built: {e}"));
    let listener = resolve(
        ML::Rust,
        "listener-host-loop",
        "listener-host-loop",
        MR::Zenoh,
    )
    .unwrap_or_else(|e| panic!("listener-host-loop fixture not built: {e}"));
    require_zenohd();
    let router = ZenohRouter::start_unique()
        .unwrap_or_else(|e| nros_tests::unmet!("zenohd failed to start: {e}"));

    let mut listener_cmd = Command::new(&listener);
    listener_cmd
        .env("RUST_LOG", "info")
        .env("NROS_LOCATOR", router.locator());
    let mut talker_cmd = Command::new(&talker);
    talker_cmd
        .env("RUST_LOG", "info")
        .env("NROS_LOCATOR", router.locator());

    let mut listener_proc = ManagedProcess::spawn_command(listener_cmd, "host-loop-listener")
        .unwrap_or_else(|e| panic!("spawn listener-host-loop: {e}"));
    let mut talker_proc = ManagedProcess::spawn_command(talker_cmd, "host-loop-talker")
        .unwrap_or_else(|e| {
            listener_proc.kill();
            panic!("spawn talker: {e}")
        });
    let out = listener_proc
        .wait_for_output_count(LISTENER_LOG_PREFIX, 3, Duration::from_secs(25))
        .unwrap_or_else(|e| {
            talker_proc.kill();
            listener_proc.kill();
            panic!(
                "the host-loop listener never saw 3 `{LISTENER_LOG_PREFIX}` deliveries — \
                 `nros::setup!()`'s assembly, spun from the program's own loop, did not \
                 deliver: {e}"
            )
        });
    talker_proc.kill();
    nros_tests::checker::assert_delivery(MW::HostLoop, &out, 3);

    // The loop must honour the board's termination guard and RETURN: one
    // SIGTERM to the pid (not the group), then a clean exit.
    let child = listener_proc.handle_mut();
    // SAFETY: plain kill(2) on a pid this test spawned and has not reaped.
    unsafe {
        libc::kill(child.id() as libc::pid_t, libc::SIGTERM);
    }
    let start = std::time::Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().expect("try_wait") {
            break status;
        }
        if start.elapsed() > Duration::from_secs(10) {
            listener_proc.kill();
            panic!(
                "listener-host-loop was still alive 10 s after one SIGTERM — its loop \
                 ignored `nros_stop_requested()`, so the board never closed the session"
            );
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    assert!(
        status.success(),
        "listener-host-loop ended {status} after SIGTERM; `nros_boot` returning Ok \
         after the loop is a clean exit"
    );
}

/// RFC-0109 D6a (phase-514 W3) — a hosted image's whole zenoh session comes
/// from `$ZENOH_SESSION_CONFIG_URI`, the json5 `rmw_zenoh` reads, lowered by
/// the same code the build runs on a bringup's `rmw/zenoh.json5`.
///
/// The talker gets NO `NROS_LOCATOR`: the router listens on a unique port, so
/// the only route to it is the file's `connect` list. Delivery therefore proves
/// the file was read, lowered and dialed — and that its `connect` replaced the
/// default router rather than sitting beside it (a dial of the default that
/// failed would refuse the session). Runs on the rust/zenoh cell's fixtures.
#[rstest]
#[case::rust(ML::Rust)]
#[case::c(ML::C)]
fn native_zenoh_session_config_file_is_the_route(#[case] lang: ML) {
    let prefix = listener_prefix(lang);
    let talker = resolve(lang, "talker", talker_bin(lang), MR::Zenoh)
        .unwrap_or_else(|e| panic!("{} talker fixture not built: {e}", lang.as_str()));
    let listener = resolve(lang, "listener", listener_bin(lang), MR::Zenoh)
        .unwrap_or_else(|e| panic!("{} listener fixture not built: {e}", lang.as_str()));
    require_zenohd();
    let router = ZenohRouter::start_unique()
        .unwrap_or_else(|e| nros_tests::unmet!("zenohd failed to start: {e}"));

    let dir = tempfile::tempdir().expect("tempdir");
    let config = dir.path().join("zenoh.json5");
    std::fs::write(
        &config,
        format!(
            "// the session's wiring, in zenoh's own language\n{{\n  mode: \"client\",\n  \
             connect: {{ endpoints: [\"{}\"] }},\n}}\n",
            router.locator()
        ),
    )
    .expect("write zenoh.json5");

    let mut listener_cmd = Command::new(&listener);
    listener_cmd
        .env("RUST_LOG", "info")
        .env("NROS_LOCATOR", router.locator());
    let mut talker_cmd = Command::new(&talker);
    talker_cmd
        .env("RUST_LOG", "info")
        .env_remove("NROS_LOCATOR")
        .env("ZENOH_SESSION_CONFIG_URI", &config);

    let mut listener_proc = ManagedProcess::spawn_command(listener_cmd, "session-config-listener")
        .unwrap_or_else(|e| panic!("spawn listener: {e}"));
    let mut talker_proc = ManagedProcess::spawn_command(talker_cmd, "session-config-talker")
        .unwrap_or_else(|e| {
            listener_proc.kill();
            panic!("spawn talker: {e}")
        });
    let out = listener_proc
        .wait_for_output_count(prefix, 3, Duration::from_secs(25))
        .unwrap_or_else(|e| {
            talker_proc.kill();
            listener_proc.kill();
            panic!(
                "[{}] the talker reached no router through ZENOH_SESSION_CONFIG_URI's \
                 `connect` — listener: {e}",
                lang.as_str()
            )
        });
    talker_proc.kill();
    listener_proc.kill();
    let n = nros_tests::count_pattern(&out, prefix);
    assert!(
        n >= 3,
        "[{}] expected ≥3 deliveries, got {n}",
        lang.as_str()
    );
}

/// RFC-0051 §7c (phase-510 W3) — the cases this file runs are exactly the
/// Runtime cells `matrix::consumer_of` assigns to it. Fixture-free: a cell
/// added, retiered or reassigned without this list moving is a red here.
#[test]
fn cases_bound_to_matrix_cells() {
    use nros_tests::matrix::{
        Consumer, Kind as K, Lang as L, PlatformId as P, Rmw as R, Workload as W,
        assert_consumer_cases,
    };
    assert_consumer_cases(
        Consumer::LinuxExamplePubsub,
        &[
            (P::Linux, L::C, R::Cyclonedds, W::Pubsub, K::Example),
            (P::Linux, L::C, R::Xrce, W::Pubsub, K::Example),
            (P::Linux, L::C, R::Zenoh, W::Pubsub, K::Example),
            (P::Linux, L::Cpp, R::Cyclonedds, W::Pubsub, K::Example),
            (P::Linux, L::Cpp, R::Xrce, W::Pubsub, K::Example),
            (P::Linux, L::Cpp, R::Zenoh, W::Pubsub, K::Example),
            (P::Linux, L::Rust, R::Cyclonedds, W::Pubsub, K::Example),
            (P::Linux, L::Rust, R::Xrce, W::Pubsub, K::Example),
            (P::Linux, L::Rust, R::Zenoh, W::Pubsub, K::Example),
            (P::Linux, L::Rust, R::Zenoh, W::HostLoop, K::Example),
        ],
    );
}
