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
//! Run via:
//! ```bash
//! cargo test --features "platform-posix link-tcp" --test status_events_matrix
//! ```
//!
//! `link-tcp` is required because zenoh-pico's `zpico_open` falls back
//! to a no-link build otherwise and refuses to bring up the session.
//! The test only exercises in-process trait methods; no wire traffic.

#![cfg(feature = "platform-posix")]

use core::ffi::c_void;
use nros_rmw::{
    EventCallback, EventKind, Publisher, QoSProfile, Session, SessionMode, Subscription, TopicInfo,
    Transport, TransportConfig, TransportError,
};
use nros_rmw_zenoh::ZenohTransport;
use nros_tests::fixtures::ZenohRouter;
use std::{
    sync::{Mutex, OnceLock},
    time::Duration,
};

/// One zenohd per test run, shared across tests via OnceLock + Mutex.
/// Returns the locator string the tests should connect to. `None` if
/// the zenohd binary isn't built (caller should skip).
///
/// Issue 0573 — this used to be a private `RouterHandle` that spawned zenohd
/// with a bare `Command::spawn()`. That copy leaked a router on EVERY run: it
/// armed no `PR_SET_PDEATHSIG`, so nextest's SIGKILL left an orphan, and its
/// `impl Drop` was dead code because Rust never drops a `static`. Eleven such
/// routers were found alive on a dev host, the oldest 3.8 days old. It also
/// re-introduced the bind-port-0-then-close race that issue 0470 removed.
///
/// `ZenohRouter` carries all three fixes already, so this goes through the
/// shared fixture. Holding it in a `static` is still fine: `PR_SET_PDEATHSIG`,
/// not `Drop`, is what bounds the child when the parent is killed.
fn router_locator() -> Option<String> {
    static ROUTER: OnceLock<Mutex<Option<ZenohRouter>>> = OnceLock::new();
    let cell = ROUTER.get_or_init(|| {
        Mutex::new(match ZenohRouter::start_unique() {
            Ok(r) => Some(r),
            Err(e) => {
                eprintln!(
                    "[zenoh-matrix] could not start zenohd ({e:?}) — run \
                     `nros setup <board> --rmw zenoh`; test skipped"
                );
                None
            }
        })
    });
    let guard = cell.lock().ok()?;
    guard.as_ref().map(|h| h.locator())
}

unsafe extern "C" fn dummy_cb(_kind: EventKind, _payload: *const c_void, _ctx: *mut c_void) {}

fn open_session() -> Option<nros_rmw_zenoh::ZenohSession> {
    let locator = router_locator()?;
    let config = TransportConfig {
        locator: Some(locator.as_str()),
        mode: SessionMode::Client,
        properties: &[("multicast_scouting", "false")],
        node_name: "",
        namespace: "",
        domain_id: 0,
    };
    ZenohTransport::open(&config).ok()
}

fn topic() -> TopicInfo<'static> {
    TopicInfo::new(
        "zenoh_event_matrix",
        "std_msgs::msg::dds_::String_",
        "RIHS01_unused",
    )
}

/// All matrix assertions packed into one `#[test]` so we open zenoh-pico's
/// session exactly once. zpico-sys's C shim keeps entity slots in `static`
/// arrays that segfault when multiple sessions are opened (or torn down
/// concurrently); a single test fn dodges that without needing a custom
/// `unsafe impl Send for Context`.
#[test]
fn zenoh_event_matrix() {
    use nros_rmw::QoSPolicyMask;
    // issue 0388 — an absent zenohd is an unmet PRECONDITION, not a defect, and
    // `router_locator` already treats it that way ("tests will skip"). This
    // `expect` then panicked anyway, so the tier reported a missing prerequisite
    // exactly like a broken assertion. `skip!` makes the two distinguishable:
    // tier 1 rewrites [SKIPPED] panics to <skipped> and passes, while a real
    // failure still fails.
    let Some(mut sess) = open_session() else {
        nros_tests::skip!(
            "zenohd unavailable (looked at the nros setup store and PATH) — the \
             vendored `build/zenohd` is gone (phase-362 W4, RFC-0075): the router \
             now comes from ROS, so install `ros-<distro>-rmw-zenoh-cpp` or set \
             NROS_RMW_ZENOHD"
        );
    };

    // ---- Subscriber-side mask ----
    let mut sub = sess
        .create_subscription(&topic(), QoSProfile::QOS_PROFILE_DEFAULT)
        .expect("create_subscription");

    // Full Tier-1 sub-side set: MessageLost (attachment seq gap),
    // RequestedDeadlineMissed (clock check), LivelinessChanged
    // (wildcard liveliness poll).
    assert!(sub.supports_event(EventKind::LivelinessChanged));
    assert!(sub.supports_event(EventKind::RequestedDeadlineMissed));
    assert!(sub.supports_event(EventKind::MessageLost));
    assert!(!sub.supports_event(EventKind::LivelinessLost));
    assert!(!sub.supports_event(EventKind::OfferedDeadlineMissed));

    let cb: EventCallback = dummy_cb;
    let res = unsafe {
        sub.register_event_callback(EventKind::MessageLost, 0, cb, core::ptr::null_mut())
    };
    assert!(res.is_ok(), "register MessageLost: {res:?}");
    let res = unsafe {
        sub.register_event_callback(
            EventKind::OfferedDeadlineMissed,
            0,
            cb,
            core::ptr::null_mut(),
        )
    };
    assert!(matches!(res, Err(TransportError::Unsupported)));

    // ---- Publisher-side mask ----
    let mut pubr = sess
        .create_publisher(&topic(), QoSProfile::QOS_PROFILE_DEFAULT)
        .expect("create_publisher");

    // Pub-side: OfferedDeadlineMissed (clock check) + LivelinessLost
    // slot (registration ok, never fires today — needs per-pub
    // keepalive timer for MANUAL_BY_*).
    assert!(pubr.supports_event(EventKind::OfferedDeadlineMissed));
    assert!(pubr.supports_event(EventKind::LivelinessLost));
    assert!(!pubr.supports_event(EventKind::LivelinessChanged));
    assert!(!pubr.supports_event(EventKind::RequestedDeadlineMissed));
    assert!(!pubr.supports_event(EventKind::MessageLost));

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

    // ---- Session-level supported QoS mask ----
    let mask = sess.supported_qos_policies();
    assert!(mask.contains(QoSPolicyMask::CORE));
    assert!(mask.contains(QoSPolicyMask::DEADLINE));
    assert!(mask.contains(QoSPolicyMask::LIFESPAN));
    assert!(mask.contains(QoSPolicyMask::LIVELINESS_AUTOMATIC));

    // ---- LivelinessLost (Phase 108.C.zenoh.4-followup) ----
    //
    // Manual liveliness with a 100 ms lease: the publisher fires
    // `LivelinessLost` from `publish_raw` when the gap since the last
    // `assert_liveliness()` exceeds the lease.
    use nros_rmw::{QoSLivelinessPolicy, QoSProfile as Qos};
    let manual_qos = Qos {
        liveliness_kind: QoSLivelinessPolicy::ManualByTopic,
        liveliness_lease_ms: 100,
        ..Qos::QOS_PROFILE_DEFAULT
    };
    let mut manual_pub = sess
        .create_publisher(&topic(), manual_qos)
        .expect("create_publisher manual liveliness");

    // Static counter — `unsafe extern "C" fn` callback can't capture.
    use core::sync::atomic::{AtomicU32, Ordering as AtomicOrdering};
    static LL_COUNT: AtomicU32 = AtomicU32::new(0);
    static LL_LAST_TOTAL: AtomicU32 = AtomicU32::new(0);
    LL_COUNT.store(0, AtomicOrdering::Relaxed);
    LL_LAST_TOTAL.store(0, AtomicOrdering::Relaxed);

    unsafe extern "C" fn on_lost(_kind: EventKind, payload: *const c_void, _ctx: *mut c_void) {
        let status = unsafe { &*(payload as *const nros_rmw::CountStatus) };
        LL_COUNT.fetch_add(status.total_count_change, AtomicOrdering::Relaxed);
        LL_LAST_TOTAL.store(status.total_count, AtomicOrdering::Relaxed);
    }

    let cb_lost: EventCallback = on_lost;
    let res = unsafe {
        manual_pub.register_event_callback(
            EventKind::LivelinessLost,
            100,
            cb_lost,
            core::ptr::null_mut(),
        )
    };
    assert!(res.is_ok(), "register LivelinessLost: {res:?}");

    // First publish: still within the just-created lease window —
    // no LivelinessLost should fire.
    manual_pub.publish_raw(b"hello").expect("publish_raw");
    assert_eq!(LL_COUNT.load(AtomicOrdering::Relaxed), 0);

    // Sleep past the lease, then publish again — should fire once.
    std::thread::sleep(Duration::from_millis(150));
    manual_pub.publish_raw(b"hello").expect("publish_raw");
    assert_eq!(
        LL_COUNT.load(AtomicOrdering::Relaxed),
        1,
        "expected one LivelinessLost fire"
    );
    assert_eq!(LL_LAST_TOTAL.load(AtomicOrdering::Relaxed), 1);

    // Assert liveliness — should reset the lease, then immediate
    // publish does NOT fire again.
    manual_pub.assert_liveliness().expect("assert_liveliness");
    manual_pub.publish_raw(b"hello").expect("publish_raw");
    assert_eq!(
        LL_COUNT.load(AtomicOrdering::Relaxed),
        1,
        "assert_liveliness should reset lease"
    );
}
