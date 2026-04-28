#![allow(non_camel_case_types)]
//! End-to-end typeless API via nros-node.
//!
//! Exercises the user-intended flow: same `Executor` / `Node` shape as
//! zenoh / xrce examples, but using `create_publisher_raw` /
//! `create_subscription_raw` because PX4 messages aren't ROS CDR.
//!
//! ```text
//!   nros::uorb::register::<sensor_combined>("/topic", 0);  // once at boot
//!   let mut node = executor.create_node("talker")?;
//!   let publisher = node.create_publisher_raw("/topic", "px4::SensorCombined", "0")?;
//!   publisher.publish_raw(unsafe { as_bytes(&msg) })?;
//! ```

#![cfg(feature = "std")]

use core::time::Duration;
use std::sync::Mutex;

use nros_node::{Executor, ExecutorConfig};
use px4_sys::orb_metadata;
use px4_uorb::{OrbMetadata, UorbTopic};

static TEST_LOCK: Mutex<()> = Mutex::new(());

#[repr(C)]
#[derive(Copy, Clone, Debug, PartialEq)]
struct Imu {
    seq: u32,
    accel: [f32; 3],
    gyro: [f32; 3],
}

struct imu_topic;
static IMU_NAME: [u8; 13] = *b"sensor_gyro\0\0";
static IMU_META: OrbMetadata = OrbMetadata::new(orb_metadata {
    o_name: IMU_NAME.as_ptr() as *const _,
    o_size: core::mem::size_of::<Imu>() as u16,
    o_size_no_padding: core::mem::size_of::<Imu>() as u16,
    message_hash: 0,
    o_id: u16::MAX,
    o_queue: 1,
});
impl UorbTopic for imu_topic {
    type Msg = Imu;
    fn metadata() -> &'static orb_metadata {
        IMU_META.get()
    }
}

fn as_bytes<T: Copy>(v: &T) -> &[u8] {
    // SAFETY: T is `#[repr(C)] Copy`; size matches.
    unsafe { core::slice::from_raw_parts(v as *const T as *const u8, core::mem::size_of::<T>()) }
}

#[test]
fn typeless_api_round_trip_via_executor() {
    let _g = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    px4_uorb::_reset_broker();
    nros_rmw_uorb::_reset();

    // 1. Register the typed shim once at boot. This is the only line
    //    a PX4 user adds beyond the zenoh-shape examples.
    nros_rmw_uorb::register::<imu_topic>("/fmu/out/sensor_gyro", 0);

    // 2. Standard nros boot: ExecutorConfig + Executor::open + create_node.
    //    Identical shape to zenoh / xrce examples.
    let config = ExecutorConfig::new("").node_name("typeless_test");
    let mut executor = Executor::open(&config).expect("open");
    let mut node = executor.create_node("typeless_test").expect("create_node");

    // 3. Typeless publisher + subscriber — no M: RosMessage bound. User
    //    supplies type_name / type_hash strings (uORB ignores them).
    let publisher = node
        .create_publisher_raw("/fmu/out/sensor_gyro", "px4::SensorGyro", "0")
        .expect("publisher");
    let mut subscriber = node
        .create_subscription_raw("/fmu/out/sensor_gyro", "px4::SensorGyro", "0")
        .expect("subscriber");

    // 4. Publish raw bytes; uORB memcpys into the broker.
    let msg = Imu {
        seq: 0xfeedface,
        accel: [1.0, 2.0, 3.0],
        gyro: [0.1, 0.2, 0.3],
    };
    publisher.publish_raw(as_bytes(&msg)).expect("publish");

    // 5. Spin once to let any internal poll execute (uORB delivers
    //    synchronously via the std mock; this is here to mirror real
    //    backends that need it).
    let _ = executor.spin_once(Duration::from_millis(0));

    // 6. Receive raw bytes and reinterpret.
    let len = subscriber.try_recv_raw().expect("recv").expect("got data");
    assert_eq!(len, core::mem::size_of::<Imu>());
    let recv: Imu =
        unsafe { core::ptr::read_unaligned(subscriber.buffer().as_ptr() as *const Imu) };
    assert_eq!(recv, msg);
}
