//! XRCE-DDS ↔ ROS 2 DDS interoperability tests
//!
//! Tests communication between nros XRCE nodes and ROS 2 nodes using
//! the standard micro-ROS architecture:
//!
//! ```text
//! nros XRCE node → XRCE Agent (Fast-DDS) ←DDS multicast→ ROS 2 node (rmw_fastrtps_cpp)
//! ```
//!
//! The XRCE Agent creates DDS participants on behalf of XRCE clients,
//! so ROS 2 nodes using the same DDS domain can discover and communicate
//! with them via standard DDS multicast.
//!
//! **These tests HARD-FAIL.** This note used to say the opposite — that they
//! were "diagnostic/informational" and reported interop status without failing
//! the suite. That stopped being true: every case here ends in an `assert!` on
//! what actually crossed the bus, and an absent prerequisite is an explicit
//! `nros_tests::unmet!`, never a silent pass (no XRCE Agent, no ROS 2 DDS,
//! a peer that would not start). A stale "does not hard-fail" line invites the
//! next reader to dismiss a red here as noise, which is precisely the
//! false-PASS reading issue 1135 is about.
//!
//! Prerequisites:
//!   just build-xrce-agent        # Build Micro-XRCE-DDS Agent
//!   ROS 2 Humble installed       # /opt/ros/humble/
//!   rmw_fastrtps_cpp available   # Default in Humble
//!   example_interfaces installed # AddTwoInts service + Fibonacci action types

use nros_tests::{
    count_pattern,
    fixtures::{
        DEFAULT_ROS_DISTRO, ManagedProcess, Ros2DdsProcess, XrceAgent, require_ros2_dds,
        require_xrce_agent, xrce_action_client_binary, xrce_action_server_binary,
        xrce_action_server_concurrent_binary, xrce_listener_binary, xrce_service_client_binary,
        xrce_service_info_server_binary, xrce_service_server_binary, xrce_talker_binary,
    },
    unique_ros_domain_id,
};
use rstest::rstest;
use std::{path::PathBuf, time::Duration};

// =============================================================================
// Detection Tests
// =============================================================================

// ~~`test_ros2_dds_detection`~~ removed: it read two `is_*_available()` booleans
// and printed them, asserting nothing, so it reported PASS on a host with no
// ROS 2 and no rmw_fastrtps_cpp. Both probes stay load-bearing as the `unmet!`
// guards on the real interop tests, where a `false` stops the run. Forbidden
// repo-wide by `check-no-vacuous-tests`.

// =============================================================================
// XRCE → ROS 2 Pub/Sub
// =============================================================================

/// nros XRCE talker → ROS 2 DDS listener
///
/// Architecture:
///   xrce-talker → XRCE Agent (UDP) → DDS multicast → ros2 topic echo (rmw_fastrtps_cpp)
#[rstest]
fn test_xrce_to_ros2_pubsub(xrce_talker_binary: PathBuf) {
    use std::process::Command;

    require_xrce_agent();

    require_ros2_dds();

    // Start XRCE Agent on ephemeral port
    let agent = XrceAgent::start_unique().expect("Failed to start XRCE Agent");
    let addr = agent.addr();
    let domain_id = unique_ros_domain_id();

    // Start ROS 2 DDS listener (uses rmw_fastrtps_cpp, DDS multicast discovery)
    eprintln!("Starting ROS 2 DDS topic echo...");
    let mut ros2_listener = match Ros2DdsProcess::topic_echo_with_domain(
        "/chatter",
        "std_msgs/msg/String",
        DEFAULT_ROS_DISTRO,
        domain_id,
    ) {
        Ok(p) => p,
        Err(e) => {
            nros_tests::unmet!(
                "ROS 2 DDS listener could not start (missing ROS 2 demo nodes / tooling?): {}",
                e
            );
        }
    };

    // Wait for DDS discovery + XRCE Agent to propagate
    std::thread::sleep(Duration::from_secs(1));

    // Start XRCE talker
    eprintln!("Starting XRCE talker...");
    let mut talker_cmd = Command::new(&xrce_talker_binary);
    talker_cmd
        .env("NROS_LOCATOR", &addr)
        .env("XRCE_AGENT_ADDR", &addr)
        .env("ROS_DOMAIN_ID", domain_id.to_string());
    let mut talker =
        ManagedProcess::spawn_command(talker_cmd, "xrce-talker").expect("Failed to start talker");

    // Wait for talker to publish
    let _ = talker.wait_for_output_pattern(
        nros_tests::output::TALKER_LOG_PREFIX,
        Duration::from_secs(5),
    );

    // phase-342 W9 — wait for the DELIVERY this test asserts (`data:` lines from
    // the ROS 2 listener) instead of sleeping 2 s for it. W8 could not convert
    // this site: `Ros2Process` had only `wait_for_output(timeout)`, which drains
    // to its deadline and cannot be asked about a pattern. W9 added the shared
    // `collect_until` engine, so the wait is now the condition rather than a
    // guess at how long it takes.
    let ros2_output = ros2_listener
        .wait_for_output_count("data:", 1, Duration::from_secs(20))
        .unwrap_or_else(|e| {
            eprintln!("ROS 2 listener saw no `data:` within 20s: {e}");
            String::new()
        });

    talker.kill();

    eprintln!("ROS 2 DDS output:\n{}", ros2_output);

    let received_count = count_pattern(&ros2_output, "data:");
    eprintln!("ROS 2 received {} messages via DDS", received_count);

    // Drop the agent before asserting so a failure doesn't leak the process.
    drop(agent);

    assert!(
        received_count > 0,
        "ROS 2 DDS listener received no messages from the XRCE talker — the \
         header-stripped XRCE topic payload must parse as DDS CDR (233.6). Output:\n{}",
        ros2_output
    );
    eprintln!("[PASS] XRCE → ROS 2 DDS pub/sub works");
}

// =============================================================================
// ROS 2 → XRCE Pub/Sub
// =============================================================================

/// ROS 2 DDS talker → nros XRCE listener
///
/// Architecture:
///   ros2 topic pub (rmw_fastrtps_cpp) → DDS multicast → XRCE Agent → xrce-listener (UDP)
#[rstest]
fn test_ros2_to_xrce_pubsub(xrce_listener_binary: PathBuf) {
    use std::process::Command;

    require_xrce_agent();

    require_ros2_dds();

    // Start XRCE Agent on ephemeral port
    let agent = XrceAgent::start_unique().expect("Failed to start XRCE Agent");
    let addr = agent.addr();
    let domain_id = unique_ros_domain_id();

    // Start XRCE listener (subscribe before publishing)
    eprintln!("Starting XRCE listener...");
    let mut listener_cmd = Command::new(&xrce_listener_binary);
    listener_cmd
        .env("NROS_LOCATOR", &addr)
        .env("XRCE_AGENT_ADDR", &addr)
        .env("ROS_DOMAIN_ID", domain_id.to_string())
        .env("XRCE_MSG_COUNT", "3")
        // nros_tests::output::INT32_LISTENER_LOG_PREFIX logs at INFO — surface them so the test can see receipt.
        .env("RUST_LOG", "info");
    let mut listener = ManagedProcess::spawn_command(listener_cmd, "xrce-listener")
        .expect("Failed to start listener");

    // Wait for listener to be ready
    let _ = listener.wait_for_output_pattern(
        nros_tests::output::LISTENER_READY_MARKER,
        Duration::from_secs(5),
    );

    // Wait for DDS discovery + XRCE Agent to propagate subscription
    std::thread::sleep(Duration::from_secs(1));

    // Start ROS 2 DDS publisher
    eprintln!("Starting ROS 2 DDS topic pub...");
    let mut ros2_publisher = match Ros2DdsProcess::topic_pub_with_domain(
        "/chatter",
        "std_msgs/msg/String",
        "{data: 'Hello World: 42'}",
        2,
        DEFAULT_ROS_DISTRO,
        domain_id,
    ) {
        Ok(p) => p,
        Err(e) => {
            listener.kill();
            nros_tests::unmet!(
                "ROS 2 DDS publisher could not start (missing ROS 2 demo nodes / tooling?): {}",
                e
            );
        }
    };

    // Wait for XRCE listener to receive messages
    let listener_output = listener.collect_until(
        nros_tests::output::LISTENER_LOG_PREFIX,
        Duration::from_secs(5),
    );

    ros2_publisher.kill();
    listener.kill();

    eprintln!("XRCE listener output:\n{}", listener_output);

    let received_count = count_pattern(&listener_output, nros_tests::output::LISTENER_LOG_PREFIX);
    eprintln!(
        "XRCE listener received {} messages from ROS 2 DDS",
        received_count
    );

    drop(agent);

    assert!(
        received_count > 0,
        "XRCE listener received no messages from the ROS 2 DDS publisher — the \
         re-prepended CDR header on the inbound XRCE topic must parse (233.6). Output:\n{}",
        listener_output
    );
    eprintln!("[PASS] ROS 2 DDS → XRCE pub/sub works");
}

// =============================================================================
// XRCE Service Server + ROS 2 DDS Client
// =============================================================================

/// nros XRCE service server + ROS 2 DDS service client
///
/// Architecture:
///   ros2 service call (rmw_fastrtps_cpp) → DDS → XRCE Agent → xrce-service-server (UDP)
#[rstest]
fn test_xrce_service_ros2_client(xrce_service_server_binary: PathBuf) {
    use std::process::Command;

    require_xrce_agent();

    require_ros2_dds();

    // Start XRCE Agent on ephemeral port
    let agent = XrceAgent::start_unique().expect("Failed to start XRCE Agent");
    let addr = agent.addr();
    let domain_id = unique_ros_domain_id();

    // issue 0741 — the precondition this test never stated: nobody else may be
    // serving `/add_two_ints` on this domain.
    //
    // A ROS 2 service name is not owned by the node that declares it. If a
    // second server holds it, the client may talk to THAT one, and if that one
    // speaks `rmw_cyclonedds_cpp` the reply is framed differently enough that
    // Fast-DDS refuses it outright — see `cross_rmw_service_framing_note`. That
    // is the entire content of issue 0741, and it read for weeks as an XRCE
    // type-registration defect because the failure was attributed to the only
    // server the test knew about.
    //
    // Checked HERE, before the fixture starts, because that is the one moment
    // the answer is unambiguous: `ros2 service list` cannot say whose endpoint
    // it found (Humble has no `ros2 service info`), so anything present now is
    // foreign by construction.
    //
    // `unmet!`, not `assert!`: a poisoned bus cannot answer the question this
    // test asks, and calling that an interop regression is what produced four
    // retracted diagnoses. `None` (probe could not run) proceeds — a probe that
    // cannot see must not invent.
    if nros_tests::ros2::service_present_on_domain(DEFAULT_ROS_DISTRO, domain_id, "/add_two_ints")
        == Some(true)
    {
        drop(agent);
        nros_tests::unmet!(
            "issue 0741 — `/add_two_ints` is ALREADY served on ROS domain {domain_id} by a peer \
             this test did not start, so the reply the ROS 2 client receives is not necessarily \
             ours. Kill the orphan (`pgrep -a add_two_ints_server`) and re-run; a Cyclone one \
             makes this test fail with a `RTPS_READER_HISTORY … cannot be resized` that has \
             nothing to do with XRCE."
        );
    }

    // Start XRCE service server
    eprintln!("Starting XRCE service server...");
    let mut server_cmd = Command::new(&xrce_service_server_binary);
    server_cmd
        .env("NROS_LOCATOR", &addr)
        .env("XRCE_AGENT_ADDR", &addr)
        .env("ROS_DOMAIN_ID", domain_id.to_string())
        .env("XRCE_TIMEOUT", "30");
    let mut server = ManagedProcess::spawn_command(server_cmd, "xrce-service-server")
        .expect("Failed to start service server");

    // Wait for the server to SAY it is ready, and KEEP what it said.
    //
    // This was `let _ = wait_for_output_pattern(…, 5s)`, and the discard is the
    // defect: a failure downstream could not distinguish "the server never came
    // up" from "the server came up and replied wrong", which are different bugs
    // with different fixes. The sibling test below (`test_ros2_service_xrce_client`)
    // was given exactly this treatment and the reason recorded there; the lesson
    // did not travel the 350 lines to here, so this one kept reporting an
    // interop regression for whatever had actually happened.
    //
    // 20 s, not 5 s, for the same reason the sibling uses 20: the bound exists to
    // catch a server that never starts, not to race one that starts slowly, and a
    // wait that returns as soon as the marker appears costs nothing when it is fast.
    let server_startup = server
        .wait_for_output_pattern(
            nros_tests::output::SERVICE_SERVER_READY_MARKER,
            Duration::from_secs(20),
        )
        .unwrap_or_else(|e| format!("<server never reported ready: {e}>"));

    // Wait for DDS discovery to propagate the service
    std::thread::sleep(Duration::from_secs(1));

    // Call service from ROS 2 DDS
    eprintln!("Calling service from ROS 2 DDS...");
    let mut ros2_client = match Ros2DdsProcess::service_call_with_domain(
        "/add_two_ints",
        "example_interfaces/srv/AddTwoInts",
        "{a: 5, b: 3}",
        DEFAULT_ROS_DISTRO,
        domain_id,
    ) {
        Ok(p) => p,
        Err(e) => {
            server.kill();
            nros_tests::unmet!(
                "ROS 2 DDS service call could not start (missing ROS 2 tooling?): {}",
                e
            );
        }
    };

    // Wait for service call to complete
    let ros2_output = ros2_client
        .wait_for_output(Duration::from_secs(5))
        .unwrap_or_default();

    eprintln!("ROS 2 DDS service call output:\n{}", ros2_output);

    // Check if service call succeeded
    let has_sum = ros2_output.contains("sum");
    let has_correct_value = ros2_output.contains("8");

    // issue 0741 — snapshot the bus BEFORE teardown, and only on failure.
    //
    // Three hosts now agree on every static axis, down to the `libfastrtps` /
    // librmw_fastrtps_cpp binaries being byte-identical, and still disagree on
    // the verdict. If the software is the same, what differs is who else is on
    // the bus — and the failure's mechanism allows exactly that: the reply
    // reader sizes its history from a max-serialized-size learned AT DISCOVERY,
    // with nothing requiring the endpoint it learned from to be ours.
    //
    // Ordering is the point. `server.kill()` and `drop(agent)` used to run
    // above this; after them the bus is empty and any snapshot is worthless, so
    // the capture moved ahead of teardown rather than into the assert.
    //
    // Only on failure: on the happy path this is three `ros2` invocations of
    // pure cost, and the passing hosts do not need it.
    let bus = if has_sum && has_correct_value {
        String::new()
    } else {
        nros_tests::ros2::dds_bus_snapshot(DEFAULT_ROS_DISTRO, domain_id)
    };

    server.kill();
    drop(agent);

    // Phase 233.6 — this direction (real ROS 2 service client ↔ nano-ros XRCE
    // service server) is the acceptance for the XRCE-DDS service CDR-header
    // interop fix. Hard-assert (unlike the other diagnostic interop tests): a
    // ROS 2 `rmw_fastrtps` client must get the correct `sum=8` reply. Before the
    // header strip/prepend in `service.c` the request deserialized misaligned
    // and the server never replied with a valid value.
    // The server's own startup output is half the evidence: "no reply" from a
    // client whose server never came up is a DIFFERENT failure from "no reply"
    // out of a server that was listening, and only one of them is the interop
    // regression this assert names.
    //
    // issue 0741 — but say WHICH failure this is before naming a regression.
    // The `RTPS_READER_HISTORY … cannot be resized` signature is provably not an
    // XRCE defect (a stock Cyclone server and a stock Fast-DDS client reproduce
    // it with no nano-ros in the picture), and the old message asserted it was
    // one. That sentence is what five sessions of this issue were spent chasing.
    let cross_rmw =
        nros_tests::ros2::cross_rmw_service_framing_note(&ros2_output).unwrap_or_default();
    assert!(
        has_sum && has_correct_value,
        "ROS 2 service client did not get sum=8 from the nano-ros XRCE service \
         server — XRCE-DDS service interop regression (233.6).\n\
         {cross_rmw}\
         --- server startup ---\n{server_startup}\n\
         --- ros2 client output ---\n{ros2_output}\n\
         {bus}{env}",
        env = nros_tests::ros2::interop_environment_fingerprint()
    );
    eprintln!("[PASS] XRCE service server ↔ ROS 2 DDS client: sum=8 verified");
}

/// One `REQUEST_ID guid=<hex> seq=<n> …` line of the `service-info-server`
/// bin, parsed: `(guid bytes, client sequence number)`.
fn parse_request_id_line(line: &str) -> Option<([u8; 16], i64)> {
    let rest = &line[line.find(nros_tests::output::SERVICE_INFO_LOG_PREFIX)?
        + nros_tests::output::SERVICE_INFO_LOG_PREFIX.len()..];
    let mut fields = rest.split_whitespace();
    let hex = fields.next()?;
    let seq = fields.next()?.strip_prefix("seq=")?.parse().ok()?;
    if hex.len() != 32 {
        return None;
    }
    let mut guid = [0u8; 16];
    for (i, b) in guid.iter_mut().enumerate() {
        *b = u8::from_str_radix(hex.get(2 * i..2 * i + 2)?, 16).ok()?;
    }
    Some((guid, seq))
}

/// Issue 2088 — the XRCE backend's `take_request_with_info` (issue 2081)
/// against a live Agent and a STOCK client.
///
/// ```text
/// ros2 service call -r (rmw_fastrtps_cpp) → DDS → XRCE Agent → service-info-server (UDP)
/// ```
///
/// The server reports each request's `ServiceInfo::request_id`, which the
/// backend fills from the Agent's `SampleIdentity`. Asserted:
///
/// * **the sequence numbers are the client's** — `ros2 service call -r` sends
///   from ONE client, numbered 1, 2, 3, … by `rmw_fastrtps_cpp` (measured
///   against a stock rclpy server reading the request header), so the server
///   must see exactly `1..=n` in order for the `n` requests the client had
///   answered;
/// * **the GUID is the client's request writer, in wire order** — non-zero,
///   identical across requests, its prefix starting with eProsima's RTPS vendor
///   id `01 0f` and its last byte the entity kind of a keyless user writer
///   (`0x03`). Those two bytes sit at opposite ends of the 16, so a reversed or
///   mis-split copy of the 12-byte prefix / 3-byte key / 1-byte kind fails here,
///   which "non-zero" alone would not catch.
///
/// * **each request carries its receive time** (issue 2135) — the backend's
///   own wall-clock stamp from the request callback, inside the window this
///   test ran in. The SOURCE timestamp is not reported: none reaches an XRCE
///   client library (`xrce_fill_service_info`).
#[rstest]
fn test_xrce_service_reports_the_stock_clients_request_id(
    xrce_service_info_server_binary: PathBuf,
) {
    use std::process::Command;

    require_xrce_agent();
    require_ros2_dds();
    let wall_ns = || {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("wall clock after 1970")
            .as_nanos() as i64
    };
    let started_ns = wall_ns();

    let agent = XrceAgent::start_unique().expect("Failed to start XRCE Agent");
    let addr = agent.addr();
    let domain_id = unique_ros_domain_id();

    // issue 0741 — a second `/add_two_ints` server on the domain would answer
    // some of the client's requests, and THEIR ids would be missing from this
    // server's sequence. Same precondition, same reasoning, as
    // `test_xrce_service_ros2_client`.
    if nros_tests::ros2::service_present_on_domain(DEFAULT_ROS_DISTRO, domain_id, "/add_two_ints")
        == Some(true)
    {
        drop(agent);
        nros_tests::unmet!(
            "issue 0741 — `/add_two_ints` is ALREADY served on ROS domain {domain_id} by a peer \
             this test did not start, so not every request reaches the server under test. Kill \
             the orphan (`pgrep -a add_two_ints_server`) and re-run."
        );
    }

    let mut server_cmd = Command::new(&xrce_service_info_server_binary);
    server_cmd
        .env("NROS_LOCATOR", &addr)
        .env("XRCE_AGENT_ADDR", &addr)
        .env("ROS_DOMAIN_ID", domain_id.to_string())
        .env("XRCE_TIMEOUT", "30")
        .env("RUST_LOG", "info");
    let mut server = ManagedProcess::spawn_command(server_cmd, "xrce-service-info-server")
        .expect("Failed to start service-info-server");
    let server_startup = server
        .wait_for_output_pattern(
            nros_tests::output::SERVICE_SERVER_READY_MARKER,
            Duration::from_secs(20),
        )
        .unwrap_or_else(|e| format!("<server never reported ready: {e}>"));

    std::thread::sleep(Duration::from_secs(1));

    // Three answered requests is the least that shows a SEQUENCE (and a GUID
    // that stays put); 2 Hz keeps the run short.
    const WANT: usize = 3;
    let mut client = match Ros2DdsProcess::service_call_repeated_with_domain(
        "/add_two_ints",
        "example_interfaces/srv/AddTwoInts",
        "{a: 5, b: 3}",
        2,
        20,
        DEFAULT_ROS_DISTRO,
        domain_id,
    ) {
        Ok(p) => p,
        Err(e) => {
            server.kill();
            nros_tests::unmet!("ROS 2 DDS service call could not start: {e}");
        }
    };
    let (client_output, client_why) =
        match client.wait_for_output_count("sum=8", WANT, Duration::from_secs(18)) {
            Ok(out) => (out, String::new()),
            Err(e) => (String::new(), format!("{e}")),
        };
    drop(client);
    let answered = count_pattern(&client_output, "sum=8");

    let (server_output, server_why) = server.collect_until_count(
        nros_tests::output::SERVICE_INFO_LOG_PREFIX,
        answered.max(WANT),
        Duration::from_secs(5),
    );
    server.kill();
    drop(agent);

    let context = format!(
        "--- server startup ---\n{server_startup}\n--- server ---\n{server_output}{}\n\
         --- ros2 client ---\n{client_output}{client_why}\n{env}",
        server_why.unwrap_or_default(),
        env = nros_tests::ros2::interop_environment_fingerprint()
    );
    assert!(
        answered >= WANT,
        "the stock client got {answered} of {WANT} replies — no request-id evidence without \
         a working service path (see test_xrce_service_ros2_client)\n{context}"
    );
    let ids: Vec<([u8; 16], i64)> = server_output
        .lines()
        .filter_map(parse_request_id_line)
        .collect();
    assert!(
        !server_output.contains("REQUEST_ID ABSENT"),
        "the XRCE backend reported NO request id for a request (issue 2081's \
         take_request_with_info slot)\n{context}"
    );
    assert!(
        ids.len() >= answered,
        "the server printed {} request ids for {answered} answered requests\n{context}",
        ids.len()
    );
    let seqs: Vec<i64> = ids.iter().map(|(_, s)| *s).collect();
    let expected: Vec<i64> = (1..=ids.len() as i64).collect();
    assert_eq!(
        seqs, expected,
        "the reported sequence numbers are not the stock client's (1, 2, 3, … from one \
         `ros2 service call -r` client)\n{context}"
    );
    let guid = ids[0].0;
    assert!(
        ids.iter().all(|(g, _)| *g == guid),
        "the reported writer GUID changed between requests from ONE client\n{context}"
    );
    assert!(
        guid.iter().any(|b| *b != 0),
        "all-zero writer GUID\n{context}"
    );
    assert_eq!(
        guid[..2],
        [0x01, 0x0f],
        "the GUID prefix does not start with eProsima's RTPS vendor id 01 0f — the client is \
         rmw_fastrtps_cpp, so the 12-byte prefix was copied out of order\n{context}"
    );
    assert_eq!(
        guid[15], 0x03,
        "the GUID's last byte is not a keyless user writer's entity kind (0x03) — the \
         3-byte key / 1-byte kind split of the entity id is wrong\n{context}"
    );
    // Issue 2135 — every request reports when the backend received it, on the
    // wall clock, between this test's start and the server's last line.
    let ended_ns = wall_ns();
    let received: Vec<Option<i64>> = server_output
        .lines()
        .filter(|l| l.contains(nros_tests::output::SERVICE_INFO_LOG_PREFIX))
        .map(|l| {
            l.split_whitespace()
                .find_map(|f| f.strip_prefix("received_ts="))
                .and_then(|v| v.parse::<i64>().ok())
        })
        .collect();
    assert!(
        !received.is_empty() && received.iter().all(Option::is_some),
        "a request reported no received_timestamp (`received_ts=absent`, issue 2135): \
         {received:?}\n{context}"
    );
    let received: Vec<i64> = received.into_iter().flatten().collect();
    assert!(
        received.iter().all(|t| started_ns <= *t && *t <= ended_ns),
        "a received_timestamp is outside this test's wall-clock window \
         [{started_ns}, {ended_ns}]: {received:?}\n{context}"
    );
    assert!(
        received.windows(2).all(|w| w[0] < w[1]),
        "received_timestamps from a 2 Hz client are not increasing: {received:?}\n{context}"
    );
    eprintln!(
        "[PASS] XRCE ServiceInfo: {} requests, seq 1..={}, writer guid {}",
        ids.len(),
        ids.len(),
        guid.iter().map(|b| format!("{b:02x}")).collect::<String>()
    );
}

/// What a stock graph reads for each endpoint of an XRCE SERVICE, one line
/// per endpoint: `QOS <writer|reader> <RELIABILITY> <DURABILITY>`.
///
/// rclpy's `get_*_info_by_topic(..., no_mangle=True)` on the two DDS topics a
/// replier owns. Polls until both endpoints are discovered, then prints.
const SERVICE_QOS_PROBE: &str = r#"
import sys, time
import rclpy
from rclpy.node import Node
rclpy.init()
n = Node('nros_service_qos_probe')
w, r = [], []
end = time.time() + 25
while time.time() < end:
    w = n.get_publishers_info_by_topic('rr/add_two_intsReply', no_mangle=True)
    r = n.get_subscriptions_info_by_topic('rq/add_two_intsRequest', no_mangle=True)
    if w and r:
        break
    time.sleep(0.5)
for kind, lst in (('writer', w), ('reader', r)):
    for e in lst:
        q = e.qos_profile
        print(f'QOS {kind} {q.reliability.name} {q.durability.name}', flush=True)
print('QOS_PROBE_DONE', flush=True)
"#;

/// Issue 2120 — an XRCE service's two endpoints hold the QoS it was CREATED
/// with, as a stock peer discovers them.
///
/// Before 2120 the replier was created from BIN, whose QoS the client library
/// drops and the Agent replaces with fixed attributes: the reply WRITER came
/// out TRANSIENT_LOCAL whatever was asked, and the request READER was forced
/// RELIABLE. Both cases here distinguish that: `default` asks the services
/// default (RELIABLE + VOLATILE), so the writer must read VOLATILE;
/// `best_effort` asks BEST_EFFORT, so the reader must read BEST_EFFORT.
#[rstest]
#[case::services_default("default", "RELIABLE")]
#[case::best_effort("best_effort", "BEST_EFFORT")]
fn test_xrce_service_endpoints_hold_the_requested_qos(
    xrce_service_info_server_binary: PathBuf,
    #[case] qos: &str,
    #[case] want_reliability: &str,
) {
    use std::process::Command;

    require_xrce_agent();
    require_ros2_dds();

    let agent = XrceAgent::start_unique().expect("Failed to start XRCE Agent");
    let addr = agent.addr();
    let domain_id = unique_ros_domain_id();

    let mut server_cmd = Command::new(&xrce_service_info_server_binary);
    server_cmd
        .env("NROS_LOCATOR", &addr)
        .env("XRCE_AGENT_ADDR", &addr)
        .env("ROS_DOMAIN_ID", domain_id.to_string())
        .env("XRCE_TIMEOUT", "30")
        .env(nros_tests::output::SERVICE_INFO_SERVER_QOS_ENV, qos)
        .env("RUST_LOG", "info");
    let mut server = ManagedProcess::spawn_command(server_cmd, "xrce-service-info-server")
        .expect("Failed to start service-info-server");
    let server_startup = server
        .wait_for_output_pattern(
            nros_tests::output::SERVICE_SERVER_READY_MARKER,
            Duration::from_secs(20),
        )
        .unwrap_or_else(|e| format!("<server never reported ready: {e}>"));

    let env_setup = nros_tests::ros2::ros2_env_setup_dds_with_domain(DEFAULT_ROS_DISTRO, domain_id);
    let deadline = nros_tests::process::deadline(40);
    let probe = Command::new("bash")
        .arg("-c")
        .arg(format!(
            "{env_setup} && {deadline} python3 -u - 2>&1 <<'NROS_PYEOF'\n{SERVICE_QOS_PROBE}\nNROS_PYEOF"
        ))
        .output();
    server.kill();
    drop(agent);
    let probe_out = match probe {
        Ok(o) => format!(
            "{}{}",
            String::from_utf8_lossy(&o.stdout),
            String::from_utf8_lossy(&o.stderr)
        ),
        Err(e) => nros_tests::unmet!("the rclpy QoS probe could not start: {e}"),
    };
    let context = format!(
        "--- server startup ---\n{server_startup}\n--- probe ---\n{probe_out}\n{env}",
        env = nros_tests::ros2::interop_environment_fingerprint()
    );
    assert!(
        probe_out.contains("QOS_PROBE_DONE"),
        "the rclpy QoS probe did not finish\n{context}"
    );
    let endpoints: Vec<(&str, &str, &str)> = probe_out
        .lines()
        .filter_map(|l| {
            let mut it = l.trim().strip_prefix("QOS ")?.split_whitespace();
            Some((it.next()?, it.next()?, it.next()?))
        })
        .collect();
    let writers: Vec<_> = endpoints.iter().filter(|e| e.0 == "writer").collect();
    let readers: Vec<_> = endpoints.iter().filter(|e| e.0 == "reader").collect();
    assert!(
        writers.len() == 1 && readers.len() == 1,
        "expected exactly the server's reply writer and request reader on domain \
         {domain_id}, found {} writer(s) and {} reader(s)\n{context}",
        writers.len(),
        readers.len()
    );
    for (kind, rel, dur) in [writers[0], readers[0]] {
        assert_eq!(
            (*rel, *dur),
            (want_reliability, "VOLATILE"),
            "the service's {kind} does not hold the QoS it was created with ({qos}: \
             {want_reliability} + VOLATILE) — the requester/replier profile is not reaching \
             the Agent (issue 2120)\n{context}"
        );
    }
    eprintln!("[PASS] XRCE service endpoints hold {want_reliability} + VOLATILE ({qos})");
}

/// The 24 bytes a `GRAPH_PROBE_PUB_GID <48 hex>` line carries.
fn probe_gid(out: &str) -> Option<Vec<u8>> {
    let hex = out
        .lines()
        .find_map(|l| {
            l.trim()
                .strip_prefix(nros_tests::output::GRAPH_PROBE_PUB_GID)
        })?
        .trim();
    if hex.len() != 48 {
        return None;
    }
    (0..24)
        .map(|i| u8::from_str_radix(hex.get(2 * i..2 * i + 2)?, 16).ok())
        .collect()
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// Issue 2110 — the XRCE backend's `get_gid_for_publisher` against what a
/// STOCK graph prints for the same publisher.
///
/// ```text
/// graph-probe (XRCE, GRAPH_PROBE_GID) → XRCE Agent → DDS ← ros2 topic info --verbose (rmw_fastrtps_cpp)
/// ```
///
/// The XRCE protocol hands the client no GUID; the backend derives each
/// endpoint's (probed participant prefix + the Agent's endpoint counter, issue
/// 1292) and lists it in `ros_discovery_info`, which is how the stock peer
/// attributes the publisher to `graph_probe` at all. Asserted: the peer
/// attributes our publisher to our node, and the GID it prints for it is the
/// 16 bytes our `get_gid()` reports, the last 8 of the 24 zero. Stock
/// `rmw_microxrcedds` would print a client-local object id here, which matches
/// nothing a peer sees.
///
/// The probe's take half reports `GRAPH_PROBE_TAKE_ABSENT` on XRCE — a topic
/// sample carries no writer GUID through the Agent (`rmw-vs-upstream.md` §9b) —
/// and is not asserted.
#[rstest]
fn test_xrce_publisher_gid_is_the_one_a_stock_graph_reports() {
    use std::process::Command;

    require_xrce_agent();
    require_ros2_dds();

    use nros_tests::fixtures::RequireFixture;
    let probe_bin = nros_tests::fixtures::build_graph_probe_rmw(nros_tests::fixtures::Rmw::Xrce)
        .require("prebuilt XRCE graph-probe");
    let agent = XrceAgent::start_unique().expect("Failed to start XRCE Agent");
    let addr = agent.addr();
    let domain_id = unique_ros_domain_id();
    const TOPIC: &str = "/nros_xrce_gid_out";

    let mut cmd = Command::new(probe_bin);
    cmd.env("NROS_LOCATOR", &addr)
        .env("XRCE_AGENT_ADDR", &addr)
        .env("ROS_DOMAIN_ID", domain_id.to_string())
        .env("GRAPH_PROBE_GID", "1")
        .env("GRAPH_PROBE_GID_PUB_TOPIC", TOPIC)
        .env("GRAPH_PROBE_GID_SUB_TOPIC", "/nros_xrce_gid_in")
        .env("GRAPH_PROBE_TIMEOUT_MS", "40000");
    let mut probe =
        ManagedProcess::spawn_command(cmd, "graph-probe-gid-xrce").expect("spawn the gid probe");
    let ready = probe
        .wait_for_output_pattern(
            nros_tests::output::GRAPH_PROBE_GID_READY,
            Duration::from_secs(20),
        )
        .unwrap_or_else(|e| format!("<probe never reported ready: {e}>"));
    let ours = probe_gid(&ready);

    // Poll: the peer learns the publisher from SPDP/SEDP and the node from a
    // `ros_discovery_info` sample, in either order (issues 0705, 0761).
    let env_setup = nros_tests::ros2::ros2_env_setup_dds_with_domain(DEFAULT_ROS_DISTRO, domain_id);
    let query =
        nros_tests::ros2::ros2_query_cmd(&env_setup, 15, &format!("topic info --verbose {TOPIC}"));
    let deadline = std::time::Instant::now() + Duration::from_secs(25);
    let report = loop {
        let out = Command::new("bash")
            .args(["-c", &query])
            .output()
            .expect("run ros2 topic info");
        let report = format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        let attributed =
            !nros_tests::ros2::topic_endpoints_for_node(&report, "PUBLISHER", "graph_probe")
                .is_empty();
        if attributed || std::time::Instant::now() >= deadline {
            break report;
        }
        std::thread::sleep(Duration::from_millis(500));
    };
    probe.kill();
    drop(agent);

    let context = format!("--- probe ---\n{ready}\n--- ros2 topic info --verbose ---\n{report}");
    let ours = ours.unwrap_or_else(|| {
        panic!(
            "the probe printed no 24-byte `{}` line — get_gid_for_publisher answered \
             UNSUPPORTED or an error (issue 2110)\n{context}",
            nros_tests::output::GRAPH_PROBE_PUB_GID.trim()
        )
    });
    assert!(
        ours[..16].iter().any(|b| *b != 0) && ours[16..].iter().all(|b| *b == 0),
        "our gid must be a 16-byte DDS GUID zero-extended to 24: {}\n{context}",
        hex(&ours)
    );
    let eps = nros_tests::ros2::topic_endpoints_for_node(&report, "PUBLISHER", "graph_probe");
    let [ep] = eps.as_slice() else {
        panic!(
            "the stock graph did not attribute exactly one publisher on {TOPIC} to `graph_probe` \
             (found {}) — ros_discovery_info did not name it (issue 1292)\n{context}",
            eps.len()
        );
    };
    let theirs = nros_tests::ros2::endpoint_gid_bytes(&ep.block)
        .unwrap_or_else(|| panic!("no GID line for our publisher\n{context}"));
    assert_eq!(
        hex(&theirs[..16]),
        hex(&ours[..16]),
        "get_gid_for_publisher is not the GID a stock peer knows our publisher by\n{context}"
    );
    eprintln!(
        "[PASS] XRCE publisher gid {} = the stock graph's",
        hex(&ours[..16])
    );
}

// =============================================================================
// Issue 2106 — the graph: node names from `ros_discovery_info`, and the edge
// =============================================================================
//
// Here rather than in `graph_interop.rs` because the nano-ros side is an XRCE
// CLIENT, not a DDS participant: the Agent is what joins the peer's bus and is
// pinned by `fixtures::xrce_agent` (issue 1009), which is this file's
// `check-dds-isolation-symmetry` allowlist entry.

/// Issue 2106 — XRCE reads node names from `ros_discovery_info` through the
/// Agent, and nothing else.
///
/// ```text
/// graph-probe (XRCE) → XRCE Agent (Fast-DDS) ←DDS→ demo_nodes_cpp talker (rmw_fastrtps_cpp)
/// ```
///
/// The talker's participant publishes `ParticipantEntitiesInfo`; the backend's
/// `ros_discovery_info` reader (created on the probe's first query) receives it
/// through the Agent, so `get_node_names` lists `/talker`. Every other graph
/// read slot is a protocol limit (no builtin-topic readers, no participant
/// enumeration — `rmw-vs-upstream.md` §9b) and must say UNSUPPORTED rather
/// than answer empty: the probe records those, and this asserts the topic
/// query is one of them.
///
/// Interop cell: `native-graph-rust-xrce-r2n` (`interop::CASE_CELLS`).
#[test]
fn xrce_enumerates_a_stock_ros2_node() {
    nros_tests::fixtures::require_xrce_agent();
    nros_tests::fixtures::require_ros2_dds();
    let agent = nros_tests::fixtures::XrceAgent::start_unique().expect("start the XRCE Agent");
    let domain = nros_tests::unique_ros_domain_id();

    let _talker =
        Ros2DdsProcess::demo_nodes_cpp_talker_fastrtps_with_domain(DEFAULT_ROS_DISTRO, domain)
            .expect("start the stock talker on fastrtps");

    use nros_tests::fixtures::RequireFixture;
    let probe = nros_tests::fixtures::build_graph_probe_rmw(nros_tests::fixtures::Rmw::Xrce)
        .require("prebuilt XRCE graph-probe");
    let mut cmd = std::process::Command::new(probe);
    cmd.env("NROS_LOCATOR", agent.addr())
        .env("XRCE_AGENT_ADDR", agent.addr())
        .env("GRAPH_PROBE_EXPECT_NODE", "talker")
        .env("GRAPH_PROBE_TIMEOUT_MS", "20000")
        .env("ROS_DOMAIN_ID", domain.to_string());
    let out = cmd.output().expect("run graph-probe");
    let output = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    drop(agent);

    assert!(
        out.status.success(),
        "XRCE graph-probe must exit 0 once it sees the talker; got {:?}\n{output}",
        out.status.code()
    );
    assert!(
        output.contains(nros_tests::output::GRAPH_PROBE_SAW),
        "XRCE must ENUMERATE the stock talker from ros_discovery_info; probe said:\n{output}"
    );
    assert!(
        output
            .lines()
            .any(|l| l.starts_with("GRAPH_PROBE_TOPICS_ERR") && l.contains("Unsupported")),
        "the topic query must answer UNSUPPORTED on XRCE (a protocol limit), never an empty \
         list; probe said:\n{output}"
    );
}

/// Issue 2106 — XRCE's graph-change edge: the stock talker LEAVING reaches the
/// guard condition.
///
/// XRCE sees no DDS discovery, so the edge is a CHANGED `ros_discovery_info`
/// sample: a stock participant that shuts down cleanly destroys its node,
/// which republishes its entities info without it, and that sample is what
/// fires the edge here. A participant that dies without that last sample stays
/// listed — the bound `graph.c` documents.
///
/// Interop cell: `native-graph-rust-xrce-r2n` (`interop::CASE_CELLS`).
#[test]
fn xrce_a_peer_leaving_fires_the_graph_change_guard() {
    nros_tests::fixtures::require_xrce_agent();
    nros_tests::fixtures::require_ros2_dds();
    let agent = nros_tests::fixtures::XrceAgent::start_unique().expect("start the XRCE Agent");
    let domain = nros_tests::unique_ros_domain_id();

    let mut talker =
        Ros2DdsProcess::demo_nodes_cpp_talker_fastrtps_with_domain(DEFAULT_ROS_DISTRO, domain)
            .expect("start the stock talker on fastrtps");

    use nros_tests::fixtures::RequireFixture;
    let probe = nros_tests::fixtures::build_graph_probe_rmw(nros_tests::fixtures::Rmw::Xrce)
        .require("prebuilt XRCE graph-probe");
    let mut cmd = std::process::Command::new(probe);
    cmd.env("NROS_LOCATOR", agent.addr())
        .env("XRCE_AGENT_ADDR", agent.addr())
        .env("GRAPH_PROBE_EXPECT_NODE", "talker")
        .env("GRAPH_PROBE_WATCH_CHANGE", "1")
        .env("GRAPH_PROBE_TIMEOUT_MS", "20000")
        .env("ROS_DOMAIN_ID", domain.to_string());
    let mut probe = ManagedProcess::spawn_command(cmd, "graph-probe-change-xrce")
        .expect("spawn the XRCE graph-change probe");
    let armed = probe
        .wait_for_output_pattern(
            nros_tests::output::GRAPH_PROBE_CHANGE_ARMED,
            Duration::from_secs(45),
        )
        .expect("the probe must arm: it needs the talker in its graph first");
    assert!(
        !armed.contains(nros_tests::output::GRAPH_PROBE_CHANGE_UNSUPPORTED),
        "XRCE declined the graph-change slot; issue 2106 fills it:\n{armed}"
    );

    // SIGTERM, not the SIGKILL `drop` sends: the departure XRCE can see is the
    // talker's own last `ros_discovery_info` sample, which only a clean
    // shutdown publishes (measured: SIGINT and SIGTERM both fire the edge,
    // SIGKILL never does).
    let clean = talker.terminate();

    let rest = probe
        .wait_for_all_output(Duration::from_secs(45))
        .expect("the probe must finish after the talker leaves");
    drop(agent);
    let out = format!("{armed}{rest}");
    assert!(
        out.contains(nros_tests::output::GRAPH_PROBE_CHANGE_FIRED),
        "the stock talker leaving must reach the XRCE graph-change guard (talker exited \
         on SIGTERM alone: {clean}; `false` means it took SIGKILL and never published its \
         last ros_discovery_info sample):\n{out}"
    );
}

// =============================================================================
// Phase 183.6 — XRCE ↔ ROS 2: action (both directions) + reverse-direction
// service. The existing tests cover pub/sub both ways + service
// (xrce-server / ros2-client). These add the missing cells. DDS interop is
// best-effort (naming/version drift), so — like the tests above — they log
// PASS/INFO and only hard-fail on a clear local error, never on a discovery
// miss. nano-XRCE nodes bridge to DDS via the XRCE Agent; ROS 2 uses
// rmw_fastrtps_cpp on the same ROS_DOMAIN_ID.
// =============================================================================

/// nano-XRCE action server ↔ ROS 2 (DDS) action client (`ros2 action send_goal`).
#[rstest]
fn test_xrce_action_ros2_client(xrce_action_server_binary: PathBuf) {
    use std::process::Command;
    require_xrce_agent();
    require_ros2_dds();
    let agent = XrceAgent::start_unique().expect("Failed to start XRCE Agent");
    let addr = agent.addr();
    let domain_id = unique_ros_domain_id();

    let mut server_cmd = Command::new(&xrce_action_server_binary);
    server_cmd
        .env("NROS_LOCATOR", &addr)
        .env("XRCE_AGENT_ADDR", &addr)
        .env("ROS_DOMAIN_ID", domain_id.to_string())
        .env("XRCE_TIMEOUT", "30");
    let mut server = ManagedProcess::spawn_command(server_cmd, "xrce-action-server")
        .expect("Failed to start xrce action server");
    let _ = server.wait_for_output_pattern("Action server ready", Duration::from_secs(8));
    std::thread::sleep(Duration::from_secs(1));

    let mut ros2_client = match Ros2DdsProcess::action_send_goal_with_domain(
        "/fibonacci",
        "example_interfaces/action/Fibonacci",
        "{order: 5}",
        DEFAULT_ROS_DISTRO,
        domain_id,
    ) {
        Ok(p) => p,
        Err(e) => {
            server.kill();
            nros_tests::unmet!(
                "ROS 2 DDS action client could not start (requires ros-humble-example-interfaces): {e}"
            );
        }
    };
    let ros2_output = ros2_client
        .wait_for_output(Duration::from_secs(20))
        .unwrap_or_default();
    server.kill();
    drop(agent);

    eprintln!("ROS 2 DDS action client output:\n{ros2_output}");
    // A real `ros2 action send_goal --feedback` against the nano-ros XRCE
    // action server accepts the goal, streams feedback, and delivers the final
    // result — exercising the send_goal/get_result services + feedback topic
    // with ROS-2-correct names/types, the fixed `uint8[16]` goal-id framing
    // (233.6), and the deferred get_result reply (Phase 237): rclcpp_action
    // sends get_result right after acceptance and the server holds the reply
    // until the goal terminates.
    let accepted = ros2_output.contains("Goal accepted");
    let got_feedback = ros2_output.contains("- 0");
    let got_result = ros2_output.contains("SUCCEEDED") || ros2_output.contains("Result:");
    assert!(
        accepted && got_feedback && got_result,
        "XRCE action server ↔ ROS 2 DDS client did not complete (233.6 / 237): \
         accepted={accepted} feedback={got_feedback} result={got_result}.\n{ros2_output}"
    );
    eprintln!("[PASS] XRCE action server ↔ ROS 2 DDS client: accept + feedback + result");
}

/// Phase 237 — TWO concurrent ROS 2 action clients against one nano-ros XRCE
/// action server (concurrent mode). Both goals are active at once, so both
/// `rclcpp_action` clients have an early `get_result` deferred SIMULTANEOUSLY —
/// exercising the XRCE seq-keyed reply-token table with multiple in-flight
/// replies (the case a single stored `SampleIdentity` would cross-wire). Each
/// client must get its OWN `SUCCEEDED` result.
#[rstest]
fn test_xrce_action_ros2_concurrent(xrce_action_server_concurrent_binary: PathBuf) {
    use std::process::Command;
    require_xrce_agent();
    require_ros2_dds();
    let agent = XrceAgent::start_unique().expect("Failed to start XRCE Agent");
    let addr = agent.addr();
    let domain_id = unique_ros_domain_id();

    // Concurrent fixture bin: accepts + advances several goals at once,
    // draining get_result every spin so two early get_result requests are
    // held simultaneously (phase-277 W5 — was the example's
    // NROS_ACTION_CONCURRENT mode).
    let mut server_cmd = Command::new(&xrce_action_server_concurrent_binary);
    server_cmd
        .env("NROS_LOCATOR", &addr)
        .env("XRCE_AGENT_ADDR", &addr)
        .env("ROS_DOMAIN_ID", domain_id.to_string())
        .env("XRCE_TIMEOUT", "40")
        .env("RUST_LOG", "info");
    let mut server = ManagedProcess::spawn_command(server_cmd, "xrce-action-server-concurrent")
        .expect("Failed to start xrce action server");
    let _ = server.wait_for_output_pattern("Action server ready", Duration::from_secs(8));
    std::thread::sleep(Duration::from_secs(1));

    // Long-running goals (order 20 ≈ 2 s) so both overlap and both early
    // get_results are deferred at the same time (the reply-token table this test
    // guards). A short 0.5 s stagger absorbs the agent-side DDS discovery /
    // acceptance-reply timing between the two clients — the request-inbox ring
    // (Phase 237 follow-up) removes the *drop-on-collision* failure mode (and is
    // covered unstaggered by the Zenoh concurrent e2e + the buffer unit test),
    // but exact sub-millisecond simultaneity over the agent is otherwise flaky.
    let spawn_client = || {
        Ros2DdsProcess::action_send_goal_with_domain(
            "/fibonacci",
            "example_interfaces/action/Fibonacci",
            "{order: 20}",
            DEFAULT_ROS_DISTRO,
            domain_id,
        )
    };
    let mut c1 = match spawn_client() {
        Ok(p) => p,
        Err(e) => {
            server.kill();
            nros_tests::unmet!("ROS 2 DDS action client could not start: {e}");
        }
    };
    std::thread::sleep(Duration::from_millis(500));
    let mut c2 = match spawn_client() {
        Ok(p) => p,
        Err(e) => {
            server.kill();
            nros_tests::unmet!("ROS 2 DDS action client 2 could not start: {e}");
        }
    };

    let out1 = c1
        .wait_for_output(Duration::from_secs(30))
        .unwrap_or_default();
    let out2 = c2
        .wait_for_output(Duration::from_secs(30))
        .unwrap_or_default();
    server.kill();
    drop(agent);

    eprintln!("client 1 output:\n{out1}\nclient 2 output:\n{out2}");
    let ok1 = out1.contains("SUCCEEDED");
    let ok2 = out2.contains("SUCCEEDED");
    assert!(
        ok1 && ok2,
        "concurrent action: both clients must get their own SUCCEEDED result \
         (237 reply-token table): client1={ok1} client2={ok2}"
    );
    eprintln!("[PASS] two concurrent ROS 2 action clients ↔ nano-ros XRCE server: both SUCCEEDED");
}

/// ROS 2 (DDS) action server ↔ nano-XRCE action client (reverse direction).
#[rstest]
fn test_ros2_action_xrce_client(xrce_action_client_binary: PathBuf) {
    use std::process::Command;
    require_xrce_agent();
    require_ros2_dds();
    let agent = XrceAgent::start_unique().expect("Failed to start XRCE Agent");
    let addr = agent.addr();
    let domain_id = unique_ros_domain_id();

    let mut ros2_server =
        match Ros2DdsProcess::action_server_fibonacci_with_domain(DEFAULT_ROS_DISTRO, domain_id) {
            Ok(p) => p,
            Err(e) => {
                // NOT `action_tutorials_py` — this peer is an inline rclpy script
                // (`Ros2DdsProcess::action_server_fibonacci_with_domain`) whose only
                // import is `example_interfaces.action.Fibonacci`, chosen precisely
                // so the cell does not depend on the tutorials package. Naming the
                // wrong package sends the reader to install something that would
                // not have helped.
                nros_tests::unmet!(
                    "ROS 2 DDS fibonacci action server (inline rclpy, needs rclpy + \
                 example_interfaces) could not start: {e}"
                );
            }
        };
    // Issue 0687 — WAIT for the server to say it is ready; do not sleep a
    // constant and hope.
    //
    // This was `sleep(6)`, with a comment recording that 3 s "was too tight
    // under test load" — tuning a race window rather than removing it. The
    // window is real: rclpy must import, create 5 action entities and announce
    // them over DDS before the client's `send_goal` fires, and the nano-ros
    // action client sends its goal ONCE with no wait_for/retry. Miss the match
    // and the goal is simply dropped, so the client then waits out its full 20 s
    // budget for a result that can never arrive — a ~30 s failure whose cause is
    // six seconds earlier and invisible.
    //
    // The script has printed `SERVER READY` with `flush=True` all along, under
    // `python3 -u … 2>&1`; nothing was reading it. Waiting on it costs the ~4 s
    // the server actually needs instead of a fixed 6, and does not expire when a
    // loaded host needs 9.
    ros2_server
        .wait_for_output_count(
            nros_tests::output::ROS2_ACTION_SERVER_READY,
            1,
            Duration::from_secs(30),
        )
        .expect("ROS 2 fibonacci action server never announced readiness");
    // The entities are announced; DDS still has to propagate them to the agent's
    // requesters. This is a genuine settle, not a stand-in for a signal.
    std::thread::sleep(Duration::from_secs(1));

    let mut client_cmd = Command::new(&xrce_action_client_binary);
    client_cmd
        .env("NROS_LOCATOR", &addr)
        .env("XRCE_AGENT_ADDR", &addr)
        .env("ROS_DOMAIN_ID", domain_id.to_string())
        .env("XRCE_TIMEOUT", "30")
        // Action result / status logs are INFO-level — surface them.
        .env("RUST_LOG", "info");
    let mut client = ManagedProcess::spawn_command(client_cmd, "xrce-action-client")
        .expect("Failed to start xrce action client");
    // The example streams `Next number in sequence received: [...]` feedback
    // and ends with the terminal `Result received: [...]` line.
    let client_output = client.collect_until(
        nros_tests::output::ACTION_RESULT_PREFIX,
        Duration::from_secs(20),
    );
    client.kill();
    // Drain the ROS 2 server BEFORE killing it. Its script prints `SERVER READY`
    // at startup and `SERVER DONE [...]` after `execute` returns, so this one
    // capture says whether the goal ever reached the server — which is exactly
    // what the client's output cannot tell us (issue 0448: the client logs
    // "Goal accepted by server" on local SEND success, not on acceptance).
    // `wait_for_output` TAKES stdout, so it is called once, here.
    let server_output = ros2_server
        .wait_for_output(Duration::from_secs(2))
        .unwrap_or_default();
    ros2_server.kill();
    drop(agent);

    eprintln!("XRCE action client output:\n{client_output}");
    eprintln!("ROS 2 action server output:\n{server_output}");
    // Goal acceptance proves the send_goal request crossed XRCE→agent→DDS and
    // the reply came back (goal_id framing + per-channel service types). At
    // least one non-empty feedback proves the feedback topic + UUID framing
    // round-trips from a real `rcl_action` server.
    // Issue 1918 — the Rust client says it SENT the goal, not that it was
    // accepted, so the shared predicate reads acceptance off the result. A
    // literal "Goal accepted" here went false for that reason alone.
    let accepted = nros_tests::output::action_goal_accepted(&client_output);
    let got_feedback = client_output.contains(&format!(
        "{} [0, 1",
        nros_tests::output::ACTION_FEEDBACK_PREFIX
    ));
    assert!(
        accepted && got_feedback,
        "ROS 2 DDS action server ↔ XRCE action client did not complete (233.6): \
         accepted={accepted} got_feedback={got_feedback}.\n\
         client:\n{client_output}\n\
         server (SERVER DONE present ⇒ the goal reached it and it replied):\n{server_output}"
    );

    // Issue 0448 — the payload assertion. `accepted` and `got_feedback` are both
    // satisfiable without the goal ever reaching the server (the client logs
    // acceptance on local SEND success), so neither proves delivery. The result
    // sequence is a FUNCTION of `goal.order`, and this is the only action test
    // whose peer is a real `rcl_action` server that computes it — the native
    // example servers use their own conventions and one ignores `order`
    // entirely (issue 0453). Order 10 ⇒ `order + 1` = 11 elements; a dropped
    // goal yields the zeroed 12-byte default result and an empty sequence.
    assert!(
        client_output.contains(nros_tests::output::FIBONACCI_ORDER_10_SEQUENCE),
        "XRCE action client did not log the order-10 sequence `{}` — the goal \
         payload did not survive the round-trip (0448: a `Result received:` line \
         proves decode, not delivery).\n\
         client:\n{client_output}\n\
         server:\n{server_output}",
        nros_tests::output::FIBONACCI_ORDER_10_SEQUENCE,
    );
    eprintln!(
        "[PASS] ROS 2 DDS action server ↔ XRCE action client: goal accepted + feedback + result payload"
    );
}

/// ROS 2 (DDS) service server ↔ nano-XRCE service client (reverse direction).
#[rstest]
fn test_ros2_service_xrce_client(xrce_service_client_binary: PathBuf) {
    use std::process::Command;
    require_xrce_agent();
    require_ros2_dds();
    let agent = XrceAgent::start_unique().expect("Failed to start XRCE Agent");
    let addr = agent.addr();
    let domain_id = unique_ros_domain_id();

    let mut ros2_server = match Ros2DdsProcess::add_two_ints_server_with_domain(
        DEFAULT_ROS_DISTRO,
        domain_id,
    ) {
        Ok(p) => p,
        Err(e) => {
            nros_tests::unmet!(
                "ROS 2 DDS add_two_ints server could not start (requires ros-humble-example-interfaces): {e}"
            );
        }
    };
    // Wait for the server to SAY it is ready, rather than sleeping a fixed
    // budget and hoping. This used to be `let _ = wait_for_output(5s)`, which
    // could not work for two compounding reasons: it discarded the output (so a
    // failure could not say whether the server ever came up), and the rclpy
    // helper ran under a BUFFERED `python3`, so "Service server ready" was still
    // sitting in Python's stdio buffer when the wait expired. The wait was
    // therefore a 5 s timeout that never observed readiness, and the client was
    // started against a server that might or might not be listening — the race
    // this test kept losing. The helpers now run `python3 -u`.
    let server_startup = ros2_server
        .wait_for_output_count(
            nros_tests::output::ROS2_SERVICE_SERVER_READY,
            1,
            Duration::from_secs(20),
        )
        .unwrap_or_else(|e| format!("<server never reported ready: {e}>"));
    std::thread::sleep(Duration::from_secs(1));

    let mut client_cmd = Command::new(&xrce_service_client_binary);
    client_cmd
        .env("NROS_LOCATOR", &addr)
        .env("XRCE_AGENT_ADDR", &addr)
        .env("ROS_DOMAIN_ID", domain_id.to_string())
        .env("XRCE_TIMEOUT", "30")
        // The client logs "Result of add_two_ints: ..." at INFO — surface it
        // so the assert below can see the reply (without this the test can't
        // tell success from failure).
        .env("RUST_LOG", "info");
    let mut client = ManagedProcess::spawn_command(client_cmd, "xrce-service-client")
        .expect("Failed to start xrce service client");
    let client_output = client.collect_until(
        nros_tests::output::SERVICE_RESULT_PREFIX,
        Duration::from_secs(20),
    );
    // The ROS 2 server's own output is half the evidence: "no reply" from a
    // client that never had a server to talk to is a DIFFERENT failure from a
    // client that could not reach a healthy one, and discarding it makes the two
    // read identically (the shape of issue 0670).
    let server_output = format!("{server_startup}\n[domain_id={domain_id} agent={addr}]");
    client.kill();
    ros2_server.kill();
    drop(agent);

    eprintln!("XRCE service client output:\n{client_output}");
    eprintln!("ROS 2 add_two_ints server output:\n{server_output}");
    // Phase 233.6 wave 2 — reverse direction (nano-ros XRCE service client →
    // real ROS 2 service server). Hard assert: the client must get a reply.
    assert!(
        client_output.contains(nros_tests::output::SERVICE_RESULT_PREFIX),
        "nano-ros XRCE service client got no reply from the ROS 2 service server \
         — XRCE-DDS reverse service interop regression.\nclient:\n{client_output}\nROS 2 server:\n{server_output}"
    );
    eprintln!("[PASS] ROS 2 DDS service server ↔ XRCE service client: reply received");
}

// Issue 0352 / phase-324 — bind this test to `interop::CELLS`. The coordinates
// below must equal what the list declares for `xrce_ros2_interop`; adding/retiring an
// interop cell for this test, or drifting a cell's coordinate (issue 0341
// defect 2), turns this RED. Needs no fixtures — runs in tier 1.
#[test]
fn cases_bound_to_interop_cells() {
    #[allow(unused_imports)]
    use nros_tests::matrix::{Lang::*, PlatformId::*, Rmw::*, Workload::*};
    nros_tests::interop::assert_test_bound(
        "xrce_ros2_interop",
        // Issue 2106 added the Graph cell (`native-graph-rust-xrce-r2n`).
        &[
            (Linux, Rust, Xrce, Pubsub),
            (Linux, Rust, Xrce, Service),
            (Linux, Rust, Xrce, Graph),
        ],
    );
}
