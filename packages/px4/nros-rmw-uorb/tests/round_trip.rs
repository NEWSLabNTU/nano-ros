#![allow(non_camel_case_types)]
//! End-to-end round-trip via the px4-uorb std mock broker.
//!
//! Verifies the typed-trampoline registry: a `register::<T>` call wires a
//! ROS 2 topic name to a typed `Publication`/`Subscription` pair, and a
//! subsequent `Session::create_publisher` / `create_subscriber` returns
//! handles whose `publish_raw` / `try_recv_raw` round-trip raw message
//! bytes through the broker.
//!
//! This exercises the same code path that real PX4 modules will hit on
//! NuttX target builds (minus the actual uORB kernel — the std mock
//! substitutes an in-process broker keyed by topic name).

#![cfg(feature = "std")]

use nros_rmw::{Publisher, QosSettings, Rmw, RmwConfig, Session, Subscriber, TopicInfo};
use nros_rmw_uorb::{UorbRmw, register};
use std::sync::Mutex;

// Tests share the global registry + broker. Serialise them so a `_reset()`
// in one test doesn't wipe state mid-execution of another.
static TEST_LOCK: Mutex<()> = Mutex::new(());

/// Hand-rolled uORB topic stand-in. Real users get this from
/// `#[px4_message(...)]`. We synthesise it here to keep the test
/// dependency-free.
mod fake_topic {
    use px4_sys::orb_metadata;
    use px4_uorb::{OrbMetadata, UorbTopic};

    #[repr(C)]
    #[derive(Copy, Clone, Debug, PartialEq)]
    pub struct TestPing {
        pub seq: u32,
        pub payload: [u8; 8],
    }

    pub struct test_ping;

    static __NAME: [u8; 10] = *b"test_ping\0";
    static __META: OrbMetadata = OrbMetadata::new(orb_metadata {
        o_name: __NAME.as_ptr() as *const _,
        o_size: core::mem::size_of::<TestPing>() as u16,
        o_size_no_padding: core::mem::size_of::<TestPing>() as u16,
        message_hash: 0,
        o_id: u16::MAX,
        o_queue: 1,
    });

    impl UorbTopic for test_ping {
        type Msg = TestPing;
        fn metadata() -> &'static orb_metadata {
            __META.get()
        }
    }
}

use fake_topic::{TestPing, test_ping};

#[test]
fn register_then_publish_subscribe_round_trips() {
    let _g = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    // Reset broker + registry between runs (tests share process state).
    px4_uorb::_reset_broker();
    nros_rmw_uorb::_reset();

    register::<test_ping>("/test/ping", 0);

    let mut session = UorbRmw
        .open(&RmwConfig {
            locator: "",
            mode: nros_rmw::SessionMode::Peer,
            domain_id: 0,
            node_name: "round_trip_test",
            namespace: "",
            properties: &[],
        })
        .expect("open session");

    // The TOPIC_MAP doesn't include /test/ping, so we must register it first
    // — but topics.toml maps ROS 2 → uORB names. For this test we hand-craft
    // a TopicInfo whose `name` is in the map. Use `/fmu/out/sensor_gyro` as
    // a stand-in: register the same key with our fake topic for the test.
    register::<test_ping>("/fmu/out/sensor_gyro", 0);

    let topic = TopicInfo::new("/fmu/out/sensor_gyro", "TestPing", "0");

    let publisher = session
        .create_publisher(&topic, QosSettings::default())
        .expect("create publisher");
    let mut subscriber = session
        .create_subscriber(&topic, QosSettings::default())
        .expect("create subscriber");

    let msg = TestPing {
        seq: 0xdeadbeef,
        payload: *b"hello-px",
    };
    let bytes: &[u8] = unsafe {
        core::slice::from_raw_parts(
            &msg as *const TestPing as *const u8,
            core::mem::size_of::<TestPing>(),
        )
    };

    publisher.publish_raw(bytes).expect("publish");

    let mut buf = [0u8; core::mem::size_of::<TestPing>()];
    let len = subscriber
        .try_recv_raw(&mut buf)
        .expect("recv ok")
        .expect("got data");
    assert_eq!(len, core::mem::size_of::<TestPing>());

    let recv: TestPing = unsafe { core::ptr::read_unaligned(buf.as_ptr() as *const TestPing) };
    assert_eq!(recv, msg);
}

#[test]
fn unregistered_topic_returns_backend_error() {
    let _g = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    px4_uorb::_reset_broker();
    nros_rmw_uorb::_reset();

    let mut session = UorbRmw
        .open(&RmwConfig {
            locator: "",
            mode: nros_rmw::SessionMode::Peer,
            domain_id: 0,
            node_name: "round_trip_test",
            namespace: "",
            properties: &[],
        })
        .expect("open session");

    let topic = TopicInfo::new("/fmu/out/sensor_gyro", "TestPing", "0");
    let publisher = session
        .create_publisher(&topic, QosSettings::default())
        .expect("create publisher");

    let bytes = [0u8; core::mem::size_of::<TestPing>()];
    let err = publisher.publish_raw(&bytes).expect_err("must fail");
    assert!(matches!(
        err,
        nros_rmw::TransportError::Backend(s) if s.contains("not registered")
    ));
}

#[test]
fn topic_not_in_topics_toml_rejected_at_create() {
    let _g = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    px4_uorb::_reset_broker();
    nros_rmw_uorb::_reset();

    let mut session = UorbRmw
        .open(&RmwConfig {
            locator: "",
            mode: nros_rmw::SessionMode::Peer,
            domain_id: 0,
            node_name: "round_trip_test",
            namespace: "",
            properties: &[],
        })
        .expect("open session");

    // /unknown/topic is not in topics.toml.
    let topic = TopicInfo::new("/unknown/topic", "X", "0");
    let err = session
        .create_publisher(&topic, QosSettings::default())
        .expect_err("must fail");
    assert!(matches!(err, nros_rmw::TransportError::InvalidConfig));
}
