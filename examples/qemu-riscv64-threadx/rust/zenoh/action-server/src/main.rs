//! ThreadX QEMU RISC-V Action Server (callback model)
//!
//! Phase 120.3 isolation test: handles `example_interfaces/Fibonacci`
//! goals on `/fibonacci` via `Executor::add_action_server` (arena +
//! callback model) instead of `Node::create_action_server` (manual-
//! poll). Mirrors the structure of the C/C++ examples to determine
//! whether the manual-poll path is what triggers the rv64 post-
//! handshake crash.

#![no_std]
#![no_main]

use example_interfaces::action::{
    Fibonacci, FibonacciFeedback, FibonacciGoal, FibonacciResult,
};
use nros::prelude::*;
use nros::{CancelResponse, GoalResponse, GoalStatus};
use nros_board_threadx_qemu_riscv64::{Config, println, run};

#[unsafe(no_mangle)]
extern "C" fn main() -> ! {
    run(Config::from_toml(include_str!("../config.toml")), |config| {
        let exec_config = ExecutorConfig::new(config.zenoh_locator)
            .domain_id(config.domain_id)
            .node_name("fibonacci_action_server");
        // Phase 115.L.x — install C-vtable backend before session open.
        let mut executor = Executor::open(&exec_config)?;
        // Note: callback-model add_action_server is on Executor, not Node.
        // The example doesn't need a Node handle — keep `_node` alive for
        // the executor's reference into session state.
        let _node = executor.create_node("fibonacci_action_server")?;

        let handle = executor.add_action_server::<Fibonacci, _, _>(
            "/fibonacci",
            |_goal_id, goal: &FibonacciGoal| {
                println!("Goal request: order={}", goal.order);
                if goal.order >= 0 {
                    GoalResponse::AcceptAndExecute
                } else {
                    GoalResponse::Reject
                }
            },
            |_goal_id, _status| CancelResponse::Ok,
        )?;
        println!("Action server ready on /fibonacci");
        println!("Waiting for goals...");

        let mut goals_handled = 0u32;

        for _ in 0..100000u32 {
            executor.spin_once(core::time::Duration::from_millis(10));

            // Drive any accepted goals one step per outer iteration.
            // Collect the goal_id + current sequence length first so we
            // don't hold a borrow on the executor while mutating.
            let mut pending: heapless::Vec<(nros::GoalId, i32, usize), 4> =
                heapless::Vec::new();
            handle.for_each_active_goal(&executor, |g| {
                if g.status == GoalStatus::Accepted || g.status == GoalStatus::Executing {
                    let _ = pending.push((g.goal_id, g.goal.order, 0));
                }
            });
            // (the active-goal iteration doesn't expose feedback sequence
            // state directly — for this isolation test we just publish
            // the full sequence once and complete the goal immediately.)
            for (goal_id, order, _) in pending {
                handle.set_goal_status(&mut executor, &goal_id, GoalStatus::Executing);

                let mut sequence: heapless::Vec<i32, 64> = heapless::Vec::new();
                for i in 0..=order {
                    let next_val = if i == 0 {
                        0
                    } else if i == 1 {
                        1
                    } else {
                        let len = sequence.len();
                        sequence[len - 1] + sequence[len - 2]
                    };
                    let _ = sequence.push(next_val);
                    let feedback = FibonacciFeedback { sequence: sequence.clone() };
                    let _ = handle.publish_feedback(&mut executor, &goal_id, &feedback);
                }

                let result = FibonacciResult { sequence };
                println!("Goal completed: id={:?}", goal_id);
                handle.complete_goal(&mut executor, &goal_id, GoalStatus::Succeeded, result);

                goals_handled += 1;
                if goals_handled >= 1 {
                    // Spin a bit to serve get_result queries before shutting down.
                    for _ in 0..2000 {
                        executor.spin_once(core::time::Duration::from_millis(10));
                    }
                    println!("Server shutting down.");
                    return Ok(());
                }
            }
        }

        println!("Server timeout.");
        Ok::<(), NodeError>(())
    })
}
