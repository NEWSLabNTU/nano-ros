//! Multi-goal Fibonacci action-client fixture (issue 0322).
//!
//! Sends MORE concurrent goals than the server's `active_goals` table holds
//! (`ActionServerCore::MAX_GOALS`, default 4) and prints one verdict line per
//! goal, so a test can assert what a full table does.
//!
//! ## Why this exists
//!
//! `accept_goal` used to reply `accepted=true` and only then
//! `let _ = active_goals.push(...)`. Once the table was full the 5th goal was
//! acknowledged on the wire and kept nowhere — no execution, no feedback, no
//! result — and an rclcpp/rclpy client that saw `accepted=true` waited on its
//! result future forever. The fix makes a full table reply `accepted=false`.
//!
//! The two behaviours are directly distinguishable from the client side, which
//! is what makes this a real regression test rather than a smoke test:
//!
//! | | goals accepted | goals rejected |
//! | --- | --- | --- |
//! | before the fix | all 6 | 0 |
//! | after the fix | 4 (`MAX_GOALS`) | 2 |
//!
//! ## Pairing
//!
//! Needs a server that HOLDS goals instead of running each to completion
//! inline — `bins/action-server-concurrent`, which advances every tracked goal
//! one Fibonacci step per spin. The goal `order` below is large enough that
//! goals stay active while the later ones are sent.
//!
//! `send_goal` is single-in-flight per client, so the goals are handshaked
//! sequentially; they still overlap on the SERVER, which is what fills the
//! table.

use example_interfaces::action::{Fibonacci, FibonacciGoal};
use log::{info, warn};
use nros::prelude::*;

extern crate nros_platform_cffi as _;

/// How many goals to send. Must exceed the server's `MAX_GOALS` (4) so the
/// table fills; 6 leaves two goals past the boundary, which distinguishes
/// "rejects when full" from "rejects the 5th only".
///
/// phase-455 W2 makes this overridable, because the two questions this fixture
/// now answers want different values. The `MAX_GOALS` regression needs MORE
/// goals than the table holds; the goal-COMPLETION rate (issue 0902) needs
/// fewer, so every goal is accepted and `completed == sent` is a statement
/// about results rather than about rejections.
const GOALS_DEFAULT: usize = 6;

/// Fibonacci order per goal. Large enough that a goal is still active while
/// the remaining goals are sent — the server advances one step per spin, so
/// this is the dwell time that makes the goals concurrent.
const ORDER: i32 = 40;

/// How long to wait for one goal's RESULT once it has been accepted.
///
/// A goal of `ORDER` steps at one step per ~100 ms server spin is ~4 s, and up
/// to `MAX_GOALS` of them advance together, so this is generous rather than
/// tight ON PURPOSE: the assertion this fixture feeds is "the result arrives",
/// and a budget near the honest duration turns a slow host into a fake
/// reproduction of issue 0902.
const RESULT_TIMEOUT_MS: u64 = 30_000;

/// Read a `usize` knob from the environment, or fall back.
///
/// A malformed value is a hard failure, not a silent fallback: a probe that
/// quietly ignores the number it was asked to run with reports a verdict about
/// a configuration nobody chose.
fn env_usize(key: &str, default: usize) -> usize {
    match std::env::var(key) {
        Ok(v) => v
            .parse()
            .unwrap_or_else(|e| panic!("{key}={v:?} is not a number: {e}")),
        Err(_) => default,
    }
}

/// issue 0902 / phase-455 W2 — the IDLE SOAK, and why a probe without one
/// proves nothing.
///
/// The reply-slot countdown 0902 measured is consumed by ELAPSED TIME with a
/// peer present, not by goal traffic: background discovery and liveliness
/// probes reach the server's queryable through the same callback as a real
/// request, and each one used to keep a slot for ever. 100 s of idling took
/// that run from 8/10 to 2/5, while goals sent back to back passed. So a probe
/// that fires N goals and exits can be green on a leaking build.
///
/// The executor is spun throughout — the soak is time spent with the session
/// LIVE, which is the condition; a sleeping process receives nothing.
fn idle_soak(executor: &mut Executor, millis: u64) {
    if millis == 0 {
        return;
    }
    info!("multigoal: idle soak {millis} ms (issue 0902 — the countdown is elapsed time, not traffic)");
    let deadline = std::time::Instant::now() + core::time::Duration::from_millis(millis);
    while std::time::Instant::now() < deadline {
        let _ = executor.spin_once(core::time::Duration::from_millis(50));
    }
}

fn main() -> ! {
    nros_board_linux::register_linked_rmw();
    env_logger::init();

    let ctx = nros::init_with_launch_auto().expect("nros init failed");
    let cfg = ctx.config("fibonacci_multigoal_client");
    let mut executor = Executor::open(&cfg).expect("Failed to open session");
    let mut node = executor
        .create_node("fibonacci_multigoal_client")
        .expect("Failed to create node");
    let mut client = node
        .create_action_client::<Fibonacci>("/fibonacci")
        .expect("Failed to create action client");

    // Same discovery wait as the single-goal demo: send_goal is a service call
    // whose first request races the endpoint match.
    match client.wait_for_action_server(&mut executor, core::time::Duration::from_secs(10)) {
        Ok(true) => {}
        Ok(false) => warn!("Action server not confirmed within 10s — sending goals anyway"),
        Err(e) => warn!("wait_for_action_server error: {e:?} — sending goals anyway"),
    }

    // issue 0902 / phase-455 W2 — soak BEFORE the goals, with the session live
    // and a peer present. See `idle_soak`: the reply-slot countdown is spent by
    // elapsed time, so a burst of goals on a fresh session is the one shape
    // that cannot reproduce the defect.
    idle_soak(
        &mut executor,
        env_usize("NROS_MULTIGOAL_SOAK_MS", 0) as u64,
    );

    let goals = env_usize("NROS_MULTIGOAL_GOALS", GOALS_DEFAULT);
    let goal = FibonacciGoal { order: ORDER };
    let mut accepted = 0usize;
    let mut rejected = 0usize;
    // phase-455 W2 — goals whose RESULT came back. The pre-W2 fixture counted
    // acceptance verdicts only, which is exactly blind to issue 0902's symptom:
    // accepted, executed, and then no result, for ever.
    let mut completed = 0usize;
    let mut result_missing = 0usize;
    let mut accepted_ids: Vec<GoalId> = Vec::new();

    for i in 0..goals {
        // One retry: on rmw_zenoh the server's liveliness token can gossip
        // ahead of its queryable route, so a first send_goal may time out
        // against a not-yet-matched queryable (issue 0153). A TIMEOUT is not
        // a verdict — only an explicit accept/reject is — so a timed-out
        // attempt is retried rather than counted.
        let mut verdict = None;
        let mut verdict_id = None;
        for attempt in 0..2 {
            if attempt > 0 {
                std::thread::sleep(core::time::Duration::from_millis(500));
            }
            let (goal_id, mut promise) = match client.send_goal(&goal) {
                Ok(pair) => pair,
                Err(e) => {
                    warn!("goal {i}: send_goal failed: {e:?}");
                    break;
                }
            };
            match promise.wait(&mut executor, core::time::Duration::from_millis(5000)) {
                Ok(true) => {
                    verdict = Some(true);
                    // phase-455 W2 — the id is what `get_result` is asked with,
                    // so the RESULT half needs it. The pre-W2 fixture dropped it
                    // as `_goal_id`, which is why it could only ever report
                    // acceptance.
                    verdict_id = Some(goal_id);
                    break;
                }
                Ok(false) => {
                    verdict = Some(false);
                    break;
                }
                Err(e) => {
                    warn!("goal {i}: acceptance timed out (attempt {}): {e:?}", attempt + 1);
                    // A timed-out promise leaves the in-flight flag set; clear
                    // it or the retry dies on RequestInFlight.
                    client.reset_send_goal_in_flight();
                }
            }
        }

        match verdict {
            Some(true) => {
                accepted += 1;
                if let Some(id) = verdict_id {
                    accepted_ids.push(id);
                }
                info!("multigoal: goal {i} accepted");
            }
            Some(false) => {
                rejected += 1;
                info!("multigoal: goal {i} rejected");
            }
            None => info!("multigoal: goal {i} no-verdict"),
        }

        // Keep the session alive between goals so the server can advance the
        // goals it already holds — without this the in-flight goals make no
        // progress and the table state is not what the test reasons about.
        for _ in 0..5 {
            let _ = executor.spin_once(core::time::Duration::from_millis(10));
        }
    }

    /* issue 0902 / phase-455 W2 — await each accepted goal's RESULT.
     *
     * This is the half the fixture never had. 0902's failure shape is
     * "accepted, executed, status published, NO RESULT": the server's deferred
     * get_result reply needs a reply slot, and once the table is exhausted the
     * reply is discarded at four layers with the C++ side printing "Goal
     * succeeded" over it. A client that stops at the acceptance verdict cannot
     * see any of that.
     *
     * Serially, because `get_result` is single-in-flight per client. A
     * timed-out promise leaves the in-flight flag set, so it is cleared before
     * the next id — the same correction the acceptance retry above needed. */
    for (n, id) in accepted_ids.iter().enumerate() {
        let mut promise = match client.get_result(id) {
            Ok(p) => p,
            Err(e) => {
                result_missing += 1;
                warn!("multigoal: goal {n} get_result request failed: {e:?}");
                client.reset_get_result_in_flight();
                continue;
            }
        };
        match promise.wait(
            &mut executor,
            core::time::Duration::from_millis(RESULT_TIMEOUT_MS),
        ) {
            Ok((status, _result)) => {
                completed += 1;
                info!("multigoal: goal {n} result received status={status:?}");
            }
            Err(e) => {
                result_missing += 1;
                warn!(
                    "multigoal: goal {n} NO RESULT after {RESULT_TIMEOUT_MS} ms: {e:?} \
                     — this is issue 0902's shape (accepted, then nothing). Read the \
                     server's `reply-slot: refusals=` line before blaming the timeout."
                );
                client.reset_get_result_in_flight();
            }
        }
    }

    // The line GAINS fields; it does not change meaning. `accepted=` and
    // `rejected=` still answer issue 0322's MAX_GOALS question, and
    // `tests/action_multigoal.rs` parses by key rather than by position.
    info!(
        "multigoal: summary accepted={accepted} rejected={rejected} of {goals} \
         completed={completed} sent={goals} result_missing={result_missing}"
    );
    // Flush stdout before exit — a full-buffered pipe can otherwise swallow
    // the summary line the test greps for.
    use std::io::Write;
    let _ = std::io::stdout().flush();
    std::process::exit(0);
}
