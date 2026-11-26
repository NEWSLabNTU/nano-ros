//! Large message and throughput stress tests
//!
//! Tests large message publishing, E2E data integrity, overflow detection,
//! and throughput at various rates for both zenoh and XRCE backends.
//!
//! Prerequisites:
//!   Zenoh tests: zenohd (built automatically)
//!   XRCE tests: just build-xrce-agent

use nros_tests::{
    count_pattern,
    fixtures::{
        ManagedProcess, XrceAgent, ZenohRouter, or_unmet, qemu_large_msg_test_binary,
        require_xrce_agent, require_zenohd, xrce_stress_test_binary,
        xrce_stress_test_large_buf_binary, zenoh_stress_test_binary,
        zenoh_stress_test_large_buf_binary, zenohd_unique,
    },
};
use rstest::rstest;
use std::{path::PathBuf, sync::Mutex, time::Duration};

static XRCE_LARGE_MSG_LOCK: Mutex<()> = Mutex::new(());

// =============================================================================
// Zenoh Large Message Tests
// =============================================================================

/// Test that publish_raw succeeds for various payload sizes (publish-only, no listener).
#[rstest]
fn test_zenoh_large_publish_sizes(zenohd_unique: ZenohRouter, zenoh_stress_test_binary: PathBuf) {
    use std::process::Command;

    require_zenohd();

    let locator = zenohd_unique.locator();

    // Test sizes from small to large (up to 32KB)
    for &size in &[64, 256, 1024, 4096, 8192, 32768] {
        let mut cmd = Command::new(&zenoh_stress_test_binary);
        cmd.env("NROS_LOCATOR", &locator)
            .env("MODE", "talker")
            .env("PAYLOAD_SIZE", size.to_string())
            .env("PUBLISH_COUNT", "3")
            .env("PUBLISH_INTERVAL_MS", "10");
        let mut proc = ManagedProcess::spawn_image(cmd, format!("zenoh-stress-{}", size))
            .expect("Failed to start stress test");

        let output = proc.collect_until("PUBLISH_DONE:", Duration::from_secs(15));

        proc.kill();

        // The stress bin keeps its own `Published: seq=N size=...` wording
        // (not the W4 demo-parity chatter), so count its lines directly.
        let published =
            nros_tests::count_pattern(&output, nros_tests::output::INT32_TALKER_LOG_PREFIX);
        assert!(
            published >= 2,
            "stress talker: expected at least 2 published messages, got {published}.\nOutput:\n{output}",
        );
    }
}

/// Test E2E data integrity: talker sends 512B payloads, listener validates them.
#[rstest]
fn test_zenoh_e2e_integrity(zenohd_unique: ZenohRouter, zenoh_stress_test_binary: PathBuf) {
    use std::process::Command;

    require_zenohd();

    let locator = zenohd_unique.locator();

    // Start listener first
    let mut listener_cmd = Command::new(&zenoh_stress_test_binary);
    listener_cmd
        .env("NROS_LOCATOR", &locator)
        .env("MODE", "listener")
        .env("PAYLOAD_SIZE", "512")
        .env("EXPECTED_COUNT", "20")
        .env("TIMEOUT_SECS", "20");
    let mut listener = ManagedProcess::spawn_image(listener_cmd, "zenoh-stress-listener")
        .expect("Failed to start listener");

    listener
        .wait_for_output_pattern("Ready: listening", Duration::from_secs(5))
        .expect("zenoh stress listener did not become ready");

    // Start talker
    let mut talker_cmd = Command::new(&zenoh_stress_test_binary);
    talker_cmd
        .env("NROS_LOCATOR", &locator)
        .env("MODE", "talker")
        .env("PAYLOAD_SIZE", "512")
        .env("PUBLISH_COUNT", "20")
        .env("PUBLISH_INTERVAL_MS", "50");
    let mut talker = ManagedProcess::spawn_image(talker_cmd, "zenoh-stress-talker")
        .expect("Failed to start talker");

    // Wait for listener to receive messages
    let listener_output = listener.collect_until("RECV_DONE:", Duration::from_secs(20));

    talker.kill();
    listener.kill();

    eprintln!("Listener output:\n{}", listener_output);

    // Parse RECV_DONE line
    let received = count_pattern(
        &listener_output,
        nros_tests::output::INT32_LISTENER_LOG_PREFIX,
    );
    let invalid = count_pattern(&listener_output, "valid=false");

    assert!(
        received >= 10,
        "Expected at least 10 received messages, got {}.\nOutput:\n{}",
        received,
        listener_output,
    );
    assert_eq!(
        invalid, 0,
        "Expected 0 invalid messages, got {}.\nOutput:\n{}",
        invalid, listener_output,
    );
}

/// Test that oversized payloads (larger than receiver buffer) are detected.
#[rstest]
fn test_zenoh_overflow_detection(zenohd_unique: ZenohRouter, zenoh_stress_test_binary: PathBuf) {
    use std::process::Command;

    require_zenohd();

    let locator = zenohd_unique.locator();

    // Start listener expecting 512B payloads
    let mut listener_cmd = Command::new(&zenoh_stress_test_binary);
    listener_cmd
        .env("NROS_LOCATOR", &locator)
        .env("MODE", "listener")
        .env("PAYLOAD_SIZE", "512")
        .env("EXPECTED_COUNT", "5")
        // Every 2048B payload overflows the 1024B shim buffer and is dropped,
        // so the listener never reaches EXPECTED_COUNT — it runs the full
        // timeout, then prints the RECV_DONE summary. Keep it short; the test's
        // RECV_DONE wait below must exceed it.
        .env("TIMEOUT_SECS", "8");
    let mut listener = ManagedProcess::spawn_image(listener_cmd, "zenoh-stress-overflow-listener")
        .expect("Failed to start listener");

    listener
        .wait_for_output_pattern("Ready: listening", Duration::from_secs(5))
        .expect("zenoh overflow listener did not become ready");

    // Talker sends 2048B payloads (larger than default 1024B receiver buffer)
    let mut talker_cmd = Command::new(&zenoh_stress_test_binary);
    talker_cmd
        .env("NROS_LOCATOR", &locator)
        .env("MODE", "talker")
        .env("PAYLOAD_SIZE", "2048")
        .env("PUBLISH_COUNT", "5")
        .env("PUBLISH_INTERVAL_MS", "100");
    let mut talker = ManagedProcess::spawn_image(talker_cmd, "zenoh-stress-overflow-talker")
        .expect("Failed to start talker");

    // Wait for the listener's RECV_DONE summary. It prints only after the
    // listener's internal 8s timeout (all payloads overflow + drop, so
    // EXPECTED_COUNT is never reached), so this wait must comfortably exceed
    // that — 25s leaves margin for slow CI. Failing here (rather than
    // `unwrap_or_default`) surfaces a real hang instead of a misleading
    // `overflow_drops=0`.
    let listener_output = listener
        .wait_for_output_pattern("RECV_DONE:", Duration::from_secs(25))
        .expect("zenoh overflow listener never printed RECV_DONE summary");

    talker.kill();
    listener.kill();

    eprintln!("Overflow test output:\n{}", listener_output);

    // The listener either receives 0 messages (overflow drops them) or
    // receives them with validation failure (size mismatch).
    // Both are acceptable overflow detection behaviors.
    let valid_count = count_pattern(&listener_output, "valid=true");
    assert_eq!(
        valid_count, 0,
        "Expected 0 valid messages (overflow should cause size mismatch or drop), got {}.\nOutput:\n{}",
        valid_count, listener_output,
    );

    // Phase 160.L.2 — overflow is detected silently inside the zenoh-pico
    // C shim (`zpico.c:595`: `payload_len > payload_stride` → call
    // `notify` with NULL payload + don't advance ring tail) and surfaced
    // through `nros_rmw_zenoh::overflow_drops_total()`, a process-wide
    // atomic the subscriber-notify callback bumps when the oversized
    // path fires. The stress-test listener prints
    // `overflow_drops=<N>` in the `RECV_DONE:` summary; the test asserts
    // the counter advanced for THIS run. Pre-Phase-160.L.2 the test
    // checked for a `Receive error` printf, which the
    // `take_serialized` path never emits — drops never reach the executor
    // ring at all, so there is no `Err` to log.
    let overflow_drops = listener_output
        .lines()
        .find_map(|line| {
            line.strip_prefix("RECV_DONE:")
                .and_then(|tail| {
                    tail.split_whitespace()
                        .find_map(|tok| tok.strip_prefix("overflow_drops="))
                })
                .and_then(|n| n.parse::<u32>().ok())
        })
        .unwrap_or(0);
    assert!(
        overflow_drops >= 1,
        "Expected overflow_drops >= 1 in RECV_DONE summary, got {}.\nOutput:\n{}",
        overflow_drops,
        listener_output,
    );
}

/// E2E test: talker sends 4096B payloads, listener with 8192B shim buffer receives them.
///
/// The listener is built with `NROS_SUBSCRIBER_BUFFER_BYTES=8192` so it can fit
/// 4096B payloads (which would overflow the default 1024B buffer). The talker
/// uses the same large-buf binary — publish path has no shim buffer constraint,
/// but using the same binary simplifies the test.
#[rstest]
fn test_zenoh_e2e_large_receive(
    zenohd_unique: ZenohRouter,
    zenoh_stress_test_large_buf_binary: PathBuf,
) {
    use std::process::Command;

    require_zenohd();

    let locator = zenohd_unique.locator();

    // Listener (built with 8192B shim buffer)
    let mut listener_cmd = Command::new(&zenoh_stress_test_large_buf_binary);
    listener_cmd
        .env("NROS_LOCATOR", &locator)
        .env("MODE", "listener")
        .env("PAYLOAD_SIZE", "4096")
        .env("EXPECTED_COUNT", "10")
        .env("TIMEOUT_SECS", "20");
    let mut listener = ManagedProcess::spawn_image(listener_cmd, "zenoh-large-recv-listener")
        .expect("spawn listener");

    listener
        .wait_for_output_pattern("Ready: listening", Duration::from_secs(5))
        .expect("zenoh large receive listener did not become ready");

    // Talker (same large-buf binary)
    let mut talker_cmd = Command::new(&zenoh_stress_test_large_buf_binary);
    talker_cmd
        .env("NROS_LOCATOR", &locator)
        .env("MODE", "talker")
        .env("PAYLOAD_SIZE", "4096")
        .env("PUBLISH_COUNT", "10")
        .env("PUBLISH_INTERVAL_MS", "50");
    let mut talker =
        ManagedProcess::spawn_image(talker_cmd, "zenoh-large-recv-talker").expect("spawn talker");

    let output = listener.collect_until("RECV_DONE:", Duration::from_secs(20));

    talker.kill();
    listener.kill();

    eprintln!("Large receive test output:\n{}", output);

    let received = count_pattern(&output, nros_tests::output::INT32_LISTENER_LOG_PREFIX);
    let invalid = count_pattern(&output, "valid=false");

    assert!(
        received >= 5,
        "Expected >=5 received at 4096B, got {}.\nOutput:\n{}",
        received,
        output,
    );
    assert_eq!(
        invalid, 0,
        "Expected 0 invalid, got {}.\nOutput:\n{}",
        invalid, output,
    );
}

// =============================================================================
// phase-384 W2 — the XRCE receive ring is a ceiling, and it must say so
// =============================================================================

/// Run one nano -> Agent -> nano exchange and return the listener's output.
fn xrce_roundtrip(binary: &std::path::Path, payload: &str, topic: &str) -> String {
    use std::process::Command;

    let agent = XrceAgent::start_unique().expect("Failed to start XRCE Agent");
    let addr = agent.addr();

    let mut listener_cmd = Command::new(binary);
    listener_cmd
        .env("XRCE_AGENT_ADDR", &addr)
        .env("STRESS_TOPIC", topic)
        .env("MODE", "listener")
        .env("PAYLOAD_SIZE", payload)
        .env("EXPECTED_COUNT", "10")
        .env("TIMEOUT_SECS", "20");
    let mut listener = ManagedProcess::spawn_image(listener_cmd, "xrce-ring-listener")
        .expect("Failed to start listener");
    listener
        .wait_for_output_pattern("Ready: listening", Duration::from_secs(10))
        .expect("XRCE listener did not become ready");

    let mut talker_cmd = Command::new(binary);
    talker_cmd
        .env("XRCE_AGENT_ADDR", &addr)
        .env("STRESS_TOPIC", topic)
        .env("MODE", "talker")
        .env("PAYLOAD_SIZE", payload)
        .env("PUBLISH_COUNT", "10")
        .env("PUBLISH_INTERVAL_MS", "100");
    let mut talker = ManagedProcess::spawn_image(talker_cmd, "xrce-ring-talker")
        .expect("Failed to start talker");

    let output = listener.collect_until("RECV_DONE:", Duration::from_secs(25));
    talker.kill();
    listener.kill();
    drop(agent);
    output
}

/// A payload over `XRCE_SUBSCRIBER_BUFFER_SIZE` must be refused by NAME.
///
/// The ceiling is ours — `XRCE_SUBSCRIBER_BUFFER_SIZE` in
/// `packages/rmw/xrce/nros-rmw-xrce/src/internal.h`, checked in the topic
/// callback as `len + XRCE_CDR_HEADER_LEN > XRCE_SUBSCRIBER_BUFFER_SIZE`. The
/// stress binary's `PAYLOAD_SIZE` includes the 4-byte CDR header that the
/// publish side strips, so the predicate is exactly `PAYLOAD_SIZE > 1024` and
/// 1025 is the first refused size.
///
/// 1024 rather than a family-specific number because phase-454 W6.b left
/// `XRCE_BUFFER_SIZE` as the DEFAULT of the three receive families, and this
/// fixture declares no endpoints for the build to derive one from. That is why
/// `NROS_XRCE_BUFFER_BYTES=8192` still raises it, in the sibling test below.
///
/// What this test actually guards is the ERROR NAME, not the refusal. Before
/// phase-384 W1 the backend's `MessageTooLarge` was rewritten to
/// `DeserializationError` by a `map_err(|_| ...)` on a code path that
/// deserializes nothing — so the one diagnostic that names its own remedy
/// (raise `NROS_XRCE_BUFFER_BYTES`) was replaced by one that sends the reader
/// looking at the serializer. Two drafts of phase-384 chased the Agent because
/// of it. Asserting only "delivery fails" would pass on that tree.
#[rstest]
fn xrce_payload_over_the_ring_is_refused_by_name(xrce_stress_test_binary: PathBuf) {
    require_xrce_agent();
    let _guard = XRCE_LARGE_MSG_LOCK.lock().expect("XRCE test lock poisoned");

    let output = xrce_roundtrip(&xrce_stress_test_binary, "1025", "/stress_xrce_ring_over");

    let too_large = count_pattern(&output, "MessageTooLarge");
    let mislabelled = count_pattern(&output, "DeserializationError");
    let delivered = count_pattern(&output, nros_tests::output::INT32_LISTENER_LOG_PREFIX);

    assert_eq!(
        delivered, 0,
        "1025 bytes is over the 1024-byte ring and must not be delivered.\nOutput:\n{output}",
    );
    assert_eq!(
        mislabelled, 0,
        "the raw take reported DeserializationError for a transport error — the \
         phase-384 W1 regression. It deserializes nothing; the backend said \
         MessageTooLarge.\nOutput:\n{output}",
    );
    assert!(
        too_large >= 5,
        "expected the refusal to name itself (MessageTooLarge), got {too_large} \
         occurrence(s).\nOutput:\n{output}",
    );
}

/// …and raising `NROS_XRCE_BUFFER_BYTES` delivers the same payload intact.
///
/// This is the half that proves the ceiling is a CONFIGURATION limit and not the
/// Agent's — phase-384's original premise was that the Agent's hardcoded
/// `m_typeSize = 1024 + 4` dropped these samples, which no env knob on our side
/// could affect. 2048 bytes over the same Agent, ring raised to 8192: delivered.
///
/// Asserts VALIDITY, not just arrival: issue 0819 is a size at which samples
/// arrive with `Ok(Some(len))` and wrong bytes, so a count-only assertion would
/// pass straight through it.
#[rstest]
fn xrce_raising_the_ring_delivers_the_same_payload(xrce_stress_test_large_buf_binary: PathBuf) {
    require_xrce_agent();
    let _guard = XRCE_LARGE_MSG_LOCK.lock().expect("XRCE test lock poisoned");

    let output = xrce_roundtrip(
        &xrce_stress_test_large_buf_binary,
        "2048",
        "/stress_xrce_ring_raised",
    );

    let delivered = count_pattern(&output, nros_tests::output::INT32_LISTENER_LOG_PREFIX);
    let invalid = count_pattern(&output, "valid=false");

    assert!(
        delivered >= 5,
        "2048 bytes fits an 8192-byte ring and must be delivered; got {delivered}.\n\
         Output:\n{output}",
    );
    assert_eq!(
        invalid, 0,
        "samples arrived but did not survive the trip ({invalid} invalid) — that is \
         issue 0819's signature at a size well below it.\nOutput:\n{output}",
    );
}

/// Issue 0819 — a payload that spans several transport fragments is delivered
/// INTACT.
///
/// A fragmented XRCE message does not arrive as contiguous bytes:
/// `uxr_next_input_reliable_buffer_available` initialises the `ucdrBuffer` over
/// the first fragment and installs `on_full_input_buffer` to swap in the next
/// one mid-read, and `read_format_data` propagates that callback onto the
/// buffer handed to our receive callback. The callback used to `memcpy` the
/// DECLARED length straight off `ub->iterator`, which cannot cross a fragment
/// boundary — so the tail arrived as whatever followed the first fragment
/// (zeros) while `entry->len` reported the full length. `xrce_stage_inbound`
/// reads through `ucdr_deserialize_array_uint8_t`, which walks the chain.
///
/// This asserted REFUSAL until 2026-08-27. That was the interim guard, not the
/// fix: refusing at the MTU rejects exactly the payloads the client is supposed
/// to fragment, so it traded a delivery bug for a capability regression. Both
/// intermediate states are named here because both look like a pass from a
/// distance — a count-only assertion passes through the truncation, and a
/// `MessageTooLarge`-only assertion passes through the regression.
///
/// The ring is raised so `XRCE_SUBSCRIBER_BUFFER_SIZE` is NOT the constraint —
/// otherwise this would pass for phase-384's reason rather than this one.
#[rstest]
fn xrce_fragmented_payload_is_delivered_intact(xrce_stress_test_large_buf_binary: PathBuf) {
    require_xrce_agent();
    let _guard = XRCE_LARGE_MSG_LOCK.lock().expect("XRCE test lock poisoned");

    let output = xrce_roundtrip(
        &xrce_stress_test_large_buf_binary,
        "4096",
        "/stress_xrce_at_mtu",
    );

    let delivered = count_pattern(&output, nros_tests::output::INT32_LISTENER_LOG_PREFIX);
    let invalid = count_pattern(&output, "valid=false");
    let too_large = count_pattern(&output, "MessageTooLarge");

    assert!(
        delivered >= 5,
        "a 4096-byte payload spans fragments and must be REASSEMBLED, not \
         refused; got {delivered} delivered and {too_large} MessageTooLarge.\n\
         Output:\n{output}",
    );
    assert_eq!(
        invalid, 0,
        "samples arrived but did not survive reassembly ({invalid} invalid) — \
         that is issue 0819's original silent-truncation signature.\nOutput:\n{output}",
    );
}

/// Throughput test at 100 Hz (10ms interval).
#[rstest]
fn test_zenoh_throughput_100hz(zenohd_unique: ZenohRouter, zenoh_stress_test_binary: PathBuf) {
    use std::process::Command;

    require_zenohd();

    let locator = zenohd_unique.locator();

    // Start listener
    let mut listener_cmd = Command::new(&zenoh_stress_test_binary);
    listener_cmd
        .env("NROS_LOCATOR", &locator)
        .env("MODE", "listener")
        .env("PAYLOAD_SIZE", "64")
        .env("EXPECTED_COUNT", "100")
        .env("TIMEOUT_SECS", "15");
    let mut listener = ManagedProcess::spawn_image(listener_cmd, "zenoh-100hz-listener")
        .expect("Failed to start listener");

    listener
        .wait_for_output_pattern("Ready: listening", Duration::from_secs(5))
        .expect("zenoh 100Hz listener did not become ready");

    // Start talker at 100 Hz
    let mut talker_cmd = Command::new(&zenoh_stress_test_binary);
    talker_cmd
        .env("NROS_LOCATOR", &locator)
        .env("MODE", "talker")
        .env("PAYLOAD_SIZE", "64")
        .env("PUBLISH_COUNT", "100")
        .env("PUBLISH_INTERVAL_MS", "10");
    let mut talker = ManagedProcess::spawn_image(talker_cmd, "zenoh-100hz-talker")
        .expect("Failed to start talker");

    let listener_output = listener.collect_until("RECV_DONE:", Duration::from_secs(15));

    talker.kill();
    listener.kill();

    let received = count_pattern(
        &listener_output,
        nros_tests::output::INT32_LISTENER_LOG_PREFIX,
    );
    eprintln!("100Hz test: received {} messages", received);

    assert!(
        received >= 20,
        "Expected at least 20 messages at 100Hz, got {}.\nOutput:\n{}",
        received,
        listener_output,
    );
}

/// Throughput burst test (0ms interval).
#[rstest]
fn test_zenoh_throughput_burst(zenohd_unique: ZenohRouter, zenoh_stress_test_binary: PathBuf) {
    use std::process::Command;

    require_zenohd();

    let locator = zenohd_unique.locator();

    // Start listener
    let mut listener_cmd = Command::new(&zenoh_stress_test_binary);
    listener_cmd
        .env("NROS_LOCATOR", &locator)
        .env("MODE", "listener")
        .env("PAYLOAD_SIZE", "64")
        .env("EXPECTED_COUNT", "100")
        .env("TIMEOUT_SECS", "15");
    let mut listener = ManagedProcess::spawn_image(listener_cmd, "zenoh-burst-listener")
        .expect("Failed to start listener");

    listener
        .wait_for_output_pattern("Ready: listening", Duration::from_secs(5))
        .expect("zenoh burst listener did not become ready");

    // Start talker with no delay (burst)
    let mut talker_cmd = Command::new(&zenoh_stress_test_binary);
    talker_cmd
        .env("NROS_LOCATOR", &locator)
        .env("MODE", "talker")
        .env("PAYLOAD_SIZE", "64")
        .env("PUBLISH_COUNT", "100")
        .env("PUBLISH_INTERVAL_MS", "0");
    let mut talker = ManagedProcess::spawn_image(talker_cmd, "zenoh-burst-talker")
        .expect("Failed to start talker");

    // Wait longer for burst messages to propagate
    let listener_output = listener.collect_until("RECV_DONE:", Duration::from_secs(15));

    talker.kill();
    listener.kill();

    let received = count_pattern(
        &listener_output,
        nros_tests::output::INT32_LISTENER_LOG_PREFIX,
    );
    eprintln!("Burst test: received {} messages", received);

    assert!(
        received >= 1,
        "Expected at least 1 message in burst mode, got {}.\nOutput:\n{}",
        received,
        listener_output,
    );
}

// =============================================================================
// XRCE Large Message Tests
// =============================================================================

/// Test E2E data integrity over XRCE: 512B payloads.
#[rstest]
fn test_xrce_e2e_integrity(xrce_stress_test_binary: PathBuf) {
    use std::process::Command;

    require_xrce_agent();

    let _xrce_guard = XRCE_LARGE_MSG_LOCK.lock().expect("XRCE test lock poisoned");
    let agent = XrceAgent::start_unique().expect("Failed to start XRCE Agent");
    let addr = agent.addr();

    // Start listener
    let mut listener_cmd = Command::new(&xrce_stress_test_binary);
    listener_cmd
        .env("XRCE_AGENT_ADDR", &addr)
        .env("STRESS_TOPIC", "/stress_xrce_integrity")
        .env("MODE", "listener")
        .env("PAYLOAD_SIZE", "512")
        .env("EXPECTED_COUNT", "20")
        .env("TIMEOUT_SECS", "20");
    let mut listener = ManagedProcess::spawn_image(listener_cmd, "xrce-stress-listener")
        .expect("Failed to start listener");

    listener
        .wait_for_output_pattern("Ready: listening", Duration::from_secs(10))
        .expect("XRCE stress listener did not become ready");

    // Start talker
    let mut talker_cmd = Command::new(&xrce_stress_test_binary);
    talker_cmd
        .env("XRCE_AGENT_ADDR", &addr)
        .env("STRESS_TOPIC", "/stress_xrce_integrity")
        .env("MODE", "talker")
        .env("PAYLOAD_SIZE", "512")
        .env("PUBLISH_COUNT", "20")
        .env("PUBLISH_INTERVAL_MS", "100");
    let mut talker = ManagedProcess::spawn_image(talker_cmd, "xrce-stress-talker")
        .expect("Failed to start talker");

    let listener_output = listener.collect_until("RECV_DONE:", Duration::from_secs(25));

    talker.kill();
    listener.kill();

    eprintln!("XRCE listener output:\n{}", listener_output);

    let received = count_pattern(
        &listener_output,
        nros_tests::output::INT32_LISTENER_LOG_PREFIX,
    );
    let invalid = count_pattern(&listener_output, "valid=false");

    assert!(
        received >= 5,
        "Expected at least 5 received messages, got {}.\nOutput:\n{}",
        received,
        listener_output,
    );
    assert_eq!(
        invalid, 0,
        "Expected 0 invalid messages, got {}.\nOutput:\n{}",
        invalid, listener_output,
    );

    drop(agent);
}

/// Test XRCE publish_raw succeeds for various sizes (publish-only).
#[rstest]
fn test_xrce_large_publish_sizes(xrce_stress_test_binary: PathBuf) {
    use std::process::Command;

    require_xrce_agent();

    let _xrce_guard = XRCE_LARGE_MSG_LOCK.lock().expect("XRCE test lock poisoned");
    let agent = XrceAgent::start_unique().expect("Failed to start XRCE Agent");
    let addr = agent.addr();

    // XRCE supports larger messages via fragmented streams (Phase 40.3)
    for &size in &[64, 256, 1024, 4096, 12288] {
        let mut cmd = Command::new(&xrce_stress_test_binary);
        cmd.env("XRCE_AGENT_ADDR", &addr)
            .env("STRESS_TOPIC", "/stress_xrce_sizes")
            .env("MODE", "talker")
            .env("PAYLOAD_SIZE", size.to_string())
            .env("PUBLISH_COUNT", "3")
            .env("PUBLISH_INTERVAL_MS", "50");
        let mut proc = ManagedProcess::spawn_image(cmd, format!("xrce-stress-{}", size))
            .expect("Failed to start stress test");

        let output = proc.collect_until("PUBLISH_DONE:", Duration::from_secs(15));

        proc.kill();

        let published = count_pattern(&output, nros_tests::output::INT32_TALKER_LOG_PREFIX);
        assert!(
            published >= 2,
            "Expected at least 2 publishes at size={}, got {}.\nOutput:\n{}",
            size,
            published,
            output,
        );
    }

    drop(agent);
}

/// XRCE throughput test at 100 Hz.
#[rstest]
fn test_xrce_throughput_100hz(xrce_stress_test_binary: PathBuf) {
    use std::process::Command;

    require_xrce_agent();

    let _xrce_guard = XRCE_LARGE_MSG_LOCK.lock().expect("XRCE test lock poisoned");
    let agent = XrceAgent::start_unique().expect("Failed to start XRCE Agent");
    let addr = agent.addr();

    let mut listener_cmd = Command::new(&xrce_stress_test_binary);
    listener_cmd
        .env("XRCE_AGENT_ADDR", &addr)
        .env("STRESS_TOPIC", "/stress_xrce_100hz")
        .env("MODE", "listener")
        .env("PAYLOAD_SIZE", "64")
        .env("EXPECTED_COUNT", "100")
        .env("TIMEOUT_SECS", "20");
    let mut listener = ManagedProcess::spawn_image(listener_cmd, "xrce-100hz-listener")
        .expect("Failed to start listener");

    listener
        .wait_for_output_pattern("Ready: listening", Duration::from_secs(10))
        .expect("XRCE 100Hz listener did not become ready");

    let mut talker_cmd = Command::new(&xrce_stress_test_binary);
    talker_cmd
        .env("XRCE_AGENT_ADDR", &addr)
        .env("STRESS_TOPIC", "/stress_xrce_100hz")
        .env("MODE", "talker")
        .env("PAYLOAD_SIZE", "64")
        .env("PUBLISH_COUNT", "100")
        .env("PUBLISH_INTERVAL_MS", "10");
    let mut talker = ManagedProcess::spawn_image(talker_cmd, "xrce-100hz-talker")
        .expect("Failed to start talker");

    let listener_output = listener.collect_until("RECV_DONE:", Duration::from_secs(20));

    talker.kill();
    listener.kill();

    let received = count_pattern(
        &listener_output,
        nros_tests::output::INT32_LISTENER_LOG_PREFIX,
    );
    eprintln!("XRCE 100Hz test: received {} messages", received);

    assert!(
        received >= 10,
        "Expected at least 10 messages at 100Hz, got {}.\nOutput:\n{}",
        received,
        listener_output,
    );

    drop(agent);
}

/// XRCE throughput burst test (0ms interval).
#[rstest]
fn test_xrce_throughput_burst(xrce_stress_test_binary: PathBuf) {
    use std::process::Command;

    require_xrce_agent();

    let _xrce_guard = XRCE_LARGE_MSG_LOCK.lock().expect("XRCE test lock poisoned");
    let agent = XrceAgent::start_unique().expect("Failed to start XRCE Agent");
    let addr = agent.addr();

    let mut listener_cmd = Command::new(&xrce_stress_test_binary);
    listener_cmd
        .env("XRCE_AGENT_ADDR", &addr)
        .env("STRESS_TOPIC", "/stress_xrce_burst")
        .env("MODE", "listener")
        .env("PAYLOAD_SIZE", "64")
        .env("EXPECTED_COUNT", "100")
        .env("TIMEOUT_SECS", "20");
    let mut listener = ManagedProcess::spawn_image(listener_cmd, "xrce-burst-listener")
        .expect("Failed to start listener");

    listener
        .wait_for_output_pattern("Ready: listening", Duration::from_secs(10))
        .expect("XRCE burst listener did not become ready");

    let mut talker_cmd = Command::new(&xrce_stress_test_binary);
    talker_cmd
        .env("XRCE_AGENT_ADDR", &addr)
        .env("STRESS_TOPIC", "/stress_xrce_burst")
        .env("MODE", "talker")
        .env("PAYLOAD_SIZE", "64")
        .env("PUBLISH_COUNT", "100")
        .env("PUBLISH_INTERVAL_MS", "0");
    let mut talker = ManagedProcess::spawn_image(talker_cmd, "xrce-burst-talker")
        .expect("Failed to start talker");

    let listener_output = listener.collect_until("RECV_DONE:", Duration::from_secs(20));

    talker.kill();
    listener.kill();

    let received = count_pattern(
        &listener_output,
        nros_tests::output::INT32_LISTENER_LOG_PREFIX,
    );
    eprintln!("XRCE burst test: received {} messages", received);

    assert!(
        received >= 3,
        "Expected at least 3 messages in burst mode, got {}.\nOutput:\n{}",
        received,
        listener_output,
    );

    drop(agent);
}

// =============================================================================
// QEMU Bare-Metal Large Message Test
// =============================================================================

/// Test that the QEMU bare-metal binary can publish at various sizes.
/// Requires qemu-system-arm + zenoh-pico-arm + QEMU slirp (user-mode) networking.
#[rstest]
fn test_qemu_zenoh_large_publish(qemu_large_msg_test_binary: PathBuf) {
    // This test uses QEMU slirp (user-mode) networking with port forwarding.
    // No TAP devices, bridge interfaces, or sudo required.
    // Issue 1900 — the lane before the emulator probe. (The rstest fixture
    // above already resolved through the lane-aware resolver; this states it
    // where the probe is, rather than leaning on argument order.)
    nros_tests::fixtures::lane::require_platform_in_lane(
        &[nros_tests::matrix::PlatformId::QemuBaremetal],
        "the bare-metal large-message image",
    );
    // An unmet precondition if QEMU is not available.
    let qemu_available = nros_tests::qemu::qemu_system_arm_cmd()
        .arg("--version")
        .output()
        .is_ok();
    if !qemu_available {
        nros_tests::unmet!("qemu-system-arm not found");
    }

    // The bench firmware bakes its own allocator aux slot (phase-295 W4) —
    // nothing else shares this router, so the test runs parallel to the
    // other bare-metal QEMU lanes.
    let _zenohd = or_unmet(ZenohRouter::start_slirp(
        nros_tests::alloc::BAREMETAL_LARGE_MSG_PORT,
    ));

    // phase-528 W5 — the board's networked boot, through `nros run --plan`.
    let mut proc =
        nros_tests::qemu::QemuProcess::start_mps2_an385_networked(&qemu_large_msg_test_binary)
            .expect("Failed to start QEMU");

    let output = proc.collect_until("All tests passed", Duration::from_secs(30));

    proc.kill();

    eprintln!("QEMU large msg test output:\n{}", output);

    // Verify all test sizes passed
    let pass_count = count_pattern(&output, "[PASS]");
    assert!(
        pass_count >= 6,
        "Expected at least 6 [PASS] markers (one per size), got {}.\nOutput:\n{}",
        pass_count,
        output,
    );
    assert!(
        !output.contains("[FAIL]"),
        "Unexpected [FAIL] in output.\nOutput:\n{}",
        output,
    );
}

// =============================================================================
// Phase 574 W2 -- the large-message capability matrix, host cells
// =============================================================================
//
// One probe (`bins/large-msg`), one image per RMW, each image sized for the
// largest payload its backend carries here. Every cell sends five samples of
// a `std_msgs/UInt8MultiArray` and the listener checks every byte of each, so
// a cell passes on CONTENT, not arrival. XRCE stops at 16 KiB: its DATA
// submessage length is 16 bits on the wire (issue 2978).

/// `key=value` from a probe summary line.
fn summary_field<'a>(line: &'a str, key: &str) -> Option<&'a str> {
    line.split_whitespace()
        .find_map(|kv| kv.strip_prefix(key)?.strip_prefix('='))
}

fn summary_line<'a>(output: &'a str, marker: &str) -> Option<&'a str> {
    output.lines().find(|l| l.contains(marker))
}

#[rstest]
#[case::zenoh_1k(nros_tests::fixtures::Rmw::Zenoh, 1 << 10)]
#[case::zenoh_16k(nros_tests::fixtures::Rmw::Zenoh, 16 << 10)]
#[case::zenoh_64k(nros_tests::fixtures::Rmw::Zenoh, 64 << 10)]
#[case::zenoh_256k(nros_tests::fixtures::Rmw::Zenoh, 256 << 10)]
#[case::zenoh_1m(nros_tests::fixtures::Rmw::Zenoh, 1 << 20)]
#[case::zenoh_4m(nros_tests::fixtures::Rmw::Zenoh, 4 << 20)]
#[case::xrce_1k(nros_tests::fixtures::Rmw::Xrce, 1 << 10)]
#[case::xrce_16k(nros_tests::fixtures::Rmw::Xrce, 16 << 10)]
#[case::cyclonedds_1k(nros_tests::fixtures::Rmw::Cyclonedds, 1 << 10)]
#[case::cyclonedds_16k(nros_tests::fixtures::Rmw::Cyclonedds, 16 << 10)]
#[case::cyclonedds_64k(nros_tests::fixtures::Rmw::Cyclonedds, 64 << 10)]
#[case::cyclonedds_256k(nros_tests::fixtures::Rmw::Cyclonedds, 256 << 10)]
#[case::cyclonedds_1m(nros_tests::fixtures::Rmw::Cyclonedds, 1 << 20)]
#[case::cyclonedds_4m(nros_tests::fixtures::Rmw::Cyclonedds, 4 << 20)]
fn large_message_is_delivered_intact_on_the_host(
    #[case] rmw: nros_tests::fixtures::Rmw,
    #[case] size: usize,
) {
    use nros_tests::{
        fixtures::{RequireFixture, Rmw, build_large_msg_probe},
        output::{LARGE_MSG_PUB_DONE, LARGE_MSG_READY_MARKER, LARGE_MSG_RECV_DONE},
    };
    use std::process::Command;

    let probe = build_large_msg_probe(rmw)
        .require("the large-message probe (rows `large-msg-probe-<rmw>`)")
        .to_path_buf();
    let topic = format!("/large_msg_{size}");
    let domain = nros_tests::unique_ros_domain_id().to_string();

    // The peer each RMW needs, held for the cell's lifetime.
    let mut _router = None;
    let mut _agent = None;
    let mut locator = None;
    match rmw {
        Rmw::Zenoh => {
            require_zenohd();
            let r = or_unmet(ZenohRouter::start_unique());
            locator = Some(r.locator());
            _router = Some(r);
        }
        Rmw::Xrce => {
            require_xrce_agent();
            let a = XrceAgent::start_unique().expect("XRCE Agent start");
            locator = Some(format!("udp/{}", a.addr()));
            _agent = Some(a);
            // The agent's accept loop settles before the first client connects.
            std::thread::sleep(Duration::from_secs(1));
        }
        Rmw::Cyclonedds => {}
    }
    let command = |mode: &str| {
        let mut cmd = Command::new(&probe);
        cmd.env("MODE", mode)
            .env("PAYLOAD_SIZE", size.to_string())
            .env("COUNT", "5")
            .env("LARGE_TOPIC", &topic)
            .env("ROS_DOMAIN_ID", &domain);
        if let Some(l) = &locator {
            cmd.env("NROS_LOCATOR", l);
        }
        if rmw == Rmw::Cyclonedds {
            nros_tests::dds_isolation::apply_to_command(&mut cmd);
        }
        cmd
    };

    let mut listener = ManagedProcess::spawn_image(command("listener"), "large-msg-listener")
        .expect("spawn the large-message listener");
    listener
        .wait_for_output_pattern(LARGE_MSG_READY_MARKER, Duration::from_secs(15))
        .expect("the large-message listener did not become ready");
    let mut talker = ManagedProcess::spawn_image(command("talker"), "large-msg-talker")
        .expect("spawn the large-message talker");

    let received = listener.collect_until(LARGE_MSG_RECV_DONE, Duration::from_secs(60));
    let published = talker.collect_until(LARGE_MSG_PUB_DONE, Duration::from_secs(60));
    talker.kill();
    listener.kill();

    let pub_line = summary_line(&published, LARGE_MSG_PUB_DONE)
        .unwrap_or_else(|| panic!("the talker printed no summary.\nOutput:\n{published}"));
    let recv_line = summary_line(&received, LARGE_MSG_RECV_DONE)
        .unwrap_or_else(|| panic!("the listener printed no summary.\nOutput:\n{received}"));
    assert_eq!(
        summary_field(pub_line, "failed"),
        Some("0"),
        "{rmw:?} {size} B: a publish failed: {pub_line}"
    );
    assert_eq!(
        (
            summary_field(recv_line, "valid"),
            summary_field(recv_line, "invalid"),
            summary_field(recv_line, "errors"),
        ),
        (Some("5"), Some("0"), Some("0")),
        "{rmw:?} {size} B: not every sample arrived intact: {recv_line}\nOutput:\n{received}"
    );
}

/// Issue 2978 -- an XRCE sample the 16-bit WRITE_DATA length cannot state is
/// refused at publish, by name, and never reaches a subscriber.
///
/// The image's reliable stream (8 KiB MTU x 16) is wider than 64 KiB, so a
/// 64 KiB sample fits the stream and only the length check can refuse it.
/// Before the check, every such publish returned `Ok` and the sample was lost,
/// or delivered cut to its size modulo 65,536. The 16 KiB cell above is the
/// same image delivering intact.
#[rstest]
#[case::at_64k(64 << 10)]
#[case::at_70000(70_000)]
fn xrce_sample_over_the_write_data_length_is_refused_at_publish(#[case] size: usize) {
    use nros_tests::{
        fixtures::{RequireFixture, Rmw, build_large_msg_probe},
        output::LARGE_MSG_PUB_DONE,
    };
    use std::process::Command;

    require_xrce_agent();
    let probe = build_large_msg_probe(Rmw::Xrce)
        .require("the large-message probe (row `large-msg-probe-xrce`)")
        .to_path_buf();
    let agent = XrceAgent::start_unique().expect("XRCE Agent start");
    std::thread::sleep(Duration::from_secs(1));

    let mut cmd = Command::new(&probe);
    cmd.env("MODE", "talker")
        .env("PAYLOAD_SIZE", size.to_string())
        .env("COUNT", "3")
        .env("LARGE_TOPIC", format!("/large_msg_refused_{size}"))
        .env(
            "ROS_DOMAIN_ID",
            nros_tests::unique_ros_domain_id().to_string(),
        )
        .env("NROS_LOCATOR", format!("udp/{}", agent.addr()));
    let mut talker = ManagedProcess::spawn_image(cmd, "large-msg-refused-talker")
        .expect("spawn the large-message talker");
    let published = talker.collect_until(LARGE_MSG_PUB_DONE, Duration::from_secs(60));
    talker.kill();

    let line = summary_line(&published, LARGE_MSG_PUB_DONE)
        .unwrap_or_else(|| panic!("the talker printed no summary.\nOutput:\n{published}"));
    assert_eq!(
        (summary_field(line, "sent"), summary_field(line, "failed")),
        (Some("0"), Some("3")),
        "{size} B over XRCE: every publish must be refused: {line}"
    );
    assert_eq!(
        summary_field(line, "first_error"),
        Some("Transport(MessageTooLarge)"),
        "{size} B over XRCE: the refusal must name the size: {line}"
    );
}
