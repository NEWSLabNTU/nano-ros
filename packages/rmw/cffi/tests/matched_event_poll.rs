//! Issue 1972 — the matched and incompatible-type kinds travel the POLLED
//! status-event path (`*_take_event`, the cyclonedds shape) end to end.
//!
//! Companion to `take_event_poll.rs` (issue 1164), which proves the wiring
//! for the Tier-1 kinds. What is new here and worth its own proof:
//!
//!  * four more kinds must have registry slots on the right SIDE — a
//!    `SubscriptionMatched` registered on a publisher is a caller error;
//!  * the payload union GREW from 8 to 16 bytes, and the runtime owns the
//!    union it hands the backend. A runtime still allocating the old 8 bytes
//!    would have the backend write `current_count`/`current_count_change` past
//!    the end; here those two fields are asserted to arrive intact;
//!  * the C discriminants 5..8 map to the right `EventKind`s both ways.
//!
//! The real Cyclone producer is proved against a live reader/writer in
//! `packages/rmw/cyclonedds/nros-rmw-cyclonedds/tests/matched_events.cpp`.

use core::{
    ffi::c_void,
    sync::atomic::{AtomicI32, AtomicU32, Ordering},
};

use nros_rmw::{
    EventKind, Publisher as _, QoSProfile, RmwConfig, Session as _, SessionMode, Subscription as _,
    TopicInfo, TransportError,
};
use nros_rmw_cffi::{
    CffiRmw, EMPTY_VTABLE, NROS_RMW_RET_INVALID_ARGUMENT, NROS_RMW_RET_OK,
    NROS_RMW_RET_UNSUPPORTED, NrosRmwClient, NrosRmwEventKind, NrosRmwEventPayload, NrosRmwNode,
    NrosRmwPublisher, NrosRmwQos, NrosRmwRet, NrosRmwService, NrosRmwSession,
    NrosRmwSessionOptions, NrosRmwSubscription, NrosRmwVtable, event_kind_from_c, event_kind_to_c,
    generated, nros_rmw_cffi_register_named, rmw_event_type_t,
};

/// The stub "DDS" match state: what the backend would read from its own
/// matched-status, reported once per change (take semantics).
static SUB_MATCHED_PENDING: AtomicU32 = AtomicU32::new(0);
static PUB_INCOMPAT_PENDING: AtomicU32 = AtomicU32::new(0);

unsafe extern "C" fn stub_subscription_take_event(
    subscription: *const NrosRmwSubscription,
    kind: NrosRmwEventKind,
    out: *mut NrosRmwEventPayload,
    taken: *mut bool,
) -> NrosRmwRet {
    if subscription.is_null() || out.is_null() || taken.is_null() {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    unsafe { *taken = false };
    match kind {
        rmw_event_type_t::NROS_RMW_EVENT_SUBSCRIPTION_MATCHED => {
            if SUB_MATCHED_PENDING.swap(0, Ordering::SeqCst) == 0 {
                return NROS_RMW_RET_OK;
            }
            unsafe {
                (*out).matched = generated::rmw_matched_status_t {
                    total_count: 7,
                    total_count_change: 0,
                    current_count: 0x1234_5678,
                    current_count_change: -3,
                };
                *taken = true;
            }
            NROS_RMW_RET_OK
        }
        rmw_event_type_t::NROS_RMW_EVENT_LIVELINESS_CHANGED
        | rmw_event_type_t::NROS_RMW_EVENT_REQUESTED_DEADLINE_MISSED
        | rmw_event_type_t::NROS_RMW_EVENT_MESSAGE_LOST
        | rmw_event_type_t::NROS_RMW_EVENT_SUBSCRIPTION_INCOMPATIBLE_TYPE => NROS_RMW_RET_OK,
        _ => NROS_RMW_RET_INVALID_ARGUMENT,
    }
}

unsafe extern "C" fn stub_publisher_take_event(
    publisher: *const NrosRmwPublisher,
    kind: NrosRmwEventKind,
    out: *mut NrosRmwEventPayload,
    taken: *mut bool,
) -> NrosRmwRet {
    if publisher.is_null() || out.is_null() || taken.is_null() {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    unsafe { *taken = false };
    match kind {
        rmw_event_type_t::NROS_RMW_EVENT_PUBLISHER_INCOMPATIBLE_TYPE => {
            let n = PUB_INCOMPAT_PENDING.swap(0, Ordering::SeqCst);
            if n == 0 {
                return NROS_RMW_RET_OK;
            }
            unsafe {
                (*out).incompatible_type = generated::rmw_incompatible_type_status_t {
                    total_count: n,
                    total_count_change: n,
                };
                *taken = true;
            }
            NROS_RMW_RET_OK
        }
        rmw_event_type_t::NROS_RMW_EVENT_LIVELINESS_LOST
        | rmw_event_type_t::NROS_RMW_EVENT_OFFERED_DEADLINE_MISSED
        | rmw_event_type_t::NROS_RMW_EVENT_PUBLICATION_MATCHED => NROS_RMW_RET_OK,
        _ => NROS_RMW_RET_INVALID_ARGUMENT,
    }
}

// ---------------------------------------------------------------------------
// Minimal session / entity plumbing for the two stub backends.
// ---------------------------------------------------------------------------

unsafe extern "C" fn stub_open(
    _locator: *const core::ffi::c_char,
    _mode: u8,
    _domain_id: u32,
    _node_name: *const core::ffi::c_char,
    _options: *const NrosRmwSessionOptions,
    out: *mut NrosRmwSession,
) -> NrosRmwRet {
    unsafe { (*out).backend_data = 0x5E55_1000usize as *mut c_void };
    NROS_RMW_RET_OK
}

unsafe extern "C" fn stub_close(_session: *mut NrosRmwSession) -> NrosRmwRet {
    NROS_RMW_RET_OK
}

unsafe extern "C" fn stub_create_publisher(
    _node: *const NrosRmwNode,
    _ts: *const generated::rmw_message_type_support_t,
    _topic_name: *const core::ffi::c_char,
    _domain_id: u32,
    _qos: *const NrosRmwQos,
    _options: *const nros_rmw_cffi::rmw_publisher_options_t,
    out: *mut NrosRmwPublisher,
) -> NrosRmwRet {
    unsafe {
        (*out).backend_data = 0x5E55_2000usize as *mut c_void;
        (*out).can_loan_messages = false;
    }
    NROS_RMW_RET_OK
}

unsafe extern "C" fn stub_destroy_publisher(_p: *mut NrosRmwPublisher) -> NrosRmwRet {
    NROS_RMW_RET_OK
}

unsafe extern "C" fn stub_publish(
    _publisher: *const NrosRmwPublisher,
    _data: generated::rmw_byte_span_t,
) -> NrosRmwRet {
    NROS_RMW_RET_OK
}

unsafe extern "C" fn stub_create_subscription(
    _node: *const NrosRmwNode,
    _ts: *const generated::rmw_message_type_support_t,
    _topic_name: *const core::ffi::c_char,
    _domain_id: u32,
    _qos: *const NrosRmwQos,
    _options: *const nros_rmw_cffi::rmw_subscription_options_t,
    out: *mut NrosRmwSubscription,
) -> NrosRmwRet {
    unsafe {
        (*out).backend_data = 0x5E55_3000usize as *mut c_void;
        (*out).can_loan_messages = false;
    }
    NROS_RMW_RET_OK
}

unsafe extern "C" fn stub_destroy_subscription(_s: *mut NrosRmwSubscription) -> NrosRmwRet {
    NROS_RMW_RET_OK
}

unsafe extern "C" fn stub_take(
    _s: *const NrosRmwSubscription,
    _span: *mut generated::rmw_mut_byte_span_t,
    taken: *mut bool,
) -> NrosRmwRet {
    unsafe { *taken = false };
    NROS_RMW_RET_OK
}

unsafe extern "C" fn stub_has_data(
    _s: *mut NrosRmwSubscription,
    out_has_data: *mut bool,
) -> NrosRmwRet {
    unsafe { *out_has_data = false };
    NROS_RMW_RET_OK
}

unsafe extern "C" fn stub_drive_io(_session: *mut NrosRmwSession, _timeout_ms: i32) -> NrosRmwRet {
    NROS_RMW_RET_OK
}

unsafe extern "C" fn stub_create_service(
    _node: *const NrosRmwNode,
    _ts: *const generated::rmw_service_type_support_t,
    _name: *const core::ffi::c_char,
    _domain_id: u32,
    _qos: *const NrosRmwQos,
    _out: *mut NrosRmwService,
) -> NrosRmwRet {
    NROS_RMW_RET_UNSUPPORTED
}

unsafe extern "C" fn stub_destroy_service(_s: *mut NrosRmwService) -> NrosRmwRet {
    NROS_RMW_RET_OK
}

unsafe extern "C" fn stub_take_request(
    _s: *const NrosRmwService,
    _span: *mut generated::rmw_mut_byte_span_t,
    _seq: *mut i64,
    taken: *mut bool,
) -> NrosRmwRet {
    unsafe { *taken = false };
    NROS_RMW_RET_OK
}

unsafe extern "C" fn stub_has_request(
    _s: *mut NrosRmwService,
    out_has_request: *mut bool,
) -> NrosRmwRet {
    unsafe { *out_has_request = false };
    NROS_RMW_RET_OK
}

unsafe extern "C" fn stub_send_response(
    _s: *const NrosRmwService,
    _seq: i64,
    _data: generated::rmw_byte_span_t,
) -> NrosRmwRet {
    NROS_RMW_RET_UNSUPPORTED
}

unsafe extern "C" fn stub_create_client(
    _node: *const NrosRmwNode,
    _ts: *const generated::rmw_service_type_support_t,
    _name: *const core::ffi::c_char,
    _domain_id: u32,
    _qos: *const NrosRmwQos,
    _out: *mut NrosRmwClient,
) -> NrosRmwRet {
    NROS_RMW_RET_UNSUPPORTED
}

unsafe extern "C" fn stub_destroy_client(_c: *mut NrosRmwClient) -> NrosRmwRet {
    NROS_RMW_RET_OK
}

/// The mandatory slots every registered vtable must carry
/// (`first_missing_vtable_slot`), minus the ones each backend below overrides.
/// Kept in one place so the two vtables differ ONLY in their event surface —
/// which is the variable this file is about.
/// The cyclonedds shape: both poll slots filled, both `*_event_init` NULL.
static POLLED_VTABLE: NrosRmwVtable = NrosRmwVtable {
    create_session: Some(stub_open),
    destroy_session: Some(stub_close),
    drive_io: Some(stub_drive_io),
    create_publisher: Some(stub_create_publisher),
    destroy_publisher: Some(stub_destroy_publisher),
    publish: Some(stub_publish),
    create_subscription: Some(stub_create_subscription),
    destroy_subscription: Some(stub_destroy_subscription),
    take: Some(stub_take),
    has_data: Some(stub_has_data),
    create_service: Some(stub_create_service),
    destroy_service: Some(stub_destroy_service),
    take_request: Some(stub_take_request),
    has_request: Some(stub_has_request),
    send_response: Some(stub_send_response),
    create_client: Some(stub_create_client),
    destroy_client: Some(stub_destroy_client),
    subscription_take_event: Some(stub_subscription_take_event),
    publisher_take_event: Some(stub_publisher_take_event),
    ..EMPTY_VTABLE
};

static SUB_FIRES: AtomicU32 = AtomicU32::new(0);
static SUB_CURRENT: AtomicU32 = AtomicU32::new(0);
static SUB_CURRENT_CHANGE: AtomicI32 = AtomicI32::new(0);
static PUB_FIRES: AtomicU32 = AtomicU32::new(0);
static PUB_INCOMPAT_CHANGE: AtomicU32 = AtomicU32::new(0);

unsafe extern "C" fn on_sub_matched(kind: EventKind, payload: *const c_void, _ctx: *mut c_void) {
    assert_eq!(kind, EventKind::SubscriptionMatched);
    match unsafe { nros_rmw::payload_from_raw(kind, payload) } {
        nros_rmw::EventPayload::SubscriptionMatched(s) => {
            assert_eq!(s.total_count, 7);
            SUB_CURRENT.store(s.current_count, Ordering::SeqCst);
            SUB_CURRENT_CHANGE.store(s.current_count_change, Ordering::SeqCst);
        }
        other => panic!("wrong payload variant: {other:?}"),
    }
    SUB_FIRES.fetch_add(1, Ordering::SeqCst);
}

unsafe extern "C" fn on_pub_incompat(kind: EventKind, payload: *const c_void, _ctx: *mut c_void) {
    assert_eq!(kind, EventKind::PublisherIncompatibleType);
    match unsafe { nros_rmw::payload_from_raw(kind, payload) } {
        nros_rmw::EventPayload::PublisherIncompatibleType(s) => {
            PUB_INCOMPAT_CHANGE.store(s.total_count_change, Ordering::SeqCst);
        }
        other => panic!("wrong payload variant: {other:?}"),
    }
    PUB_FIRES.fetch_add(1, Ordering::SeqCst);
}

fn config(node: &'static str) -> RmwConfig<'static> {
    RmwConfig {
        mode: SessionMode::Client,
        locator: "tcp/127.0.0.1:7447",
        domain_id: 0,
        node_name: node,
        namespace: "",
        properties: &[],
    }
}

#[test]
fn the_new_kinds_round_trip_through_the_c_discriminants() {
    use EventKind::*;
    for (k, c) in [
        (
            PublicationMatched,
            rmw_event_type_t::NROS_RMW_EVENT_PUBLICATION_MATCHED,
        ),
        (
            SubscriptionMatched,
            rmw_event_type_t::NROS_RMW_EVENT_SUBSCRIPTION_MATCHED,
        ),
        (
            PublisherIncompatibleType,
            rmw_event_type_t::NROS_RMW_EVENT_PUBLISHER_INCOMPATIBLE_TYPE,
        ),
        (
            SubscriptionIncompatibleType,
            rmw_event_type_t::NROS_RMW_EVENT_SUBSCRIPTION_INCOMPATIBLE_TYPE,
        ),
    ] {
        assert_eq!(event_kind_to_c(k), c, "{k:?}");
        assert_eq!(event_kind_from_c(c), k, "{k:?}");
    }
    // Appended, never inserted: the Tier-1 values did not move.
    assert_eq!(rmw_event_type_t::NROS_RMW_EVENT_OFFERED_DEADLINE_MISSED, 4);
    assert_eq!(rmw_event_type_t::NROS_RMW_EVENT_PUBLICATION_MATCHED, 5);
    assert_eq!(
        rmw_event_type_t::NROS_RMW_EVENT_SUBSCRIPTION_INCOMPATIBLE_TYPE,
        8
    );
    assert_eq!(core::mem::size_of::<NrosRmwEventPayload>(), 16);
}

#[test]
fn matched_and_incompatible_type_reach_the_callback_through_the_poll_path() {
    assert_eq!(
        unsafe { nros_rmw_cffi_register_named(c"mep_polled".as_ptr(), &POLLED_VTABLE) },
        NROS_RMW_RET_OK
    );
    let mut session =
        CffiRmw::open_with_rmw("mep_polled", &config("mep_node")).expect("open polled backend");

    // ---- subscription: SubscriptionMatched -------------------------------
    let topic = TopicInfo::new("/mep", "std_msgs/msg/Int32", "RIHS01_mep");
    let mut sub = session
        .create_subscription(&topic, QoSProfile::default())
        .expect("create subscription");
    assert!(sub.supports_event(EventKind::SubscriptionMatched));
    assert!(sub.supports_event(EventKind::SubscriptionIncompatibleType));
    assert!(
        !sub.supports_event(EventKind::PublicationMatched),
        "a publisher-side kind is not a subscription capability"
    );
    let err = unsafe {
        sub.register_event_callback(
            EventKind::PublicationMatched,
            0,
            on_sub_matched,
            core::ptr::null_mut(),
        )
    }
    .expect_err("a publisher-side kind registered on a subscription");
    assert_eq!(err, TransportError::Unsupported);
    unsafe {
        sub.register_event_callback(
            EventKind::SubscriptionMatched,
            0,
            on_sub_matched,
            core::ptr::null_mut(),
        )
    }
    .expect("register SubscriptionMatched");

    assert!(!sub.has_data());
    assert_eq!(
        SUB_FIRES.load(Ordering::SeqCst),
        0,
        "nothing moved, nothing fires"
    );

    SUB_MATCHED_PENDING.store(1, Ordering::SeqCst);
    assert!(!sub.has_data());
    assert_eq!(SUB_FIRES.load(Ordering::SeqCst), 1);
    // The two fields past the old 8-byte union arrive intact.
    assert_eq!(SUB_CURRENT.load(Ordering::SeqCst), 0x1234_5678);
    assert_eq!(SUB_CURRENT_CHANGE.load(Ordering::SeqCst), -3);
    assert!(!sub.has_data());
    assert_eq!(
        SUB_FIRES.load(Ordering::SeqCst),
        1,
        "consumed by the first poll"
    );

    // ---- publisher: PublisherIncompatibleType ----------------------------
    let pub_topic = TopicInfo::new("/mep_pub", "std_msgs/msg/Int32", "RIHS01_mep");
    let mut publisher = session
        .create_publisher(&pub_topic, QoSProfile::default())
        .expect("create publisher");
    assert!(publisher.supports_event(EventKind::PublicationMatched));
    assert!(publisher.supports_event(EventKind::PublisherIncompatibleType));
    assert!(!publisher.supports_event(EventKind::SubscriptionMatched));
    unsafe {
        publisher.register_event_callback(
            EventKind::PublisherIncompatibleType,
            0,
            on_pub_incompat,
            core::ptr::null_mut(),
        )
    }
    .expect("register PublisherIncompatibleType");
    PUB_INCOMPAT_PENDING.store(2, Ordering::SeqCst);
    publisher.publish_raw(&[0u8; 4]).expect("publish");
    assert_eq!(
        PUB_FIRES.load(Ordering::SeqCst),
        1,
        "publish is the publisher's poll point"
    );
    assert_eq!(PUB_INCOMPAT_CHANGE.load(Ordering::SeqCst), 2);
    publisher.publish_raw(&[0u8; 4]).expect("publish");
    assert_eq!(PUB_FIRES.load(Ordering::SeqCst), 1);
}
