//! Phase 124.A.8.b — zero-copy loan E2E against a real zenoh-pico backend.
//!
//! Spins up zenohd, opens an nros Executor against it, creates a raw
//! publisher + raw subscription, loans a slot via the
//! `EmbeddedRawPublisher::try_loan` API (which dispatches through the
//! Phase 124.A.4.b zenoh native loan trampolines under `rmw-lending`),
//! commits, and verifies the subscriber receives the same bytes.
//!
//! Run with: `cargo nextest run -p nros-tests --test loan_e2e \
//!   --features loan-e2e`

#![cfg(feature = "loan-e2e")]

use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};

use nros_node::ExecutorConfig;
use nros_node::executor::{EmbeddedRawPublisher, Executor};
use nros_tests::fixtures::{ZenohRouter, require_zenohd, zenohd_unique};
use rstest::rstest;

// Pull the zenoh backend in so its `.init_array` ctor fires before
// `Executor::open_with_rmw` — without this, the registry is empty and
// open returns ConnectionFailed.
#[allow(unused_imports)]
use nros_rmw_zenoh as _;

const TYPE_NAME: &str = "std_msgs/msg/dds_/String_";
const TYPE_HASH: &str = "RIHS01_loan_e2e_test_42424242424242424242424242424242";

#[rstest]
fn loan_commit_delivers_to_subscriber(zenohd_unique: ZenohRouter) {
    if !require_zenohd() {
        nros_tests::skip!("zenohd not found");
    }

    let locator = zenohd_unique.locator();

    // Two separate executors → two zenoh-pico sessions. Same-process
    // pub/sub on a SINGLE session hits zenoh-pico's write filter; the
    // executors here are independent sessions that round-trip through
    // zenohd. Pattern mirrors `trigger_conditions.rs`'s workaround.

    // Subscriber thread setup.
    static RX_COUNT: AtomicUsize = AtomicUsize::new(0);
    RX_COUNT.store(0, Ordering::SeqCst);
    let last_rx: Arc<Mutex<Vec<u8>>> = Arc::new(Mutex::new(Vec::new()));
    let last_rx_thread = Arc::clone(&last_rx);
    let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let stop_thread = Arc::clone(&stop);
    let locator_thread = locator.clone();

    let sub_handle = std::thread::spawn(move || {
        let cfg = ExecutorConfig::new(&locator_thread)
            .node_name("loan_e2e_sub")
            .domain_id(199);
        let mut sub_exec = Executor::open(&cfg).expect("sub executor open");
        sub_exec
            .register_subscription_buffered_raw::<_, 1024>(
                "/loan_e2e",
                TYPE_NAME,
                TYPE_HASH,
                Default::default(),
                move |bytes: &[u8]| {
                    RX_COUNT.fetch_add(1, Ordering::SeqCst);
                    if let Ok(mut g) = last_rx_thread.lock() {
                        *g = bytes.to_vec();
                    }
                },
            )
            .expect("register subscription");

        while !stop_thread.load(Ordering::SeqCst) {
            sub_exec.spin_once(Duration::from_millis(20));
        }
    });

    // Publisher executor.
    let cfg = ExecutorConfig::new(&locator)
        .node_name("loan_e2e_pub")
        .domain_id(199);
    let mut pub_exec = match Executor::open(&cfg) {
        Ok(e) => e,
        Err(e) => {
            stop.store(true, Ordering::SeqCst);
            let _ = sub_handle.join();
            nros_tests::skip!("pub executor open failed: {:?}", e);
        }
    };
    let node_id = pub_exec
        .node_builder("loan_e2e_node")
        .build()
        .expect("node build");
    let raw_pub: EmbeddedRawPublisher = pub_exec
        .with_node_try(node_id, |n| {
            n.create_publisher_raw("/loan_e2e", TYPE_NAME, TYPE_HASH)
                .map_err(|e| e.into())
        })
        .expect("create raw publisher");

    // Let discovery settle.
    let discovery_deadline = Instant::now() + Duration::from_secs(2);
    while Instant::now() < discovery_deadline {
        pub_exec.spin_once(Duration::from_millis(50));
    }

    // Publish via loan — the path under test. Loop a few times so a
    // single dropped sample during discovery doesn't fail the test.
    let payload = b"PHASE_124_A_8_B";
    for _ in 0..5 {
        let mut loan = raw_pub
            .try_loan(payload.len())
            .expect("try_loan should succeed (native zenoh loan path)");
        loan.as_mut().copy_from_slice(payload);
        loan.commit().expect("commit loan");
        pub_exec.spin_once(Duration::from_millis(50));
    }

    // Wait for at least one delivery.
    let recv_deadline = Instant::now() + Duration::from_secs(5);
    while RX_COUNT.load(Ordering::SeqCst) == 0 && Instant::now() < recv_deadline {
        pub_exec.spin_once(Duration::from_millis(20));
    }

    stop.store(true, Ordering::SeqCst);
    let _ = sub_handle.join();

    assert!(
        RX_COUNT.load(Ordering::SeqCst) >= 1,
        "subscriber must observe ≥ 1 message from the loan path",
    );
    let rx = last_rx.lock().expect("rx lock").clone();
    assert_eq!(
        &rx[..],
        payload.as_slice(),
        "loan-delivered payload must round-trip byte-identical",
    );
}
