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
    publisher.publish_raw(unsafe { as_bytes(&msg) }).expect("publish");

    let _ = executor.spin_once(Duration::from_millis(0));

    let len = subscriber.try_recv_raw().expect("recv").expect("got data");
    assert_eq!(len, core::mem::size_of::<Imu>());
    let recv: Imu =
        unsafe { core::ptr::read_unaligned(subscriber.buffer().as_ptr() as *const Imu) };
    assert_eq!(recv, msg);
}
