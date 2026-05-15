//! FreeRTOS QEMU Action Server (callback model)
//!
//! Phase 122.4 — uses `Executor::register_action_server` (L2 callback
//! + arena) instead of `Node::create_action_server` (L1 manual-poll)
//! to match the unified two-layer API discipline. Handles
//! `example_interfaces/Fibonacci` goals on `/fibonacci`.

#![no_std]
#![no_main]

use example_interfaces::action::{Fibonacci, FibonacciFeedback, FibonacciGoal, FibonacciResult};
use nros::{CancelResponse, GoalResponse, GoalStatus, prelude::*};
use nros_board_mps2_an385_freertos::{Config, println, run};
use panic_semihosting as _;

#[unsafe(no_mangle)]
extern "C" fn _start() -> ! {
    run(
        Config::from_toml(include_str!("../config.toml")),
        |config| {
            let exec_config = ExecutorConfig::new(config.zenoh_locator)
                .domain_id(config.domain_id)
                .node_name("fibonacci_action_server");
            // Phase 104.A — bare-metal callers explicitly register the RMW
            // backend before `Executor::open`. POSIX hosts auto-register via
            // `.init_array`; this target doesn't walk that section.
            nros_rmw_zenoh::register().expect("Failed to register RMW backend");
            let mut executor = Executor::open(&exec_config)?;
            let _node = executor.create_node("fibonacci_action_server")?;

            let handle = executor.register_action_server::<Fibonacci, _, _>(
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

                let mut pending: heapless::Vec<(nros::GoalId, i32), 4> = heapless::Vec::new();
                handle.for_each_active_goal(&executor, |g| {
                    if g.status == GoalStatus::Accepted || g.status == GoalStatus::Executing {
                        let _ = pending.push((g.goal_id, g.goal.order));
                    }
                });
                for (goal_id, order) in pending {
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
                        let feedback = FibonacciFeedback {
                            sequence: sequence.clone(),
                        };
                        let _ = handle.publish_feedback(&mut executor, &goal_id, &feedback);
                    }

                    let result = FibonacciResult { sequence };
                    println!("Goal completed");
                    handle.complete_goal(&mut executor, &goal_id, GoalStatus::Succeeded, result);

                    goals_handled += 1;
                    if goals_handled >= 1 {
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
        },
    )
}
