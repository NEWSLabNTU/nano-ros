//! issue 2452 — the runtime consumer for the `CustomLink` cell.
//!
//! `examples/linux/c/custom-transport-loopback` registers its own link
//! (`rclc_set_custom_transport`) and opens its session on a `custom/…`
//! locator. Its `examples/fixtures.toml` row was BUILD-ONLY, and the binary
//! exited 1 on every host: `rclc_support_init_with_locator` answered
//! `ConnectionFailed` before the transport's `open` ever ran. Three defects
//! stood in a row, each hiding the next:
//!
//!   1. no C/C++ image compiled zenoh-pico's custom link at all — only a Rust
//!      manifest could name `link-custom`, so `custom` was an unknown scheme;
//!   2. the example's ring buffer echoed the image's own frames back, so even
//!      with the link the handshake read its own `InitSyn` where it waits for
//!      an `InitAck`;
//!   3. a closed zenoh session was never DROPPED, so its link's `close` never
//!      ran (nor did a TCP link's socket close).
//!
//! What it asserts, in the order a regression would show:
//!
//!   1. the image EXITS 0 — its own verdict that all four callbacks fired;
//!   2. each count it printed: `open` once, `write` and `read` at least once,
//!      `close` once. The exit code alone would not say WHICH callback a
//!      regression lost;
//!   3. it published, i.e. the session that opened through the link carried
//!      the node's traffic.
//!
//! Run with:
//! `cargo nextest run -p nros-tests --test linux_example_custom_link_e2e`.

use nros_tests::{
    fixtures::{
        ManagedProcess, RequireFixture, Rmw as FixtureRmw, ZenohRouter, build_native_c_example_rmw,
        require_zenohd,
    },
    matrix::{
        Cell as MCell, Kind as MK, Lang as ML, PlatformId, Rmw as MR, Tier as MT, Workload as MW,
    },
    output::{
        CUSTOM_LINK_CALLBACKS, CUSTOM_LINK_COUNTS_HEADER, INT32_TALKER_LOG_PREFIX,
        custom_link_count,
    },
};
use std::{process::Command, time::Duration};

/// The cell this file consumes, as a predicate — ONE definition, read by both
/// the test and the tripwire so they cannot disagree.
fn is_custom_link_cell(c: &MCell) -> bool {
    matches!(c.platform, PlatformId::Linux)
        && matches!(c.kind, MK::Example)
        && matches!(c.workload, MW::CustomLink)
        && matches!(c.tier, MT::Runtime)
}

#[test]
fn an_application_supplied_link_carries_the_session_and_is_closed() {
    let cell = nros_tests::matrix::CELLS
        .iter()
        .find(|c| is_custom_link_cell(c))
        .expect("matrix regression: no Linux/CustomLink/Example/Runtime cell");
    assert!(matches!(cell.lang, ML::C) && matches!(cell.rmw, MR::Zenoh));

    let binary = build_native_c_example_rmw(
        "custom-transport-loopback",
        "c_custom_transport_loopback",
        FixtureRmw::Zenoh,
    )
    .require("examples/linux/c/custom-transport-loopback (zenoh)");

    require_zenohd();
    let router = ZenohRouter::start_unique()
        .unwrap_or_else(|e| nros_tests::unmet!("zenohd failed to start: {e}"));
    // The example's own link dials the router; it takes `host:port`.
    let locator = router.locator();
    let target = locator
        .strip_prefix("tcp/")
        .unwrap_or_else(|| panic!("zenohd locator is not tcp/…: {locator}"));

    let mut cmd = Command::new(&binary);
    cmd.env("NROS_CUSTOM_TCP_TARGET", target)
        .env("RUST_LOG", "info");
    let mut demo = ManagedProcess::spawn_image(cmd, "custom-link-demo")
        .unwrap_or_else(|e| panic!("spawn custom-transport-loopback: {e}"));

    // The image spins ~3 s and then tears down by itself; 30 s is the budget
    // for a loaded host, and reaching it kills the process (a red below).
    let out = demo
        .wait_for_all_output(Duration::from_secs(30))
        .unwrap_or_else(|e| panic!("custom-transport-loopback printed nothing ({e})"));
    let status = demo
        .handle_mut()
        .wait()
        .expect("reap custom-transport-loopback");

    assert!(
        out.contains(CUSTOM_LINK_COUNTS_HEADER),
        "no `{CUSTOM_LINK_COUNTS_HEADER}` line: the image never reached teardown. A session \
         that refuses the custom link stops at `rclc_support_init_with_locator … -> -4` \
         (issue 2452).\n--- output ---\n{out}"
    );
    let count = |cb: &str| {
        custom_link_count(&out, cb)
            .unwrap_or_else(|| panic!("no `{cb}` count under the header\n--- output ---\n{out}"))
    };
    let [open, write, read, close] = CUSTOM_LINK_CALLBACKS;
    assert_eq!(
        count(open),
        1,
        "`open` must fire once\n--- output ---\n{out}"
    );
    assert!(
        count(write) >= 1,
        "`write` never fired\n--- output ---\n{out}"
    );
    assert!(
        count(read) >= 1,
        "`read` never fired\n--- output ---\n{out}"
    );
    assert_eq!(
        count(close),
        1,
        "`close` must fire once — a closed session that is never DROPPED keeps its link \
         open (issue 2452)\n--- output ---\n{out}"
    );
    assert!(
        status.success(),
        "custom-transport-loopback exited {status}\n--- output ---\n{out}"
    );

    let published = nros_tests::count_pattern(&out, INT32_TALKER_LOG_PREFIX);
    assert!(
        published >= 1,
        "the session opened but the node never published through it\n--- output ---\n{out}"
    );
}

/// Tripwire — a cell added to the matrix without a case here would never run,
/// which is the exact shape (`build-only, reads as coverage`) this file exists
/// to close.
#[test]
fn custom_link_cases_cover_every_matrix_cell() {
    let cells: Vec<&MCell> = nros_tests::matrix::CELLS
        .iter()
        .filter(|c| is_custom_link_cell(c))
        .collect();
    assert_eq!(
        cells.len(),
        1,
        "expected exactly ONE Linux/CustomLink/Example/Runtime cell (the C \
         custom-transport demo); found {cells:?}. A new cell needs a case in this file, or it \
         runs nowhere."
    );
}

/// RFC-0051 §7c — the cases this file runs are exactly the Runtime cells
/// `matrix::consumer_of` assigns to it. Fixture-free.
#[test]
fn cases_bound_to_matrix_cells() {
    use nros_tests::matrix::{
        Consumer, Kind as K, Lang as L, PlatformId as P, Rmw as R, Workload as W,
        assert_consumer_cases,
    };
    assert_consumer_cases(
        Consumer::LinuxExampleCustomLink,
        &[(P::Linux, L::C, R::Zenoh, W::CustomLink, K::Example)],
    );
}

/// The count parser the test reads the verdict through, against the lines the
/// example prints — fixture-free, so a drifted spelling is a red here and not
/// a 30 s wait.
#[test]
fn the_count_lines_parse() {
    let [open, write, read, close] = CUSTOM_LINK_CALLBACKS;
    let out = format!(
        "{INT32_TALKER_LOG_PREFIX} 0\n{CUSTOM_LINK_COUNTS_HEADER}\n  {open}  1\n  {write} 18\n  \
         {read}  2738\n  {close} 0\n"
    );
    assert_eq!(custom_link_count(&out, open), Some(1));
    assert_eq!(custom_link_count(&out, write), Some(18));
    assert_eq!(custom_link_count(&out, read), Some(2738));
    assert_eq!(custom_link_count(&out, close), Some(0));
    // A count above the header is not the verdict.
    assert_eq!(custom_link_count(&format!("  {open}  1\n"), open), None);
}
