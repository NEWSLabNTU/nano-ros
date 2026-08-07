//! Shared output validation utilities for integration tests.
//!
//! All nano-ros standalone examples match the official ROS 2 demo wording
//! (phase-277 W4 chatter, W5 service/action):
//! - Talker: `"Publishing: 'Hello World: N'"`
//! - Listener: `"I heard: [Hello World: N]"`
//! - Service server: `"Incoming request"` + `"a: A b: B"`;
//!   client: `"Result of add_two_ints: N"`
//! - Action server: `"Received goal request with order N"`, `"Executing goal"`,
//!   `"Publish feedback"`, `"Goal succeeded"`; client: `"Sending goal"`,
//!   `"Goal accepted by server, waiting for result"`,
//!   `"Next number in sequence received: [...]"`, `"Result received: [...]"`
//!
//! This module provides `parse_*` functions to extract structured data from
//! process output, and `assert_*` convenience functions that panic with
//! diagnostic messages on failure.
//!
//! phase-277 W2.a — [`TALKER_LOG_PREFIX`] / [`LISTENER_LOG_PREFIX`] (plus the
//! [`talker_line`] / [`listener_line`] helpers) are the SINGLE source of truth
//! for the standalone talker/listener chatter wording. Every test that
//! asserts on the plain talker/listener example output (any platform / RMW /
//! language variant of `examples/*/talker` + `examples/*/listener`) should go
//! through these instead of hard-coding the wording, so a future wording flip
//! stays a one-file change. This does NOT apply to nodes with their own
//! wording (workspace feature packages like the QoS/lifecycle demos, bridge
//! forwarders, or purpose-built test bins) — see
//! `packages/testing/nros-tests/tests/*.rs` call sites for the per-test
//! rationale.

/// The talker (publisher) log-line prefix used by the standalone
/// talker/listener chatter examples (`"Publishing:"`, as in the official
/// ROS 2 demo `Publishing: 'Hello World: N'`).
pub const TALKER_LOG_PREFIX: &str = "Publishing:";

/// The listener (subscriber) log-line prefix used by the standalone
/// talker/listener chatter examples (`"I heard:"`, as in the official
/// ROS 2 demo `I heard: [Hello World: N]`).
pub const LISTENER_LOG_PREFIX: &str = "I heard:";

/// Readiness marker: the talker is considered alive once it prints its
/// first chatter line. phase-277 W4 dropped the separate
/// `"Publishing messages"` boot banner, so "talker up" == "it printed its
/// first `Publishing:` line". Kept as a distinct constant so call sites
/// that only need liveness (not a specific N) stay self-documenting.
pub const TALKER_READY_MARKER: &str = TALKER_LOG_PREFIX;

/// Readiness marker: the Rust chatter LISTENER is subscribed and ready.
///
/// `examples/native/rust/listener` prints
/// `Subscriber created for topic: /chatter` once its subscription exists —
/// the line its own source names as the readiness gate.
///
/// Issue 0471: several suites (`qos`, `multi_node`, `safety_e2e`,
/// `nano2nano`) waited for the literal `"Waiting for"` instead, a banner this
/// binary does not print. That wait could never succeed, and NOTHING noticed,
/// because `wait_for_output_pattern` returned `Ok` on timeout as long as the
/// process had printed anything — which a starting listener always has. The
/// literal is exactly what the "use `output::*` constants, never literal
/// strings" rule exists to prevent (phase-277); this constant is the fix, and
/// the strict wait is what made the breakage visible.
pub const LISTENER_READY_MARKER: &str = "Subscriber created for topic:";

/// Readiness marker for the SAFETY chatter listener
/// (`safety_chatter_listener`), which prints
/// `Safety subscriber created for topic: /chatter`.
///
/// Deliberately a separate constant rather than a prefix of
/// [`LISTENER_READY_MARKER`]: the safety binary spells it
/// `Safety subscriber` (lower-case `s`), so the plain listener's marker is
/// NOT a substring of it and matching one against the other would silently
/// never fire — the same failure mode issue 0471 exposed.
pub const SAFETY_LISTENER_READY_MARKER: &str = "Safety subscriber created for topic:";

/// Talker line WITH the opening payload quote — distinguishes a real
/// publish line from setup prose containing "Publishing" (phase-295 W2).
pub const TALKER_PAYLOAD_PREFIX: &str = "Publishing: '";

/// MessageInfo attachment trace (issue 0429). The zenoh publisher shim logs the
/// per-message MessageInfo it stamps into the wire attachment under `RUST_LOG=trace`
/// (`nros-rmw-zenoh/src/shim/publisher.rs`): `… with attachment: seq=N, ts=…,
/// gid=[..]`. This is the authoritative source of the sequence/GID a subscriber
/// then reads — the DEMO listener is slim and no longer traces the receive side, so
/// tests observe the values here. The line marker proves the attachment path fired.
pub const MESSAGE_INFO_ATTACHMENT_MARKER: &str = "with attachment:";
/// The per-message sequence number inside [`MESSAGE_INFO_ATTACHMENT_MARKER`]
/// (`seq=N,`). Monotonic per publisher.
pub const MESSAGE_INFO_SEQ_PREFIX: &str = "seq=";
/// The publisher GID inside [`MESSAGE_INFO_ATTACHMENT_MARKER`] (`gid=[..]`).
/// Constant per publisher.
pub const MESSAGE_INFO_GID_PREFIX: &str = "gid=";

/// Pre-W4 Int32 chatter wording, retained by nodes OUTSIDE the phase-277 W4
/// demo-parity flip: the purpose-built fixture bins
/// (`packages/testing/nros-tests/bins/{param,safety,header}-chatter-*`,
/// `int32-sink`), the workspace demo packages
/// (`examples/workspaces/{rust,c,cpp,mixed,ws-*}`), and the nros-bench
/// stress bins. Tests that assert on THOSE outputs use these constants, so
/// the standalone-example constants above can evolve independently.
pub const INT32_TALKER_LOG_PREFIX: &str = "Published:";

/// See [`INT32_TALKER_LOG_PREFIX`] — the listener/sink side (`"Received:"`).
pub const INT32_LISTENER_LOG_PREFIX: &str = "Received:";

/// issue 0441 — the receive-side `MessageInfo` marker emitted by the
/// `message-info-observer` bin (`seq=<n> gid=<hex> ts=<t>`).
///
/// A constant rather than a literal for the reason this whole module exists:
/// the previous zero-copy assertion grepped `seq=` out of the listener EXAMPLE,
/// and when phase-277 slimmed that example to the two lines a ROS 2 demo prints
/// the test kept looking for a string nothing emitted any more. The observer is
/// now the one producer, and this is the one spelling of what it produces.
pub const MESSAGE_INFO_LOG_PREFIX: &str = "seq=";

/// issues 0459 / 0460 — tell "produced output that LACKS marker X" apart from
/// "produced nothing at all", and say which.
///
/// A narrow assertion at the end of a chain names the last missing thing, so an
/// image that emitted nothing after its boot banner gets reported as, say, a
/// missing EDF marker — and the reader goes looking at the scheduler. Issue
/// 0459 was exactly that: the Zephyr C++ realtime entry produced four lines
/// total and the failure said `expected exactly 1 "nros: EDF deadline set
/// tier=", saw 0`. It is not a scheduling problem; it never reached tier
/// startup. Issue 0460's message went further and blamed a subsystem
/// ("the embedded LAUNCH-entry runtime delivery did not work") from the
/// OBSERVER's silence, without showing the guest's output at all.
///
/// The signal is whether the nano-ros runtime ever spoke. Every runtime line
/// this project emits carries `nros` (`nros: …`, `[nros] …`, or a target of
/// `nros_*` under `env_logger`), so zero such lines means the image did not
/// reach application code and a missing application marker says nothing about
/// that marker.
///
/// Returns `None` when the runtime did speak — then a missing marker really is
/// about the marker, and the caller's own message is the right one.
pub fn runtime_silence_note(log: &str) -> Option<String> {
    let lines: Vec<&str> = log.lines().filter(|l| !l.trim().is_empty()).collect();
    if lines.iter().any(|l| l.contains("nros")) {
        return None;
    }
    let tail = lines
        .iter()
        .rev()
        .take(3)
        .rev()
        .map(|l| format!("    {}", l.trim_end()))
        .collect::<Vec<_>>()
        .join("\n");
    Some(format!(
        "NO RUNTIME OUTPUT: {} non-empty line(s), none from the nano-ros runtime.\n           The image did not reach application code, so a missing marker below is NOT \n           evidence about that marker — look between boot and the first `nros` line \n           (issues 0459, 0460).{}",
        lines.len(),
        if tail.is_empty() {
            String::new()
        } else {
            format!("\n  last line(s) seen:\n{tail}")
        }
    ))
}

/// The exact `int32-sink` / workspace-listener log line for value `n`
/// (`"Received: N"`).
pub fn int32_listener_line(n: impl std::fmt::Display) -> String {
    format!("{INT32_LISTENER_LOG_PREFIX} {n}")
}

/// The parameter values the `features` workspace's `param_talker` resolves to,
/// and the two WRONG values that each name a specific resolution rule.
///
/// `demo_bringup/system.toml` gives the node an inline `publish_period_ms` and a
/// LATER `params_files` entry whose `param_talker:` block sets 120 and whose
/// `/**:` block sets 999. ROS applies parameter sources in list order, and a
/// node's own block beats `/**` within a file, so the resolved value is 120.
///
/// These live here rather than in one test file because two suites assert on
/// them — `param_live_read_e2e` (the nros<->nros half) and `params`
/// (the `ros2 param set` reconfig half) — and they disagreed: the reconfig test
/// waited for 250, which is the value that means the params FILE was dropped.
/// It passed only while a stale resolver was dropping it (issue 0409), so the
/// test encoded the bug and turned RED when the bug was fixed. One constant, one
/// meaning.
pub mod param_talker {
    /// What the node must publish: the params-file value for its own block.
    pub const RESOLVED: i64 = 120;
    /// The inline value. Seeing it means source ORDERING was lost — an inline
    /// value beat a later param file (play_launch issue 0007).
    pub const ORDERING_LOST: i64 = 250;
    /// The `/**` value. Seeing it means within-file SPECIFICITY was lost — the
    /// wildcard block beat the node's own because it is written later.
    pub const SPECIFICITY_LOST: i64 = 999;
}

/// The exact Int32 fixture-talker log line for value `n` (`"Published: N"`).
pub fn int32_talker_line(n: impl std::fmt::Display) -> String {
    format!("{INT32_TALKER_LOG_PREFIX} {n}")
}

/// Readiness marker of the `int32-sink` fixture bin: it prints
/// `"Waiting for Int32 messages on <topic>..."` once its subscription is
/// live (its boot banner also contains `"Listener"`, but this line is the
/// post-subscribe signal every observer spawn should key on).
pub const INT32_SINK_READY_MARKER: &str = "Waiting for Int32";

/// FreeRTOS realtime-tier workspace nodes (`ws-realtime-{c,cpp}-mps2`)
/// print `"[<tier>] tick=N"` on the QEMU serial console **only when the
/// tier's publish succeeds** — the marker doubles as a delivery proof for
/// lanes observed via serial output instead of host-side subscribers.
pub fn tier_tick_marker(tier: impl std::fmt::Display) -> String {
    format!("[{tier}] tick=")
}

/// The exact talker log line for sequence value `n`
/// (`"Publishing: 'Hello World: N'"`).
pub fn talker_line(n: impl std::fmt::Display) -> String {
    format!("{TALKER_LOG_PREFIX} 'Hello World: {n}'")
}

// ---------------------------------------------------------------------------
// Workspace entry-pkg wording (phase-295 W3.b consolidation).
//
// Markers printed by the `examples/workspaces/*` demo packages and the
// `nros::main!` hosted spin — consumed by the multihost / roundtrip matrix
// files. Single source, same one-file-flip rationale as the demo constants.
// ---------------------------------------------------------------------------

/// `nros::main!` env-gated hosted spin exit marker: the entry prints a
/// `"nros: hosted spin complete …"` line (with its callback counters) once
/// its `NROS_ENTRY_SPIN_MS` budget elapses.
pub const HOSTED_SPIN_COMPLETE_MARKER: &str = "hosted spin complete";

/// Counter key inside the hosted-spin exit line (`"message_callbacks=N"`) —
/// N is how many subscription callbacks fired during the spin.
pub const HOSTED_SPIN_MESSAGE_CALLBACKS_KEY: &str = "message_callbacks=";

/// Readiness marker of the C workspace listener component
/// (`"Waiting for messages"`); the C++ workspace listener prints NO ready
/// marker (its observers settle on a fixed delay instead).
pub const WS_C_LISTENER_READY_MARKER: &str = "Waiting for messages";

/// Readiness marker of the C/C++ workspace service + action SERVER
/// components (`"server ready"`).
pub const WS_SERVER_READY_MARKER: &str = "server ready";

/// Per-reply prefix of the C/C++ workspace service CLIENT component
/// (`"sum: N"` for each server-computed AddTwoInts reply).
pub const WS_SERVICE_CLIENT_SUM_PREFIX: &str = "sum:";

/// The exact C/C++ workspace service-client reply line for `sum`
/// (`"sum: N"`).
pub fn ws_service_client_sum_line(sum: impl std::fmt::Display) -> String {
    format!("{WS_SERVICE_CLIENT_SUM_PREFIX} {sum}")
}

/// The C/C++ workspace action CLIENT result line for the last sequence
/// element (`"result last=N"` — 55 for a Fibonacci goal of order 10).
pub fn ws_action_result_last_line(n: impl std::fmt::Display) -> String {
    format!("result last={n}")
}

/// Per-publish prefix of the ws-custom-msg workspace talker components
/// (`"sent seq=N"` — C/C++/mixed `reading_talker_pkg`).
pub const WS_CUSTOM_MSG_SENT_PREFIX: &str = "sent seq=";

/// Per-receive prefix of the ws-custom-msg workspace listener components
/// (`"reading seq=N …"` — the decoded `sequence` field of
/// `custom_msgs/Reading`).
pub const WS_CUSTOM_MSG_READING_PREFIX: &str = "reading seq=";

/// Decoded second field of the ws-custom-msg listener line (`"temp="`) —
/// proves the full CDR layout, not just a counter, survives the trip.
pub const WS_CUSTOM_MSG_TEMP_FIELD: &str = "temp=";

/// The Rust workspace `talker_pkg`'s per-tick `nros_info!` line marker
/// (`"talker publishing chatter seq=N"` — phase-263 A5 logging demo).
pub const WS_RUST_LOGGING_MARKER: &str = "talker publishing chatter";

/// Issue 0469 — the per-tick line the phase-209 port template publishes. The
/// template vendors the upstream ROS 2 tutorial's `minimal_publisher.cpp`
/// VERBATIM, so this string is the tutorial's, not ours: changing the template
/// to make a test pass would defeat the point of the fixture (a stock rclcpp
/// node building and running against nano-ros unmodified).
pub const CPP_PORT_PUBLISH_MARKER: &str = "Publishing: 'Hello, world!";

/// The C workspace talker's per-tick `NROS_LOG_INFO` line marker
/// (`"c_talker logging seq=N"`); the MIXED workspace reuses the C talker,
/// so its logging cell greps the same marker.
pub const WS_C_LOGGING_MARKER: &str = "c_talker logging";

/// The C++ workspace talker's per-tick `NROS_LOG_INFO` line marker
/// (`"cpp_talker logging seq=N"`).
pub const WS_CPP_LOGGING_MARKER: &str = "cpp_talker logging";

// ---------------------------------------------------------------------------
// Service (AddTwoInts) demo wording — phase-277 W5.
//
// Single source of truth for the standalone service-server / service-client
// example wording (any platform / RMW / language variant), matching the
// official `demo_nodes_cpp` `add_two_ints` demo. Same one-file-flip rationale
// as the chatter constants above. Workspace feature packages and purpose-built
// test bins keep their own wording.
// ---------------------------------------------------------------------------

/// First line the service server logs per request (`"Incoming request"`).
pub const SERVICE_INCOMING_REQUEST_MARKER: &str = "Incoming request";

/// Second line the service server logs per request (`"a: A b: B"`).
pub fn service_request_line(a: impl std::fmt::Display, b: impl std::fmt::Display) -> String {
    format!("a: {a} b: {b}")
}

/// Prefix of the service client's single result line
/// (`"Result of add_two_ints:"`, as in the official demo
/// `Result of add_two_ints: 5`).
pub const SERVICE_RESULT_PREFIX: &str = "Result of add_two_ints:";

/// The exact service client result line for `sum`
/// (`"Result of add_two_ints: N"`).
pub fn service_result_line(sum: impl std::fmt::Display) -> String {
    format!("{SERVICE_RESULT_PREFIX} {sum}")
}

/// Readiness marker: the service server prints a line containing this once
/// its service is up (`"Waiting for service requests"`).
pub const SERVICE_SERVER_READY_MARKER: &str = "Waiting for service requests";

// ---------------------------------------------------------------------------
// Action (Fibonacci) demo wording — phase-277 W5.
//
// Single source of truth for the standalone action-server / action-client
// example wording, matching the official `action_tutorials` fibonacci demo
// (feedback/result sequences printed rclpy-style: `[0, 1, 1, 2, ...]`).
// ---------------------------------------------------------------------------

/// Action server log prefix when a goal request arrives
/// (`"Received goal request with order"`, followed by the order).
pub const ACTION_GOAL_REQUEST_PREFIX: &str = "Received goal request with order";

/// issue 0461 — the order every `action-client` example sends.
///
/// Named so a test can assert the value ROUND-TRIPS rather than merely that a
/// goal arrived. Nothing checked this, which is how a server that read `1`
/// (Rust), `256` (C/C++) or `0` for every goal passed every action e2e for
/// months: the assertions covered delivery markers, and the one consumer of the
/// decoded order was a log line.
pub const ACTION_GOAL_ORDER: i32 = 10;

/// Parse the order out of a server's `Received goal request with order N` line.
///
/// `None` when the line is absent or unparseable — the caller decides whether
/// that is a failure, since some cells legitimately never receive a goal.
pub fn goal_order_in(log: &str) -> Option<i32> {
    log.lines()
        .find_map(|l| l.split_once(ACTION_GOAL_REQUEST_PREFIX))
        .and_then(|(_, rest)| rest.split_whitespace().next())
        .and_then(|tok| tok.trim_end_matches(['.', ',']).parse().ok())
}

/// Action server log line when goal execution starts (`"Executing goal"`).
pub const ACTION_EXECUTING_MARKER: &str = "Executing goal";

/// Action server log line on each feedback publish (`"Publish feedback"`).
pub const ACTION_PUBLISH_FEEDBACK_MARKER: &str = "Publish feedback";

/// Action server log line when the goal succeeds (`"Goal succeeded"`).
pub const ACTION_GOAL_SUCCEEDED_MARKER: &str = "Goal succeeded";

/// Readiness marker: the action server prints a line containing this once
/// its action is up (`"Waiting for action goals"`).
pub const ACTION_SERVER_READY_MARKER: &str = "Waiting for action goals";

/// Action client log line before sending the goal (`"Sending goal"`).
pub const ACTION_SENDING_GOAL_MARKER: &str = "Sending goal";

/// Multi-goal stress fixture (`bins/action-client-multigoal`, issue 0322) —
/// the one summary line it prints after sending every goal.
///
/// The whole regression lives in this line's numbers: with a server whose
/// `active_goals` table holds `MAX_GOALS` (4), a 6-goal run must report
/// `accepted=4 rejected=2`. Before the fix it reported `accepted=6 rejected=0`
/// — the overflow goals were acknowledged and then dropped.
pub const MULTIGOAL_SUMMARY_PREFIX: &str = "multigoal: summary accepted=";

/// The exact summary line for a completed multi-goal run.
pub fn multigoal_summary_line(accepted: usize, rejected: usize, total: usize) -> String {
    format!("{MULTIGOAL_SUMMARY_PREFIX}{accepted} rejected={rejected} of {total}")
}

/// Action client log PREFIX once the server accepts the goal
/// (`"Goal accepted by server"`). The stock demo continues
/// `", waiting for result"` — see [`ACTION_GOAL_ACCEPTED_MARKER`] for the full
/// line. The prefix exists separately because `output_marker_gate` scans for
/// the shortest form a test might spell; matching only the full line would let
/// a bare `"Goal accepted by server"` literal through.
pub const ACTION_GOAL_ACCEPTED_PREFIX: &str = "Goal accepted by server";

/// Action client log line once the server accepts the goal
/// (`"Goal accepted by server, waiting for result"`) —
/// [`ACTION_GOAL_ACCEPTED_PREFIX`] plus `", waiting for result"`.
pub const ACTION_GOAL_ACCEPTED_MARKER: &str = "Goal accepted by server, waiting for result";

/// Action client log prefix for each feedback sample
/// (`"Next number in sequence received:"`, followed by the partial sequence
/// like `[0, 1, 1, 2]`).
pub const ACTION_FEEDBACK_PREFIX: &str = "Next number in sequence received:";

/// Action client terminal log prefix for the result
/// (`"Result received:"`, followed by the full sequence).
pub const ACTION_RESULT_PREFIX: &str = "Result received:";

/// The full-sequence suffix for a Fibonacci goal of order 10, as the action
/// client prints it (`"[0, 1, 1, 2, 3, 5, 8, 13, 21, 34, 55]"`).
pub const FIBONACCI_ORDER_10_SEQUENCE: &str = "[0, 1, 1, 2, 3, 5, 8, 13, 21, 34, 55]";

/// The exact listener log line for value `n`
/// (`"I heard: [Hello World: N]"`).
pub fn listener_line(n: impl std::fmt::Display) -> String {
    format!("{LISTENER_LOG_PREFIX} [Hello World: {n}]")
}

/// Extract the sequence number from a chatter payload, i.e. the `N` out of
/// `'Hello World: N'` (talker) or `[Hello World: N]` (listener). Returns
/// `None` when the payload doesn't have the official demo shape.
fn parse_hello_world_n(rest: &str) -> Option<i64> {
    let inner = rest
        .strip_prefix('\'')
        .and_then(|s| s.strip_suffix('\''))
        .or_else(|| rest.strip_prefix('[').and_then(|s| s.strip_suffix(']')))
        .unwrap_or(rest);
    inner
        .trim()
        .strip_prefix("Hello World:")?
        .trim()
        .parse()
        .ok()
}

/// Parsed talker (publisher) output.
#[derive(Debug)]
pub struct TalkerOutput {
    /// Number of [`TALKER_LOG_PREFIX`] lines found.
    pub published_count: usize,
    /// Sequence numbers extracted from `"Publishing: 'Hello World: N'"` lines.
    pub values: Vec<i64>,
}

/// Parsed listener (subscriber) output.
#[derive(Debug)]
pub struct ListenerOutput {
    /// Number of [`LISTENER_LOG_PREFIX`] lines found.
    pub received_count: usize,
    /// Sequence numbers extracted from `"I heard: [Hello World: N]"` lines.
    pub values: Vec<i64>,
}

/// Parsed action client output.
#[derive(Debug)]
pub struct ActionClientOutput {
    /// Whether the goal was accepted.
    pub goal_accepted: bool,
    /// Number of [`ACTION_FEEDBACK_PREFIX`] lines.
    pub feedback_count: usize,
    /// Whether the action completed.
    pub completed: bool,
}

/// Parse talker output, extracting `"Publishing: 'Hello World: N'"` lines.
pub fn parse_talker(output: &str) -> TalkerOutput {
    let mut values = Vec::new();
    let mut count = 0;
    for line in output.lines() {
        if let Some(rest) = extract_after(line, TALKER_LOG_PREFIX) {
            count += 1;
            if let Some(v) = parse_hello_world_n(rest) {
                values.push(v);
            }
        }
    }
    TalkerOutput {
        published_count: count,
        values,
    }
}

/// Parse listener output, extracting `"I heard: [Hello World: N]"` lines.
pub fn parse_listener(output: &str) -> ListenerOutput {
    let mut values = Vec::new();
    let mut count = 0;
    for line in output.lines() {
        if let Some(rest) = extract_after(line, LISTENER_LOG_PREFIX) {
            count += 1;
            if let Some(v) = parse_hello_world_n(rest) {
                values.push(v);
            }
        }
    }
    ListenerOutput {
        received_count: count,
        values,
    }
}

/// Parse action client output (the W5 fibonacci demo wording).
pub fn parse_action_client(output: &str) -> ActionClientOutput {
    // `"Goal accepted"` also matches the official rclpy/rclcpp client's
    // `"Goal accepted by server, waiting for result"` line.
    let goal_accepted = output.contains("Goal accepted");
    let feedback_count = output.matches(ACTION_FEEDBACK_PREFIX).count();
    let completed = output.contains(ACTION_RESULT_PREFIX);
    ActionClientOutput {
        goal_accepted,
        feedback_count,
        completed,
    }
}

/// Assert that the talker published at least `min_count` messages.
///
/// Panics with diagnostic output on failure.
pub fn assert_talker(output: &str, min_count: usize) -> TalkerOutput {
    let result = parse_talker(output);
    assert!(
        result.published_count >= min_count,
        "Talker: expected at least {} published messages, got {}.\nOutput:\n{}",
        min_count,
        result.published_count,
        output,
    );
    result
}

/// Assert that the listener received at least `min_count` messages.
///
/// Panics with diagnostic output on failure.
pub fn assert_listener(output: &str, min_count: usize) -> ListenerOutput {
    let result = parse_listener(output);
    assert!(
        result.received_count >= min_count,
        "Listener: expected at least {} received messages, got {}.\nOutput:\n{}",
        min_count,
        result.received_count,
        output,
    );
    result
}

/// Assert that the action client accepted a goal, received feedback, and completed.
///
/// Panics with diagnostic output on failure.
pub fn assert_action_client(output: &str) -> ActionClientOutput {
    let result = parse_action_client(output);
    assert!(
        result.goal_accepted && result.feedback_count > 0 && result.completed,
        "Action client: goal_accepted={}, feedback_count={}, completed={}.\nOutput:\n{}",
        result.goal_accepted,
        result.feedback_count,
        result.completed,
        output,
    );
    result
}

/// Assert that the values are monotonically non-decreasing.
pub fn assert_monotonic(values: &[i64]) {
    if values.len() < 2 {
        return;
    }
    for window in values.windows(2) {
        assert!(
            window[0] <= window[1],
            "Values are not monotonically increasing: {} > {} in {:?}",
            window[0],
            window[1],
            values,
        );
    }
}

// ---------------------------------------------------------------------------
// RFC-0052 / phase-296 W3b.4/.5 — contract-monitor parity fixture markers.
// The rule ids are the play_launch runtime-enforcement vocabulary (RFC-0050),
// so the same contract yields the same rule string on either runtime.
// ---------------------------------------------------------------------------

/// `contract-monitor-diagsink` per-status prefix (`"DIAG rule=<id> hw=<ep>"`).
pub const CONTRACT_MONITOR_DIAG_PREFIX: &str = "DIAG rule=";

/// Readiness marker of the `contract-monitor-diagsink` observer (its banner
/// contains "Listener", like the other sink fixtures).
pub const CONTRACT_MONITOR_DIAGSINK_READY_MARKER: &str = "diagsink Listener";

/// Publisher-rate-contract violation rule id (`min_rate_hz` guarantee).
pub const RULE_RATE_HIERARCHY_RUNTIME: &str = "rate-hierarchy-runtime";

/// Subscriber max-data-age violation rule id (`max_age_ms` assumption).
pub const RULE_MAX_AGE_RUNTIME: &str = "max-age-runtime";

/// Emitted by `nros-board-zephyr`'s `run_tiers` when a real-time tier's
/// kernel EDF deadline is applied (phase-296 W5.5). MIRRORS the literal
/// `::log::info!` prefix in `nros-board-zephyr/src/entry_tiers.rs`
/// (`apply_tier_deadline`) AND the C/C++ arm's `zephyr_apply_tier_deadline`
/// in `nros-board-zephyr/c/zephyr_run_tiers.c` — keep all three in lockstep
/// (the no_std board crate cannot depend on this crate).
pub const ZEPHYR_EDF_DEADLINE_MARKER: &str = "nros: EDF deadline set tier=";

/// Emitted by the NuttX board seam (`nuttx_run_tiers.c`,
/// `nros_nuttx_apply_current_sporadic` — shared by the C/C++ AND Rust tier
/// arms) when the kernel actually accepted SCHED_SPORADIC for a tier
/// (phase-296 W5.9). MIRRORS the printf literal there — keep in lockstep.
pub const NUTTX_SPORADIC_MARKER: &str = "nros: sporadic budget set tier=";

/// The honest-fallback sibling: printed when a tier DECLARED a sporadic
/// budget but the running kernel lacks `CONFIG_SCHED_SPORADIC` (the
/// executor's cooperative Sporadic SchedContext stays the enforcement).
/// MIRRORS the printf literal in `nuttx_run_tiers.c` — keep in lockstep.
pub const NUTTX_SPORADIC_FALLBACK_MARKER: &str = "nros: sporadic budget declared for tier=";

/// Emitted by the ThreadX board (`nros-board-threadx/src/entry.rs`, both the
/// boot reprioritize and the spawn path) when the kernel ACCEPTED a tier's
/// preemption threshold (phase-296 W5.10, the non_preempt_scope dim).
/// MIRRORS the `B::println` literal there — keep in lockstep.
pub const THREADX_PREEMPT_MARKER: &str = "nros: preempt threshold set tier=";

/// Emitted by the Zephyr board when the kernel ACCEPTED a tier's CPU pin
/// (phase-296 W5.11, the `placement` dim). MIRRORS the `::log::info!` prefix
/// in `nros-board-zephyr/src/entry_tiers.rs` (`apply_tier_core_pin`) AND the
/// `printk` literal in the C/C++ arm's `zephyr_apply_core_pin`
/// (`nros-board-zephyr/c/zephyr_run_tiers.c`) — keep all three in lockstep
/// (the no_std board crate cannot depend on this crate).
pub const ZEPHYR_CORE_PIN_MARKER: &str = "nros: core pin tier=";

/// The honest-fallback sibling: printed when a tier DECLARED a `core` but the
/// running image cannot honor the pin (`CONFIG_SCHED_CPU_MASK_PIN_ONLY` off /
/// no SMP / bad cpu) — the tier runs unpinned, loudly. MIRRORS the `FAILED`
/// literals in both Zephyr arms — keep in lockstep.
pub const ZEPHYR_CORE_PIN_FALLBACK_MARKER: &str = "nros: core pin FAILED tier=";

/// Emitted by the NuttX board seam (`nuttx_run_tiers.c`,
/// `nros_nuttx_apply_current_affinity` — shared by the C/C++ AND Rust tier
/// arms) when the kernel accepted a tier's SMP core pin (phase-296 W5.11, the
/// placement dim). The board-agnostic literal is identical to the Zephyr arm's;
/// kept board-scoped here for lockstep clarity. MIRRORS the printf literal in
/// `nuttx_run_tiers.c` — keep in lockstep.
pub const NUTTX_CORE_PIN_MARKER: &str = "nros: core pin tier=";

/// phase-302 W3 (issue 0263) — the nuttx Rust arm's tier-priority adopt
/// marker (`nros_nuttx_apply_current_priority`, nuttx_run_tiers.c).
pub const NUTTX_TIER_PRIORITY_MARKER: &str = "nros: tier priority set tier=";

/// The honest-fallback sibling: printed when a tier DECLARED a `core` but the
/// NuttX image lacks `CONFIG_SMP` (or the kernel rejected the pin) — the tier
/// runs unpinned, loudly. MIRRORS the `FAILED` literals in both NuttX seams —
/// keep in lockstep.
pub const NUTTX_CORE_PIN_FALLBACK_MARKER: &str = "nros: core pin FAILED tier=";

/// Emitted by the FreeRTOS board seam (`freertos_run_tiers.c`) over the
/// semihosting console when a tier's SMP core pin is applied
/// (`vTaskCoreAffinitySet` on a `configUSE_CORE_AFFINITY` build) — phase-296
/// W5.11, the placement dim. Board-agnostic literal; kept board-scoped for
/// lockstep clarity. MIRRORS the `semihosting_write0` literal — keep in lockstep.
pub const FREERTOS_CORE_PIN_MARKER: &str = "nros: core pin tier=";

/// The honest-fallback sibling: printed when a tier DECLARED a `core` but the
/// FreeRTOS build is uniprocessor (no `configUSE_CORE_AFFINITY`) — the tier
/// runs unpinned, loudly (before W5.11 this was a SILENT `(void)task`). MIRRORS
/// the `FAILED` literal in `freertos_run_tiers.c` — keep in lockstep.
pub const FREERTOS_CORE_PIN_FALLBACK_MARKER: &str = "nros: core pin FAILED tier=";

/// Emitted by the ThreadX board (`nros-board-threadx/src/entry.rs`,
/// `apply_tier_core_exclude`, boot + spawned) when the kernel accepted a tier's
/// SMP core pin (`tx_thread_smp_core_exclude` — the placement dim's ThreadX
/// realization, RFC-0052's "SMP core exclude"), phase-296 W5.13. Board-agnostic
/// literal; MIRRORS the `B::println` literal there — keep in lockstep.
pub const THREADX_CORE_PIN_MARKER: &str = "nros: core pin tier=";

/// The honest-fallback sibling: printed when a tier DECLARED a `core` but the
/// ThreadX build lacks `TX_THREAD_SMP` (no core-affinity API) — the tier runs
/// unpinned, loudly. MIRRORS the `FAILED` literal in `entry.rs` — keep in
/// lockstep.
pub const THREADX_CORE_PIN_FALLBACK_MARKER: &str = "nros: core pin FAILED tier=";

/// Emitted by the ThreadX board (`nros-board-threadx/src/entry.rs`, boot +
/// spawned) when a tier's round-robin time slice was applied
/// (`tx_thread_create` slice param / `tx_thread_time_slice_change`), phase-296
/// issue #0266. ThreadX honors a per-thread slice unconditionally, so this is
/// accept-only (no fallback). MIRRORS the `B::println` literal there — keep in
/// lockstep.
pub const THREADX_TIME_SLICE_MARKER: &str = "nros: time slice set tier=";

/// Emitted by the POSIX board (`nros-board-linux/src/lib.rs`,
/// `apply_tier_affinity`, boot + spawned) when `sched_setaffinity` pinned a
/// tier thread to its declared `core` (phase-296 W5.13, the placement dim). A
/// Linux host is genuinely multi-core and the call needs no privilege, so this
/// is the FIRST RUNTIME accept-arm proof of the core-pin consumer (issue #260).
/// MIRRORS the `B::println` literal there — keep in lockstep.
pub const POSIX_CORE_PIN_MARKER: &str = "nros: core pin tier=";

/// The honest-fallback sibling: printed when `sched_setaffinity` rejects a
/// declared `core` (bad cpu id) — the tier runs unpinned, loudly. MIRRORS the
/// `FAILED` literal in `nros-board-linux/src/lib.rs` — keep in lockstep.
pub const POSIX_CORE_PIN_FALLBACK_MARKER: &str = "nros: core pin FAILED tier=";

/// Extract the trimmed text after a marker in a line.
///
/// Returns `None` if the marker is not found.
fn extract_after<'a>(line: &'a str, marker: &str) -> Option<&'a str> {
    let idx = line.find(marker)?;
    Some(line[idx + marker.len()..].trim())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_talker_line_and_listener_line() {
        assert_eq!(talker_line(4), "Publishing: 'Hello World: 4'");
        assert_eq!(listener_line(250), "I heard: [Hello World: 250]");
        // The helpers build on the same prefix constants `parse_talker` /
        // `parse_listener` use, so a line built by `talker_line`/`listener_line`
        // round-trips through the parser.
        let output = format!("{}\n", talker_line(7));
        assert_eq!(parse_talker(&output).values, vec![7]);
        let output = format!("{}\n", listener_line(7));
        assert_eq!(parse_listener(&output).values, vec![7]);
    }

    #[test]
    fn test_parse_talker() {
        let output = "[INFO talker] Publishing: 'Hello World: 1'\n\
                      [INFO talker] Publishing: 'Hello World: 2'\n\
                      [INFO talker] Publishing: 'Hello World: 3'\n";
        let result = parse_talker(output);
        assert_eq!(result.published_count, 3);
        assert_eq!(result.values, vec![1, 2, 3]);
    }

    #[test]
    fn test_parse_listener() {
        let output = "[INFO listener] I heard: [Hello World: 5]\n\
                      [INFO listener] I heard: [Hello World: 6]\n";
        let result = parse_listener(output);
        assert_eq!(result.received_count, 2);
        assert_eq!(result.values, vec![5, 6]);
    }

    #[test]
    fn test_parse_talker_with_noise() {
        let output = "Starting up...\nPublishing: 'Hello World: 1'\nsome noise\n\
                      Publishing: 'abc'\nPublishing: 'Hello World: 2'\n";
        let result = parse_talker(output);
        // "Publishing: 'abc'" counts as a published line but yields no N
        assert_eq!(result.published_count, 3);
        assert_eq!(result.values, vec![1, 2]);
    }

    #[test]
    fn test_parse_hello_world_n_shapes() {
        // Quoted (talker), bracketed (listener), and bare payloads all parse.
        assert_eq!(parse_hello_world_n("'Hello World: 12'"), Some(12));
        assert_eq!(parse_hello_world_n("[Hello World: 12]"), Some(12));
        assert_eq!(parse_hello_world_n("Hello World: 12"), Some(12));
        assert_eq!(parse_hello_world_n("'Hello World: x'"), None);
        assert_eq!(parse_hello_world_n("42"), None);
    }

    #[test]
    fn test_parse_action_client() {
        let output = "Goal accepted by server, waiting for result\n\
                      Next number in sequence received: [0]\n\
                      Next number in sequence received: [0, 1]\n\
                      Result received: [0, 1]\n";
        let result = parse_action_client(output);
        assert!(result.goal_accepted);
        assert_eq!(result.feedback_count, 2);
        assert!(result.completed);
    }

    #[test]
    fn test_assert_monotonic() {
        assert_monotonic(&[0, 1, 2, 3]);
        assert_monotonic(&[0, 0, 1, 1, 2]);
        assert_monotonic(&[]);
        assert_monotonic(&[42]);
    }

    #[test]
    #[should_panic(expected = "not monotonically increasing")]
    fn test_assert_monotonic_fails() {
        assert_monotonic(&[0, 2, 1, 3]);
    }

    #[test]
    fn test_extract_after() {
        assert_eq!(
            extract_after("[INFO] Published: 42", "Published:"),
            Some("42")
        );
        assert_eq!(extract_after("no match here", "Published:"), None);
        assert_eq!(extract_after("Received: hello", "Received:"), Some("hello"));
    }
}

#[cfg(test)]
mod silence_tests {
    use super::runtime_silence_note;

    /// The exact shape issue 0459 reported: a Zephyr image that booted and then
    /// said nothing. The old failure named a missing EDF marker, which reads as
    /// a scheduling problem and is not one.
    #[test]
    fn a_boot_banner_and_nothing_else_is_silence() {
        let log = "WARNING: Using a test - not safe - entropy source\n\
                   *** Booting Zephyr OS build v3.7.0 ***\n";
        let note = runtime_silence_note(log).expect("must classify as silent");
        assert!(note.contains("NO RUNTIME OUTPUT"), "{note}");
        assert!(note.contains("2 non-empty line(s)"), "{note}");
        assert!(
            note.contains("Booting Zephyr"),
            "tail must be shown: {note}"
        );
    }

    #[test]
    fn empty_output_is_silence() {
        assert!(runtime_silence_note("").is_some());
        assert!(runtime_silence_note("\n\n  \n").is_some());
    }

    /// The other half, and the one that keeps this from swallowing real
    /// failures: once the runtime has spoken, a missing marker IS about the
    /// marker and the caller's own message must stand alone.
    #[test]
    fn a_runtime_that_spoke_is_not_silence() {
        let log = "*** Booting Zephyr OS ***\n[nros] tier task entered\n";
        assert!(runtime_silence_note(log).is_none());
        assert!(runtime_silence_note("nros: session open\n").is_none());
    }
}
