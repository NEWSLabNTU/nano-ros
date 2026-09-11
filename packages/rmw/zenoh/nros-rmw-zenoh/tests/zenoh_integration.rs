//! Integration tests for zenoh transport
//!
//! Run with:
//! cargo test --features platform-posix -p nros-rmw-zenoh

#![cfg(feature = "platform-posix")]

use nros_rmw::{
    Publisher, QoSProfile, ServiceInfo, Session, SessionMode, Subscription, TopicInfo, Transport,
    TransportConfig,
};
use nros_rmw_zenoh::{
    DEFAULT_LOCATOR, ZenohServiceServer, ZenohTransport, effective_client_locator,
    keyexpr::{ServiceKeyExpr, TopicKeyExpr},
    normalize_locator,
};
use nros_tests::fixtures::ZenohRouter;
use std::{thread, time::Duration};

/// Start a private zenohd on an EPHEMERAL port for one test (issue 0328).
///
/// These tests used to hardcode `tcp/127.0.0.1:7447` and carry
/// `#[ignore = "requires zenohd router on tcp/127.0.0.1:7447"]`, so they never
/// ran in any lane. The precondition is self-provisioning:
/// `ZenohRouter::start_unique()` picks a free port, reaps an orphaned router
/// from a previous run, and shuts down on `Drop`.
///
/// An ephemeral port is also the CORRECT choice here rather than a slot from
/// `nros_tests::alloc` — that allocator is for baked-isolation cells, whose
/// images compile a locator in. Its own docs say native host tests should take
/// runtime-ephemeral ports instead, which are parallel-safe by construction.
///
/// Returns `None` when zenohd is absent so the test can skip rather than fail
/// on a machine that never provisioned it.
fn router() -> Option<ZenohRouter> {
    if let Some(why) = nros_tests::process::zenohd_unavailable_reason() {
        eprintln!("[SKIP] {why}");
        return None;
    }
    Some(ZenohRouter::start_unique().expect("failed to start zenohd"))
}

/// Test that we can open and close a session in peer mode
/// (doesn't require a router).
/// Multicast scouting is disabled to avoid contention under parallel test load.
/// Issue 0682 — what a PEER-mode `open()` must do, given how the shim was built.
///
/// These tests used to accept either outcome ("connection failure is acceptable
/// in CI/test environments"), which made them unable to fail: peer mode has
/// never worked in a nano-ros build and they reported green throughout. The
/// answer is a compile-time fact, so there is ONE place that states it.
fn assert_peer_open_matches_build(
    result: Result<<ZenohTransport as nros_rmw::Transport>::Session, nros_rmw::TransportError>,
) {
    if nros_rmw_zenoh::zpico::ZPICO_PEER_MODE_SUPPORTED {
        let mut session = result
            .expect("the shim is compiled WITH multicast transport, so a peer session must open");
        assert!(session.is_open(), "Session should be open");
        assert!(session.close().is_ok(), "Failed to close session");
    } else {
        assert!(
            matches!(result, Err(nros_rmw::TransportError::Unsupported)),
            "peer mode is compiled out, so open() must be REFUSED with Unsupported \
             rather than attempted and failed as a transport error — got {:?}",
            result.map(|_| "Ok(session)")
        );
    }
}

#[test]
fn test_session_open_close_peer() {
    let config = TransportConfig {
        locator: None,
        mode: SessionMode::Peer,
        properties: &[("multicast_scouting", "false")],
        node_name: "",
        namespace: "",
        domain_id: 0,
    };

    let result = ZenohTransport::open(&config);
    assert_peer_open_matches_build(result);
}

/// Test topic info generation
#[test]
fn test_topic_info_key_generation() {
    let topic = TopicInfo::new("/chatter", "std_msgs::msg::dds_::Int32_", "abc123def456");

    let key: heapless::String<256> = topic.to_key();

    assert!(key.contains("chatter"), "Key should contain topic name");
    assert!(
        key.contains("std_msgs::msg::dds_::Int32_"),
        "Key should contain type name"
    );
    // For Humble, data keyexprs use TypeHashNotSupported (liveliness uses RIHS01_ prefix)
    assert!(
        key.contains("TypeHashNotSupported"),
        "Key should use TypeHashNotSupported for Humble"
    );
}

/// Test CDR message format for Int32
#[test]
fn test_cdr_int32_format() {
    // CDR little-endian format for Int32 with value 42
    let cdr_msg: [u8; 8] = [
        0x00, 0x01, 0x00, 0x00, // CDR encapsulation header (LE)
        0x2A, 0x00, 0x00, 0x00, // Int32: 42 (little-endian)
    ];

    assert_eq!(cdr_msg[0], 0x00, "First byte should be 0x00");
    assert_eq!(cdr_msg[1], 0x01, "Second byte should be 0x01 (LE)");

    let value = i32::from_le_bytes([cdr_msg[4], cdr_msg[5], cdr_msg[6], cdr_msg[7]]);
    assert_eq!(value, 42, "Decoded value should be 42");
}

/// Test full pub/sub cycle (requires working zenoh network)
/// This test requires a zenoh router running: ZENOH_CONFIG_OVERRIDE='listen/endpoints=["tcp/127.0.0.1:7447"];scouting/multicast/enabled=false' ros2 run rmw_zenoh_cpp rmw_zenohd
#[test]
fn test_pubsub_loopback() {
    let Some(_router) = router() else { return };
    let router_locator = _router.locator();
    // Connect to router as client
    let config = TransportConfig {
        locator: Some(router_locator.as_str()),
        mode: SessionMode::Client,
        properties: &[],
        node_name: "",
        namespace: "",
        domain_id: 0,
    };

    let mut session = match ZenohTransport::open(&config) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Could not open session: {:?}", e);
            // The inner quotes are part of the zenoh config value and must be
            // ESCAPED — unescaped they close the Rust literal, which is how this
            // line arrived as a syntax error (#0654's rewrite).
            eprintln!(
                "Start a router with: ZENOH_CONFIG_OVERRIDE='listen/endpoints=[\"{}\"];scouting/multicast/enabled=false' ros2 run rmw_zenoh_cpp rmw_zenohd",
                router_locator
            );
            panic!("Failed to connect to zenoh router");
        }
    };

    // Create topic with simple key for testing
    let topic = TopicInfo::new("test/loopback", "Int32", "hash123");

    // Create subscriber first
    let mut subscriber = session
        .create_subscription(&topic, QoSProfile::BEST_EFFORT)
        .expect("Failed to create subscriber");

    // Wait for subscriber to be established
    thread::sleep(Duration::from_secs(1));

    // Create publisher
    let publisher = session
        .create_publisher(&topic, QoSProfile::BEST_EFFORT)
        .expect("Failed to create publisher");

    // Publish a CDR-encoded Int32 message
    let test_value: i32 = 12345;
    let cdr_msg: [u8; 8] = [
        0x00,
        0x01,
        0x00,
        0x00, // CDR header (LE)
        (test_value & 0xFF) as u8,
        ((test_value >> 8) & 0xFF) as u8,
        ((test_value >> 16) & 0xFF) as u8,
        ((test_value >> 24) & 0xFF) as u8,
    ];

    publisher
        .publish_raw(&cdr_msg)
        .expect("Failed to publish message");

    // Wait for message to arrive
    thread::sleep(Duration::from_secs(2));

    // Try to receive
    let mut recv_buf = [0u8; 64];
    match subscriber.take_serialized(&mut recv_buf) {
        Ok(Some(len)) => {
            assert_eq!(len, 8, "Message length should be 8 bytes");

            // Verify CDR header
            assert_eq!(recv_buf[0], 0x00);
            assert_eq!(recv_buf[1], 0x01);

            // Verify value
            let received_value =
                i32::from_le_bytes([recv_buf[4], recv_buf[5], recv_buf[6], recv_buf[7]]);
            assert_eq!(
                received_value, test_value,
                "Received value should match sent value"
            );

            println!("Successfully received message: {}", received_value);
        }
        Ok(None) => {
            panic!("No message received");
        }
        Err(e) => {
            panic!("Error receiving message: {:?}", e);
        }
    }

    session.close().expect("Failed to close session");
}

/// phase-328 (issue 0348) — two independent zpico sessions in ONE process
/// deliver across each other through the router.
///
/// This replaces `second_session_open_in_one_process_is_refused` (issue 0347's
/// honest stop-gap). 0347 made a second `ZenohTransport::open` FAIL rather than
/// memset the first session's registration tables out from under it; 0348 adds
/// the actual capability. `zpico.c`'s `g_session` and every per-session `g_*`
/// table now live inside a pooled `zpico_session_t` (`ZPICO_MAX_SESSIONS`,
/// default 1), and every `zpico_*` entry point takes a handle — so a second
/// open takes its own pool slot instead of clobbering the first.
///
/// Proof of independence: session A holds the SUBSCRIBER, session B (opened
/// second) holds the PUBLISHER, on the SAME topic + domain through the router.
/// Under the old global-state shim the second open would have wiped A's
/// subscriber registration and delivery would fail; with per-session state it
/// succeeds, exactly as two separate processes do (verified 11/11 with the
/// stock talker/listener).
///
/// Requires the shim built with `ZPICO_MAX_SESSIONS >= 2`. When it is 1 (the
/// default), the second open returns `Err` (pool exhausted) and the test skips
/// rather than failing — rebuild with `ZPICO_MAX_SESSIONS=2` to exercise it.
#[test]
fn two_sessions_deliver_cross_session_through_router() {
    let Some(_router) = router() else { return };
    let router_locator = _router.locator();
    let config = TransportConfig {
        locator: Some(router_locator.as_str()),
        mode: SessionMode::Client,
        properties: &[],
        node_name: "",
        namespace: "",
        domain_id: 0,
    };

    // Session A — opened first, owns the subscriber.
    let mut session_a = ZenohTransport::open(&config).expect("first session should open");

    // Session B — opened SECOND. Under ZPICO_MAX_SESSIONS=1 this is refused
    // (the 0347 contract); skip rather than fail so the default single-session
    // build stays green.
    let mut session_b = match ZenohTransport::open(&config) {
        Ok(s) => s,
        Err(_) => {
            nros_tests::skip!(
                "second session refused — shim built with ZPICO_MAX_SESSIONS=1; \
                 rebuild with ZPICO_MAX_SESSIONS=2 to exercise multi-session"
            );
        }
    };

    let topic = TopicInfo::new("test/cross_session", "Int32", "hash348");

    // Subscriber on session A, established BEFORE session B publishes.
    let mut subscriber = session_a
        .create_subscription(&topic, QoSProfile::BEST_EFFORT)
        .expect("Failed to create subscriber on session A");
    thread::sleep(Duration::from_secs(2));

    // Publisher on session B.
    let publisher = session_b
        .create_publisher(&topic, QoSProfile::BEST_EFFORT)
        .expect("Failed to create publisher on session B");

    let test_value: i32 = 348;
    let cdr_msg: [u8; 8] = [
        0x00,
        0x01,
        0x00,
        0x00, // CDR header (LE)
        (test_value & 0xFF) as u8,
        ((test_value >> 8) & 0xFF) as u8,
        ((test_value >> 16) & 0xFF) as u8,
        ((test_value >> 24) & 0xFF) as u8,
    ];

    // Cross-session delivery goes through the router (not the same-session
    // local-loopback path), so allow discovery/matching to settle: republish
    // and poll for up to ~10 s.
    let mut recv_buf = [0u8; 64];
    let mut received: Option<i32> = None;
    for attempt in 0..20 {
        publisher
            .publish_raw(&cdr_msg)
            .expect("Failed to publish from session B");
        thread::sleep(Duration::from_millis(500));
        match subscriber.take_serialized(&mut recv_buf) {
            Ok(Some(len)) => {
                assert_eq!(len, 8, "cross-session message length should be 8 bytes");
                received = Some(i32::from_le_bytes([
                    recv_buf[4],
                    recv_buf[5],
                    recv_buf[6],
                    recv_buf[7],
                ]));
                eprintln!("[cross-session] delivered on attempt {attempt}");
                break;
            }
            Ok(None) => continue,
            Err(e) => panic!("Error receiving on session A: {:?}", e),
        }
    }

    match received {
        Some(v) => assert_eq!(
            v, test_value,
            "session A must receive what session B published — the two \
             sessions are independent (issue 0348)"
        ),
        None => panic!(
            "session A received nothing after 20 attempts — a second open wiped \
             its subscriber, or two in-process sessions do not route through the \
             router (the pre-0348 global-state failure)"
        ),
    }

    session_a.close().expect("Failed to close session A");
    session_b.close().expect("Failed to close session B");
}

/// Test multiple publishers on same session
#[test]
fn test_multiple_publishers() {
    let Some(_router) = router() else { return };
    let router_locator = _router.locator();
    let config = TransportConfig {
        locator: Some(router_locator.as_str()),
        mode: SessionMode::Client,
        properties: &[],
        node_name: "",
        namespace: "",
        domain_id: 0,
    };

    let mut session = match ZenohTransport::open(&config) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Could not open session: {:?}", e);
            panic!("Failed to connect to zenoh router");
        }
    };

    let topic1 = TopicInfo::new("test/pub1", "Int32", "hash1");
    let topic2 = TopicInfo::new("test/pub2", "Int32", "hash2");

    let _pub1 = session
        .create_publisher(&topic1, QoSProfile::BEST_EFFORT)
        .expect("Failed to create publisher 1");

    let _pub2 = session
        .create_publisher(&topic2, QoSProfile::BEST_EFFORT)
        .expect("Failed to create publisher 2");

    session.close().expect("Failed to close session");
}

/// Test multiple subscribers on same session
#[test]
fn test_multiple_subscribers() {
    let Some(_router) = router() else { return };
    let router_locator = _router.locator();
    let config = TransportConfig {
        locator: Some(router_locator.as_str()),
        mode: SessionMode::Client,
        properties: &[],
        node_name: "",
        namespace: "",
        domain_id: 0,
    };

    let mut session = match ZenohTransport::open(&config) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Could not open session: {:?}", e);
            panic!("Failed to connect to zenoh router");
        }
    };

    let topic1 = TopicInfo::new("test/sub1", "Int32", "hash1");
    let topic2 = TopicInfo::new("test/sub2", "Int32", "hash2");

    let _sub1 = session
        .create_subscription(&topic1, QoSProfile::BEST_EFFORT)
        .expect("Failed to create subscriber 1");

    let _sub2 = session
        .create_subscription(&topic2, QoSProfile::BEST_EFFORT)
        .expect("Failed to create subscriber 2");

    session.close().expect("Failed to close session");
}

// =============================================================================
// Transport Configuration Properties Tests
// =============================================================================

/// Test TransportConfig with properties field
#[test]
fn test_transport_config_with_properties() {
    let props: &[(&str, &str)] = &[
        ("multicast_scouting", "false"),
        ("scouting_timeout_ms", "1000"),
    ];

    let config = TransportConfig {
        // Never connected — this test only asserts the struct's fields, so a
        // literal locator is right and no router is needed.
        locator: Some("tcp/127.0.0.1:7447"),
        mode: SessionMode::Client,
        properties: props,
        node_name: "",
        namespace: "",
        domain_id: 0,
    };

    assert_eq!(config.properties.len(), 2);
    assert_eq!(config.properties[0], ("multicast_scouting", "false"));
    assert_eq!(config.properties[1], ("scouting_timeout_ms", "1000"));
}

/// Test TransportConfig default has empty properties
#[test]
fn test_transport_config_default_has_empty_properties() {
    let config = TransportConfig::default();
    assert!(config.properties.is_empty());
    assert!(config.locator.is_none());
}

/// Test that a peer session opens with multicast_scouting disabled
///
/// This verifies that properties are passed through the FFI boundary
/// without crashing. Peer mode doesn't require a router.
#[test]
fn test_session_open_peer_with_scouting_disabled() {
    let config = TransportConfig {
        locator: None,
        mode: SessionMode::Peer,
        properties: &[("multicast_scouting", "false")],
        node_name: "",
        namespace: "",
        domain_id: 0,
    };

    let result = ZenohTransport::open(&config);
    assert_peer_open_matches_build(result);
}

/// Test that a peer session opens with ZENOH_MULTICAST_SCOUTING env var
///
/// Verifies that env vars are read and passed through to zenoh-pico
/// without crashing. Peer mode doesn't require a router.
#[test]
fn test_session_open_with_env_scouting_disabled() {
    // Safety: test-only env var manipulation, tests run serially via nextest
    unsafe { std::env::set_var("ZENOH_MULTICAST_SCOUTING", "false") };

    let config = TransportConfig {
        locator: None,
        mode: SessionMode::Peer,
        properties: &[], // Empty — env var should fill in
        node_name: "",
        namespace: "",
        domain_id: 0,
    };

    let result = ZenohTransport::open(&config);
    // Issue 0682 — peer mode is refused BEFORE the property/env plumbing runs, so
    // this no longer proves the env var reaches zenoh-pico; it proves the refusal.
    // Still better than the old accept-anything arm, and the gap is recorded
    // rather than papered over: env/property precedence needs a CLIENT-mode test.
    assert_peer_open_matches_build(result);

    unsafe { std::env::remove_var("ZENOH_MULTICAST_SCOUTING") };
}

/// Test that explicit properties take precedence over ZENOH_* env vars
#[test]
fn test_session_explicit_props_override_env() {
    // Safety: test-only env var manipulation, tests run serially via nextest
    unsafe { std::env::set_var("ZENOH_MULTICAST_SCOUTING", "true") };

    // But explicitly set to "false" via properties
    let config = TransportConfig {
        locator: None,
        mode: SessionMode::Peer,
        properties: &[("multicast_scouting", "false")],
        node_name: "",
        namespace: "",
        domain_id: 0,
    };

    let result = ZenohTransport::open(&config);
    // Issue 0682 — same as above: the refusal precedes the property merge, so this
    // asserts the refusal; precedence coverage is follow-up work.
    assert_peer_open_matches_build(result);

    unsafe { std::env::remove_var("ZENOH_MULTICAST_SCOUTING") };
}

/// Test pub/sub loopback with multicast_scouting disabled
///
/// This proves that the multicast_scouting property actually reaches
/// zenoh-pico: with scouting disabled, the client only connects to the
/// specified router (no multicast discovery). Communication still works
/// because we explicitly provide the router locator.
#[test]
fn test_pubsub_loopback_with_scouting_disabled() {
    let Some(_router) = router() else { return };
    let router_locator = _router.locator();
    let config = TransportConfig {
        locator: Some(router_locator.as_str()),
        mode: SessionMode::Client,
        properties: &[("multicast_scouting", "false")],
        node_name: "",
        namespace: "",
        domain_id: 0,
    };

    let mut session = match ZenohTransport::open(&config) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Could not open session: {:?}", e);
            // The inner quotes are part of the zenoh config value and must be
            // ESCAPED — unescaped they close the Rust literal, which is how this
            // line arrived as a syntax error (#0654's rewrite).
            eprintln!(
                "Start a router with: ZENOH_CONFIG_OVERRIDE='listen/endpoints=[\"{}\"];scouting/multicast/enabled=false' ros2 run rmw_zenoh_cpp rmw_zenohd",
                router_locator
            );
            panic!("Failed to connect to zenoh router");
        }
    };

    let topic = TopicInfo::new("test/props-loopback", "Int32", "hash_props");

    let mut subscriber = session
        .create_subscription(&topic, QoSProfile::BEST_EFFORT)
        .expect("Failed to create subscriber");

    thread::sleep(Duration::from_secs(1));

    let publisher = session
        .create_publisher(&topic, QoSProfile::BEST_EFFORT)
        .expect("Failed to create publisher");

    // Publish a CDR-encoded Int32 message
    let test_value: i32 = 99999;
    let cdr_msg: [u8; 8] = [
        0x00,
        0x01,
        0x00,
        0x00, // CDR header (LE)
        (test_value & 0xFF) as u8,
        ((test_value >> 8) & 0xFF) as u8,
        ((test_value >> 16) & 0xFF) as u8,
        ((test_value >> 24) & 0xFF) as u8,
    ];

    publisher
        .publish_raw(&cdr_msg)
        .expect("Failed to publish message");

    thread::sleep(Duration::from_secs(2));

    let mut recv_buf = [0u8; 64];
    match subscriber.take_serialized(&mut recv_buf) {
        Ok(Some(len)) => {
            assert_eq!(len, 8, "Message length should be 8 bytes");
            let received_value =
                i32::from_le_bytes([recv_buf[4], recv_buf[5], recv_buf[6], recv_buf[7]]);
            assert_eq!(
                received_value, test_value,
                "Received value should match sent value"
            );
            println!(
                "SUCCESS: Pub/sub works with scouting disabled, received: {}",
                received_value
            );
        }
        Ok(None) => {
            panic!("No message received (scouting disabled should not affect client-router path)");
        }
        Err(e) => {
            panic!("Error receiving message: {:?}", e);
        }
    }

    session.close().expect("Failed to close session");
}

// =============================================================================
// Issue 0330 — the backend owns the default locator
// =============================================================================

/// Pure unit coverage of the normalization contract: `None` and `""` are the
/// SAME thing ("caller supplied nothing"), and only this crate turns that into
/// a concrete endpoint.
///
/// These do NOT prove the session path applies it — that is what
/// `client_session_with_absent_locator_dials_backend_default` below is for.
#[test]
fn absent_locator_normalizes_to_the_backend_default() {
    assert_eq!(normalize_locator(None), None);
    assert_eq!(normalize_locator(Some("")), None, "empty string == absent");
    assert_eq!(
        normalize_locator(Some("tcp/1.2.3.4:1234")),
        Some("tcp/1.2.3.4:1234")
    );

    // This is a hosted build (`target_os != "none"`), so the default applies.
    // On an embedded image `effective_client_locator` yields `None` — dialling
    // the board's own loopback would be strictly worse than zenoh-pico's
    // multicast scouting, which is what "no endpoint" gets you.
    assert_eq!(effective_client_locator(None), Some(DEFAULT_LOCATOR));
    assert_eq!(effective_client_locator(Some("")), Some(DEFAULT_LOCATOR));
    assert_eq!(
        effective_client_locator(Some("tcp/1.2.3.4:1234")),
        Some("tcp/1.2.3.4:1234"),
        "an explicitly supplied locator must never be overridden"
    );
}

/// Parse the TCP port out of [`DEFAULT_LOCATOR`] (`tcp/<host>:<port>`), so the
/// test tracks the const instead of restating the literal it is guarding.
fn default_locator_port() -> u16 {
    DEFAULT_LOCATOR
        .rsplit(':')
        .next()
        .and_then(|p| p.parse().ok())
        .unwrap_or_else(|| panic!("DEFAULT_LOCATOR {DEFAULT_LOCATOR:?} has no `:<port>` suffix"))
}

/// Issue 0330 — **the** regression test for the rung this issue moved.
///
/// Every other hosted test in the tree pins `NROS_LOCATOR` or passes an
/// explicit router locator, so none of them touch the bottom rung. Here the
/// caller supplies NOTHING (`locator: None`, and the two locator env vars are
/// removed) and a router is started on the DEFAULT port. The session must
/// still open — which can only happen if the zenoh backend substituted
/// [`DEFAULT_LOCATOR`] itself.
///
/// Why "session opened" is the right assertion and delivery is not: same-session
/// pub/sub is served by zenoh-pico's local loopback
/// (`Z_FEATURE_LOCAL_SUBSCRIBER=1` on host builds), so a delivery check would
/// pass even with a dead endpoint (see the note on
/// `two_sessions_deliver_cross_session_through_router`). `ZenohTransport::open`, by
/// contrast, only returns `Ok` after zenoh-pico completed a TCP connect +
/// session handshake against the endpoint it was configured with. Remove the
/// substitution and this test fails: `zpico_init_with_config` gets no connect
/// endpoint (or a bare `""` one) and the open errors out.
///
/// Unlike the rest of this file, this test cannot use an ephemeral port — the
/// value under test IS the default port. It probes first and skips (rather than
/// stomping) if something else on this host already holds it.
#[test]
fn client_session_with_absent_locator_dials_backend_default() {
    if let Some(why) = nros_tests::process::zenohd_unavailable_reason() {
        nros_tests::skip!("{why}");
    }

    let port = default_locator_port();

    // The default port is shared-host territory. Confirm it is free before
    // `ZenohRouter::start` (which reaps whatever is listening) so this test
    // never kills an unrelated router.
    match std::net::TcpListener::bind(("127.0.0.1", port)) {
        Ok(probe) => drop(probe),
        // issue 1161 — `resource`, not the `capability` a bare `skip!` defaults
        // to. A port already bound is a RUNTIME prerequisite that a rerun or a
        // quieter host resolves; a capability is something this host cannot do
        // at all, and the two have opposite remedies. The distinction became
        // load-bearing when an undeclared capability skip started failing the
        // budget: mislabelled, this would have demanded a baseline entry for a
        // condition nobody can provision away.
        Err(e) => nros_tests::skip_class!(
            resource,
            "default locator port {port} (from {DEFAULT_LOCATOR}) is already in use \
             by something else on this host: {e}"
        ),
    }

    // The rung under test is "nothing supplied anywhere" — make sure the env
    // overlay really is absent (this process only; nextest gives each test its
    // own process).
    // SAFETY: single-threaded test setup, before any session or extra thread
    // is created.
    unsafe {
        std::env::remove_var("NROS_LOCATOR");
        std::env::remove_var("ZENOH_LOCATOR");
    }

    let _router = ZenohRouter::start(port).expect("failed to start zenohd on the default port");

    let config = TransportConfig {
        // Nothing supplied. The backend — and ONLY the backend — decides.
        locator: None,
        mode: SessionMode::Client,
        properties: &[],
        node_name: "",
        namespace: "",
        domain_id: 0,
    };

    let mut session = ZenohTransport::open(&config).unwrap_or_else(|e| {
        panic!(
            "client session with NO locator must fall back to the backend default \
             ({DEFAULT_LOCATOR}) and connect to the router on port {port}; got {e:?}"
        )
    });

    session.close().expect("Failed to close session");
}

// ============================================================================
// issue 0902 / issue 1332 — the declined-query probe
// ============================================================================

/// The service the probe declares. Nothing answers on this key but the server
/// under test, and the key is built by the same [`ServiceKeyExpr`] the server
/// uses — a hand-spelled key that MISSES is indistinguishable from a fix that
/// works, which is the failure mode issue 1332 is about.
const PROBE_SERVICE: ServiceInfo<'static> = ServiceInfo::new(
    "/nros_reply_slot_decline_probe",
    "example_interfaces/srv/AddTwoInts",
    "TypeHashNotSupported",
);

/// How long a probe query waits for a reply that will never come.
///
/// The server DECLINES these, so every one of them ends in the client-side
/// timeout; short enough that `capacity + 2` of them cost under a second.
const PROBE_QUERY_TIMEOUT_MS: u32 = 200;

/// issue 0902 / issue 1332 — a query the service callback DECLINES gives its
/// reply slot back, and the probe proves the declined arm was reached at all.
///
/// ## What this controls, and why the obvious test could not
///
/// zenoh-pico clones a query into one of `ZPICO_MAX_PENDING_REPLIES` slots
/// BEFORE the user callback runs. `shim/service.rs`'s callback returns early on
/// two ordinary paths — an empty payload (what a liveliness/discovery probe
/// looks like through a queryable) and a full request ring — and before
/// `1a032a10b` the first of those did not return the slot. Four such queries and
/// the table was gone for the life of the queryable: every later reply failed
/// silently, which is issue 0902's 20-90 % goal-completion spread.
///
/// Issue 1332 measured that the native lane cannot exercise this AT ALL. Graph
/// discovery here is a liveliness SUBSCRIBER (phase-381 / issue 0903), and a
/// subscriber delivers samples, not queries; both action client paths are
/// single-in-flight. So no declined query ever reaches a queryable, and the
/// refusal counter reads zero on a tree WITH the fix, on a tree with the fix
/// REVERTED, and even with a ONE-slot table. A green that cannot fail.
///
/// This test builds the missing population instead of hoping for it: it sends
/// `capacity + 2` EMPTY-PAYLOAD queries to the server's own keyexpr — exactly
/// what the callback's liveliness arm sees — and then asks, in order:
///
/// 1. `declines + refusals >= started` — the queries REACHED the queryable.
///    Without this the assertions below are vacuous, which IS issue 1332.
/// 2. `held == 0` — no slot is still held. This is the leak, read directly, and
///    it is what goes red on a default build.
/// 3. `refusals == 0` — no allocation was refused.
/// 4. `started == attempts` — the querier's own pool never ran dry.
///
/// ## Measured, both directions (2026-09-12), with `1a032a10b`'s reclaim
/// removed as the negative control
///
/// | `ZPICO_MAX_PENDING_REPLIES` | tree | verdict | counters |
/// | ---: | --- | --- | --- |
/// | 4 (shipped) | reclaim present | PASS | `started=6 declines=6 held=0 refusals=0` |
/// | 4 (shipped) | reclaim REVERTED | FAIL | `started=4 declines=4 held=4 refusals=0` |
/// | 2 | reclaim present | PASS | `started=4 declines=4 held=0 refusals=0` |
/// | 2 | reclaim REVERTED | FAIL | `started=4 declines=2 held=2 refusals=2` |
///
/// `refusals` does NOT move on the shipped build, and that is a property of the
/// sizes rather than of the fix: `ZPICO_MAX_PENDING_REPLIES` (4) equals the
/// querier's `ZPICO_MAX_PENDING_GETS` (4), and a leaked slot holds the cloned
/// query for ever — so it never finalises, the querier's own slot is never
/// released, and the pool runs dry one query short of a refusal. `held` is the
/// observable there; `refusals` is what a REAL graph, whose queries come from
/// other processes with pools of their own, would show.
///
/// ## What this does NOT reach
///
/// The RING-FULL arm. An empty payload returns above the ring check, so the
/// request ring stays empty. And at the shipped sizes that arm cannot leak
/// anyway: `SERVICE_REQUEST_RING_DEPTH` (4) EQUALS `ZPICO_MAX_PENDING_REPLIES`
/// (4) and an enqueued request consumes one of each, so a query meeting a full
/// ring has already been refused a slot and holds nothing to reclaim. Measured
/// with a payload-carrying scratch variant: `declines=0 held=4 refusals=8` at
/// capacity 4, `declines=8 held=4 refusals=0` at capacity 8. Nor `b56e3d50a`'s
/// arm, which is the FAILED-REPLY path inside `zpico_query_reply` — this probe
/// never makes the server reply at all.
#[test]
fn a_declined_query_hands_its_reply_slot_back() {
    // `skip!`, not this file's older `Option`-and-return `router()`: a bare
    // return reports PASS, and a probe whose whole purpose is to be a negative
    // control must never be able to pass by not running (CLAUDE.md; issue 0584
    // for the class).
    if let Some(why) = nros_tests::process::zenohd_unavailable_reason() {
        nros_tests::skip_class!(capability, "{why}");
    }
    let _router = ZenohRouter::start_unique().expect("failed to start zenohd");
    let router_locator = _router.locator();

    let config = TransportConfig {
        locator: Some(router_locator.as_str()),
        mode: SessionMode::Client,
        properties: &[],
        node_name: "",
        namespace: "",
        domain_id: 0,
    };
    let mut session = ZenohTransport::open(&config)
        .unwrap_or_else(|e| panic!("could not open a client session on {router_locator}: {e:?}"));

    // The REAL shim server, so the callback under test is the one that ships.
    let server = ZenohServiceServer::new(session.inner(), &PROBE_SERVICE, None)
        .expect("failed to declare the probe service server");
    let queryable = server.queryable_handle();

    let (held, refusals, capacity) = session
        .inner()
        .reply_slot_stats(queryable)
        .expect("the probe server's queryable handle must address a reply-slot table");
    assert_eq!(held, 0, "a freshly declared queryable holds no reply slot");
    assert_eq!(refusals, 0, "a freshly declared queryable refused nothing");
    assert!(capacity >= 1, "ZPICO_MAX_PENDING_REPLIES must be >= 1");

    // One MORE than the table holds, so a tree that never reclaims runs OUT
    // rather than merely filling up — the difference between a leak that is
    // latent and a leak that is observable.
    let attempts = capacity + 2;

    // The key the SERVER declared, derived the same way it derives it.
    let key: heapless::String<256> = PROBE_SERVICE.to_key();
    let mut keyexpr = [0u8; 257];
    assert!(key.len() < keyexpr.len(), "probe keyexpr does not fit");
    keyexpr[..key.len()].copy_from_slice(key.as_bytes());

    let mut reply = [0u8; 64];
    let mut started = 0u32;
    let mut finalised = 0u32;
    let mut start_error = None;

    for _ in 0..attempts {
        // An EMPTY payload is what makes this a DECLINED query: the callback's
        // first early return (`shim/service.rs`) drops exactly this shape.
        //
        // A start FAILURE is not fatal here, and the reason is the defect
        // itself: a leaked slot holds the cloned query for ever, so the query
        // never finalises, so the CLIENT's own pending-get slot is never
        // released either and the pool runs dry a few queries in. That is the
        // leak arriving one layer out, not a separate fault — record it and let
        // the reply-slot assertions below say what happened.
        let handle = match session
            .inner()
            .get_start(&keyexpr, &[], PROBE_QUERY_TIMEOUT_MS)
        {
            Ok(h) => h,
            Err(e) => {
                start_error = Some(e);
                break;
            }
        };
        started += 1;

        // Best-effort drain. A declined query finalises as soon as the server
        // drops it, which is what makes the client slot reusable; if it does
        // not, move on rather than asserting on the client side — the table is
        // what is under test.
        let deadline = std::time::Instant::now()
            + Duration::from_millis(u64::from(PROBE_QUERY_TIMEOUT_MS) * 2);
        while std::time::Instant::now() < deadline {
            let _ = session.spin_once(5);
            match session.inner().get_check(handle, &mut reply) {
                // A reply would mean the server ANSWERED an empty-payload
                // query, which is not the arm under test.
                Ok(Some(len)) => panic!(
                    "a probe query was answered with {len} bytes — an empty-payload \
                     query must be DECLINED by the service callback, not replied to"
                ),
                Ok(None) => {}
                // The query ended with no reply: the declined path, completed.
                Err(_) => {
                    finalised += 1;
                    break;
                }
            }
        }
    }

    let declines = session
        .inner()
        .reply_slot_declines(queryable)
        .expect("the probe server's queryable handle must address a decline count");
    let (held, refusals, _) = session
        .inner()
        .reply_slot_stats(queryable)
        .expect("the probe server's queryable handle must address a reply-slot table");

    println!(
        "phase-455 W2.b: attempts={attempts} started={started} finalised={finalised} \
         capacity={capacity} declines={declines} held={held} refusals={refusals}"
    );

    // 1 — the POSITIVE control, and the reason this test is not another green
    // that cannot fail. Everything below is about a population that has to
    // exist first, and on this lane nothing else creates it (issue 1332).
    //
    // `declines + refusals`, not `declines`: a query REFUSED a slot never gets
    // a reply seq, so `query_handler`'s decline arm cannot see it. Both counts
    // together are "reached this queryable and was not answered", which is what
    // reach means here. Measured — on a two-slot table with the reclaim
    // reverted this read `declines=2 refusals=2` for four queries, and
    // `declines >= started` alone called that a probe that had missed.
    assert!(
        declines + refusals >= started && started > 0,
        "{declines} declines + {refusals} refusals for {started} empty-payload \
         queries started — the probe did not reach the queryable under test, so the \
         reply-slot assertions below would say nothing (issue 1332). The server \
         declared {key}"
    );

    // 2 — the leak, read directly off the table.
    assert_eq!(
        held, 0,
        "{held} of {capacity} reply slots are still HELD after {declines} declined \
         queries — a query the callback dropped kept its slot for the life of the \
         queryable, which is issue 0902's leak (fixed by `1a032a10b`). \
         started={started}, finalised={finalised}, start_error={start_error:?} \
         (a client-side pool exhaustion here is the same leak: a held query never \
         finalises, so the querier's slot is never released either)"
    );

    // 3 — and what the leak costs once the table is gone: from here on every
    // reply fails silently and a goal is accepted, executed and never answered.
    assert_eq!(
        refusals, 0,
        "the reply-slot table refused {refusals} allocation(s) over {declines} \
         declined queries against a table of {capacity} (issue 0902)"
    );

    // 4 — and only now, the client side. Stated last because its failure is a
    // CONSEQUENCE of 2: with slots reclaimed, every probe query finalises and
    // the querier's pool recycles, so all of them start.
    assert_eq!(
        started, attempts,
        "only {started} of {attempts} probe queries could be started \
         ({start_error:?}) — with the reply slot reclaimed, a declined query \
         finalises at once and the querier's pending-get pool recycles"
    );

    session.close().expect("Failed to close session");
}
