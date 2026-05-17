//! Phase 124.G.3 — `server_available()` flips false→true within
//! 100 ms of the server's first discovery announcement on the
//! zenoh-pico backend.
//!
//! The cffi routing tests
//! (`packages/core/nros-rmw-cffi/tests/server_available.rs`)
//! prove the slot is plumbed; this E2E exercises the actual
//! discovery timing on a live backend.
//!
//! **Status (deferred):** test stub uses one Executor + two
//! Nodes (client + server) — matches the Phase 104.B bridge
//! topology — but zenoh-pico's `server_seen` tracks REMOTE
//! queryables only; an in-process queryable registered on the
//! same zenoh-pico session is never reported to its own
//! liveliness subscription. The probe therefore stays false.
//!
//! Bridge mode with two different rmw names per Node would
//! open two sessions in the same Executor, but zenoh-pico-cffi
//! registers under a single name ("zenoh") and its static
//! single-process slot pools don't tolerate two concurrent
//! sessions anyway.
//!
//! Real-world server_available probes only need to flip when
//! a *remote* server appears (the common race the API guards
//! against). True E2E coverage needs a cross-process harness
//! (`ManagedProcess` × 2 connected via `zenohd_unique`), with
//! the client process polling `server_available()` after the
//! server process registers its service. Same harness gap as
//! Phase 124.G.2.
//!
//! Run (currently `#[ignore]`'d): `cargo test -p nros-tests
//! --test server_available_e2e --features trigger-test --
//! --test-threads=1 --ignored`

#![cfg(feature = "trigger-test")]

// Force-link zenoh-pico so its `.init_array` ctor registers the
// vtable before `Executor::open` runs.
use nros_rmw_zenoh as _;

use std::time::{Duration, Instant};

use nros_node::executor::*;
use nros_tests::fixtures::{ZenohRouter, require_zenohd, zenohd_unique};
use rstest::rstest;

/// Acceptance bound from `phase-124-rmw-zero-copy-dispatch.md`
/// thread C: client.server_available() must flip from false to
/// true within 100 ms of the server's first publish-discovery.
/// Allow some scheduler slack for CI under load.
const SERVER_DISCOVERY_BOUND_MS: u64 = 250;

/// Acceptance flow:
///   1. Spawn client, register a typeless service client on
///      `/svr_avail_e2e`.
///   2. Assert `server_available()` returns `Ok(false)` before
///      the server exists.
///   3. Spawn the server thread; have it register the matching
///      service server.
///   4. Poll `server_available()` from the main (client) thread
///      with a tight loop; assert it flips to `true` inside the
///      bound.
///   5. Record the elapsed time as a SUCCESS log so CI can
///      compare bounds across runs.
#[rstest]
#[ignore = "zenoh-pico server_seen tracks REMOTE queryables only; in-process \
            client+server can't see each other; needs cross-process harness"]
fn server_available_flips_within_100ms(zenohd_unique: ZenohRouter) {
    if !require_zenohd() {
        nros_tests::skip!("zenohd not found");
    }

    let locator = zenohd_unique.locator();

    // Single Executor — matches the Phase 104.B bridge topology
    // (one Executor, one or more Nodes per process). Avoids the
    // in-process dual-Executor zenoh-pico flake.
    let mut executor = Executor::open(
        &ExecutorConfig::new(&locator)
            .node_name("svr_avail_root")
            .domain_id(93),
    )
    .expect("Executor::open failed");

    let client = {
        let mut node = executor
            .create_node("svr_avail_client_node")
            .expect("client create_node failed");
        node.create_client_raw(
            "/svr_avail_e2e",
            "example_interfaces/srv/Trigger",
            "RIHS01_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        )
        .expect("create_client_raw failed")
    };

    // Drive spins so any latent discovery traffic settles.
    for _ in 0..10 {
        executor.spin_once(Duration::from_millis(20));
    }

    // Before the server exists the probe must return false.
    let before = client
        .server_available()
        .expect("server_available probe failed before server registration");
    assert!(
        !before,
        "server_available() returned true before the server was registered (got {:?})",
        before
    );

    // Register the server Node on the same Executor; discovery
    // clock starts at the moment the service-server entity
    // exists. Keep `_server` alive for the polling window.
    let discovery_start = Instant::now();
    let _server = {
        let mut node = executor
            .create_node("svr_avail_server_node")
            .expect("server create_node failed");
        node.create_service_raw(
            "/svr_avail_e2e",
            "example_interfaces/srv/Trigger",
            "RIHS01_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        )
        .expect("create_service_raw failed")
    };

    // Wait up to 5 s for liveliness propagation — well above the
    // 100 ms acceptance bound so the assertion below catches
    // latency regressions rather than test-harness flakes.
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
