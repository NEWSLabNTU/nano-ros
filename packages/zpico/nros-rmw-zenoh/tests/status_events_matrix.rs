//! Phase 108.C.x.1 — cross-backend status-event matrix (zenoh-pico slice).
//!
//! Verifies the zenoh-pico shim's `Subscriber::supports_event` /
//! `register_event_callback` and `Publisher::supports_event` /
//! `register_event_callback` API surface. Pairs with the dust-DDS,
//! XRCE-DDS, and uORB matrix tests.
//!
//! In-process — opens a zenoh peer-mode session (no router needed),
//! creates a subscriber + publisher, asserts the supported-event
//! mask, and confirms `register_event_callback` returns `Ok` for
//! supported kinds and `Err(Unsupported)` for the rest.
//!
//! ## Status
//!
//! All three tests carry `#[ignore]` and are unblocked once
//! `nros-platform-posix::net::udp_send` (line ~455) is fixed: that
//! function reads an `Endpoint` through a misaligned `*const c_void`,
//! which trips the unaligned-pointer-dereference panic on stable
//! rustc when zenoh-pico's `_z_open` calls into the posix shim.
//! Running these tests today via `cargo test --features platform-posix
//! --test status_events_matrix` produces SIGSEGV / SIGABRT inside
//! `udp_send` before any matrix assertion runs. The pre-existing
//! `tests/zenoh_integration.rs::test_session_open_close_peer` hits
//! the same panic — this is a platform-layer issue, not a regression
//! from Phase 108. Run via `cargo test -- --ignored` after the
//! posix-net fix lands.

#![cfg(feature = "platform-posix")]

use core::ffi::c_void;
use nros_rmw::{
    EventCallback, EventKind, Publisher, QosSettings, Session, SessionMode, Subscriber, TopicInfo,
    Transport, TransportConfig, TransportError,
};
use nros_rmw_zenoh::ZenohTransport;

unsafe extern "C" fn dummy_cb(_kind: EventKind, _payload: *const c_void, _ctx: *mut c_void) {}

fn open_session() -> nros_rmw_zenoh::ZenohSession {
    let config = TransportConfig {
        locator: None,
        mode: SessionMode::Peer,
        properties: &[("multicast_scouting", "false")],
    };
    ZenohTransport::open(&config).expect("open ZenohTransport peer mode")
}

fn topic() -> TopicInfo<'static> {
    TopicInfo::new(
        "zenoh_event_matrix",
        "std_msgs::msg::dds_::String_",
        "RIHS01_unused",
    )
}

#[test]
#[ignore = "blocked on nros-platform-posix net.rs:455 alignment bug"]
fn zenoh_subscriber_event_mask() {
    let mut sess = open_session();
    let mut sub = sess
        .create_subscriber(&topic(), QosSettings::QOS_PROFILE_DEFAULT)
        .expect("create_subscriber");

    // zenoh-pico shim supports the full Tier-1 sub-side set
    // (MessageLost via attachment seq gap, RequestedDeadlineMissed
    // via clock check, LivelinessChanged via wildcard liveliness
    // poll).
    assert!(sub.supports_event(EventKind::LivelinessChanged));
    assert!(sub.supports_event(EventKind::RequestedDeadlineMissed));
    assert!(sub.supports_event(EventKind::MessageLost));

    // Pub-side kinds always false on a subscriber.
    assert!(!sub.supports_event(EventKind::LivelinessLost));
    assert!(!sub.supports_event(EventKind::OfferedDeadlineMissed));

    let cb: EventCallback = dummy_cb;
    let res = unsafe {
        sub.register_event_callback(EventKind::MessageLost, 0, cb, core::ptr::null_mut())
    };
    assert!(res.is_ok(), "register MessageLost: {res:?}");

    let res = unsafe {
        sub.register_event_callback(EventKind::OfferedDeadlineMissed, 0, cb, core::ptr::null_mut())
    };
    assert!(matches!(res, Err(TransportError::Unsupported)));
}

#[test]
#[ignore = "blocked on nros-platform-posix net.rs:455 alignment bug"]
fn zenoh_publisher_event_mask() {
    let mut sess = open_session();
    let mut pubr = sess
        .create_publisher(&topic(), QosSettings::QOS_PROFILE_DEFAULT)
        .expect("create_publisher");

    // zenoh shim — pub side surfaces OfferedDeadlineMissed (clock
    // check) + LivelinessLost slot (registration accepted, never
    // fires today; needs per-pub keepalive timer for MANUAL_BY_*).
    assert!(pubr.supports_event(EventKind::OfferedDeadlineMissed));
    assert!(pubr.supports_event(EventKind::LivelinessLost));

    // Sub-side kinds always false on a publisher.
    assert!(!pubr.supports_event(EventKind::LivelinessChanged));
    assert!(!pubr.supports_event(EventKind::RequestedDeadlineMissed));
    assert!(!pubr.supports_event(EventKind::MessageLost));

    let cb: EventCallback = dummy_cb;
    let res = unsafe {
        pubr.register_event_callback(
            EventKind::OfferedDeadlineMissed,
            15,
            cb,
            core::ptr::null_mut(),
        )
    };
    assert!(res.is_ok(), "register OfferedDeadlineMissed: {res:?}");

    let res = unsafe {
        pubr.register_event_callback(EventKind::MessageLost, 0, cb, core::ptr::null_mut())
    };
    assert!(matches!(res, Err(TransportError::Unsupported)));
}

#[test]
#[ignore = "blocked on nros-platform-posix net.rs:455 alignment bug"]
fn zenoh_supported_qos_mask() {
    use nros_rmw::QosPolicyMask;
    let sess = open_session();
    let mask = sess.supported_qos_policies();
    // zenoh-pico — CORE + shim-emulated DEADLINE / LIFESPAN /
    // LIVELINESS_AUTOMATIC + LEASE.
    assert!(mask.contains(QosPolicyMask::CORE));
    assert!(mask.contains(QosPolicyMask::DEADLINE));
    assert!(mask.contains(QosPolicyMask::LIFESPAN));
    assert!(mask.contains(QosPolicyMask::LIVELINESS_AUTOMATIC));
}
