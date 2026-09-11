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
    output::{MULTIGOAL_SUMMARY_PREFIX, REPLY_SLOT_REPORT_PREFIX},
};
use rstest::rstest;
use std::{path::PathBuf, process::Command, time::Duration};

/// The server's `ActionServerCore::MAX_GOALS` default.
const MAX_GOALS: usize = 4;
/// Must match `GOALS_DEFAULT` in the client fixture.
const GOALS_SENT: usize = 6;

/// Parse `key<n>` out of the client's one summary line.
///
/// By KEY, not by position — the line gained `completed=`/`sent=`/
/// `result_missing=` in phase-455 W2 and both tests here must keep reading the
/// fields they care about. A missing key is a panic naming the line, because a
/// summary that lost a field is a different failure from a field with a wrong
/// value and the two must not read alike.
fn summary_field(summary: &str, key: &str) -> usize {
    summary
        .split_whitespace()
        .find_map(|tok| tok.strip_prefix(key))
        .and_then(|v| v.parse().ok())
        .unwrap_or_else(|| panic!("no `{key}<n>` in summary line: {summary}"))
}

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
    let accepted = summary_field(summary, "accepted=");
    let rejected = summary_field(summary, "rejected=");

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

/// issue 0902 / phase-455 W2 — the goal COMPLETION rate, with a verdict that is
/// not an inference.
///
/// Issue 0902 measured action goals completing between 20 % and 90 % on one
/// build, with no session expiry, no crash and no discovery failure, and said
/// plainly why the number alone could not close it:
///
/// > A 20-90 % spread with no observable cause is worse than a hard failure. It
/// > is not measurable as a regression gate, and any future change to this path
/// > will be evaluated against noise wide enough to hide it.
///
/// So a rate is recorded here as EVIDENCE and never as the assertion. What is
/// asserted is two things that are each a statement rather than a sample:
///
/// * `completed == sent` — every accepted goal's RESULT came back. The
///   pre-phase-455 fixture awaited ACCEPTANCE only, which is exactly blind to
///   0902's shape (accepted, executed, status published, no result, for ever).
/// * the server's reply-slot refusal count is **zero** — the W1 counter, read
///   from the process that owns the queryable. Zero means "no slot allocation
///   was ever refused here", which `zpico_queryable_take_reply_seq`'s `-1` could
///   not say, because that value also means "no query pending".
///
/// Together they separate this defect from a timeout, a slow peer or a
/// scheduling artefact: a green run with `refusals=0` is a claim about the
/// MECHANISM, not about the sample size.
///
/// ## The conditions, without which a green proves nothing
///
/// **An idle soak.** The countdown 0902 measured is consumed by ELAPSED TIME
/// with a peer present, not by goal traffic — 100 s of idling took the original
/// run from 8/10 to 2/5 while back-to-back goals passed. `NROS_MULTIGOAL_SOAK_MS`
/// spins the client's session through that time.
///
/// **A peer that probes.** The `zenohd_unique` router is loopback, multicast
/// off, ephemeral port, so no third party reaches the queryable. But liveliness
/// arrives through the SAME queryable callback as a real request, and our own
/// second node is a sufficient source — so one is brought UP and DOWN during
/// the soak rather than assumed to appear.
///
/// **Fewer goals than `MAX_GOALS`.** `completed == sent` has to be a statement
/// about RESULTS; with six goals against a four-slot table two are rejected by
/// design and the equality could never hold. Issue 0322's question keeps its own
/// test above, with its own goal count.
///
/// ## What a green run here does NOT prove — issue 1332, MEASURED
///
/// This is a gate for the COMPLETION path and not for the reply-slot leak. On
/// this lane the counter cannot move at all, so reverting either leak fix
/// changes no observable here:
///
/// | tree under test | refusals | completed |
/// | --- | ---: | ---: |
/// | both leak arms fixed | 0 | 3/3 |
/// | `1a032a10b` reverted (declined queries keep their slot) | 0 | 3/3 |
/// | same, with 10 peer up/down cycles | 0 | — |
/// | `-DZPICO_MAX_PENDING_REPLIES=1`, a ONE-slot table | 0 | 3/3 |
///
/// A one-slot table refusing nothing is the decisive row: the server's
/// queryable never has more than one query in flight here, so the allocation
/// never fails. The declined-query shape that feeds the leak (an empty-payload
/// liveliness probe) does not arrive, because graph discovery on this lane is a
/// liveliness SUBSCRIBER rather than a query (phase-381 / issue 0903) and both
/// action-client paths are single-in-flight. Issue 0902 measured its spread on a
/// bare-metal serial board inside a REAL ROS 2 graph — that peer population is
/// what is missing, not the code path.
///
/// So: do not cite this test as coverage for 0902's mechanism. Issue 1332 has
/// the three routes that would make it one, cheapest first.
#[rstest]
fn every_accepted_goal_returns_a_result_and_no_reply_slot_is_refused(
    zenohd_unique: ZenohRouter,
    action_server_concurrent_binary: PathBuf,
    action_client_multigoal_binary: PathBuf,
) {
    if !require_zenohd() {
        nros_tests::skip!("zenohd not found");
    }
    let locator = zenohd_unique.locator();

    /// Below `MAX_GOALS`, so every goal is accepted and the equality is about
    /// results rather than rejections.
    const GOALS: usize = 3;
    /// Elapsed time with the session live, ON TOP of the peer churn above.
    /// 0902's own soak was 100 s; this is the affordable end of the same
    /// condition — the server has already been up and probed for tens of
    /// seconds by the time the first goal is sent.
    const SOAK_MS: u64 = 10_000;

    let mut server_cmd = Command::new(&action_server_concurrent_binary);
    server_cmd.env("NROS_LOCATOR", &locator);
    server_cmd.env("RUST_LOG", "info");
    let mut server = ManagedProcess::spawn_command(server_cmd, "action-server-concurrent")
        .expect("Failed to start concurrent action server");

    let server_boot = server.collect_until("Waiting for action", Duration::from_secs(10));
    if !server_boot.contains("Waiting for action") {
        panic!(
            "concurrent action server never printed its readiness banner within 10s \
             (still running: {}). Server output:\n{}",
            server.is_running(),
            server_boot
        );
    }

    /* The peer that probes: our own second node, brought UP and DOWN against
     * the live server, three times, BEFORE the goals fly.
     *
     * `NROS_MULTIGOAL_GOALS=0` makes it a pure graph participant — it opens a
     * session, declares its action client's entities, idles briefly, and exits
     * — so what reaches the server's queryable is discovery and liveliness,
     * which is the traffic issue 0902 says consumes the reply slots. The server
     * has been up and churned for tens of seconds by the time a goal is sent,
     * which is the CONDITION; nothing about it requires our goal-sending client
     * to be alive for it.
     *
     * MEASURED, and the ordering is load-bearing: peers running CONCURRENTLY
     * with the goal sends made all three `send_goal` calls time out
     * (`accepted=0 rejected=0`) while the server reported `refusals=0` — four
     * clients querying one queryable overrun the server's 4-deep request ring,
     * which drops the newest. That is a real property and a different one; a
     * probe for 0902 that trips over it measures the ring, not the slots. */
    for cycle in 0..3 {
        let mut peer_cmd = Command::new(&action_client_multigoal_binary);
        peer_cmd.env("NROS_LOCATOR", &locator);
        // `info`, not `warn`: the summary line this loop waits for is an
        // `info!`, so a quieter peer is a peer that never reports and a
        // precondition that can only ever fail.
        peer_cmd.env("RUST_LOG", "info");
        peer_cmd.env("NROS_MULTIGOAL_GOALS", "0");
        peer_cmd.env("NROS_MULTIGOAL_SOAK_MS", "1000");
        let mut peer = ManagedProcess::spawn_command(peer_cmd, format!("multigoal-peer-{cycle}"))
            .expect("Failed to start the liveliness peer");
        let peer_out = peer.collect_until(MULTIGOAL_SUMMARY_PREFIX, Duration::from_secs(30));
        assert!(
            peer_out.contains(MULTIGOAL_SUMMARY_PREFIX),
            "liveliness peer {cycle} never reached its summary line, so the graph churn this \
             test depends on did not happen and a green below would prove nothing. \
             Peer output:\n{peer_out}"
        );
        drop(peer);
    }

    let mut client_cmd = Command::new(&action_client_multigoal_binary);
    client_cmd.env("NROS_LOCATOR", &locator);
    client_cmd.env("RUST_LOG", "info");
    client_cmd.env("NROS_MULTIGOAL_GOALS", GOALS.to_string());
    client_cmd.env("NROS_MULTIGOAL_SOAK_MS", SOAK_MS.to_string());
    let mut client = ManagedProcess::spawn_command(client_cmd, "action-client-multigoal")
        .expect("Failed to start multi-goal action client");

    // Generous: the soak, then GOALS handshakes, then GOALS results at ~4 s of
    // Fibonacci each. A budget near the honest duration would turn a loaded host
    // into a fake reproduction of the very defect under test.
    let out = client.collect_until(MULTIGOAL_SUMMARY_PREFIX, Duration::from_secs(180));

    let summary = out
        .lines()
        .find(|l| l.contains(MULTIGOAL_SUMMARY_PREFIX))
        .unwrap_or_else(|| {
            panic!("multi-goal client printed no summary line. Output:\n{out}");
        });

    let accepted = summary_field(summary, "accepted=");
    let completed = summary_field(summary, "completed=");
    let sent = summary_field(summary, "sent=");
    let result_missing = summary_field(summary, "result_missing=");

    // Give the server one more heartbeat after the client is done, so the count
    // read below covers the whole run rather than most of it.
    let server_out = server.collect_until("\u{1}never-matches\u{1}", Duration::from_secs(3));
    let refusal_line = server_out
        .lines()
        .rfind(|l| l.contains(REPLY_SLOT_REPORT_PREFIX))
        .unwrap_or_else(|| {
            panic!(
                "the action server printed no `{REPLY_SLOT_REPORT_PREFIX}` heartbeat, so the \
                 W1 counter was never read and this run says NOTHING about reply-slot \
                 exhaustion — which is the whole point of the test. Server output:\n{server_out}"
            )
        });

    // EVIDENCE, printed whatever the verdict: the rate is what issue 0902 has
    // been unable to compare against anything, and the next reader needs the
    // number even when the assertions pass.
    println!(
        "phase-455 W2 evidence: completed {completed}/{sent} after a {SOAK_MS} ms soak \
         with 3 peer up/down cycles; server says `{}`",
        refusal_line.trim()
    );

    // issue 1044's lesson, one file over: the assertion and the evidence travel
    // together. Diagnosing `accepted=0` from the summary line alone cost a
    // hand-run of the pair; the client's transcript says which of `send_goal
    // failed`, `acceptance timed out` and `no-verdict` actually happened.
    assert_eq!(
        accepted,
        GOALS,
        "expected all {GOALS} goals accepted (below MAX_GOALS), got {accepted}. \
         Zero accepted with zero rejected means every `send_goal` TIMED OUT, which is \
         a different failure from a goal the server refused. Server reply-slot line: \
         `{}`. Client output:\n{out}",
        refusal_line.trim()
    );
    assert_eq!(
        completed,
        sent,
        "issue 0902: {result_missing} of {sent} accepted goals never returned a RESULT. \
         That is the defect's exact shape — accepted, executed, and then nothing. \
         Read the server's reply-slot line (`{}`) to tell an exhausted reply table from \
         a slow peer. Summary: {summary}",
        refusal_line.trim()
    );
    assert!(
        refusal_line.contains(&format!("{REPLY_SLOT_REPORT_PREFIX}0")),
        "the server refused at least one reply-slot allocation during this run: `{}`. \
         Every request after a refusal is accepted and can never be answered (issue 0902). \
         A non-zero count here means the reply-slot table leaked or is too small — NOT that \
         the goals were slow. `n/a no-zenoh-shim` means this server build links no zenoh \
         shim, so it could never have answered and the run proves nothing.",
        refusal_line.trim()
    );
}
