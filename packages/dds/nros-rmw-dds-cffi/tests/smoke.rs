//! Phase 115.L.1 — end-to-end smoke test for dust-DDS via cffi.
//!
//! `register_returns_ok` + `vtable_open_close_round_trip` cover the
//! register entry. `cffi_pubsub_round_trip` opens a `CffiSession`,
//! creates publisher+subscriber via the trait, publishes raw bytes,
//! and confirms the subscriber sees them after dust-dds discovery
//! converges. Uses domain id 211 so parallel test invocations don't
//! cross-talk with other workspace tests.

#![cfg(feature = "platform-posix")]

use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

use nros_rmw::{
    Publisher, QosSettings, ServiceClientTrait, ServiceInfo, ServiceServerTrait, Session,
    Subscriber, TopicInfo,
};
use nros_rmw_cffi::{CffiSession, NROS_RMW_RET_OK, NrosRmwSession, RustBackendAdapter};
use nros_rmw_dds::DdsRmw;

#[test]
fn dds_cffi_register_returns_ok() {
    let rc = nros_rmw_dds_cffi::register();
    assert!(rc.is_ok(), "register failed: {rc:?}");
}

#[test]
fn dds_vtable_open_close_round_trip() {
    // Same VTABLE the runtime sees post-register. Directly drive it.
    let vt = &RustBackendAdapter::<DdsRmw>::VTABLE;
    let mut sess = NrosRmwSession {
        node_name: b"test-node\0".as_ptr(),
        namespace_: b"/\0".as_ptr(),
        _reserved: [0u8; 8],
        backend_data: core::ptr::null_mut(),
    };
    // dust-dds's `DdsRmw::open` only cares about `domain_id` on
    // POSIX/std; the locator is ignored. Use a unique domain id per
    // test so parallel runs don't trip on each other's discovery.
    let rc = unsafe {
        (vt.open)(
            b"\0".as_ptr(),
            0,
            201, /* test-only domain id */
            b"test-node\0".as_ptr(),
            &mut sess,
        )
    };
    assert_eq!(rc, NROS_RMW_RET_OK, "DdsRmw open returned {rc}");
    assert!(!sess.backend_data.is_null());

    let rc = unsafe { (vt.close)(&mut sess) };
    assert_eq!(rc, NROS_RMW_RET_OK);
}

/// End-to-end: `register` → two `CffiSession`s on the same domain →
/// publisher in one, subscriber in the other → `publish_raw` →
/// `try_recv_raw` reads the bytes back.
///
/// dust-dds is brokerless (SPDP/SEDP over UDP multicast on lo) so no
/// agent / router is required. Two participants on the same domain
/// discover each other inside the loopback multicast group.
///
/// Why two sessions: `nros-rmw-dds`'s `Session::create_publisher` and
/// `Session::create_subscriber` both call
/// `DomainParticipant::create_topic` for the topic name, and stock
/// dust-dds rejects a second `create_topic` call with the same name
/// on the same participant. Using two participants sidesteps that
/// limitation — and matches the realistic pub-from-one-node /
/// sub-from-another-node ROS shape.
///
/// Discovery typically converges within 1–2 s on POSIX; the test
/// budgets up to 10 s before declaring a failure.
#[test]
fn cffi_pubsub_round_trip() {
    nros_rmw_dds_cffi::register().expect("register");

    // Domain id chosen out of band to avoid clashing with the other
    // workspace tests that hardcode domains 200–210.
    const DOMAIN: u32 = 211;

    let mut pub_session = CffiSession::open("", 0, DOMAIN, "l1_pub").expect("open pub");
    let mut sub_session = CffiSession::open("", 0, DOMAIN, "l1_sub").expect("open sub");
    let topic = TopicInfo::new(
        "/nros/test/cffi_pubsub",
        "std_msgs::msg::dds_::String_",
        "RIHS01_cffi_pubsub",
    )
    .with_domain(DOMAIN);
    let qos = QosSettings::default();

    let mut subscriber = sub_session
        .create_subscriber(&topic, qos)
        .expect("create_subscriber");
    let publisher = pub_session
        .create_publisher(&topic, qos)
        .expect("create_publisher");

    // Payload: CDR header (4B) + u32 = 0xdeadbeef.
    let payload: [u8; 12] = [
        0x00, 0x01, 0x00, 0x00, // CDR header: little-endian + opts
        0xef, 0xbe, 0xad, 0xde, // u32 LE
        0, 0, 0, 0, // align
    ];

    let deadline = Instant::now() + Duration::from_secs(10);
    let mut got: Option<usize> = None;
    let mut buf = [0u8; 256];
    while Instant::now() < deadline {
        // Re-publish each loop iteration. dust-dds publishers may
        // drop early samples before the matched-reader callback
        // fires, so repeated send is the standard pattern.
        let _ = publisher.publish_raw(&payload);
        std::thread::sleep(Duration::from_millis(100));
        match subscriber.try_recv_raw(&mut buf) {
            Ok(Some(n)) if n > 0 => {
                got = Some(n);
                break;
            }
            _ => continue,
        }
    }
    drop(publisher);
    drop(subscriber);
    drop(pub_session);
    drop(sub_session);

    let n = got.expect("subscriber received no data within 10 s");
    assert!(
        n >= payload.len(),
        "got {n} bytes, expected ≥ {}",
        payload.len()
    );
    // First 4 bytes are the CDR-encapsulation header; bytes [4..8]
    // must match the u32 payload we serialised.
    assert_eq!(&buf[4..8], &payload[4..8], "received payload mismatch");
}

/// Service round-trip via the C vtable: server in one participant,
/// client in another, both opened through `CffiSession`. Client
/// `call_raw` is forwarded to `DdsServiceClient::call_raw` which
/// drives `send_request_raw` + polls `try_recv_reply_raw`; server
/// runs in a background thread polling `try_recv_request` and
/// echoing the bytes back via `send_reply`.
///
/// Uses a fresh domain id so the test doesn't cross-talk with the
/// pubsub round-trip above.
#[test]
fn cffi_service_round_trip() {
    nros_rmw_dds_cffi::register().expect("register");
    const DOMAIN: u32 = 212;

    let mut server_session = CffiSession::open("", 0, DOMAIN, "l1_srv").expect("open srv");
    let mut client_session = CffiSession::open("", 0, DOMAIN, "l1_cli").expect("open cli");
    let info = ServiceInfo {
        name: "/nros/test/cffi_service",
        type_name: "test::srv::dds_::Echo_",
        type_hash: "RIHS01_cffi_service",
        domain_id: DOMAIN,
        node_name: None,
        namespace: "/",
    };

    let server = server_session
        .create_service_server(&info)
        .expect("create_service_server");
    let mut client = client_session
        .create_service_client(&info)
        .expect("create_service_client");

    // CffiServiceServer holds a `*mut c_void` (backend_data) so the
    // auto-trait analysis flags it as !Send. We move the value once
    // at spawn time and never touch it from the main thread again,
    // which is sound. Use a single-field newtype + explicit Send
    // impl AND mark the field MaybeUninit-style to convince rustc
    // the closure body never re-borrows the underlying pointer.
    //
    // Workaround for rustc's auto-trait propagation defeating the
    // outer Send: pass the server through a raw pointer so the
    // closure only captures `*mut SendServer` (which we Send-wrap).
    struct SendServer(nros_rmw_cffi::CffiServiceServer);
    unsafe impl Send for SendServer {}

    // Server-side reply loop. Echoes the request bytes back.
    let stop = Arc::new(AtomicBool::new(false));
    let stop_for_thread = Arc::clone(&stop);
    // Move into a Box so we can pass a raw pointer through to the
    // worker thread (the auto-Send analysis sees only a `usize` for
    // the pointer once we cast). The worker takes ownership back
    // via `Box::from_raw` and drops on its side.
    let boxed_server: Box<SendServer> = Box::new(SendServer(server));
    let server_ptr = Box::into_raw(boxed_server) as usize;
    let server_thread = thread::spawn(move || {
        // SAFETY: this is the only handle to the boxed server; main
        // thread never dereferences `server_ptr` after spawn.
        let boxed: Box<SendServer> = unsafe { Box::from_raw(server_ptr as *mut SendServer) };
        let SendServer(mut server) = *boxed;
        let mut buf = [0u8; 256];
        while !stop_for_thread.load(Ordering::SeqCst) {
            // try_recv_request borrows `buf` immutably for the
            // duration of the `Some(req)` arm so we can't reuse it
            // for send_reply. Copy out the seq + payload before
            // dropping the borrow.
            // Copy the request payload out of the borrowed buffer
            // immediately so we can re-use `server` for send_reply
            // afterwards (try_recv_request holds an immutable borrow
            // on `buf` for the duration of the returned struct).
            let mut got: Option<(i64, Vec<u8>)> = None;
            if let Ok(Some(req)) = server.try_recv_request(&mut buf) {
                got = Some((req.sequence_number, req.data.to_vec()));
            }
            if let Some((seq, payload)) = got {
                let _ = server.send_reply(seq, &payload);
            } else {
                thread::sleep(Duration::from_millis(50));
            }
        }
        // Drop the server inside the worker thread; nothing to
        // return (the destructor handles cleanup via the cffi
        // vtable's destroy_service_server entry).
        drop(server);
    });

    // Build the request payload (CDR header + u32).
    let request: [u8; 8] = [
        0x00, 0x01, 0x00, 0x00, // CDR header LE
        0xef, 0xbe, 0xad, 0xde, // u32 LE
    ];
    let mut reply_buf = [0u8; 64];

    // Drive call_raw until reply arrives or the budget expires.
    let deadline = Instant::now() + Duration::from_secs(15);
    let mut reply_len: Option<usize> = None;
    while Instant::now() < deadline {
        // `call_raw` is the trait's deprecated blocking path. The
        // cffi vtable still exposes a blocking call entry for C
        // consumers that don't have an executor; the test follows
        // suit so it exercises the matching trampoline.
        #[allow(deprecated)]
        match client.call_raw(&request, &mut reply_buf) {
            Ok(n) if n > 0 => {
                reply_len = Some(n);
                break;
            }
            _ => thread::sleep(Duration::from_millis(50)),
        }
    }

    stop.store(true, Ordering::SeqCst);
    let _ = server_thread.join();
    drop(client);
    drop(client_session);
    drop(server_session);

    let n = reply_len.expect("client did not receive reply within 15 s");
    assert!(
        n >= request.len(),
        "reply len {n} < request len {}",
        request.len()
    );
    // Echo server: bytes [4..8] should match the request payload.
    assert_eq!(&reply_buf[4..8], &request[4..8], "reply payload mismatch");
}
