//! End-to-end typed pub/sub via the px4-uorb std mock broker.
//!
//! Mirror of `round_trip.rs` but using the **direct typed API**
//! (`nros_rmw_uorb::publication::<T>` / `subscription::<T>`) instead of
//! the type-erased `nros-rmw::Session` path. This is the recommended
//! user-facing entry point.

#![cfg(feature = "std")]

use nros_rmw_uorb::{publication, subscription};
use px4_sys::orb_metadata;
use px4_uorb::{OrbMetadata, UorbTopic};

#[repr(C)]
#[derive(Copy, Clone, Debug, PartialEq)]
struct Tick {
    seq: u32,
    payload: [u8; 8],
}

struct tick_topic;

static TICK_NAME: [u8; 13] = *b"sensor_gyro\0\0";
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
fn typed_publish_subscribe_round_trips() {
    px4_uorb::_reset_broker();

    // /fmu/out/sensor_gyro is in topics.toml → uorb name "sensor_gyro",
    // which matches TICK_NAME above.
    let pub_ = publication::<tick_topic>("/fmu/out/sensor_gyro", 0).expect("publication");
    let sub = subscription::<tick_topic>("/fmu/out/sensor_gyro", 0).expect("subscription");

    let msg = Tick {
        seq: 0xcafebabe,
        payload: *b"px4-rmw\0",
    };
    pub_.publish(&msg).expect("publish");

    let recv = sub.try_recv().expect("got message");
    assert_eq!(recv, msg);
}

#[test]
fn topic_not_in_topics_toml_rejected() {
    let err = publication::<tick_topic>("/unknown/topic", 0).map(|_| ()).expect_err("must fail");
    assert!(matches!(err, nros_rmw::TransportError::InvalidConfig));
}

#[test]
fn topic_meta_name_mismatch_rejected() {
    // /fmu/out/sensor_accel is in topics.toml → uorb name "sensor_accel",
    // but tick_topic's metadata says "sensor_gyro". Must reject.
    let err = publication::<tick_topic>("/fmu/out/sensor_accel", 0).map(|_| ()).expect_err("must fail");
    assert!(matches!(
        err,
        nros_rmw::TransportError::Backend(s) if s.contains("does not match")
    ));
}
