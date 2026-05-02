#![allow(non_camel_case_types)]
//! End-to-end loan/borrow API via the px4-uorb std mock broker, exercising
//! the Phase 99.L byte-shaped path:
//!
//! - `Executor::open` → `create_node` → `node.session_mut()` →
//!   `UorbSession::create_publisher_uorb(metadata, instance)`
//!   → `EmbeddedRawPublisher::new(handle)`.
//! - `try_loan` returns a writable `PublishLoan`; user fills bytes in
//!   place; `commit` calls `orb_publish` directly via the
//!   `px4_uorb::RawPublication` wrapper.
//! - `try_borrow` returns a `RecvView` lent from the subscriber's
//!   internal buffer (post-99.E uORB stays on the arena fallback —
//!   no native lending).
//!
//! No registry, no `register::<T>`, no `topics.toml`, no
//! `critical_section`. Each publisher/subscriber owns its own
//! `&'static orb_metadata` pointer and FFI handle.

#![cfg(feature = "std")]

use core::time::Duration;
use std::sync::Mutex;

use nros_node::{EmbeddedRawPublisher, Executor, ExecutorConfig, RawSubscription};
use px4_sys::orb_metadata;
use px4_uorb::OrbMetadata;

static TEST_LOCK: Mutex<()> = Mutex::new(());

#[repr(C)]
#[derive(Copy, Clone, Debug, PartialEq)]
struct Tick {
    seq: u32,
    payload: [u8; 8],
}

static TICK_NAME: [u8; 11] = *b"sensor_acc\0";
static TICK_META: OrbMetadata = OrbMetadata::new(orb_metadata {
    o_name: TICK_NAME.as_ptr() as *const _,
    o_size: core::mem::size_of::<Tick>() as u16,
    o_size_no_padding: core::mem::size_of::<Tick>() as u16,
    message_hash: 0,
    o_id: u16::MAX,
    o_queue: 1,
});

#[test]
fn loan_borrow_round_trip_via_executor() {
    let _g = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    px4_uorb::_reset_broker();

    let config = ExecutorConfig::new("").node_name("loan_borrow");
    let mut executor = Executor::open(&config).expect("open");
    let mut node = executor.create_node("loan_borrow").expect("create_node");

    let publisher_handle = node
        .session_mut()
        .create_publisher_uorb(TICK_META.get(), 0)
        .expect("publisher_uorb");
    let publisher: EmbeddedRawPublisher = EmbeddedRawPublisher::new(publisher_handle);

    let subscriber_handle = node
        .session_mut()
        .create_subscription_uorb(TICK_META.get(), 0)
        .expect("subscription_uorb");
    let mut subscriber: RawSubscription = RawSubscription::new(subscriber_handle);

    // Loan + fill in place + commit.
    let msg = Tick {
        seq: 0xabcdef01,
        payload: *b"loan-bor",
    };
    let mut loan = publisher
        .try_loan(core::mem::size_of::<Tick>())
        .expect("loan");
    let bytes: &[u8] = unsafe {
        core::slice::from_raw_parts(
            &msg as *const Tick as *const u8,
            core::mem::size_of::<Tick>(),
        )
    };
    loan.as_mut().copy_from_slice(bytes);
    loan.commit().expect("commit");

    // Spin once so any internal poll executes (uORB std mock fires
    // callbacks synchronously on publish, but spin_once is harmless).
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

    let config = ExecutorConfig::new("").node_name("loan_borrow");
    let mut executor = Executor::open(&config).expect("open");
    let mut node = executor.create_node("loan_borrow").expect("create_node");
    let publisher_handle = node
        .session_mut()
        .create_publisher_uorb(TICK_META.get(), 0)
        .expect("publisher_uorb");
    let publisher: EmbeddedRawPublisher = EmbeddedRawPublisher::new(publisher_handle);

    // Default arena slot is DEFAULT_LOAN_BUF (1024). Request larger.
    let err = publisher.try_loan(2048).map(|_| ()).expect_err("must fail");
    assert!(matches!(err, nros_node::LoanError::TooLarge));
}

#[test]
fn concurrent_loan_returns_would_block() {
    let _g = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    px4_uorb::_reset_broker();

    let config = ExecutorConfig::new("").node_name("loan_borrow");
    let mut executor = Executor::open(&config).expect("open");
    let mut node = executor.create_node("loan_borrow").expect("create_node");
    let publisher_handle = node
        .session_mut()
        .create_publisher_uorb(TICK_META.get(), 0)
        .expect("publisher_uorb");
    let publisher: EmbeddedRawPublisher = EmbeddedRawPublisher::new(publisher_handle);

    let _loan1 = publisher.try_loan(16).expect("first loan");
    // Second loan w/o committing first → WouldBlock (single-slot arena).
    let err = publisher.try_loan(16).map(|_| ()).expect_err("must fail");
    assert!(matches!(err, nros_node::LoanError::WouldBlock));
    // _loan1 dropped at end of scope, slot freed.
}

/// Phase 99.H': dropping a `LoanFuture` before it resolves must NOT
/// leak the arena slot reservation.
#[test]
fn loan_future_drop_does_not_leak_slot() {
    use core::{
        future::Future,
        pin::pin,
        task::{Context, Poll, Waker},
    };

    let _g = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    px4_uorb::_reset_broker();

    let config = ExecutorConfig::new("").node_name("loan_borrow");
    let mut executor = Executor::open(&config).expect("open");
    let mut node = executor.create_node("loan_borrow").expect("create_node");
    let publisher_handle = node
        .session_mut()
        .create_publisher_uorb(TICK_META.get(), 0)
        .expect("publisher_uorb");
    let publisher: EmbeddedRawPublisher = EmbeddedRawPublisher::new(publisher_handle);

    // Take the slot via try_loan, then build a LoanFuture that will
    // see WouldBlock on its first poll.
    let outstanding = publisher.try_loan(16).expect("first loan");

    let waker = Waker::noop().clone();
    let mut cx = Context::from_waker(&waker);
    {
        let fut = publisher.loan(16);
        let mut fut = pin!(fut);
        match fut.as_mut().poll(&mut cx) {
            Poll::Pending => {}
            Poll::Ready(_) => panic!("expected Pending while slot is busy"),
        }
        // Drop the future without ever resolving it.
    }

    // Outstanding loan still holds the slot.
    assert!(matches!(
        publisher.try_loan(16).map(|_| ()),
        Err(nros_node::LoanError::WouldBlock)
    ));

    // Release the outstanding loan; arena should now serve a fresh
    // try_loan (proving the cancelled future didn't corrupt state).
    drop(outstanding);
    let _fresh = publisher
        .try_loan(16)
        .expect("slot reusable after future drop");
}
