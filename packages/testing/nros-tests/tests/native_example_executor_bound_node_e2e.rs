//! issue 1384 — the runtime consumer for the `ExecutorBoundNode` cell.
//!
//! `examples/native/c/custom-platform` is the one in-tree image that builds its
//! node with `nros_executor_node_init` AND then creates an entity eagerly
//! (`rclc_publisher_init_default`, three lines further down). PR #1064 moved it
//! onto that entry point because `nros_node_create_guard_condition` needs a node
//! that reaches an executor — correctly — and from then on its publisher
//! answered `NROS_RET_NOT_INIT`, because the predicate deciding which dispatch
//! arm to take also required `node_id != 0` and the first node an executor
//! builds takes slot 0.
//!
//! **Nothing noticed for four phases**, and the reason is this file's reason for
//! existing: the example's `examples/fixtures.toml` row was BUILD-ONLY. It
//! compiled and linked in every lane that built it and no lane ever ran the
//! binary, so `Failed to init publisher: -7` was printed to a stderr nobody
//! read — and the demo then fell through its cleanup chain into `return 0`, so
//! even a lane that HAD run it would have called it a pass. Both halves are
//! closed here: the demo exits non-zero on a failed init, and this test runs it.
//!
//! What it asserts, in the order the failure would appear:
//!
//!   1. the publisher was CREATED — the success marker, and the failure marker
//!      is asserted absent beside it, because "the success line never came" and
//!      "the failure line came, with -7" are different verdicts;
//!   2. the timer callback PUBLISHES, repeatedly. Creating a publisher that
//!      never publishes is the shape a session-less node would also produce.
//!
//! No peer and no delivery assertion: the subject is the node→executor binding,
//! which fails entirely inside one process. That is why `ExecutorBoundNode` is
//! its own workload rather than a `Pubsub` case — and a Pubsub cell would not
//! have caught this anyway, because every pubsub example uses the legacy
//! `rclc_node_init_default`, which was never affected.
//!
//! Run with:
//! `cargo nextest run -p nros-tests --test native_example_executor_bound_node_e2e`.

use nros_tests::{
    fixtures::{
        ManagedProcess, RequireFixture, Rmw as FixtureRmw, ZenohRouter, build_native_c_example_rmw,
        require_zenohd,
    },
    matrix::{
        Cell as MCell, Kind as MK, Lang as ML, PlatformId, Rmw as MR, Tier as MT, Workload as MW,
    },
    output::{
        CUSTOM_PLATFORM_PUBLISHER_CREATED_PREFIX, CUSTOM_PLATFORM_PUBLISHER_FAILED_PREFIX,
        INT32_TALKER_LOG_PREFIX,
    },
};
use std::{process::Command, time::Duration};

/// The cell this file consumes, as a predicate — ONE definition, read by both
/// the test and the tripwire so they cannot disagree.
fn is_executor_bound_node_cell(c: &MCell) -> bool {
    matches!(c.platform, PlatformId::Linux)
        && matches!(c.kind, MK::Example)
        && matches!(c.workload, MW::ExecutorBoundNode)
        && matches!(c.tier, MT::Runtime)
}

/// Tripwire — a cell added to the matrix without a case here would never run,
/// which is the exact shape (`build-only, reads as coverage`) this file exists
/// to close. Mirrors `native_example_pubsub_e2e`'s.
#[test]
fn executor_bound_node_cases_cover_every_matrix_cell() {
    let cells: Vec<&MCell> = nros_tests::matrix::CELLS
        .iter()
        .filter(|c| is_executor_bound_node_cell(c))
        .collect();
    assert_eq!(
        cells.len(),
        1,
        "expected exactly ONE Linux/ExecutorBoundNode/Example/Runtime cell (the \
         custom-platform demo); found {cells:?}. A new cell needs a case in this file, or it \
         runs nowhere."
    );
    assert!(
        matches!(cells[0].lang, ML::C) && matches!(cells[0].rmw, MR::Zenoh),
        "the cell moved off c/zenoh: {:?}",
        cells[0]
    );
}

#[test]
fn an_executor_bound_node_creates_its_eager_publisher_and_publishes() {
    let cell = nros_tests::matrix::CELLS
        .iter()
        .find(|c| is_executor_bound_node_cell(c))
        .expect("matrix regression: no Linux/ExecutorBoundNode/Example/Runtime cell");
    assert!(matches!(cell.rmw, MR::Zenoh));

    let binary = build_native_c_example_rmw("custom-platform", "baremetal_demo", FixtureRmw::Zenoh)
        .require("examples/native/c/custom-platform (baremetal_demo, zenoh)");

    if !require_zenohd() {
        nros_tests::skip!("zenohd not found");
    }
    let router = ZenohRouter::start_unique()
        .unwrap_or_else(|e| nros_tests::skip!("zenohd failed to start: {e}"));

    let mut cmd = Command::new(&binary);
    cmd.env("NROS_LOCATOR", router.locator())
        .env("RUST_LOG", "info");
    let mut demo = ManagedProcess::spawn_command(cmd, "custom-platform-demo")
        .unwrap_or_else(|e| panic!("spawn custom-platform: {e}"));

    // The demo publishes every 500 ms, so three lines is ~1.5 s of a 30 s
    // budget. Waiting for the PUBLISHES rather than the creation line means one
    // wait covers both assertions: the log it returns contains everything
    // printed up to that point.
    let out = demo
        .wait_for_output_count(INT32_TALKER_LOG_PREFIX, 3, Duration::from_secs(30))
        .unwrap_or_else(|e| {
            let tail = demo.collect_until("", Duration::from_millis(200));
            demo.kill();
            panic!(
                "custom-platform never published 3 `{INT32_TALKER_LOG_PREFIX}` lines ({e}).\n\
                 An executor-bound node whose eager publisher was refused prints \
                 `{CUSTOM_PLATFORM_PUBLISHER_FAILED_PREFIX} -7` and then goes quiet \
                 (issue 1384).\n--- output ---\n{tail}"
            )
        });
    demo.kill();

    assert!(
        !out.contains(CUSTOM_PLATFORM_PUBLISHER_FAILED_PREFIX),
        "the eager publisher was REFUSED on an executor-bound node — this is issue 1384 \
         (`{CUSTOM_PLATFORM_PUBLISHER_FAILED_PREFIX} -7` means resolve_session_and_domain took \
         the legacy arm into the NULL support that nros_executor_node_init sets on \
         purpose).\n--- output ---\n{out}"
    );
    assert!(
        out.contains(CUSTOM_PLATFORM_PUBLISHER_CREATED_PREFIX),
        "no `{CUSTOM_PLATFORM_PUBLISHER_CREATED_PREFIX}` line — the demo published without \
         announcing its publisher, so this test's markers have drifted from what it \
         prints.\n--- output ---\n{out}"
    );

    let published = nros_tests::count_pattern(&out, INT32_TALKER_LOG_PREFIX);
    assert!(
        published >= 3,
        "expected ≥3 publishes, got {published}\n--- output ---\n{out}"
    );
}
