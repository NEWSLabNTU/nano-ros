//! nros to nros communication tests
//!
//! Tests communication between native nros binaries via zenoh.
//!
//! **Bucket (phase-329): KEEP — behavior one-offs, not matrix cells.** The plain
//! router-based rust/zenoh DELIVERY case folded into the native-example pubsub
//! matrix consumer (`native_example_pubsub_e2e.rs`); what remains here tests
//! things no cell covers: talker/listener startup, peer-mode (no-router
//! multicast discovery), MessageInfo sequence-number monotonicity + GID, and the
//! TLS transport.

use nros_tests::{
    fixtures::{
        ManagedProcess, ZenohRouter, listener_binary, listener_tls_binary, require_zenohd,
        talker_binary, talker_tls_binary, tls_certs, zenohd_unique,
    },
    output,
};
use rstest::rstest;
use std::{path::PathBuf, time::Duration};

/// Phase 150.H — unwrap a fixture-builder result, but if the
/// failure is `BuildFailed("...not prebuilt...")`, surface it as
/// `nros_tests::skip!` instead of a hard panic. Mirrors the
/// 150.F treatment for the xrce/stress rstest fixtures
/// (`packages/testing/nros-tests/src/fixtures/binaries/mod.rs`).
///
/// "Not prebuilt" means the user / CI hasn't run
/// `just build-test-fixtures` first. That's an environment
/// precondition, not a test-logic failure — same policy as 150.F.
/// Any OTHER build error panics normally and counts as a real
/// failure.
fn require_prebuilt(
    result: nros_tests::TestResult<&'static std::path::Path>,
    name: &str,
) -> &'static std::path::Path {
    match result {
        Ok(p) => p,
        Err(nros_tests::TestError::BuildFailed(msg)) if msg.contains("not prebuilt") => {
            nros_tests::skip!("{name}: {msg}")
        }
        Err(e) => panic!("Failed to build {name}: {e:?}"),
    }
}

// =============================================================================
// Native Pub/Sub Tests
// =============================================================================

#[rstest]
fn test_native_talker_starts(zenohd_unique: ZenohRouter, talker_binary: PathBuf) {
    use std::process::Command;

    if !require_zenohd() {
        nros_tests::skip!("zenohd not found");
    }

    let locator = zenohd_unique.locator();

    let mut cmd = Command::new(&talker_binary);
    cmd.env("NROS_LOCATOR", &locator);
    let mut talker =
        ManagedProcess::spawn_command(cmd, "native-rs-talker").expect("Failed to start talker");

    // Wait for readiness (talker prints "Publishing" after setup)
    match talker.wait_for_output_pattern(
        nros_tests::output::TALKER_READY_MARKER,
        Duration::from_secs(5),
    ) {
        Ok(_) => eprintln!("native-rs-talker started successfully"),
        Err(_) => {
            // Issue 0702 — the readiness marker did not arrive. That is only
            // acceptable if the process is STILL RUNNING (a slow start under
            // load); a process that EXITED is the failure this test exists to
            // catch, and it used to be reported as a printed note on a green
            // run, because this `match` is the test's last statement.
            assert!(
                talker.is_running(),
                "native-rs-talker exited before printing its readiness marker — it did \
                 not start. Output above is the whole story this test has."
            );
            eprintln!("native-rs-talker running (no readiness marker yet)");
        }
    }
}

#[rstest]
fn test_native_listener_starts(zenohd_unique: ZenohRouter, listener_binary: PathBuf) {
    use std::process::Command;

    if !require_zenohd() {
        nros_tests::skip!("zenohd not found");
    }

    let locator = zenohd_unique.locator();

    let mut cmd = Command::new(&listener_binary);
    cmd.env("NROS_LOCATOR", &locator);
    let mut listener =
        ManagedProcess::spawn_command(cmd, "native-rs-listener").expect("Failed to start listener");

    // Wait for readiness (listener prints its `Subscriber created` line)
    match listener.wait_for_output_pattern(
        nros_tests::output::LISTENER_READY_MARKER,
        Duration::from_secs(5),
    ) {
        Ok(_) => eprintln!("native-rs-listener started successfully"),
        Err(_) => {
            // Issue 0702 — the readiness marker did not arrive. That is only
            // acceptable if the process is STILL RUNNING (a slow start under
            // load); a process that EXITED is the failure this test exists to
            // catch, and it used to be reported as a printed note on a green
            // run, because this `match` is the test's last statement.
            assert!(
                listener.is_running(),
                "native-rs-listener exited before printing its readiness marker — it did \
                 not start. Output above is the whole story this test has."
            );
            eprintln!("native-rs-listener running (no readiness marker yet)");
        }
    }
}

// phase-329 W4 — the router-based rust/zenoh delivery test FOLDED into the
// native-example pubsub matrix consumer (`tests/native_example_pubsub_e2e.rs`,
// the `(Linux, Rust, Zenoh, Pubsub)` cell, which asserts the stronger ≥3). The
// tests kept below are genuine one-offs no cell covers: peer-mode (no-router
// multicast discovery), MessageInfo sequence/GID, and the TLS transport.

// =============================================================================
// Peer Mode Tests (no router required)
// =============================================================================

/// Peer-to-peer delivery with no zenohd router — issue 0711.
///
/// # Why this needs its own fixture pair
///
/// Peer mode needs the multicast transport, scouting and multicast declarations
/// that issue 0682 compiled OUT by default: three more code paths in a library
/// whose point is fitting on an MCU. The default native pair therefore refuses
/// peer mode up front, so this test could only ever SKIP — and it spawns
/// PREBUILT binaries, so no env exported around the test crate could change
/// what they were compiled with. Hence a fixture variant carrying
/// `ZPICO_MULTICAST_TRANSPORT=1`, in its own artifact root.
///
/// # Why `ZENOH_LISTEN` and not a multicast locator
///
/// This is the part issue 0711 had wrong, and it cost the first attempt.
/// `multicast_locator` configures SCOUTING — where to look for a router to
/// connect to. It does not open a multicast transport. With neither `connect`
/// nor `listen` set, `_z_open` finds no locators in the config and falls
/// through to `_z_locators_by_scout`, which waits out the scouting timeout and
/// returns nothing, so the session fails with `ConnectionFailed` about 13
/// seconds in. Setting `multicast_locator` only changes WHERE it scouts; it
/// still scouts, and still fails.
///
/// The multicast transport comes from a LISTEN endpoint. `ZENOH_LISTEN` already
/// mapped to the `listen` property, so no backend change was needed at all.
///
/// `#iface=` is not optional: `_z_f_link_open_udp_multicast` reads
/// `UDP_CONFIG_IFACE_KEY` and returns `_Z_ERR_CONFIG_LOCATOR_INVALID` when it is
/// absent, and zenoh-pico's default multicast locator names no interface. So
/// peer mode cannot open on ANY host until something supplies one — which is
/// why the old failure looked like a network-configuration quirk when it was
/// unconditional.
///
/// `lo` rather than a real NIC deliberately: it keeps the test host-independent
/// and off the LAN. Measured identical delivery over `lo` and `eno1`.
#[test]
fn test_peer_mode_communication() -> nros_tests::TestResult<()> {
    use std::process::Command;

    let talker_binary = nros_tests::fixtures::build_native_talker_peer()?;
    let listener_binary = nros_tests::fixtures::build_native_listener_peer()?;

    let domain = nros_tests::unique_ros_domain_id().to_string();
    // The multicast group zenoh-pico defaults to, with the interface it will
    // not open without.
    let listen = "udp/224.0.0.224:7446#iface=lo";

    let mut listener_cmd = Command::new(listener_binary);
    listener_cmd
        .env("NROS_SESSION_MODE", "peer")
        .env("ZENOH_LISTEN", listen)
        .env("NROS_DOMAIN_ID", &domain);
    let mut listener = ManagedProcess::spawn_command(listener_cmd, "native-rs-listener-peer")
        .expect("Failed to start listener in peer mode");

    let startup = listener
        .wait_for_output_pattern(
            nros_tests::output::LISTENER_READY_MARKER,
            Duration::from_secs(20),
        )
        .unwrap_or_else(|e| format!("{e}"));

    // The capability assertion, and it is an ASSERT rather than a skip.
    //
    // This pair's whole reason to exist is that it was built with the multicast
    // transport on. If the backend refuses peer mode here, the row's env did not
    // reach the build — a broken fixture, not an absent capability, and issue
    // 0650 is exactly about not letting the second spelling hide the first.
    assert!(
        !startup.contains(nros_tests::output::ZENOH_PEER_MODE_UNSUPPORTED_MARKER),
        "the peer fixture was built WITHOUT multicast transport — the row's \
         `ZPICO_MULTICAST_TRANSPORT=1` did not reach the build, so this pair \
         cannot answer the question it exists for. Output:\n{startup}"
    );
    assert!(
        startup.contains(nros_tests::output::LISTENER_READY_MARKER),
        "peer listener never reported readiness (still running: {}). A session \
         that fails here takes ~13 s and reports `ConnectionFailed`, which is \
         what a missing or interface-less `ZENOH_LISTEN` looks like. Output:\n{startup}",
        listener.is_running()
    );

    let mut talker_cmd = Command::new(talker_binary);
    talker_cmd
        .env("NROS_SESSION_MODE", "peer")
        .env("ZENOH_LISTEN", listen)
        .env("NROS_DOMAIN_ID", &domain);
    let mut talker = ManagedProcess::spawn_command(talker_cmd, "native-rs-talker-peer")
        .expect("Failed to start talker in peer mode");

    assert!(
        talker
            .wait_for_output_pattern(
                nros_tests::output::TALKER_READY_MARKER,
                Duration::from_secs(20),
            )
            .is_ok(),
        "peer talker never reported readiness (still running: {})",
        talker.is_running()
    );

    let listener_output = listener.collect_until(
        nros_tests::output::LISTENER_LOG_PREFIX,
        Duration::from_secs(15),
    );
    talker.kill();

    let result = output::parse_listener(&listener_output);

    // Issue 0711 — this tail used to be two `eprintln!("[INFO] …")` lines and a
    // fall-through, so a run whose session never opened and which received
    // ZERO messages was reported GREEN. Its message blamed "some network
    // configurations"; the failure was unconditional. An assertion, because the
    // delivery is the whole claim.
    assert!(
        result.received_count > 0,
        "peer mode delivered NOTHING. Both processes reported ready, so the \
         session opened and this is a delivery failure rather than a config \
         one. Listener output:\n{listener_output}"
    );

    Ok(())
}

// =============================================================================
// MessageInfo Tests (sequence number, GID)
// =============================================================================

// Issue 0429 — these observe the per-message MessageInfo (sequence + GID) the
// zenoh publisher shim stamps into the wire attachment and logs under
// `RUST_LOG=trace` (`output::MESSAGE_INFO_ATTACHMENT_MARKER`). They USED to grep
// the demo listener's stderr, but phase-277 slimmed the example — it no longer
// traces the receive side — so the greps silently found nothing ("got 0"). The
// authoritative source of these values is the PUBLISHER, so observe it there and
// assert on `output::*` constants, not literals, so the next banner change breaks
// the constant rather than these tests (CLAUDE.md grep-drift rule).

/// Run the native pair with `RUST_LOG=trace` on the TALKER and return the talker's
/// captured output. Fails loudly if the MessageInfo attachment trace is absent —
/// the drift #0429 fixed must not recur as a silent "got 0".
fn capture_publisher_msginfo_trace(
    locator: &str,
    talker_binary: &std::path::Path,
    listener_binary: &std::path::Path,
) -> String {
    use std::process::Command;

    // The listener is the peer that makes this a real pair; it is not observed.
    let mut listener_cmd = Command::new(listener_binary);
    listener_cmd.env("NROS_LOCATOR", locator);
    let mut listener = ManagedProcess::spawn_command(listener_cmd, "native-rs-listener")
        .expect("Failed to start listener");

    let mut talker_cmd = Command::new(talker_binary);
    talker_cmd
        .env("NROS_LOCATOR", locator)
        .env("RUST_LOG", "trace");
    let mut talker = ManagedProcess::spawn_command(talker_cmd, "native-rs-talker")
        .expect("Failed to start talker");

    // phase-342 W8 — wait for the publishes this helper's callers need (≥2 for
    // the increment/consistency checks) instead of sleeping 3 s for them. The
    // marker is the same one the assertion below greps, so the wait and the
    // assertion cannot disagree about what "published" means.
    let talker_output = talker
        .wait_for_output_count(
            output::MESSAGE_INFO_ATTACHMENT_MARKER,
            2,
            Duration::from_secs(20),
        )
        .unwrap_or_else(|e| {
            talker.kill();
            listener.kill();
            panic!("talker emitted fewer than 2 MessageInfo trace lines: {e}")
        });

    talker.kill();
    listener.kill();

    eprintln!("Talker trace output:\n{}", talker_output);
    assert!(
        talker_output.contains(output::MESSAGE_INFO_ATTACHMENT_MARKER),
        "talker (RUST_LOG=trace) emitted no `{}` line — the publisher-shim MessageInfo \
         trace moved or was removed (issue 0429 grep-drift class). Diff the marker \
         constant against what the shim prints before assuming a delivery bug.\nOutput:\n{}",
        output::MESSAGE_INFO_ATTACHMENT_MARKER,
        talker_output
    );
    talker_output
}

/// Test that sequence numbers increment monotonically per publisher.
#[rstest]
fn test_sequence_number_increment(
    zenohd_unique: ZenohRouter,
    talker_binary: PathBuf,
    listener_binary: PathBuf,
) {
    if !require_zenohd() {
        nros_tests::skip!("zenohd not found");
    }
    let out =
        capture_publisher_msginfo_trace(&zenohd_unique.locator(), &talker_binary, &listener_binary);

    // `… seq=N, ts=…` — take the leading digits after the prefix (stops at the comma).
    let seq_values: Vec<i64> = out
        .lines()
        .filter_map(|line| {
            let pos = line.find(output::MESSAGE_INFO_SEQ_PREFIX)?;
            let rest = &line[pos + output::MESSAGE_INFO_SEQ_PREFIX.len()..];
            let end = rest
                .find(|c: char| !c.is_ascii_digit())
                .unwrap_or(rest.len());
            rest[..end].parse::<i64>().ok()
        })
        .collect();

    eprintln!("Parsed sequence numbers: {:?}", seq_values);
    assert!(
        seq_values.len() >= 2,
        "Need at least 2 sequence numbers to verify increment, got {}",
        seq_values.len()
    );
    output::assert_monotonic(&seq_values);
    eprintln!(
        "[PASS] Sequence numbers increment monotonically ({} messages)",
        seq_values.len()
    );
}

/// Test that publisher GID stays consistent across messages.
#[rstest]
fn test_gid_consistency(
    zenohd_unique: ZenohRouter,
    talker_binary: PathBuf,
    listener_binary: PathBuf,
) {
    if !require_zenohd() {
        nros_tests::skip!("zenohd not found");
    }
    let out =
        capture_publisher_msginfo_trace(&zenohd_unique.locator(), &talker_binary, &listener_binary);

    // `… gid=[c0, 3b, 8b, a3]` — take the whole bracketed byte array.
    let gid_values: Vec<String> = out
        .lines()
        .filter_map(|line| {
            let pos = line.find(output::MESSAGE_INFO_GID_PREFIX)?;
            let rest = &line[pos + output::MESSAGE_INFO_GID_PREFIX.len()..];
            let start = rest.find('[')?;
            let end = rest[start..].find(']')? + start;
            Some(rest[start..=end].to_string())
        })
        .collect();

    eprintln!("Parsed GIDs: {:?}", gid_values);
    assert!(
        gid_values.len() >= 2,
        "Need at least 2 GID values to verify consistency, got {}",
        gid_values.len()
    );

    let first_gid = &gid_values[0];
    for (i, gid) in gid_values.iter().enumerate() {
        assert_eq!(
            gid, first_gid,
            "GID at message {} ({}) should match first GID ({})",
            i, gid, first_gid
        );
    }

    // Not an all-zero GID — a real publisher ID has a non-zero hex byte.
    let has_nonzero = first_gid
        .chars()
        .filter(|c| c.is_ascii_hexdigit())
        .any(|c| c != '0');
    assert!(
        has_nonzero,
        "GID should not be all zeros (should contain a real publisher ID): {first_gid}"
    );

    eprintln!(
        "[PASS] Publisher GID is consistent across {} messages: {}",
        gid_values.len(),
        first_gid
    );
}

// =============================================================================
// TLS Transport Tests
// =============================================================================

/// Test that TLS talker/listener communicate through a TLS-enabled zenohd
#[rstest]
fn test_tls_talker_listener_communication(
    talker_tls_binary: PathBuf,
    listener_tls_binary: PathBuf,
) {
    use std::process::Command;

    if !require_zenohd() {
        nros_tests::skip!("zenohd not found");
    }

    if !tls_certs::is_openssl_available() {
        nros_tests::skip!("openssl not available — cannot generate TLS certs");
    }

    // Generate self-signed certificate
    let certs = tls_certs::TlsCerts::generate().expect("Failed to generate TLS certs");

    // Start zenohd with TLS listener
    let router = ZenohRouter::start_tls_unique(certs.cert_path(), certs.key_path())
        .expect("Failed to start zenohd with TLS");
    let locator = router.locator();
    eprintln!("TLS router at: {}", locator);

    let cert_path = certs.cert_path().to_str().unwrap().to_string();

    // Start listener with TLS locator and CA certificate
    let mut listener_cmd = Command::new(&listener_tls_binary);
    listener_cmd
        .env("NROS_LOCATOR", &locator)
        .env("ZENOH_TLS_ROOT_CA_CERTIFICATE", &cert_path)
        .env("RUST_LOG", "info");
    let mut listener = ManagedProcess::spawn_command(listener_cmd, "native-rs-listener-tls")
        .expect("Failed to start TLS listener");

    // Wait for listener to be ready
    let _ = listener.wait_for_output_pattern(
        nros_tests::output::LISTENER_READY_MARKER,
        Duration::from_secs(10),
    );

    // Start talker with TLS locator and CA certificate
    let mut talker_cmd = Command::new(&talker_tls_binary);
    talker_cmd
        .env("NROS_LOCATOR", &locator)
        .env("ZENOH_TLS_ROOT_CA_CERTIFICATE", &cert_path);
    let mut talker = ManagedProcess::spawn_command(talker_cmd, "native-rs-talker-tls")
        .expect("Failed to start TLS talker");

    // Wait for listener to receive messages
    let listener_output = listener.collect_until(
        nros_tests::output::LISTENER_LOG_PREFIX,
        Duration::from_secs(15),
    );

    talker.kill();

    eprintln!("TLS Listener output:\n{}", listener_output);

    let result = output::assert_listener(&listener_output, 1);
    eprintln!(
        "[PASS] TLS talker/listener communication works ({} messages)",
        result.received_count
    );
}

// =============================================================================
// RTIC Pattern Tests (zero callbacks, spin_once(0), try_recv())
// =============================================================================

/// Test RTIC-pattern talker/listener interop via zenoh.
///
/// Validates the RTIC integration pattern works for end-to-end communication:
/// - `Executor<_, 0, 0>` (zero callback arena)
/// - `spin_once(0)` (non-blocking I/O drive)
/// - `publisher.publish()` (direct, outside executor)
/// - `subscription.try_recv()` (manual polling)
#[rstest]
fn test_rtic_pattern_communication(zenohd_unique: ZenohRouter) {
    use std::process::Command;

    if !require_zenohd() {
        nros_tests::skip!("zenohd not found");
    }

    let rtic_talker = require_prebuilt(
        nros_tests::fixtures::build_native_rtic_talker(),
        "native rtic-talker",
    );
    let rtic_listener = require_prebuilt(
        nros_tests::fixtures::build_native_rtic_listener(),
        "native rtic-listener",
    );

    let locator = zenohd_unique.locator();

    // Start listener first
    let mut listener_cmd = Command::new(rtic_listener);
    listener_cmd
        .env("NROS_LOCATOR", &locator)
        .env("RUST_LOG", "info");
    let mut listener = ManagedProcess::spawn_command(listener_cmd, "rtic-listener")
        .expect("Failed to start rtic-listener");

    // Wait for listener readiness
    let _ = listener.wait_for_output_pattern(
        nros_tests::output::LISTENER_READY_MARKER,
        Duration::from_secs(5),
    );

    // Start talker
    let mut talker_cmd = Command::new(rtic_talker);
    talker_cmd
        .env("NROS_LOCATOR", &locator)
        .env("RUST_LOG", "info");
    let mut talker = ManagedProcess::spawn_command(talker_cmd, "rtic-talker")
        .expect("Failed to start rtic-talker");

    // Wait for talker to publish messages
    let _ = talker.wait_for_output_pattern(
        nros_tests::output::TALKER_LOG_PREFIX,
        Duration::from_secs(30),
    );

    // Wait for listener to receive messages
    let listener_output = listener.collect_until(
        nros_tests::output::LISTENER_LOG_PREFIX,
        Duration::from_secs(10),
    );

    talker.kill();

    let result = output::assert_listener(&listener_output, 1);
    eprintln!(
        "[PASS] RTIC pattern communication works ({} messages)",
        result.received_count
    );
}

/// Test RTIC-pattern service server/client interop via zenoh.
#[rstest]
fn test_rtic_pattern_service(zenohd_unique: ZenohRouter) {
    use nros_tests::count_pattern;
    use std::process::Command;

    if !require_zenohd() {
        nros_tests::skip!("zenohd not found");
    }

    let rtic_server = require_prebuilt(
        nros_tests::fixtures::build_native_rtic_service_server(),
        "native rtic-service-server",
    );
    let rtic_client = require_prebuilt(
        nros_tests::fixtures::build_native_rtic_service_client(),
        "native rtic-service-client",
    );

    let locator = zenohd_unique.locator();

    // Start server first
    let mut server_cmd = Command::new(rtic_server);
    server_cmd
        .env("NROS_LOCATOR", &locator)
        .env("RUST_LOG", "info");
    let mut server = ManagedProcess::spawn_command(server_cmd, "rtic-service-server")
        .expect("Failed to start rtic-service-server");

    // Wait for server readiness
    let _ = server.wait_for_output_pattern(
        nros_tests::output::SERVICE_SERVER_READY_MARKER,
        Duration::from_secs(5),
    );

    // Start client
    let mut client_cmd = Command::new(rtic_client);
    client_cmd
        .env("NROS_LOCATOR", &locator)
        .env("RUST_LOG", "info");
    let mut client = ManagedProcess::spawn_command(client_cmd, "rtic-service-client")
        .expect("Failed to start rtic-service-client");

    // Wait for the client's single result line
    let client_output = client.collect_until(
        nros_tests::output::SERVICE_RESULT_PREFIX,
        Duration::from_secs(30),
    );

    server.kill();

    let reply_count = count_pattern(&client_output, nros_tests::output::SERVICE_RESULT_PREFIX);
    eprintln!("RTIC service: client got {} replies", reply_count);
    eprintln!("Client output:\n{}", client_output);

    assert!(
        reply_count > 0,
        "RTIC-pattern service client should receive at least 1 reply"
    );
    eprintln!("[PASS] RTIC pattern service works");
}

/// Test RTIC-pattern action server/client interop via zenoh.
#[rstest]
fn test_rtic_pattern_action(zenohd_unique: ZenohRouter) {
    use nros_tests::count_pattern;
    use std::process::Command;

    if !require_zenohd() {
        nros_tests::skip!("zenohd not found");
    }

    let rtic_server = require_prebuilt(
        nros_tests::fixtures::build_native_rtic_action_server(),
        "native rtic-action-server",
    );
    let rtic_client = require_prebuilt(
        nros_tests::fixtures::build_native_rtic_action_client(),
        "native rtic-action-client",
    );

    let locator = zenohd_unique.locator();

    // Start server first
    let mut server_cmd = Command::new(rtic_server);
    server_cmd
        .env("NROS_LOCATOR", &locator)
        .env("RUST_LOG", "info");
    let mut server = ManagedProcess::spawn_command(server_cmd, "rtic-action-server")
        .expect("Failed to start rtic-action-server");

    // Wait for server readiness
    let _ = server.wait_for_output_pattern(
        nros_tests::output::ACTION_SERVER_READY_MARKER,
        Duration::from_secs(5),
    );

    // Start client
    let mut client_cmd = Command::new(rtic_client);
    client_cmd
        .env("NROS_LOCATOR", &locator)
        .env("RUST_LOG", "info");
    let mut client = ManagedProcess::spawn_command(client_cmd, "rtic-action-client")
        .expect("Failed to start rtic-action-client");

    // Wait for the client to report acceptance
    let client_output = client.collect_until("Goal accepted", Duration::from_secs(30));

    server.kill();

    let feedback_count = count_pattern(&client_output, nros_tests::output::ACTION_FEEDBACK_PREFIX);
    let accepted = count_pattern(&client_output, "Goal accepted");
    eprintln!(
        "RTIC action: accepted={}, feedback={}",
        accepted, feedback_count
    );
    eprintln!("Client output:\n{}", client_output);

    assert!(
        accepted > 0,
        "RTIC-pattern action client should get goal accepted"
    );
    eprintln!("[PASS] RTIC pattern action works");
}

// =============================================================================
// Detection Tests
// =============================================================================

// `test_zenohd_detection` removed: it read `is_zenohd_available()` and printed
// it, asserting nothing, so it reported PASS on a host with no router at all.
// The probe stays load-bearing as the `skip!` guard on the real tests, where a
// `false` stops the run. Forbidden repo-wide by `check-no-vacuous-tests`.
