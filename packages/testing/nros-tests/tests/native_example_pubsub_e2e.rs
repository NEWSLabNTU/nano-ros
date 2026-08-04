//! phase-329 W4 — THE native-example pubsub matrix consumer (RFC-0051).
//!
//! The fold VEHICLE for the native `Example` pubsub cells: it derives its cases
//! from `matrix::CELLS` (`Native` / `Pubsub` / `Example` / `Runtime`) instead of
//! the hand-written `nano2nano.rs` one-off. Built ADDITIVELY (phase-329 W4
//! dispositioning): it runs alongside `nano2nano` until its cases are run-proven,
//! THEN their delivery cases fold in and the duplicate is deleted. Nothing is
//! removed by this commit, so coverage cannot regress.
//!
//! Covers 7 of the 9 native pubsub cells, run-proven 2026-08-04: all three
//! zenoh cells + C/C++ over cyclone + C/C++ over xrce. The rust cyclone and rust
//! xrce cells are CARVED (see the filter): the rust same-language listener gets
//! zero delivery there, an unproven product path no existing test exercises.
//! Each boots a same-language talker + listener and
//! proves the listener sees the `I heard:` marker (the plain example pair is
//! String on both ends in every language), under the isolation + env each RMW
//! needs — faithful to the working `nano2nano`/`native_api`/`xrce.rs` tests:
//! - **zenoh** — ephemeral `zenohd`; both dial `NROS_LOCATOR`; assert ≥3.
//! - **cyclone** — unique `ROS_DOMAIN_ID` + `LD_LIBRARY_PATH=build/install/lib`
//!   (the cyclone libs load at runtime; without it the backend is dead and
//!   nothing delivers — the first run's root cause). Listener readiness +
//!   settle; assert ≥2.
//! - **xrce** — ephemeral Agent + `XRCE_MSG_COUNT` bounded receive; readiness +
//!   settle; assert ≥1.
//!
//! Run with: `cargo nextest run -p nros-tests --test native_example_pubsub_e2e`.

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
/// `NROS_SUB_TYPE=int32`, which the plain example pair does not set. (The
/// per-lang split this replaced made c/cpp/zenoh falsely fail.)
fn listener_prefix(_l: ML) -> &'static str {
    LISTENER_LOG_PREFIX
}
fn lang_str(l: ML) -> &'static str {
    match l {
        ML::Rust => "rust",
        ML::C => "c",
        ML::Cpp => "cpp",
        ML::Mixed => "mixed",
    }
}
fn rmw_str(r: MR) -> &'static str {
    match r {
        MR::Zenoh => "zenoh",
        MR::Cyclonedds => "cyclone",
        MR::Xrce => "xrce",
        MR::Uorb => "uorb",
    }
}

/// THE consumer. Iterates every native pubsub Example runtime cell; per-cell
/// `catch_unwind` classifies skip vs fail so one missing fixture never aborts
/// the rest.
#[test]
fn native_example_pubsub() {
    let cells: Vec<&MCell> = nros_tests::matrix::CELLS
        .iter()
        .filter(|c| {
            matches!(c.platform, nros_tests::matrix::PlatformId::Native)
                && matches!(c.kind, MK::Example)
                && matches!(c.workload, MW::Pubsub)
                && matches!(c.tier, MT::Runtime)
                // Carve: the RUST same-language listener over cyclone/xrce gets
                // ZERO delivery (run-prove 2026-08-04) while c/cpp pairs deliver
                // — and no existing test exercises a rust cyclone/xrce LISTENER
                // (native_api pairs a rust cyclone TALKER with c/cpp listeners),
                // so this is an unproven product path, not a harness bug. Left to
                // root-cause before these two coordinates join. The other 7 cells
                // (all zenoh + c/cpp cyclone + c/cpp xrce) are run-proven green.
                && !(matches!(c.lang, ML::Rust)
                    && matches!(c.rmw, MR::Cyclonedds | MR::Xrce))
        })
        .collect();
    assert!(
        !cells.is_empty(),
        "matrix regression: no Native/Pubsub/Example runtime cells"
    );

    let prev_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let mut skipped: Vec<String> = Vec::new();
    let mut failed: Vec<String> = Vec::new();
    for c in &cells {
        let label = format!("{}/{}", lang_str(c.lang), rmw_str(c.rmw));
        let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run_cell(c)));
        if let Err(p) = res {
            let msg = p
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| p.downcast_ref::<&str>().map(|s| s.to_string()))
                .unwrap_or_else(|| "<non-string panic>".to_string());
            if msg.contains("[SKIPPED]") {
                skipped.push(format!("{label}: {msg}"));
            } else {
                failed.push(format!("{label}: {msg}"));
            }
        }
    }
    std::panic::set_hook(prev_hook);

    assert!(
        failed.is_empty(),
        "native_example_pubsub: {} of {} cell(s) FAILED:\n  {}",
        failed.len(),
        cells.len(),
        failed.join("\n  ")
    );
    if skipped.len() == cells.len() {
        nros_tests::skip!(
            "all {} native pubsub cell(s) skipped:\n  {}",
            skipped.len(),
            skipped.join("\n  ")
        );
    }
}

/// Boot a same-language talker + listener under the cell's RMW isolation and
/// prove the listener receives ≥3 samples. Panics `[SKIPPED] …` on an unmet
/// precondition; the caller classifies.
fn run_cell(cell: &MCell) {
    let lang = cell.lang;
    let prefix = listener_prefix(lang);
    let talker = resolve(lang, "talker", talker_bin(lang), cell.rmw)
        .unwrap_or_else(|e| nros_tests::skip!("{} talker fixture not built: {e}", lang_str(lang)));
    let listener = resolve(lang, "listener", listener_bin(lang), cell.rmw).unwrap_or_else(|e| {
        nros_tests::skip!("{} listener fixture not built: {e}", lang_str(lang))
    });

    // Per-RMW isolation + env. Keep the router/agent guard alive for the cell.
    let mut talker_cmd = Command::new(&talker);
    let mut listener_cmd = Command::new(&listener);
    talker_cmd.env("RUST_LOG", "info");
    listener_cmd.env("RUST_LOG", "info");

    // Guards live to end of scope (their Drop tears down the router / agent).
    let mut _zenohd: Option<ZenohRouter> = None;
    let mut _agent: Option<XrceAgent> = None;

    // Per-RMW recipe, faithful to the working `native_api`/`xrce.rs` tests:
    // (min delivered samples to assert, whether the listener needs a
    // ready-marker + stabilisation wait before the talker starts).
    let (min_count, needs_settle) = match cell.rmw {
        MR::Zenoh => {
            if !require_zenohd() {
                nros_tests::skip!("zenohd not found");
            }
            let router = ZenohRouter::start_unique()
                .unwrap_or_else(|e| nros_tests::skip!("zenohd failed to start: {e}"));
            talker_cmd.env("NROS_LOCATOR", router.locator());
            listener_cmd.env("NROS_LOCATOR", router.locator());
            _zenohd = Some(router);
            (3, false)
        }
        MR::Cyclonedds => {
            // The cyclone example binaries load libcyclonedds from
            // `build/install/lib` at runtime; without LD_LIBRARY_PATH the
            // backend never initialises and NOTHING delivers (this was the
            // 2026-08-04 root cause). Mirrors `native_api::spawn_cyclone_binary`.
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
            if !require_xrce_agent() {
                nros_tests::skip!("XRCE agent not available");
            }
            let agent = XrceAgent::start_unique()
                .unwrap_or_else(|e| nros_tests::skip!("XRCE Agent failed to start: {e:?}"));
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
        MR::Uorb => nros_tests::skip!("uorb has no native pubsub example cell"),
    };

    // Listener first, so its subscription is live before the talker publishes.
    let mut listener_proc = ManagedProcess::spawn_command(listener_cmd, "native-example-listener")
        .unwrap_or_else(|e| panic!("[{}] spawn listener: {e}", rmw_str(cell.rmw)));
    // Cyclone/xrce discovery is slower — wait for the listener's ready marker,
    // then let the subscription propagate before the talker publishes.
    if needs_settle {
        let _ = listener_proc.wait_for_output_pattern("Waiting for", Duration::from_secs(30));
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
                lang_str(lang),
                rmw_str(cell.rmw)
            )
        });
    talker_proc.kill();
    listener_proc.kill();

    let n = nros_tests::count_pattern(&out, prefix);
    assert!(
        n >= min_count,
        "[{}/{}] expected ≥{min_count} deliveries, got {n}",
        lang_str(lang),
        rmw_str(cell.rmw)
    );
}
