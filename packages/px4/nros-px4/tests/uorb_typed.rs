#![allow(non_camel_case_types)]
//! Phase 99.M acceptance tests for [`nros_px4::uorb`] — typed
//! convenience layer over the byte-shaped Phase 99 surface.
//!
//! Round-trips a hand-rolled `UorbTopic` through the std-mock broker
//! end-to-end via `Publisher<T>::publish` / `Subscriber<T>::try_recv`,
//! `try_borrow`, and the typed-loan path.

#![cfg(feature = "test-helpers")]

use core::time::Duration;
use std::sync::Mutex;

use nros_node::{Executor, ExecutorConfig};
use nros_px4::uorb;
use px4_sys::orb_metadata;
use px4_uorb::{OrbMetadata, UorbTopic};

static TEST_LOCK: Mutex<()> = Mutex::new(());

#[repr(C)]
#[derive(Copy, Clone, Debug, PartialEq)]
struct Tick {
    seq: u32,
    timestamp: u64,
    payload: [u8; 8],
}

struct tick_topic;
static TICK_NAME: [u8; 12] = *b"sensor_tick\0";
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
fn typed_publish_recv_round_trip() {
    let _g = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    px4_uorb::_reset_broker();

    let cfg = ExecutorConfig::new("").node_name("typed_uorb");
    let mut executor = Executor::open(&cfg).expect("open");
    let mut node = executor.create_node("typed_uorb").expect("create_node");

    let publisher = uorb::create_publisher::<tick_topic>(&mut node, "/fmu/out/sensor_tick", 0)
        .expect("create_publisher");

    let mut subscriber =
        uorb::create_subscription::<tick_topic>(&mut node, "/fmu/out/sensor_tick", 0)
            .expect("create_subscription");

    let msg = Tick {
        seq: 0xfeed_face,
        timestamp: 0xdead_beef,
        payload: *b"hi-typed",
    };
    publisher.publish(&msg).expect("publish");

    let _ = executor.spin_once(Duration::from_millis(0));

    let recv = subscriber.try_recv().expect("recv ok").expect("got data");
    assert_eq!(recv, msg);
}

#[test]
fn typed_loan_writes_in_place() {
    let _g = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    px4_uorb::_reset_broker();

    let cfg = ExecutorConfig::new("").node_name("typed_loan");
    let mut executor = Executor::open(&cfg).expect("open");
    let mut node = executor.create_node("typed_loan").expect("create_node");

    let publisher = uorb::create_publisher::<tick_topic>(&mut node, "/fmu/out/sensor_tick", 0)
        .expect("create_publisher");

    let mut subscriber =
        uorb::create_subscription::<tick_topic>(&mut node, "/fmu/out/sensor_tick", 0)
            .expect("create_subscription");

    // Typed loan path: write fields directly into the loan slot via
    // MaybeUninit, no user-side T construction.
    let mut loan = publisher.try_loan().expect("try_loan");
    loan.as_uninit().write(Tick {
        seq: 0x1234_5678,
        timestamp: 0xcafe_babe,
        payload: *b"typedlon",
    });
    loan.commit().expect("commit");

    let _ = executor.spin_once(Duration::from_millis(0));

    let recv = subscriber.try_recv().expect("recv ok").expect("got data");
    assert_eq!(recv.seq, 0x1234_5678);
    assert_eq!(recv.timestamp, 0xcafe_babe);
    assert_eq!(&recv.payload, b"typedlon");
}

#[test]
fn typed_callback_fires_with_typed_msg() {
    use std::sync::{
        Arc,
        atomic::{AtomicU32, Ordering},
    };

    let _g = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    px4_uorb::_reset_broker();

    let cfg = ExecutorConfig::new("").node_name("typed_callback");
    let mut executor = Executor::open(&cfg).expect("open");

    let observed = Arc::new(AtomicU32::new(0));
    let observed_seq = Arc::new(AtomicU32::new(0));
    let observed_clone = Arc::clone(&observed);
    let observed_seq_clone = Arc::clone(&observed_seq);

    // Publisher first (still uses Node).
    let publisher = {
        let mut node = executor.create_node("typed_callback").expect("create_node");
        uorb::create_publisher::<tick_topic>(&mut node, "/fmu/out/sensor_tick", 0)
            .expect("create_publisher")
    };

    // Then register the callback against the executor — node ref is
    // dropped above so the executor reborrow is safe under NLL.
    uorb::create_subscription_with_callback::<tick_topic, _>(
        &mut executor,
        "/fmu/out/sensor_tick",
        0,
        move |msg: &Tick| {
            observed_clone.fetch_add(1, Ordering::Relaxed);
            observed_seq_clone.store(msg.seq, Ordering::Relaxed);
        },
    )
    .expect("create_subscription_with_callback");

    // Publish twice; spin once between to drive the callback.
    publisher
        .publish(&Tick {
            seq: 1,
            timestamp: 1,
            payload: *b"cb-1____",
        })
        .expect("publish 1");

    let _ = executor.spin_once(Duration::from_millis(0));

    publisher
        .publish(&Tick {
            seq: 7,
            timestamp: 2,
            payload: *b"cb-7____",
        })
        .expect("publish 2");

    let _ = executor.spin_once(Duration::from_millis(0));

    // Callback should have fired at least once with seq=7 (most
    // recent message; uORB std mock keeps the latest only with
    // queue depth 1).
    assert!(
        observed.load(Ordering::Relaxed) >= 1,
        "callback never fired"
    );
    assert_eq!(observed_seq.load(Ordering::Relaxed), 7);
}

#[test]
fn typed_borrow_in_place_returns_typed_view() {
    let _g = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    px4_uorb::_reset_broker();

    let cfg = ExecutorConfig::new("").node_name("typed_borrow");
    let mut executor = Executor::open(&cfg).expect("open");
    let mut node = executor.create_node("typed_borrow").expect("create_node");

    let publisher = uorb::create_publisher::<tick_topic>(&mut node, "/fmu/out/sensor_tick", 0)
        .expect("create_publisher");

    let mut subscriber =
        uorb::create_subscription::<tick_topic>(&mut node, "/fmu/out/sensor_tick", 0)
            .expect("create_subscription");

    let msg = Tick {
        seq: 42,
        timestamp: 99,
        payload: *b"borrowed",
    };
    publisher.publish(&msg).expect("publish");

    let _ = executor.spin_once(Duration::from_millis(0));

    let view = subscriber
        .try_borrow()
        .expect("borrow ok")
        .expect("got data");
    // Deref<Target = T::Msg>
    assert_eq!(view.seq, 42);
    assert_eq!(view.timestamp, 99);
    assert_eq!(&view.payload, b"borrowed");
}
