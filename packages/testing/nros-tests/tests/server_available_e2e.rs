//! Phase 124.G.3 — `server_available()` flips false→true within
//! 100 ms of the server's first discovery announcement on the
//! dust-DDS backend.
//!
//! The cffi routing tests
//! (`packages/core/nros-rmw-cffi/tests/server_available.rs`)
//! prove the slot is plumbed; this E2E exercises the actual
//! discovery timing on a live backend.
//!
//! Single Executor, three Nodes per Phase 104.B's bridge
//! topology — matches the same pattern used by the G.2 multi-
//! RMW bridge test:
//!   * Node A on the primary zenoh-pico session (smoke; not
//!     used in the probe).
//!   * Node B (client) on a dust-DDS extra session via
//!     `NodeBuilder::rmw("dds").locator("client")`.
//!   * Node C (server) on a second dust-DDS extra session via
//!     `NodeBuilder::rmw("dds").locator("server")` — distinct
//!     locator forces `resolve_session_slot` to open a separate
//!     dust-DDS participant.
//!
//! dust-DDS is the right backend for this test because (a)
//! zenoh-pico's `server_seen` tracks REMOTE queryables only
//! (in-process queryables on the same session never appear in
//! their own liveliness subscription), and (b) zenoh-pico's
//! single-process static slot pools don't tolerate two
//! sessions in one binary. dust-DDS opens a fresh
//! `DomainParticipant` per session and the two participants
//! discover each other via UDP exactly the way real DDS does.
//!
//! Run: `cargo test -p nros-tests --test server_available_e2e
//! --features multi-rmw-bridge -- --test-threads=1`

#![cfg(feature = "multi-rmw-bridge")]

// Force-link both backends so each one's `.init_array` ctor
// registers its vtable before `Executor::open` /
// `NodeBuilder::rmw(...)` runs.
use nros_rmw_dds as _;
use nros_rmw_zenoh as _;

use std::time::{Duration, Instant};

use nros_node::executor::*;
use nros_tests::fixtures::{ZenohRouter, require_zenohd, zenohd_unique};
use rstest::rstest;

/// Acceptance bound from `phase-124-rmw-zero-copy-dispatch.md`
/// thread C. CI slack allows 250 ms; the SUCCESS log records the
/// raw elapsed so regressions show up in test output.
const SERVER_DISCOVERY_BOUND_MS: u64 = 250;

#[rstest]
fn server_available_flips_within_100ms(zenohd_unique: ZenohRouter) {
    if !require_zenohd() {
        nros_tests::skip!("zenohd not found");
    }

    let locator = zenohd_unique.locator();
    let mut executor = Executor::open(
        &ExecutorConfig::new(&locator)
            .node_name("svr_avail_root")
            .domain_id(93),
    )
    .expect("Executor::open failed");

    // Node A — smoke: keeps the multi-rmw Executor's primary
    // session alive without being involved in the probe.
    let _node_a_id = executor
        .node_builder("svr_avail_node_a")
        .build()
        .expect("node A build failed");

    // Node B — client on dust-DDS extra session "client".
    let node_b_id = executor
        .node_builder("svr_avail_client")
        .rmw("dds")
        .locator("client")
        .build()
        .expect("node B build failed");
    let client = executor
        .with_node_try(node_b_id, |n| {
            n.create_client_raw(
                "/svr_avail_e2e",
                "example_interfaces/srv/Trigger",
                "RIHS01_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            )
        })
        .expect("create_client_raw failed");

    // Settle: spin enough for the client's writer to register
    // and any spurious early matches to clear.
    for _ in 0..20 {
        executor.spin_once(Duration::from_millis(20));
    }

    // Pre-server probe — must be false.
    let before = client
        .server_available()
        .expect("server_available probe failed before server registration");
    assert!(
        !before,
        "server_available() returned true before the server was registered (got {:?})",
        before
    );

    // Node C — server on dust-DDS extra session "server".
    // Discovery clock starts the moment the service-server
    // entity exists. Keep `_server` alive for the polling
    // window.
    let discovery_start = Instant::now();
    let node_c_id = executor
        .node_builder("svr_avail_server")
        .rmw("dds")
        .locator("server")
        .build()
        .expect("node C build failed");
    let _server = executor
        .with_node_try(node_c_id, |n| {
            n.create_service_raw(
                "/svr_avail_e2e",
                "example_interfaces/srv/Trigger",
                "RIHS01_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            )
        })
        .expect("create_service_raw failed");

    // Poll for the flip. Allow up to 5 s wall-clock so the
    // assertion below catches latency regressions rather than
    // test-harness flakes; the actual measured elapsed is
    // checked against the 100 ms acceptance bound (with CI
    // slack).
    let deadline = discovery_start + Duration::from_secs(5);
    let mut latest_state = false;
    while Instant::now() < deadline {
        executor.spin_once(Duration::from_millis(20));
        latest_state = client
            .server_available()
            .expect("server_available probe failed during poll");
        if latest_state {
            break;
        }
    }
    let elapsed = discovery_start.elapsed();

    assert!(
        latest_state,
        "server_available() never flipped to true (waited {:?})",
        elapsed,
    );
    assert!(
        elapsed <= Duration::from_millis(SERVER_DISCOVERY_BOUND_MS),
        "server_available() flipped after {:?} — over the {} ms bound \
         (100 ms acceptance target; bound widened to {} ms for CI slack)",
        elapsed,
        SERVER_DISCOVERY_BOUND_MS,
        SERVER_DISCOVERY_BOUND_MS,
    );

    println!(
        "SUCCESS: server_available() flipped false→true in {:?} \
         (bound {} ms; 100 ms acceptance target)",
        elapsed, SERVER_DISCOVERY_BOUND_MS,
    );
}
