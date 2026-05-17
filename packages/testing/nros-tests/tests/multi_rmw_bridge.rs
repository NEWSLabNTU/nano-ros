//! Phase 124.G.2 — Multi-RMW bridge ≥ 99% delivery.
//!
//! Single Executor with two Nodes per Phase 104.B's bridge
//! topology:
//!   * Node A uses the primary session (default rmw).
//!   * Node B uses an extra session opened via
//!     `NodeBuilder::rmw("name")` against a second backend.
//!
//! The "bridge" is a tiny callback registered on Node A's
//! subscriber that republishes received bytes through Node B's
//! publisher (the typical drone-bridge shape from
//! `docs/roadmap/phase-104-multi-backend-bridges.md`). A
//! third subscriber on Node B's session counts what makes it
//! through.
//!
//! Run: `cargo test -p nros-tests --test multi_rmw_bridge
//! --features multi-rmw-bridge -- --test-threads=1`

#![cfg(feature = "multi-rmw-bridge")]

// Force-link both backends so each one's `.init_array` ctor
// registers its vtable before `Executor::open` /
// `NodeBuilder::rmw(...)` runs.
use nros_rmw_dds as _;
use nros_rmw_zenoh as _;

use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicU32, Ordering},
    },
    time::{Duration, Instant},
};

use nros_node::{QosSettings, executor::*};
use nros_tests::fixtures::{ZenohRouter, require_zenohd, zenohd_unique};
use rstest::rstest;

/// Acceptance: at least 99% of N messages forwarded by the
/// bridge reach the destination subscriber within the
/// `condvar_wake_latency + drive_io_drain` budget. CI slack
/// included.
const DELIVERY_THRESHOLD: f64 = 0.99;
const MESSAGE_COUNT: u32 = 50;
const DELIVERY_BUDGET: Duration = Duration::from_secs(10);

#[rstest]
#[ignore = "Phase 124.G.2 dust-DDS topic-cache landed (writer + reader on the same \
            participant no longer trip PreconditionNotMet); test runs end-to-end but \
            delivers 0/N — same-participant local loopback isn't wired in dust-DDS \
            by default. Needs either an explicit `ignore_self` toggle or two dust-DDS \
            participants (one per Node) and discovery between them"]
fn bridge_zenoh_to_dds_delivers_99pct(zenohd_unique: ZenohRouter) {
    if !require_zenohd() {
        nros_tests::skip!("zenohd not found");
    }

    let locator = zenohd_unique.locator();
    let mut executor = Executor::open(
        &ExecutorConfig::new(&locator)
            .node_name("bridge_root")
            .domain_id(92),
    )
    .expect("Executor::open failed");

    let received = Arc::new(AtomicU32::new(0));
    let received_cb = Arc::clone(&received);

    // Bridge callback state. Shared between the source sub
    // (fires on receive, forwards to dest pub) and the dest
    // sub (counts what arrives).
    let bridge_buffer = Arc::new(Mutex::new(Vec::<u8>::new()));
    let bridge_buffer_recv = Arc::clone(&bridge_buffer);

    // Build Node A (default rmw = zenoh primary). Subscribes
    // to `/bridge_src`, captures the payload, hands it to the
    // shared buffer for the bridge thread to forward.
    let node_a_id = executor
        .node_builder("bridge_node_a")
        .build()
        .expect("node A build failed");
    executor
        .register_subscription_buffered_raw_on::<_, 256>(
            node_a_id,
            "/bridge_src",
            "std_msgs/msg/UInt32",
            "",
            QosSettings::default(),
            move |data: &[u8]| {
                let mut buf = bridge_buffer.lock().unwrap();
                buf.clear();
                buf.extend_from_slice(data);
            },
        )
        .expect("Node A sub register failed");
    let publisher_a = executor
        .with_node_try(node_a_id, |n| {
            n.create_publisher_raw("/bridge_src", "std_msgs/msg/UInt32", "")
        })
        .expect("Node A pub register failed");

    // Build Node B with rmw = "dds". Publishes to `/bridge_dst`,
    // subscribes (on the same session) to count deliveries.
    let node_b_id = executor
        .node_builder("bridge_node_b")
        .rmw("dds")
        .build()
        .expect("node B build failed");
    executor
        .register_subscription_buffered_raw_on::<_, 256>(
            node_b_id,
            "/bridge_dst",
            "std_msgs/msg/UInt32",
            "",
            QosSettings::default(),
            move |_data: &[u8]| {
                received_cb.fetch_add(1, Ordering::SeqCst);
            },
        )
        .expect("Node B sub register failed");
    let publisher_b = executor
        .with_node_try(node_b_id, |n| {
            n.create_publisher_raw("/bridge_dst", "std_msgs/msg/UInt32", "")
        })
        .expect("Node B pub register failed");

    // Settle discovery on both sides.
    for _ in 0..20 {
        executor.spin_once(Duration::from_millis(50));
    }

    let start = Instant::now();
    for i in 0u32..MESSAGE_COUNT {
        // Publish on A.
        let msg = i.to_le_bytes();
        publisher_a
            .publish_raw(&msg)
            .expect("publish on /bridge_src");

        // Drive A's sub callback so the bridge buffer fills.
        for _ in 0..3 {
            executor.spin_once(Duration::from_millis(10));
        }

        // Forward the captured bytes to B's pub.
        let payload = {
            let buf = bridge_buffer_recv.lock().unwrap();
            buf.clone()
        };
        if !payload.is_empty() {
            publisher_b
                .publish_raw(&payload)
                .expect("publish on /bridge_dst");
        }

        // Drive B's sub callback so the counter increments.
        for _ in 0..3 {
            executor.spin_once(Duration::from_millis(10));
        }
    }

    // Drain.
    let deadline = start + DELIVERY_BUDGET;
    while Instant::now() < deadline
        && (received.load(Ordering::SeqCst) as f64) < (MESSAGE_COUNT as f64 * DELIVERY_THRESHOLD)
    {
        executor.spin_once(Duration::from_millis(50));
    }
    let elapsed = start.elapsed();
    let count = received.load(Ordering::SeqCst);
    let ratio = count as f64 / MESSAGE_COUNT as f64;

    assert!(
        ratio >= DELIVERY_THRESHOLD,
        "bridge delivered {count}/{MESSAGE_COUNT} ({:.1}%) in {:?} — below {:.0}% threshold",
        ratio * 100.0,
        elapsed,
        DELIVERY_THRESHOLD * 100.0,
    );

    println!(
        "SUCCESS: bridge delivered {count}/{MESSAGE_COUNT} ({:.1}%) in {:?}",
        ratio * 100.0,
        elapsed,
    );
}
