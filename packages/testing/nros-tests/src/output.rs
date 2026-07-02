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

/// The exact `int32-sink` / workspace-listener log line for value `n`
/// (`"Received: N"`).
pub fn int32_listener_line(n: impl std::fmt::Display) -> String {
    format!("{INT32_LISTENER_LOG_PREFIX} {n}")
}

/// The exact Int32 fixture-talker log line for value `n` (`"Published: N"`).
pub fn int32_talker_line(n: impl std::fmt::Display) -> String {
    format!("{INT32_TALKER_LOG_PREFIX} {n}")
}

/// The exact talker log line for sequence value `n`
/// (`"Publishing: 'Hello World: N'"`).
pub fn talker_line(n: impl std::fmt::Display) -> String {
    format!("{TALKER_LOG_PREFIX} 'Hello World: {n}'")
}

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

/// Action client log line once the server accepts the goal
/// (`"Goal accepted by server, waiting for result"`).
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
