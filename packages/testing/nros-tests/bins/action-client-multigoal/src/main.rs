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
const GOALS: usize = 6;

/// Fibonacci order per goal. Large enough that a goal is still active while
/// the remaining goals are sent — the server advances one step per spin, so
/// this is the dwell time that makes the goals concurrent.
const ORDER: i32 = 40;

fn main() -> ! {
    nros_board_native::register_linked_rmw();
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

    let goal = FibonacciGoal { order: ORDER };
    let mut accepted = 0usize;
    let mut rejected = 0usize;

    for i in 0..GOALS {
        // One retry: on rmw_zenoh the server's liveliness token can gossip
        // ahead of its queryable route, so a first send_goal may time out
        // against a not-yet-matched queryable (issue 0153). A TIMEOUT is not
        // a verdict — only an explicit accept/reject is — so a timed-out
        // attempt is retried rather than counted.
        let mut verdict = None;
        for attempt in 0..2 {
            if attempt > 0 {
                std::thread::sleep(core::time::Duration::from_millis(500));
            }
            let (_goal_id, mut promise) = match client.send_goal(&goal) {
                Ok(pair) => pair,
                Err(e) => {
                    warn!("goal {i}: send_goal failed: {e:?}");
                    break;
                }
            };
            match promise.wait(&mut executor, core::time::Duration::from_millis(5000)) {
                Ok(true) => {
                    verdict = Some(true);
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

    info!("multigoal: summary accepted={accepted} rejected={rejected} of {GOALS}");
    // Flush stdout before exit — a full-buffered pipe can otherwise swallow
    // the summary line the test greps for.
    use std::io::Write;
    let _ = std::io::stdout().flush();
    std::process::exit(0);
}
