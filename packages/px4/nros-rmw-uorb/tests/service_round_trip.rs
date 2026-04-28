//! End-to-end test for paired-topic services over uORB
//! (Phase 90.4b).
//!
//! Wire format on both `<svc>/_request` and `<svc>/_reply`:
//!
//! ```text
//! [8 bytes: u64 LE seq] [N bytes: payload]
//! ```
//!
//! Test flow:
//! 1. Register two big-enough uORB topic markers as request + reply
//!    carriers for service `/test/echo`.
//! 2. Build a server + client via the `Session::create_service_*`
//!    paths (exercises the same wiring real code uses).
//! 3. Client sends a request payload through `send_request_raw`.
//! 4. Server sees it via `try_recv_request`, echoes it back via
//!    `send_reply` with the same seq.
//! 5. Client retrieves the echoed payload via `try_recv_reply_raw`.

#![allow(non_camel_case_types)]
#![cfg(feature = "std")]

use std::sync::Mutex;

use nros_rmw::Rmw;
use nros_rmw::{ServiceClientTrait, ServiceServerTrait, Session};
use nros_rmw_uorb::{UorbRmw, register};
use px4_sys::orb_metadata;
use px4_uorb::{OrbMetadata, UorbTopic};

static TEST_LOCK: Mutex<()> = Mutex::new(());

use nros_rmw_uorb::UORB_SERVICE_TOPIC_BYTES;

#[repr(C)]
#[derive(Copy, Clone, Debug, PartialEq)]
struct Envelope {
    bytes: [u8; UORB_SERVICE_TOPIC_BYTES],
}

struct request_topic;
static REQ_NAME: [u8; 13] = *b"sensor_accel\0";
static REQ_META: OrbMetadata = OrbMetadata::new(orb_metadata {
    o_name: REQ_NAME.as_ptr() as *const _,
    o_size: core::mem::size_of::<Envelope>() as u16,
    o_size_no_padding: core::mem::size_of::<Envelope>() as u16,
    message_hash: 0,
    o_id: u16::MAX,
    o_queue: 1,
});
impl UorbTopic for request_topic {
    type Msg = Envelope;
    fn metadata() -> &'static orb_metadata {
        REQ_META.get()
    }
}

struct reply_topic;
static REP_NAME: [u8; 12] = *b"sensor_baro\0";
static REP_META: OrbMetadata = OrbMetadata::new(orb_metadata {
    o_name: REP_NAME.as_ptr() as *const _,
    o_size: core::mem::size_of::<Envelope>() as u16,
    o_size_no_padding: core::mem::size_of::<Envelope>() as u16,
    message_hash: 0,
    o_id: u16::MAX,
    o_queue: 1,
});
impl UorbTopic for reply_topic {
    type Msg = Envelope;
    fn metadata() -> &'static orb_metadata {
        REP_META.get()
    }
}

#[test]
fn paired_topic_service_round_trip() {
    let _g = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    px4_uorb::_reset_broker();
    nros_rmw_uorb::_reset();

    // Register the two carrier topics. Naming follows the
    // `paired_names` convention inside service.rs:
    // `<service>/_request` + `<service>/_reply`.
    register::<request_topic>("/test/echo/_request", 0).expect("request topic register");
    register::<reply_topic>("/test/echo/_reply", 0).expect("reply topic register");

    // Open a session through the standard Rmw path so we exercise
    // the same code real users go through.
    let cfg = nros_rmw::RmwConfig {
        locator: "",
        mode: nros_rmw::SessionMode::Peer,
        domain_id: 0,
        node_name: "svc_test",
        namespace: "",
        properties: &[],
    };
    let mut session = UorbRmw.open(&cfg).expect("open session");

    let svc_info = nros_rmw::ServiceInfo {
        name: "/test/echo",
        type_name: "test::Echo",
        type_hash: "0",
        domain_id: 0,
        node_name: Some("svc_test"),
        namespace: "",
    };
    let mut server = session.create_service_server(&svc_info).expect("server");
    let mut client = session.create_service_client(&svc_info).expect("client");

    // Send a request.
    let request_payload: &[u8] = b"hello-uorb-service";
    client
        .send_request_raw(request_payload)
        .expect("send_request_raw");

    // Server polls the request.
    let mut server_buf = [0u8; UORB_SERVICE_TOPIC_BYTES];
    let req = server
        .try_recv_request(&mut server_buf)
        .expect("try_recv_request")
        .expect("request arrived");
    assert_eq!(req.data, request_payload);
    let seq = req.sequence_number;

    // Server echoes back.
    let reply_payload: &[u8] = b"echo:hello-uorb-service";
    server.send_reply(seq, reply_payload).expect("send_reply");

    // Client polls the reply.
    let mut client_buf = [0u8; UORB_SERVICE_TOPIC_BYTES];
    let len = client
        .try_recv_reply_raw(&mut client_buf)
        .expect("try_recv_reply_raw")
        .expect("reply arrived");
    assert_eq!(&client_buf[..len], reply_payload);
}

#[test]
fn reply_for_other_seq_is_ignored() {
    let _g = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    px4_uorb::_reset_broker();
    nros_rmw_uorb::_reset();

    register::<request_topic>("/test/echo/_request", 0).expect("request topic register");
    register::<reply_topic>("/test/echo/_reply", 0).expect("reply topic register");

    let cfg = nros_rmw::RmwConfig {
        locator: "",
        mode: nros_rmw::SessionMode::Peer,
        domain_id: 0,
        node_name: "svc_test",
        namespace: "",
        properties: &[],
    };
    let mut session = UorbRmw.open(&cfg).expect("open session");
    let svc_info = nros_rmw::ServiceInfo {
        name: "/test/echo",
        type_name: "test::Echo",
        type_hash: "0",
        domain_id: 0,
        node_name: Some("svc_test"),
        namespace: "",
    };
    let mut server = session.create_service_server(&svc_info).expect("server");
    let mut client = session.create_service_client(&svc_info).expect("client");

    // Client sends seq=1 (auto-assigned).
    client.send_request_raw(b"req-1").expect("send req-1");

    // Server "replies" with a seq the client never sent.
    server
        .send_reply(0xdead_beef, b"stale-reply")
        .expect("send stale reply");

    // Client should NOT see the stale reply.
    let mut buf = [0u8; UORB_SERVICE_TOPIC_BYTES];
    let result = client
        .try_recv_reply_raw(&mut buf)
        .expect("try_recv_reply_raw");
    assert!(
        result.is_none(),
        "client must not accept replies tagged with a seq it did not send"
    );
}
