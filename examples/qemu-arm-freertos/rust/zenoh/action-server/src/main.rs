//! FreeRTOS QEMU Action Server
//!
//! Handles `example_interfaces/Fibonacci` goals on `/fibonacci`.

#![no_std]
#![no_main]

use example_interfaces::action::{
    Fibonacci, FibonacciFeedback, FibonacciGoal, FibonacciResult,
};
use nros::prelude::*;
use nros_board_mps2_an385_freertos::{Config, println, run};
use panic_semihosting as _;

#[unsafe(no_mangle)]
extern "C" fn _start() -> ! {
    run(Config::from_toml(include_str!("../config.toml")), |config| {
        let exec_config = ExecutorConfig::new(config.zenoh_locator)
            .domain_id(config.domain_id)
            .node_name("fibonacci_action_server");
        let mut executor = Executor::open(&exec_config)?;
        let mut node = executor.create_node("fibonacci_action_server")?;

        let mut server = node.create_action_server::<Fibonacci>("/fibonacci")?;
        println!("Action server ready on /fibonacci");
        println!("Waiting for goals...");

        let mut goals_handled = 0u32;

        for _ in 0..100000u32 {
            match server.try_accept_goal(|_goal_id, goal: &FibonacciGoal| {
                println!("Goal request: order={}", goal.order);
                GoalResponse::AcceptAndExecute
            }) {
                Ok(Some(goal_id)) => {
                    println!("Goal accepted: {}", goal_id);

                    if let Some(active_goal) = server.get_goal(&goal_id) {
                        let order = active_goal.goal.order;
                        server.set_goal_status(&goal_id, GoalStatus::Executing);

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
                            let _ = server.publish_feedback(&goal_id, &feedback);

                            // Yield to network
                            for _ in 0..50 {
                                executor.spin_once(core::time::Duration::from_millis(10));
                            }
                        }

                        let result = FibonacciResult { sequence };
                        println!("Goal completed");
                        server.complete_goal(&goal_id, GoalStatus::Succeeded, result);
                    }

                    goals_handled += 1;
                    if goals_handled >= 1 {
                        // Spin to serve get_result queries before shutting down.
                        // The manual-polling action server is NOT in the executor
                        // arena, so spin_once() alone won't process get_result
                        // queries — we must call try_handle_get_result() explicitly.
                        for _ in 0..2000 {
                            executor.spin_once(core::time::Duration::from_millis(10));
                            let _ = server.try_handle_get_result();
                        }
                        println!("Server shutting down.");
                        return Ok(());
                    }
                }
                Ok(None) => {}
                Err(e) => {
                    println!("Error: {:?}", e);
                }
            }

            executor.spin_once(core::time::Duration::from_millis(10));
        }

        println!("Server timeout.");
        Ok::<(), NodeError>(())
    })
}
