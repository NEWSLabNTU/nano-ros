//! Integration tests for zenoh transport
//!
//! Run with:
//! cargo test --features platform-posix -p nros-rmw-zenoh

#![cfg(feature = "platform-posix")]

use nros_rmw::{
    Publisher, QosSettings, Session, SessionMode, Subscription, TopicInfo, Transport,
    TransportConfig,
};
use nros_rmw_zenoh::{
    DEFAULT_LOCATOR, ZenohTransport, effective_client_locator, keyexpr::TopicKeyExpr,
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
    if !nros_tests::fixtures::require_zenohd() {
        eprintln!("[SKIP] zenohd not found — run `just build-zenohd`");
        return None;
    }
    Some(ZenohRouter::start_unique().expect("failed to start zenohd"))
}

/// Test that we can open and close a session in peer mode
/// (doesn't require a router).
/// Multicast scouting is disabled to avoid contention under parallel test load.
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
    match result {
        Ok(mut session) => {
            let close_result = session.close();
            assert!(close_result.is_ok(), "Failed to close session");
        }
        Err(e) => {
            // Connection failure is acceptable in CI/test environments
            println!(
                "Session open failed (expected in some environments): {:?}",
                e
            );
        }
    }
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
/// This test requires a zenoh router running: zenohd --listen tcp/127.0.0.1:7447
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
            eprintln!("Start a router with: zenohd --listen {}", router_locator);
            panic!("Failed to connect to zenoh router");
        }
    };

    // Create topic with simple key for testing
    let topic = TopicInfo::new("test/loopback", "Int32", "hash123");

    // Create subscriber first
    let mut subscriber = session
        .create_subscription(&topic, QosSettings::BEST_EFFORT)
        .expect("Failed to create subscriber");

    // Wait for subscriber to be established
    thread::sleep(Duration::from_secs(1));

    // Create publisher
    let publisher = session
        .create_publisher(&topic, QosSettings::BEST_EFFORT)
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
    match subscriber.try_recv_raw(&mut recv_buf) {
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

/// zpico is SINGLE-SESSION per process — a second open must FAIL, not corrupt
/// the first session (issue 0347).
///
/// This was `test_pubsub_separate_sessions`, ignored for "requires zenohd
/// router". Un-ignoring it (issue 0328) showed it failing against a live
/// router, and localisation found the cause: `zpico.c` holds one static
/// `g_session` and process-global registration tables, and
/// `zpico_init_with_config` used to memset those tables and replace the
/// session on a SECOND open — silently destroying the first session's
/// subscribers while returning Ok. The old test was therefore asserting
/// delivery through a configuration the shim does not implement.
///
/// The evidence, for anyone tempted to "fix" the routing instead:
/// - two processes, one session each, via a router: WORKS (verified with the
///   stock talker/listener, 11 published / 11 received);
/// - same-session pub/sub: works via the local loopback
///   (`Z_FEATURE_LOCAL_SUBSCRIBER=1` on host builds), not the router;
/// - cross-session in one process: delivered only when the subscriber was
///   declared AFTER the second open — i.e. after the memset that erased it.
///
/// Multi-session support would mean moving `g_session` and every `g_*` table
/// into a per-session context; until then the honest contract is to refuse.
#[test]
fn second_session_open_in_one_process_is_refused() {
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

    let _first = ZenohTransport::open(&config).expect("first session should open");

    // The second open must report failure rather than quietly re-initialising
    // the global state out from under `_first`.
    let second = ZenohTransport::open(&config);
    assert!(
        second.is_err(),
        "a second zpico session in one process must be refused — before issue \
         0347 this returned Ok and wiped the first session's registrations"
    );
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
        .create_publisher(&topic1, QosSettings::BEST_EFFORT)
        .expect("Failed to create publisher 1");

    let _pub2 = session
        .create_publisher(&topic2, QosSettings::BEST_EFFORT)
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
        .create_subscription(&topic1, QosSettings::BEST_EFFORT)
        .expect("Failed to create subscriber 1");

    let _sub2 = session
        .create_subscription(&topic2, QosSettings::BEST_EFFORT)
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
    match result {
        Ok(mut session) => {
            assert!(session.is_open(), "Session should be open");
            let close_result = session.close();
            assert!(close_result.is_ok(), "Failed to close session");
            println!("SUCCESS: Peer session with scouting disabled opened and closed");
        }
        Err(e) => {
            // Connection failure is acceptable in CI/test environments
            println!(
                "Session open failed (expected in some environments): {:?}",
                e
            );
        }
    }
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
    match result {
        Ok(mut session) => {
            assert!(session.is_open(), "Session should be open");
            let close_result = session.close();
            assert!(close_result.is_ok(), "Failed to close session");
            println!(
                "SUCCESS: Peer session with ZENOH_MULTICAST_SCOUTING env var opened and closed"
            );
        }
        Err(e) => {
            println!(
                "Session open failed (expected in some environments): {:?}",
                e
            );
        }
    }

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
    match result {
        Ok(mut session) => {
            assert!(session.is_open(), "Session should be open");
            let close_result = session.close();
            assert!(close_result.is_ok(), "Failed to close session");
            println!("SUCCESS: Explicit property overrides env var");
        }
        Err(e) => {
            println!(
                "Session open failed (expected in some environments): {:?}",
                e
            );
        }
    }

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
            eprintln!("Start a router with: zenohd --listen {}", router_locator);
            panic!("Failed to connect to zenoh router");
        }
    };

    let topic = TopicInfo::new("test/props-loopback", "Int32", "hash_props");

    let mut subscriber = session
        .create_subscription(&topic, QosSettings::BEST_EFFORT)
        .expect("Failed to create subscriber");

    thread::sleep(Duration::from_secs(1));

    let publisher = session
        .create_publisher(&topic, QosSettings::BEST_EFFORT)
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
    match subscriber.try_recv_raw(&mut recv_buf) {
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
/// `second_session_open_in_one_process_is_refused`). `ZenohTransport::open`, by
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
    if !nros_tests::fixtures::require_zenohd() {
        nros_tests::skip!("zenohd not found — run `just build-zenohd`");
    }

    let port = default_locator_port();

    // The default port is shared-host territory. Confirm it is free before
    // `ZenohRouter::start` (which reaps whatever is listening) so this test
    // never kills an unrelated router.
    match std::net::TcpListener::bind(("127.0.0.1", port)) {
        Ok(probe) => drop(probe),
        Err(e) => nros_tests::skip!(
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
