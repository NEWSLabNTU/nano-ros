#![allow(non_camel_case_types)]
//! End-to-end byte-shaped publish/subscribe via nros-node + the
//! Phase 99.L uORB Session redesign.
//!
//! Mirrors the user-intended flow that `nros-px4::uorb` will wrap:
//!
//! ```text
//! let mut node = executor.create_node("talker")?;
//! let pub_handle = node
//!     .session_mut()
//!     .create_publisher_uorb(metadata, instance)?;
//! let publisher: nros::EmbeddedRawPublisher = nros::EmbeddedRawPublisher::new(pub_handle);
//! publisher.publish_raw(&bytes)?;
//! ```
//!
//! No registry, no `register::<T>`, no `topics.toml`. The metadata
//! pointer flows from the user (here a hand-rolled `OrbMetadata`; in
//! `nros-px4::uorb` it comes from `T::metadata()`).

#![cfg(feature = "std")]

use core::time::Duration;
use std::sync::Mutex;

use nros_node::{EmbeddedRawPublisher, Executor, ExecutorConfig, RawSubscription};
use px4_sys::orb_metadata;
use px4_uorb::OrbMetadata;

static TEST_LOCK: Mutex<()> = Mutex::new(());

#[repr(C)]
#[derive(Copy, Clone, Debug, PartialEq)]
struct Imu {
    seq: u32,
    accel: [f32; 3],
    gyro: [f32; 3],
}

static IMU_NAME: [u8; 12] = *b"sensor_gyro\0";
static IMU_META: OrbMetadata = OrbMetadata::new(orb_metadata {
    o_name: IMU_NAME.as_ptr() as *const _,
    o_size: core::mem::size_of::<Imu>() as u16,
    o_size_no_padding: core::mem::size_of::<Imu>() as u16,
    message_hash: 0,
    o_id: u16::MAX,
    o_queue: 1,
});

unsafe fn as_bytes<T>(t: &T) -> &[u8] {
    unsafe { core::slice::from_raw_parts(t as *const T as *const u8, core::mem::size_of::<T>()) }
}

#[test]
fn byte_shaped_round_trip_via_executor() {
    let _g = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    px4_uorb::_reset_broker();

    let config = ExecutorConfig::new("").node_name("typeless_test");
    let mut executor = Executor::open(&config).expect("open");
    let mut node = executor.create_node("typeless_test").expect("create_node");

    let pub_handle = node
        .session_mut()
        .create_publisher_uorb(IMU_META.get(), 0)
        .expect("publisher_uorb");
    let publisher: EmbeddedRawPublisher = EmbeddedRawPublisher::new(pub_handle);

    let sub_handle = node
        .session_mut()
        .create_subscription_uorb(IMU_META.get(), 0)
        .expect("subscription_uorb");
    let mut subscriber: RawSubscription = RawSubscription::new(sub_handle);

    let msg = Imu {
        seq: 0xfeedface,
        accel: [1.0, 2.0, 3.0],
        gyro: [0.1, 0.2, 0.3],
    };
    publisher
        .publish_raw(unsafe { as_bytes(&msg) })
        .expect("publish");

    let _ = executor.spin_once(Duration::from_millis(0));

    let len = subscriber.try_recv_raw().expect("recv").expect("got data");
    assert_eq!(len, core::mem::size_of::<Imu>());
    let recv: Imu =
        unsafe { core::ptr::read_unaligned(subscriber.buffer().as_ptr() as *const Imu) };
    assert_eq!(recv, msg);
}

/// Phase 108.C.uorb.2 — verify MessageLost callback fires when the
/// publisher writes faster than the subscriber polls. Uses the std
/// host mock broker which tracks per-topic seq exactly.
#[test]
fn message_lost_event_fires_on_dropped_messages() {
    use std::sync::atomic::{AtomicU32, Ordering};

    let _g = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    px4_uorb::_reset_broker();

    let config = ExecutorConfig::new("").node_name("lost_test");
    let mut executor = Executor::open(&config).expect("open");
    let mut node = executor.create_node("lost_test").expect("create_node");

    let pub_handle = node
        .session_mut()
        .create_publisher_uorb(IMU_META.get(), 0)
        .expect("publisher_uorb");
    let publisher: EmbeddedRawPublisher = EmbeddedRawPublisher::new(pub_handle);

    let sub_handle = node
        .session_mut()
        .create_subscription_uorb(IMU_META.get(), 0)
        .expect("subscription_uorb");
    let mut subscriber: RawSubscription = RawSubscription::new(sub_handle);

    // Register MessageLost callback via nros-node's typed wrapper —
    // exercises the closure-trampoline path (Phase 108.A.7) on top
    // of the raw fn-pointer ABI.
    static LOST_TOTAL: AtomicU32 = AtomicU32::new(0);
    static LOST_DELTA: AtomicU32 = AtomicU32::new(0);
    LOST_TOTAL.store(0, Ordering::Relaxed);
    LOST_DELTA.store(0, Ordering::Relaxed);
    subscriber
        .on_message_lost(|status: nros_rmw::CountStatus| {
            LOST_TOTAL.store(status.total_count, Ordering::Relaxed);
            LOST_DELTA.store(status.total_count_change, Ordering::Relaxed);
        })
        .expect("register MessageLost");

    // Align last_seen with the topic's current seq via one publish +
    // recv cycle. The mock broker bumps seq on both advertise (the
    // implicit initial sample written by the first publish) and the
    // publish itself, so we drain that here and start the lost-count
    // measurement from a known baseline.
    let prime = Imu {
        seq: u32::MAX,
        accel: [-1.0, -1.0, -1.0],
        gyro: [-1.0, -1.0, -1.0],
    };
    publisher
        .publish_raw(unsafe { as_bytes(&prime) })
        .expect("prime publish");
    let _ = executor.spin_once(Duration::from_millis(0));
    let _ = subscriber.try_recv_raw().expect("prime recv");
    LOST_TOTAL.store(0, Ordering::Relaxed);
    LOST_DELTA.store(0, Ordering::Relaxed);

    // Publish 5 samples without polling between → 4 dropped, 1
    // delivered on next poll.
    for i in 0..5u32 {
        let msg = Imu {
            seq: i,
            accel: [i as f32, 0.0, 0.0],
            gyro: [0.0, 0.0, 0.0],
        };
        publisher
            .publish_raw(unsafe { as_bytes(&msg) })
            .expect("publish");
    }

    let _ = executor.spin_once(Duration::from_millis(0));
    let len = subscriber.try_recv_raw().expect("recv").expect("got data");
    assert_eq!(len, core::mem::size_of::<Imu>());

    // 5 publishes, 1 delivered ⇒ 4 lost in this burst.
    // Cumulative total counts the prime cycle's 1-sample gap (mock
    // bumps seq on advertise + first publish; only the last is
    // delivered, so 1 was missed). Total = 1 (prime) + 4 = 5.
    assert_eq!(LOST_DELTA.load(Ordering::Relaxed), 4);
    assert_eq!(LOST_TOTAL.load(Ordering::Relaxed), 5);

    // Another burst of 3, all but the last lost.
    for i in 100..103u32 {
        let msg = Imu {
            seq: i,
            accel: [0.0, 0.0, 0.0],
            gyro: [0.0, 0.0, 0.0],
        };
        publisher
            .publish_raw(unsafe { as_bytes(&msg) })
            .expect("publish");
    }
    let _ = executor.spin_once(Duration::from_millis(0));
    let _ = subscriber.try_recv_raw().expect("recv");

    // 3 more publishes, 1 delivered ⇒ 2 lost. Total = 5 + 2 = 7.
    assert_eq!(LOST_DELTA.load(Ordering::Relaxed), 2);
    assert_eq!(LOST_TOTAL.load(Ordering::Relaxed), 7);
}
