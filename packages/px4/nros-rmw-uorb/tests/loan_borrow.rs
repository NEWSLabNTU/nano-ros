#![allow(non_camel_case_types)]
//! End-to-end loan/borrow API via the px4-uorb std mock broker.
//!
//! Mirror of `typeless_api.rs` but using the new Phase 97 zero-copy
//! API: `try_loan` returns a writable `PublishLoan` slice; user fills
//! in place; `commit` triggers the wire write. `try_borrow` returns a
//! `RecvView` lent from the subscriber's internal buffer.
//!
//! On uORB the underlying backend has no native lending support, so
//! the loan goes through `EmbeddedRawPublisher`'s per-publisher arena
//! and memcpys at commit time. Saves the user-side copy that
//! `publish_raw(&[u8])` would have made before calling the backend;
//! same on-the-wire semantics otherwise.

#![cfg(feature = "std")]

use core::time::Duration;
use std::sync::Mutex;

use nros_node::{Executor, ExecutorConfig};
use px4_sys::orb_metadata;
use px4_uorb::{OrbMetadata, UorbTopic};

static TEST_LOCK: Mutex<()> = Mutex::new(());

#[repr(C)]
#[derive(Copy, Clone, Debug, PartialEq)]
struct Tick {
    seq: u32,
    payload: [u8; 8],
}

struct tick_topic;
static TICK_NAME: [u8; 11] = *b"sensor_acc\0";
static TICK_META: OrbMetadata = OrbMetadata::new(orb_metadata {
    o_name: TICK_NAME.as_ptr() as *const _,
    o_size: core::mem::size_of::<Tick>() as u16,
    o_size_no_padding: core::mem::size_of::<Tick>() as u16,
    message_hash: 0,
    o_id: u16::MAX,
    o_queue: 1,
});
impl UorbTopic for tick_topic {
    type Msg = Tick;
    fn metadata() -> &'static orb_metadata {
        TICK_META.get()
    }
}

#[test]
fn loan_borrow_round_trip_via_executor() {
    let _g = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    px4_uorb::_reset_broker();
    nros_rmw_uorb::_reset();

    nros_rmw_uorb::register::<tick_topic>("/fmu/out/sensor_accel", 0).expect("register");

    let config = ExecutorConfig::new("").node_name("loan_borrow");
    let mut executor = Executor::open(&config).expect("open");
    let mut node = executor.create_node("loan_borrow").expect("create_node");

    let publisher = node
        .create_publisher_raw("/fmu/out/sensor_accel", "px4::Tick", "0")
        .expect("publisher");
    let mut subscriber = node
        .create_subscription_raw("/fmu/out/sensor_accel", "px4::Tick", "0")
        .expect("subscriber");

    // Loan + fill in place + commit.
    let msg = Tick {
        seq: 0xabcdef01,
        payload: *b"loan-bor",
    };
    let mut loan = publisher
        .try_loan(core::mem::size_of::<Tick>())
        .expect("loan");
    // Write the message bytes directly into the loan's slice. No
    // intermediate user buffer.
    let bytes: &[u8] = unsafe {
        core::slice::from_raw_parts(
            &msg as *const Tick as *const u8,
            core::mem::size_of::<Tick>(),
        )
    };
    loan.as_mut().copy_from_slice(bytes);
    loan.commit().expect("commit");

    // Spin once so the broker delivers (uORB std mock fires callbacks
    // synchronously on publish, but spin_once is harmless).
    let _ = executor.spin_once(Duration::from_millis(0));

    // Borrow the message in place via RecvView.
    let view = subscriber
        .try_borrow()
        .expect("borrow ok")
        .expect("got data");
    assert_eq!(view.as_ref().len(), core::mem::size_of::<Tick>());
    let recv: Tick = unsafe { core::ptr::read_unaligned(view.as_ref().as_ptr() as *const Tick) };
    assert_eq!(recv, msg);
    let _ = view;
}

#[test]
fn loan_too_large_returns_error() {
    let _g = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    px4_uorb::_reset_broker();
    nros_rmw_uorb::_reset();

    nros_rmw_uorb::register::<tick_topic>("/fmu/out/sensor_accel", 0).expect("register");

    let config = ExecutorConfig::new("").node_name("loan_borrow");
    let mut executor = Executor::open(&config).expect("open");
    let mut node = executor.create_node("loan_borrow").expect("create_node");
    let publisher = node
        .create_publisher_raw("/fmu/out/sensor_accel", "px4::Tick", "0")
        .expect("publisher");

    // Default arena slot is DEFAULT_LOAN_BUF (1024). Request larger.
    let err = publisher.try_loan(2048).map(|_| ()).expect_err("must fail");
    assert!(matches!(err, nros_node::LoanError::TooLarge));
}

#[test]
fn concurrent_loan_returns_would_block() {
    let _g = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    px4_uorb::_reset_broker();
    nros_rmw_uorb::_reset();

    nros_rmw_uorb::register::<tick_topic>("/fmu/out/sensor_accel", 0).expect("register");

    let config = ExecutorConfig::new("").node_name("loan_borrow");
    let mut executor = Executor::open(&config).expect("open");
    let mut node = executor.create_node("loan_borrow").expect("create_node");
    let publisher = node
        .create_publisher_raw("/fmu/out/sensor_accel", "px4::Tick", "0")
        .expect("publisher");

    let _loan1 = publisher.try_loan(16).expect("first loan");
    // Second loan w/o committing first → WouldBlock (single-slot arena).
    let err = publisher.try_loan(16).map(|_| ()).expect_err("must fail");
    assert!(matches!(err, nros_node::LoanError::WouldBlock));
    // _loan1 dropped at end of scope, slot freed.
}
