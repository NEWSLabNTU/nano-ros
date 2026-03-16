//! NuttX QEMU ARM Action Server Example
//!
//! Implements a Fibonacci action server on `/fibonacci`.
//! Computes the Fibonacci sequence up to a given order, publishing feedback.
//! Uses NuttX QEMU ARM virt (Cortex-A7 + virtio-net).

use example_interfaces::action::{Fibonacci, FibonacciFeedback, FibonacciGoal, FibonacciResult};
use nros::prelude::*;
use nros_nuttx_qemu_arm::{Config, run};

fn main() {
    run(Config::from_toml(include_str!("../config.toml")), |config| {
        let exec_config = ExecutorConfig::new(config.zenoh_locator)
            .domain_id(config.domain_id)
            .node_name("fibonacci_action_server");
        let mut executor = Executor::open(&exec_config)?;
        let mut node = executor.create_node("fibonacci_action_server")?;

        println!("Creating action server: /fibonacci (Fibonacci)");
        let mut server = node.create_action_server::<Fibonacci>("/fibonacci")?;
        println!("Action server ready");
        println!();
        println!("Waiting for goals...");

        let mut goals_handled = 0u32;

        loop {
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

                            if let Err(e) = server.publish_feedback(&goal_id, &feedback) {
                                eprintln!("Feedback error: {:?}", e);
                            } else {
                                println!("Feedback: {:?}", feedback.sequence);
                            }

                            // Brief delay between feedback messages
                            for _ in 0..50 {
                                executor.spin_once(10);
                            }
                        }

                        let result = FibonacciResult { sequence };
                        println!("Goal completed: {:?}", result.sequence);
                        server.complete_goal(&goal_id, GoalStatus::Succeeded, result);
                    }

                    goals_handled += 1;
                    if goals_handled >= 3 {
                        println!();
                        println!("Handled 3 goals, exiting.");
                        break;
                    }
                }
                Ok(None) => {
                    executor.spin_once(10);
                }
                Err(e) => {
                    eprintln!("Error accepting goal: {:?}", e);
                }
            }
        }

        Ok::<(), NodeError>(())
    })
}
