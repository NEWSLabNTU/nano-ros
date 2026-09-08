//! Issue 1164 — the runtime must be the CALLER of `*_take_event`.
//!
//! A backend fills exactly one half of the status-event surface. cyclonedds
//! fills `*_take_event` (its `drive_io` is a sleep, so it has nowhere safe to
//! call a callback from) and leaves `*_event_init` NULL. Both of its poll
//! slots were implemented and read real `dds_get_*_status` counters — and
//! nothing in the tree ever called them, so `register_event_callback` on a
//! Cyclone entity answered `Unsupported` and no application could observe a
//! status event through either half.
//!
//! ## Which half of the proof this is
//!
//! Two claims have to hold, and they are proved in different places:
//!
//!  1. *the backend's `take_event` reports a counter the DDS stack actually
//!     moved* — `packages/rmw/cyclonedds/nros-rmw-cyclonedds/tests/
//!     status_events.cpp`, which provokes a real `REQUESTED_DEADLINE_MISSED`
//!     out of a live Cyclone reader and fails if the change count is zero.
//!  2. *the runtime polls that slot and delivers to the registered callback* —
//!     this file.
//!
//! So the backend here is a stub, deliberately: what is under test is the
//! wiring above the slot, and a stub is the only way to drive the slot's
//! contract (reset-on-read, `taken = false`, wrong-side kinds) deterministically
//! and without a DDS domain. The stub is written to Cyclone's contract, not to
//! ours: `take_event` CONSUMES the change counter exactly as
//! `dds_get_liveliness_changed_status` does, so a runtime that polls twice and
//! reports twice fails here.
//!
//! Every assertion below fails against the pre-1164 tree: registration was
//! refused, so no callback existed to fire.

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
    NrosRmwSessionOptions, NrosRmwSubscription, NrosRmwVtable, generated,
    nros_rmw_cffi_register_named, rmw_event_type_t,
};

// ---------------------------------------------------------------------------
// The stub backend's "DDS" counters.
//
// `*_change` is what the read CONSUMES; the cumulative total is not reset.
// This is `dds_get_*_status`'s documented behaviour and the reason
// cyclonedds' comment calls the read "exactly `take` semantics".
// ---------------------------------------------------------------------------

static SUB_ALIVE: AtomicU32 = AtomicU32::new(0);
static SUB_ALIVE_CHANGE: AtomicI32 = AtomicI32::new(0);
static PUB_DEADLINE_TOTAL: AtomicU32 = AtomicU32::new(0);
static PUB_DEADLINE_CHANGE: AtomicU32 = AtomicU32::new(0);

/// How many times the runtime asked the backend for an event. A runtime that
/// registers and then never polls leaves this at zero, which is exactly the
/// pre-1164 behaviour.
static TAKE_CALLS: AtomicU32 = AtomicU32::new(0);

unsafe extern "C" fn stub_subscription_take_event(
    subscription: *const NrosRmwSubscription,
    kind: NrosRmwEventKind,
    out: *mut NrosRmwEventPayload,
    taken: *mut bool,
) -> NrosRmwRet {
    if subscription.is_null() || out.is_null() || taken.is_null() {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    TAKE_CALLS.fetch_add(1, Ordering::SeqCst);
    unsafe { *taken = false };
    match kind {
        rmw_event_type_t::NROS_RMW_EVENT_LIVELINESS_CHANGED => {
            let change = SUB_ALIVE_CHANGE.swap(0, Ordering::SeqCst);
            if change == 0 {
                return NROS_RMW_RET_OK;
            }
            unsafe {
                (*out).liveliness_changed = generated::rmw_liveliness_changed_status_t {
                    alive_count: SUB_ALIVE.load(Ordering::SeqCst) as u16,
                    not_alive_count: 0,
                    alive_count_change: change as i16,
                    not_alive_count_change: 0,
                };
                *taken = true;
            }
            NROS_RMW_RET_OK
        }
        rmw_event_type_t::NROS_RMW_EVENT_REQUESTED_DEADLINE_MISSED
        | rmw_event_type_t::NROS_RMW_EVENT_MESSAGE_LOST => NROS_RMW_RET_OK,
        // A publisher-side kind on a subscription is a caller error, which is
        // what both cyclonedds slots answer.
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
    TAKE_CALLS.fetch_add(1, Ordering::SeqCst);
    unsafe { *taken = false };
    match kind {
        rmw_event_type_t::NROS_RMW_EVENT_OFFERED_DEADLINE_MISSED => {
            let change = PUB_DEADLINE_CHANGE.swap(0, Ordering::SeqCst);
            if change == 0 {
                return NROS_RMW_RET_OK;
            }
            unsafe {
                (*out).count = generated::rmw_count_status_t {
                    total_count: PUB_DEADLINE_TOTAL.load(Ordering::SeqCst),
                    total_count_change: change,
                };
                *taken = true;
            }
            NROS_RMW_RET_OK
        }
        rmw_event_type_t::NROS_RMW_EVENT_LIVELINESS_LOST => NROS_RMW_RET_OK,
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

/// The negative control: a backend with NO status-event surface at all, which
/// is what xrce looks like. Registration must still be refused — the poll path
/// is not allowed to invent a capability out of nothing.
static EVENTLESS_VTABLE: NrosRmwVtable = NrosRmwVtable {
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
    ..EMPTY_VTABLE
};

// ---------------------------------------------------------------------------
// Observation sinks for the registered callbacks.
// ---------------------------------------------------------------------------

static SUB_FIRES: AtomicU32 = AtomicU32::new(0);
static SUB_LAST_ALIVE_CHANGE: AtomicI32 = AtomicI32::new(0);
static PUB_FIRES: AtomicU32 = AtomicU32::new(0);
static PUB_LAST_CHANGE: AtomicU32 = AtomicU32::new(0);

unsafe extern "C" fn on_sub_event(kind: EventKind, payload: *const c_void, _user_ctx: *mut c_void) {
    assert_eq!(kind, EventKind::LivelinessChanged);
    assert!(!payload.is_null());
    let status = unsafe { nros_rmw::payload_from_raw(kind, payload) };
    match status {
        nros_rmw::EventPayload::LivelinessChanged(s) => {
            SUB_LAST_ALIVE_CHANGE.store(s.alive_count_change as i32, Ordering::SeqCst);
        }
        other => panic!("wrong payload variant: {other:?}"),
    }
    SUB_FIRES.fetch_add(1, Ordering::SeqCst);
}

unsafe extern "C" fn on_pub_event(kind: EventKind, payload: *const c_void, _user_ctx: *mut c_void) {
    assert_eq!(kind, EventKind::OfferedDeadlineMissed);
    assert!(!payload.is_null());
    let status = unsafe { nros_rmw::payload_from_raw(kind, payload) };
    match status {
        nros_rmw::EventPayload::OfferedDeadlineMissed(s) => {
            PUB_LAST_CHANGE.store(s.total_count_change, Ordering::SeqCst);
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
fn take_event_backend_delivers_status_events_to_a_registered_callback() {
    assert_eq!(
        unsafe { nros_rmw_cffi_register_named(c"tep_polled".as_ptr(), &POLLED_VTABLE) },
        NROS_RMW_RET_OK
    );

    let mut session =
        CffiRmw::open_with_rmw("tep_polled", &config("tep_node")).expect("open polled backend");
    let topic = TopicInfo::new("/tep", "std_msgs/msg/Int32", "RIHS01_tep");
    let mut sub = session
        .create_subscription(&topic, QoSProfile::default())
        .expect("create subscription");

    // ---- capability -------------------------------------------------------
    // The book tells applications to gate registration on this. It answered a
    // flat `false` for every backend behind this vtable before 1164, so a
    // book-following application skipped registration on a backend whose
    // events work.
    assert!(
        sub.supports_event(EventKind::LivelinessChanged),
        "a backend with `subscription_take_event` can carry a subscription-side kind"
    );
    assert!(
        !sub.supports_event(EventKind::LivelinessLost),
        "a publisher-side kind is not a subscription capability"
    );

    // ---- registration -----------------------------------------------------
    unsafe {
        sub.register_event_callback(
            EventKind::LivelinessChanged,
            0,
            on_sub_event,
            core::ptr::null_mut(),
        )
    }
    .expect("registration must succeed on a `take_event` backend");

    // ---- nothing moved, nothing fires -------------------------------------
    let takes_before = TAKE_CALLS.load(Ordering::SeqCst);
    assert!(!sub.has_data());
    assert!(
        TAKE_CALLS.load(Ordering::SeqCst) > takes_before,
        "the runtime must POLL the slot; not polling is the pre-1164 defect"
    );
    assert_eq!(
        SUB_FIRES.load(Ordering::SeqCst),
        0,
        "a poll that found no change must not fire a callback"
    );

    // ---- the backend's counter moves --------------------------------------
    SUB_ALIVE.store(1, Ordering::SeqCst);
    SUB_ALIVE_CHANGE.store(1, Ordering::SeqCst);

    assert!(!sub.has_data());
    assert_eq!(
        SUB_FIRES.load(Ordering::SeqCst),
        1,
        "a moved counter must reach the application callback"
    );
    assert_eq!(SUB_LAST_ALIVE_CHANGE.load(Ordering::SeqCst), 1);

    // ---- the read CONSUMED it ---------------------------------------------
    // `dds_get_*_status` resets the change counter as it reads. A runtime that
    // re-reported the same status on the next poll would look like a working
    // event surface and be reporting an event that did not happen.
    assert!(!sub.has_data());
    let _ = sub.take_serialized(&mut [0u8; 8]);
    assert_eq!(
        SUB_FIRES.load(Ordering::SeqCst),
        1,
        "the same event must not be delivered twice"
    );

    // ---- the take path is a poll point too --------------------------------
    SUB_ALIVE.store(2, Ordering::SeqCst);
    SUB_ALIVE_CHANGE.store(1, Ordering::SeqCst);
    let _ = sub.take_serialized(&mut [0u8; 8]);
    assert_eq!(
        SUB_FIRES.load(Ordering::SeqCst),
        2,
        "`take_serialized` must poll as well as `has_data` — a caller that only \
         takes still has to see its events"
    );

    // ---- publisher side ---------------------------------------------------
    let pub_topic = TopicInfo::new("/tep_pub", "std_msgs/msg/Int32", "RIHS01_tep");
    let mut publisher = session
        .create_publisher(&pub_topic, QoSProfile::default())
        .expect("create publisher");
    assert!(publisher.supports_event(EventKind::OfferedDeadlineMissed));
    assert!(!publisher.supports_event(EventKind::MessageLost));
    unsafe {
        publisher.register_event_callback(
            EventKind::OfferedDeadlineMissed,
            0,
            on_pub_event,
            core::ptr::null_mut(),
        )
    }
    .expect("registration must succeed on a `take_event` backend");

    PUB_DEADLINE_TOTAL.store(3, Ordering::SeqCst);
    PUB_DEADLINE_CHANGE.store(2, Ordering::SeqCst);
    publisher.publish_raw(&[0u8; 4]).expect("publish");
    assert_eq!(
        PUB_FIRES.load(Ordering::SeqCst),
        1,
        "the publisher's data path is its poll point"
    );
    assert_eq!(PUB_LAST_CHANGE.load(Ordering::SeqCst), 2);

    publisher.publish_raw(&[0u8; 4]).expect("publish");
    assert_eq!(
        PUB_FIRES.load(Ordering::SeqCst),
        1,
        "consumed by the first read; the second publish must report nothing"
    );
}

#[test]
fn a_backend_with_neither_half_still_refuses_registration() {
    assert_eq!(
        unsafe { nros_rmw_cffi_register_named(c"tep_eventless".as_ptr(), &EVENTLESS_VTABLE) },
        NROS_RMW_RET_OK
    );
    let mut session = CffiRmw::open_with_rmw("tep_eventless", &config("tep_none"))
        .expect("open eventless backend");
    let topic = TopicInfo::new("/tep_none", "std_msgs/msg/Int32", "RIHS01_tep");
    let mut sub = session
        .create_subscription(&topic, QoSProfile::default())
        .expect("create subscription");

    assert!(!sub.supports_event(EventKind::LivelinessChanged));
    let err = unsafe {
        sub.register_event_callback(
            EventKind::LivelinessChanged,
            0,
            on_sub_event,
            core::ptr::null_mut(),
        )
    }
    .expect_err("no event surface at all must stay `Unsupported`");
    assert_eq!(err, TransportError::Unsupported);

    // And polling a backend with no slot must not touch the stub's counters.
    let before = TAKE_CALLS.load(Ordering::SeqCst);
    assert!(!sub.has_data());
    assert_eq!(TAKE_CALLS.load(Ordering::SeqCst), before);
}
