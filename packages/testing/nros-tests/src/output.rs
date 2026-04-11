//! Shared output validation utilities for integration tests.
//!
//! All nano-ros examples print messages in a unified format:
//! - Talker: `"Published: N"`
//! - Listener: `"Received: N"`
//! - Service: `"[OK]"` for successful responses
//! - Action: `"Feedback #N: [...]"`, `"Goal accepted"`, `"Action client finished"`
//!
//! This module provides `parse_*` functions to extract structured data from
//! process output, and `assert_*` convenience functions that panic with
//! diagnostic messages on failure.

/// Parsed talker (publisher) output.
#[derive(Debug)]
pub struct TalkerOutput {
    /// Number of `"Published:"` lines found.
    pub published_count: usize,
    /// Integer values extracted from `"Published: N"` lines.
    pub values: Vec<i64>,
}

/// Parsed listener (subscriber) output.
#[derive(Debug)]
pub struct ListenerOutput {
    /// Number of `"Received:"` lines found.
    pub received_count: usize,
    /// Integer values extracted from `"Received: N"` lines.
    pub values: Vec<i64>,
}

/// Parsed action client output.
#[derive(Debug)]
pub struct ActionClientOutput {
    /// Whether the goal was accepted.
    pub goal_accepted: bool,
    /// Number of `"Feedback #"` lines.
    pub feedback_count: usize,
    /// Whether the action completed.
    pub completed: bool,
}

/// Parse talker output, extracting `"Published: N"` lines.
pub fn parse_talker(output: &str) -> TalkerOutput {
    let mut values = Vec::new();
    let mut count = 0;
    for line in output.lines() {
        if let Some(rest) = extract_after(line, "Published:") {
            count += 1;
            if let Ok(v) = rest.parse::<i64>() {
                values.push(v);
            }
        }
    }
    TalkerOutput {
        published_count: count,
        values,
    }
}

/// Parse listener output, extracting `"Received: N"` lines.
pub fn parse_listener(output: &str) -> ListenerOutput {
    let mut values = Vec::new();
    let mut count = 0;
    for line in output.lines() {
        if let Some(rest) = extract_after(line, "Received:") {
            count += 1;
            if let Ok(v) = rest.parse::<i64>() {
                values.push(v);
            }
        }
    }
    ListenerOutput {
        received_count: count,
        values,
    }
}

/// Parse action client output.
pub fn parse_action_client(output: &str) -> ActionClientOutput {
    let goal_accepted = output.contains("Goal accepted");
    let feedback_count = output.matches("Feedback #").count();
    let completed = output.contains("action completed")
        || output.contains("Action client finished")
        || output.contains("Action client done");
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
    fn test_parse_talker() {
        let output = "[INFO talker] Published: 0\n[INFO talker] Published: 1\n[INFO talker] Published: 2\n";
        let result = parse_talker(output);
        assert_eq!(result.published_count, 3);
        assert_eq!(result.values, vec![0, 1, 2]);
    }

    #[test]
    fn test_parse_listener() {
        let output = "[INFO listener] Received: 5\n[INFO listener] Received: 6\n";
        let result = parse_listener(output);
        assert_eq!(result.received_count, 2);
        assert_eq!(result.values, vec![5, 6]);
    }

    #[test]
    fn test_parse_talker_with_noise() {
        let output = "Starting up...\nPublished: 0\nsome noise\nPublished: abc\nPublished: 1\n";
        let result = parse_talker(output);
        // "Published: abc" counts as a published line but doesn't parse as i64
        assert_eq!(result.published_count, 3);
        assert_eq!(result.values, vec![0, 1]);
    }

    #[test]
    fn test_parse_action_client() {
        let output = "Goal accepted! ID: [1,2,3]\nFeedback #1: [0]\nFeedback #2: [0, 1]\nAction client finished\n";
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
        assert_eq!(extract_after("[INFO] Published: 42", "Published:"), Some("42"));
        assert_eq!(extract_after("no match here", "Published:"), None);
        assert_eq!(extract_after("Received: hello", "Received:"), Some("hello"));
    }
}
