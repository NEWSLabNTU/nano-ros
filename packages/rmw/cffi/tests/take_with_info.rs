//! Issue 1495 — a C/C++ backend's per-sample metadata reaches `MessageInfo`.
//!
//! Before this, the runtime's only metadata channel was a side table that the
//! Rust adapter writes and nothing else can, so a pure C/C++ backend (Cyclone)
//! delivered every sample with `MessageInfo` = `None` — a subscriber could not
//! name the publisher that sent it. The ABI's `take_with_info` slot is the
//! channel built for exactly this and nothing dispatched it. These pin the
//! dispatch from both sides: a backend that fills the slot is HEARD (gid,
//! timestamp, the unsupported-sequence sentinel mapped to 0), and one that
//! leaves it NULL keeps the side-table behaviour unchanged.

use core::ffi::c_void;

use nros_rmw::{QoSProfile, RmwConfig, Session as _, SessionMode, Subscription as _, TopicInfo};
use nros_rmw_cffi::{
    CffiRmw, EMPTY_VTABLE, NROS_RMW_RET_ERROR, NROS_RMW_RET_OK, NROS_RMW_RET_UNSUPPORTED,
    NrosRmwClient, NrosRmwEventCallback, NrosRmwEventKind, NrosRmwNode, NrosRmwPublisher,
    NrosRmwQos, NrosRmwRet, NrosRmwService, NrosRmwSession, NrosRmwSessionOptions,
    NrosRmwSubscription, NrosRmwVtable, nros_rmw_cffi_register_named,
};

/// Not uniform and not all-zero, for the reason `publisher_gid.rs` gives: a
/// body that copied the wrong number of bytes still passes a uniform pattern.
const WRITER_GID: [u8; 24] = [
    0x01, 0x10, 0x9a, 0x2b, 0x3c, 0x4d, 0x5e, 0x6f, 0x70, 0x81, 0x92, 0xa3, 0x00, 0x00, 0x01, 0x03,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
];
const SOURCE_NS: i64 = 1_700_000_000_123_456_789;
const PAYLOAD: [u8; 8] = [0x00, 0x01, 0x00, 0x00, 0x2a, 0x00, 0x00, 0x00];

/// One sample, every call — enough for a take to have something to return.
unsafe fn deliver(
    span: *mut nros_rmw_cffi::generated::rmw_mut_byte_span_t,
    taken: *mut bool,
) -> NrosRmwRet {
    unsafe {
        core::ptr::copy_nonoverlapping(PAYLOAD.as_ptr(), (*span).data, PAYLOAD.len());
        (*span).len = PAYLOAD.len();
        *taken = true;
    }
    NROS_RMW_RET_OK
}

unsafe extern "C" fn take_with_info(
    _: *const NrosRmwSubscription,
    span: *mut nros_rmw_cffi::generated::rmw_mut_byte_span_t,
    taken: *mut bool,
    info: *mut nros_rmw_cffi::generated::rmw_message_info_t,
) -> NrosRmwRet {
    unsafe {
        (*info).source_timestamp = SOURCE_NS;
        (*info).received_timestamp = 0;
        (*info).publication_sequence_number = u64::MAX;
        (*info).reception_sequence_number = u64::MAX;
        (*info).publisher_gid.data = WRITER_GID;
        (*info).from_intra_process = false;
        deliver(span, taken)
    }
}

// ---- minimal backend plumbing (as in publisher_gid.rs) ------------------

unsafe extern "C" fn open(
    _: *const core::ffi::c_char,
    _: u8,
    _: u32,
    _: *const core::ffi::c_char,
    _: *const NrosRmwSessionOptions,
    out: *mut NrosRmwSession,
) -> NrosRmwRet {
    unsafe { (*out).backend_data = 0x6110_0001usize as *mut c_void };
    NROS_RMW_RET_OK
}
unsafe extern "C" fn close(_: *mut NrosRmwSession) -> NrosRmwRet {
    NROS_RMW_RET_OK
}
unsafe extern "C" fn drive_io(_: *mut NrosRmwSession, _: i32) -> NrosRmwRet {
    NROS_RMW_RET_OK
}
unsafe extern "C" fn create_publisher(
    _: *const NrosRmwNode,
    _: *const nros_rmw_cffi::generated::rmw_message_type_support_t,
    _: *const core::ffi::c_char,
    _: u32,
    _: *const NrosRmwQos,
    _: *const nros_rmw_cffi::rmw_publisher_options_t,
    out: *mut NrosRmwPublisher,
) -> NrosRmwRet {
    unsafe {
        (*out).backend_data = 0x6110_0002usize as *mut c_void;
        (*out).can_loan_messages = false;
    }
    NROS_RMW_RET_OK
}
unsafe extern "C" fn destroy_publisher(_: *mut NrosRmwPublisher) -> NrosRmwRet {
    NROS_RMW_RET_OK
}
unsafe extern "C" fn publish_raw(
    _: *const NrosRmwPublisher,
    _: nros_rmw_cffi::generated::rmw_byte_span_t,
) -> NrosRmwRet {
    NROS_RMW_RET_OK
}
unsafe extern "C" fn noop_csub(
    _: *const NrosRmwNode,
    _: *const nros_rmw_cffi::generated::rmw_message_type_support_t,
    _: *const core::ffi::c_char,
    _: u32,
    _: *const NrosRmwQos,
    _: *const nros_rmw_cffi::rmw_subscription_options_t,
    out: *mut NrosRmwSubscription,
) -> NrosRmwRet {
    unsafe {
        (*out).backend_data = 0x1495_0003usize as *mut c_void;
        (*out).can_loan_messages = false;
    }
    NROS_RMW_RET_OK
}
unsafe extern "C" fn noop_dsub(_: *mut NrosRmwSubscription) -> NrosRmwRet {
    NROS_RMW_RET_OK
}
unsafe extern "C" fn noop_recv(
    _: *const NrosRmwSubscription,
    span: *mut nros_rmw_cffi::generated::rmw_mut_byte_span_t,
    taken: *mut bool,
) -> NrosRmwRet {
    unsafe { deliver(span, taken) }
}
unsafe extern "C" fn noop_hasd(_: *mut NrosRmwSubscription, has: *mut bool) -> NrosRmwRet {
    unsafe { *has = false };
    NROS_RMW_RET_OK
}
unsafe extern "C" fn noop_csrv(
    _: *const NrosRmwNode,
    _: *const nros_rmw_cffi::generated::rmw_service_type_support_t,
    _: *const core::ffi::c_char,
    _: u32,
    _: *const NrosRmwQos,
    _: *mut NrosRmwService,
) -> NrosRmwRet {
    NROS_RMW_RET_UNSUPPORTED
}
unsafe extern "C" fn noop_dsrv(_: *mut NrosRmwService) -> NrosRmwRet {
    NROS_RMW_RET_OK
}
unsafe extern "C" fn noop_recvreq(
    _: *const NrosRmwService,
    _: *mut nros_rmw_cffi::generated::rmw_mut_byte_span_t,
    _: *mut i64,
    _: *mut bool,
) -> NrosRmwRet {
    NROS_RMW_RET_ERROR
}
unsafe extern "C" fn noop_hasreq(_: *mut NrosRmwService, has: *mut bool) -> NrosRmwRet {
    unsafe { *has = false };
    NROS_RMW_RET_OK
}
unsafe extern "C" fn noop_reply(
    _: *const NrosRmwService,
    _: i64,
    _: nros_rmw_cffi::generated::rmw_byte_span_t,
) -> NrosRmwRet {
    NROS_RMW_RET_UNSUPPORTED
}
unsafe extern "C" fn noop_ccli(
    _: *const NrosRmwNode,
    _: *const nros_rmw_cffi::generated::rmw_service_type_support_t,
    _: *const core::ffi::c_char,
    _: u32,
    _: *const NrosRmwQos,
    _: *mut NrosRmwClient,
) -> NrosRmwRet {
    NROS_RMW_RET_UNSUPPORTED
}
unsafe extern "C" fn noop_dcli(_: *mut NrosRmwClient) -> NrosRmwRet {
    NROS_RMW_RET_OK
}
unsafe extern "C" fn noop_regsubev(
    _: *const NrosRmwSubscription,
    _: NrosRmwEventKind,
    _: u32,
    _: NrosRmwEventCallback,
    _: *mut c_void,
) -> NrosRmwRet {
    NROS_RMW_RET_UNSUPPORTED
}
unsafe extern "C" fn noop_regpubev(
    _: *const NrosRmwPublisher,
    _: NrosRmwEventKind,
    _: u32,
    _: NrosRmwEventCallback,
    _: *mut c_void,
) -> NrosRmwRet {
    NROS_RMW_RET_UNSUPPORTED
}

const BASE: NrosRmwVtable = NrosRmwVtable {
    create_session: Some(open),
    destroy_session: Some(close),
    drive_io: Some(drive_io),
    create_publisher: Some(create_publisher),
    destroy_publisher: Some(destroy_publisher),
    publish: Some(publish_raw),
    create_subscription: Some(noop_csub),
    destroy_subscription: Some(noop_dsub),
    take: Some(noop_recv),
    has_data: Some(noop_hasd),
    create_service: Some(noop_csrv),
    destroy_service: Some(noop_dsrv),
    take_request: Some(noop_recvreq),
    has_request: Some(noop_hasreq),
    send_response: Some(noop_reply),
    create_client: Some(noop_ccli),
    destroy_client: Some(noop_dcli),
    subscription_event_init: Some(noop_regsubev),
    publisher_event_init: Some(noop_regpubev),
    ..EMPTY_VTABLE
};

static NO_INFO_VTABLE: NrosRmwVtable = BASE;

static INFO_VTABLE: NrosRmwVtable = NrosRmwVtable {
    take_with_info: Some(take_with_info),
    ..BASE
};

fn config(node_name: &'static str) -> RmwConfig<'static> {
    RmwConfig {
        mode: SessionMode::Client,
        locator: "tcp/127.0.0.1:7447",
        domain_id: 0,
        node_name,
        namespace: "",
        properties: &[],
    }
}

#[test]
fn a_filled_take_with_info_slot_reaches_message_info() {
    assert_eq!(
        unsafe { nros_rmw_cffi_register_named(c"twi_filled".as_ptr(), &INFO_VTABLE) },
        NROS_RMW_RET_OK,
    );
    let mut session =
        CffiRmw::open_with_rmw("twi_filled", &config("twi_filled_node")).expect("open");
    let mut sub = session
        .create_subscription(
            &TopicInfo::new("/twi", "std_msgs/msg/Int32", "RIHS01_twi"),
            QoSProfile::default(),
        )
        .expect("create subscription");

    let mut buf = [0u8; 64];
    let (len, info) = sub
        .take_serialized_with_info(&mut buf)
        .expect("take")
        .expect("the backend delivered a sample");
    assert_eq!(&buf[..len], &PAYLOAD);
    let info = info.expect(
        "a backend that fills take_with_info reported metadata, and MessageInfo is None — \
         the runtime did not dispatch the slot (issue 1495)",
    );
    assert_eq!(info.publisher_gid(), &WRITER_GID);
    assert_eq!(info.source_timestamp().to_nanos(), SOURCE_NS);
    // The ABI's UNSUPPORTED sentinel is not a sequence number; read as `i64`
    // it would be -1, a number no publisher sent.
    assert_eq!(info.publication_sequence_number(), 0);
    assert_eq!(info.reception_sequence_number(), 0);
}

#[test]
fn a_null_take_with_info_slot_keeps_the_side_table_path() {
    assert_eq!(
        unsafe { nros_rmw_cffi_register_named(c"twi_null".as_ptr(), &NO_INFO_VTABLE) },
        NROS_RMW_RET_OK,
    );
    let mut session = CffiRmw::open_with_rmw("twi_null", &config("twi_null_node")).expect("open");
    let mut sub = session
        .create_subscription(
            &TopicInfo::new("/twi", "std_msgs/msg/Int32", "RIHS01_twi"),
            QoSProfile::default(),
        )
        .expect("create subscription");

    let mut buf = [0u8; 64];
    let (len, info) = sub
        .take_serialized_with_info(&mut buf)
        .expect("take")
        .expect("the backend delivered a sample");
    assert_eq!(&buf[..len], &PAYLOAD);
    // Nothing wrote the side table and the slot is NULL: no metadata, which is
    // the documented answer — not a zeroed MessageInfo posing as one.
    assert!(info.is_none(), "{info:?}");
}
