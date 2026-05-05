//! Phase 108.C.x.1 — cross-backend status-event matrix (dust-DDS slice).
//!
//! Verifies the `Subscriber::supports_event` / `register_event_callback`
//! and `Publisher::supports_event` / `register_event_callback` API
//! surface for the dust-DDS backend. Pairs with
//! `nros-rmw-zenoh/tests/status_events_matrix.rs`,
//! `nros-rmw-xrce/tests/status_events_matrix.rs`, and
//! `nros-rmw-uorb/tests/typeless_api.rs::message_lost_event_fires_*`.
//!
//! Per-policy dynamic-fire tests live under each backend's wiring path
//! (e.g. dust-dds's own `DataReaderListener` E2E suite). The matrix
//! here is the smoke layer: confirms each backend reports the right
//! mask + accepts a registration without panicking.

#![cfg(feature = "platform-posix")]

use core::ffi::c_void;
use nros_rmw::{
    EventCallback, EventKind, Publisher, QosSettings, Rmw, RmwConfig, Session, Subscriber,
    TopicInfo, TransportError,
};
use nros_rmw_dds::DdsRmw;

/// Domain id chosen high to avoid colliding with other DDS daemons.
const DOMAIN_ID: u32 = 199;

unsafe extern "C" fn dummy_cb(_kind: EventKind, _payload: *const c_void, _ctx: *mut c_void) {}

fn open_session() -> nros_rmw_dds::DdsSession {
    let config = RmwConfig {
        domain_id: DOMAIN_ID,
        node_name: "events_matrix",
        ..RmwConfig::default()
    };
    DdsRmw.open(&config).expect("open DdsRmw")
}

fn topic() -> TopicInfo<'static> {
    TopicInfo::new(
        "rt/dds_event_matrix",
        "std_msgs::msg::dds_::String_",
        "RIHS01_unused",
    )
    .with_domain(DOMAIN_ID)
}

#[test]
fn dds_subscriber_event_mask() {
    let mut sess = open_session();
    let mut sub = sess
        .create_subscriber(&topic(), QosSettings::QOS_PROFILE_DEFAULT)
        .expect("create_subscriber");

    // dust-DDS supports the full Tier-1 sub-side set.
    assert!(sub.supports_event(EventKind::LivelinessChanged));
    assert!(sub.supports_event(EventKind::RequestedDeadlineMissed));
    assert!(sub.supports_event(EventKind::MessageLost));

    // Pub-side kinds always false on a subscriber.
    assert!(!sub.supports_event(EventKind::LivelinessLost));
    assert!(!sub.supports_event(EventKind::OfferedDeadlineMissed));

    // Registering a supported kind succeeds.
    let cb: EventCallback = dummy_cb;
    let res = unsafe {
        sub.register_event_callback(EventKind::LivelinessChanged, 0, cb, core::ptr::null_mut())
    };
    assert!(res.is_ok(), "register LivelinessChanged: {res:?}");

    // Registering a pub-side kind on the sub returns Unsupported.
    let res = unsafe {
        sub.register_event_callback(EventKind::LivelinessLost, 0, cb, core::ptr::null_mut())
    };
    assert!(matches!(res, Err(TransportError::Unsupported)));
}

#[test]
fn dds_publisher_event_mask() {
    let mut sess = open_session();
    let mut pubr = sess
        .create_publisher(&topic(), QosSettings::QOS_PROFILE_DEFAULT)
        .expect("create_publisher");

    // dust-DDS supports the full Tier-1 pub-side set.
    assert!(pubr.supports_event(EventKind::LivelinessLost));
    assert!(pubr.supports_event(EventKind::OfferedDeadlineMissed));

    // Sub-side kinds always false on a publisher.
    assert!(!pubr.supports_event(EventKind::LivelinessChanged));
    assert!(!pubr.supports_event(EventKind::RequestedDeadlineMissed));
    assert!(!pubr.supports_event(EventKind::MessageLost));

    let cb: EventCallback = dummy_cb;
    let res = unsafe {
        pubr.register_event_callback(EventKind::LivelinessLost, 0, cb, core::ptr::null_mut())
    };
    assert!(res.is_ok(), "register LivelinessLost: {res:?}");

    let res = unsafe {
        pubr.register_event_callback(EventKind::LivelinessChanged, 0, cb, core::ptr::null_mut())
    };
    assert!(matches!(res, Err(TransportError::Unsupported)));
}

#[test]
fn dds_supported_qos_mask_contains_full_dds_surface() {
    use nros_rmw::QosPolicyMask;
    let sess = open_session();
    let mask = sess.supported_qos_policies();
    // dust-DDS native — full DDS QoS surface.
    assert!(mask.contains(QosPolicyMask::CORE));
    assert!(mask.contains(QosPolicyMask::DEADLINE));
    assert!(mask.contains(QosPolicyMask::LIFESPAN));
    assert!(mask.contains(QosPolicyMask::LIVELINESS_AUTOMATIC));
    assert!(mask.contains(QosPolicyMask::LIVELINESS_MANUAL_BY_TOPIC));
}
