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
