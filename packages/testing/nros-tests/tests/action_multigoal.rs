//! Multi-goal stress for the action server's `MAX_GOALS` table (issue 0322).
//!
//! `accept_goal` used to reply `accepted=true` and only then
//! `let _ = active_goals.push(...)`. Once the table was full, the overflow
//! goals were acknowledged on the wire and kept nowhere: no execution, no
//! feedback, no result, no terminal status. An rclcpp/rclpy client that saw
//! `accepted=true` waited on its result future forever.
//!
//! The regression is observable from the client alone, which is what makes
//! this a real gate rather than a smoke test — six goals against a server
//! whose table holds four:
//!
//! | | accepted | rejected |
//! | --- | --- | --- |
//! | before the fix | 6 | 0 |
//! | after the fix | 4 | 2 |
//!
//! Both numbers were observed on this pair while the fix was reverted and
//! restored, so the assertion below is known to FAIL on the buggy build
//! rather than merely passing on the fixed one.
//!
//! Needs the concurrent server (`bins/action-server-concurrent`), which
//! advances each tracked goal one step per spin instead of running one goal
//! to completion inline — otherwise goals never overlap and the table never
//! fills.

use nros_tests::{
    fixtures::{
        ManagedProcess, ZenohRouter, action_client_multigoal_binary,
        action_server_concurrent_binary, require_zenohd, zenohd_unique,
    },
    output::MULTIGOAL_SUMMARY_PREFIX,
};
use rstest::rstest;
use std::{path::PathBuf, process::Command, time::Duration};

/// The server's `ActionServerCore::MAX_GOALS` default.
const MAX_GOALS: usize = 4;
/// Must match `GOALS` in the client fixture.
const GOALS_SENT: usize = 6;

#[rstest]
fn full_goal_table_rejects_rather_than_acknowledging(
    zenohd_unique: ZenohRouter,
    action_server_concurrent_binary: PathBuf,
    action_client_multigoal_binary: PathBuf,
) {
    if !require_zenohd() {
        nros_tests::skip!("zenohd not found");
    }
    let locator = zenohd_unique.locator();

    let mut server_cmd = Command::new(&action_server_concurrent_binary);
    server_cmd.env("NROS_LOCATOR", &locator);
    server_cmd.env("RUST_LOG", "info");
    let mut server = ManagedProcess::spawn_command(server_cmd, "action-server-concurrent")
        .expect("Failed to start concurrent action server");

    // issue 1044 — a readiness check that tolerates "no banner, still running"
    // tolerates a HUNG server, which is the state this pair fails worst on: the
    // client then sends six goals into nothing and the test reports a wrong
    // summary rather than an unready server. `wait_for_output_pattern` does not
    // kill on timeout, so the banner's absence is an independent fact and can be
    // required on its own; `is_running` is kept only to say WHICH of the two
    // failures happened.
    let server_boot = server.collect_until("Waiting for action", Duration::from_secs(10));
    if !server_boot.contains("Waiting for action") {
        panic!(
            "concurrent action server never printed its readiness banner within 10s \
             (still running: {}). Server output:\n{}",
            server.is_running(),
            server_boot
        );
    }

    let mut client_cmd = Command::new(&action_client_multigoal_binary);
    client_cmd.env("NROS_LOCATOR", &locator);
    client_cmd.env("RUST_LOG", "info");
    let mut client = ManagedProcess::spawn_command(client_cmd, "action-client-multigoal")
        .expect("Failed to start multi-goal action client");

    // The client sends all six goals then prints one summary line, so a
    // run-to-completion wait is the right SHAPE here — unlike issue 1026's six
    // sites, this one is not aimed at a free-running node.
    //
    // issue 1044 — what was wrong was the failure path. The old spelling was
    // `wait_for_output_pattern(...).or_else(|_| wait_for_all_output(2s))
    // .unwrap_or_default()`: on timeout the strict call returns `Err` with the
    // whole 60 s transcript inside the error's MESSAGE, `or_else` discarded it
    // and re-read a client that had already been drained, and
    // `unwrap_or_default()` turned a second failure into `""`. So the panic
    // below — whose entire job is to show what the client printed — could report
    // an empty string about a client that had printed sixty seconds of output.
    // That is issue 0471's shape: the path carrying the evidence was not the
    // path that reported.
    //
    // `collect_until` is the lenient sibling: it returns what it read whether or
    // not the pattern showed up, so the assertion and the evidence travel
    // together.
    let out = client.collect_until(MULTIGOAL_SUMMARY_PREFIX, Duration::from_secs(60));

    let summary = out
        .lines()
        .find(|l| l.contains(MULTIGOAL_SUMMARY_PREFIX))
        .unwrap_or_else(|| {
            panic!("multi-goal client printed no summary line. Output:\n{out}");
        });

    // Parse rather than string-match the whole line, so the failure message
    // can say WHICH number is wrong — the counts are the entire assertion.
    let field = |key: &str| -> usize {
        summary
            .split_whitespace()
            .find_map(|tok| tok.strip_prefix(key))
            .and_then(|v| v.parse().ok())
            .unwrap_or_else(|| panic!("no `{key}<n>` in summary line: {summary}"))
    };
    let accepted = field("accepted=");
    let rejected = field("rejected=");

    assert_eq!(
        accepted, MAX_GOALS,
        "expected exactly MAX_GOALS ({MAX_GOALS}) goals accepted, got {accepted}. \
         If this is {GOALS_SENT}, the server is acknowledging goals it has no room \
         to record — issue 0322, the acknowledge-then-drop regression. Summary: {summary}"
    );
    assert_eq!(
        rejected,
        GOALS_SENT - MAX_GOALS,
        "expected the {} overflow goals to be REJECTED, got {rejected}. Summary: {summary}",
        GOALS_SENT - MAX_GOALS
    );
}
