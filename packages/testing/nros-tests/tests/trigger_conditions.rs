//! Trigger Conditions Integration Tests
//!
//! Tests guard conditions with real zenoh transport.
//! Trigger logic (AllOf, AnyOf, One, etc.) is unit-tested in nros-node
//! using MockSession — in-process zenoh pub/sub doesn't work due to
//! write filter limitations in zenoh-pico.
//!
//! Run with: `cargo test -p nros-tests --test trigger_conditions --features trigger-test`

// issue 0652 — force-link the zenoh backend. Without a direct reference rustc
// drops the dependency, its `#[no_mangle]` registration never reaches the test
// binary, and `Executor::open` fails with `Transport(InvalidConfig)` — which
// reads like a bad locator rather than a missing backend. `wake_latency.rs`
// carries the same line; this file was the one that did not, which is why it
// was the target that failed while its siblings passed.
//
// Same class as the `use nros_platform_cffi as _;` anchors issues 0619 and 0612
// needed: a dependency nothing references is not linked.
use nros_rmw_zenoh as _;

use nros_tests::fixtures::{ZenohRouter, require_zenohd, zenohd_unique};
use rstest::rstest;
use std::sync::atomic::{AtomicUsize, Ordering};

// =============================================================================
// Guard condition with real zenoh session
// =============================================================================

/// Test that guard conditions work with real zenoh transport.
#[rstest]
fn test_guard_condition_with_zenoh(zenohd_unique: ZenohRouter) {
    use nros_node::executor::*;

    if !require_zenohd() {
        nros_tests::skip!("zenohd not found");
    }

    let locator = zenohd_unique.locator();
    let config = ExecutorConfig::new(&locator)
        .node_name("guard_test")
        .domain_id(97);

    let mut executor = Executor::open(&config).expect("Failed to open session");

    static GUARD_FIRED: AtomicUsize = AtomicUsize::new(0);
    GUARD_FIRED.store(0, Ordering::SeqCst);

    let (_guard_id, guard_handle) = executor
        .register_guard_condition(|| {
            GUARD_FIRED.fetch_add(1, Ordering::SeqCst);
        })
        .expect("Failed to add guard condition");

    // Trigger from "external" (same thread, but simulates cross-thread trigger)
    guard_handle.trigger();

    // Spin to process
    executor.spin_once(core::time::Duration::from_millis(10));

    let fired = GUARD_FIRED.load(Ordering::SeqCst);
    assert_eq!(
        fired, 1,
        "Guard condition callback should fire exactly once (got {})",
        fired
    );

    // Spin again — should NOT fire (cleared after first trigger)
    executor.spin_once(core::time::Duration::from_millis(10));
    let fired2 = GUARD_FIRED.load(Ordering::SeqCst);
    assert_eq!(
        fired2, 1,
        "Guard condition should not fire again without re-trigger (got {})",
        fired2
    );

    println!("SUCCESS: Guard condition works with real zenoh transport");
}
