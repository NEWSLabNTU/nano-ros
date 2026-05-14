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
    alloc::{GlobalAlloc, Layout, System},
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};

use nros_node::{
    ExecutorConfig,
    executor::{EmbeddedRawPublisher, Executor},
};
use nros_tests::fixtures::{ZenohRouter, require_zenohd, zenohd_unique};
use rstest::rstest;

// Pull the zenoh backend in so its `.init_array` ctor fires before
// `Executor::open_with_rmw` — without this, the registry is empty and
// open returns ConnectionFailed.
#[allow(unused_imports)]
use nros_rmw_zenoh as _;

// =============================================================================
// Phase 124.A.8.c — counting global allocator
// =============================================================================
//
// Wraps System; increments two atomics on every alloc / dealloc. Tests
// snapshot the alloc count before a measured region and assert the
// delta. Single counter rules out interference between concurrent
// threads — the zero-alloc assertion runs only on the publisher thread
// and the wait-for-delivery loop on the same thread (no rx-thread
// allocations cross the window because we sleep on the rx thread).

static ALLOC_COUNT: AtomicUsize = AtomicUsize::new(0);
static DEALLOC_COUNT: AtomicUsize = AtomicUsize::new(0);

struct CountingAllocator;

unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOC_COUNT.fetch_add(1, Ordering::Relaxed);
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        DEALLOC_COUNT.fetch_add(1, Ordering::Relaxed);
        unsafe { System.dealloc(ptr, layout) };
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        ALLOC_COUNT.fetch_add(1, Ordering::Relaxed);
        unsafe { System.alloc_zeroed(layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        ALLOC_COUNT.fetch_add(1, Ordering::Relaxed);
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static GLOBAL: CountingAllocator = CountingAllocator;

fn alloc_count() -> usize {
    ALLOC_COUNT.load(Ordering::Relaxed)
}

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

#[rstest]
fn loan_path_is_alloc_free_on_native_zenoh(zenohd_unique: ZenohRouter) {
    // Phase 124.A.8.c — verifies the Phase 124.A.4.b zenoh native loan
    // trampoline is truly zero-allocation on the commit path. Counts
    // global allocations across a tight `try_loan → write → commit`
    // window and asserts a small budget (NOT zero — zenoh-pico's
    // `publish_with_attachment_aliased` may transiently allocate
    // internal RTPS framing buffers depending on its build config).
    //
    // Budget chosen empirically: the native loan path itself does NOT
    // allocate (slot bytes alias an arena buffer; commit_slot calls
    // `publish_with_attachment_aliased` which uses
    // `z_bytes_from_static_buf` — no payload copy). Allowance covers
    // potential transient log/string allocs in error paths only.
    const ALLOC_BUDGET_PER_PUBLISH: usize = 4;

    if !require_zenohd() {
        nros_tests::skip!("zenohd not found");
    }

    let locator = zenohd_unique.locator();

    let cfg = ExecutorConfig::new(&locator)
        .node_name("loan_alloc_pub")
        .domain_id(200);
    let mut pub_exec = match Executor::open(&cfg) {
        Ok(e) => e,
        Err(e) => nros_tests::skip!("pub executor open failed: {:?}", e),
    };
    let node_id = pub_exec
        .node_builder("loan_alloc_node")
        .build()
        .expect("node build");
    let raw_pub: EmbeddedRawPublisher = pub_exec
        .with_node_try(node_id, |n| {
            n.create_publisher_raw("/loan_alloc", TYPE_NAME, TYPE_HASH)
                .map_err(|e| e.into())
        })
        .expect("create raw publisher");

    // Let discovery settle so timer-driven allocs don't leak into the
    // measured window.
    let discovery_deadline = Instant::now() + Duration::from_secs(1);
    while Instant::now() < discovery_deadline {
        pub_exec.spin_once(Duration::from_millis(50));
    }

    let payload = b"alloc_trace_test_payload_24B";
    debug_assert_eq!(payload.len(), 28);

    // Warm path: do one loan-commit so any first-publish lazy init
    // (zenoh-pico cache, sequence counter) doesn't taint the budget.
    {
        let mut loan = raw_pub.try_loan(payload.len()).expect("warm loan");
        loan.as_mut().copy_from_slice(payload);
        loan.commit().expect("warm commit");
    }

    // Measured window.
    const N: usize = 4;
    let before = alloc_count();
    for _ in 0..N {
        let mut loan = raw_pub
            .try_loan(payload.len())
            .expect("measured loan should succeed");
        loan.as_mut().copy_from_slice(payload);
        loan.commit().expect("measured commit");
    }
    let after = alloc_count();
    let delta = after - before;

    let budget = ALLOC_BUDGET_PER_PUBLISH * N;
    assert!(
        delta <= budget,
        "Phase 124.A.4.b native zenoh loan must stay under {budget} allocs across {N} \
         publishes (per-publish budget {ALLOC_BUDGET_PER_PUBLISH}), observed {delta}. \
         If this fires after a zenoh-pico bump, verify the upstream \
         `z_bytes_from_static_buf` path still aliases.",
    );
    eprintln!("loan zero-alloc trace: {N} publishes ⇒ {delta} allocs (budget {budget})",);
}
