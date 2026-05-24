//! NuttX QEMU ARM Action Server (callback model)
//!
//! Phase 122.4 — uses `Executor::register_action_server` (L2 callback
//! + arena) for the unified two-layer API. Fibonacci server on
//! `/fibonacci` over NuttX QEMU ARM virt (Cortex-A7 + virtio-net).

use example_interfaces::action::{Fibonacci, FibonacciFeedback, FibonacciGoal, FibonacciResult};
use nros::{CancelResponse, GoalResponse, GoalStatus, prelude::*};
use nros_board_nuttx_qemu_arm::{Config, run};
use nros_log::{Logger, nros_info, nros_warn};

// Phase 88.16.D — diagnostics route through `nros-log`.
static LOGGER: Logger = Logger::new("action-server");

fn main() {
    run(
        Config::from_toml(include_str!("../config.toml")),
        |config| {
            nros_log::register_logger(&LOGGER);
            nros_log::init(nros_log::sinks::default());

            let exec_config = ExecutorConfig::new(config.zenoh_locator)
                .domain_id(config.domain_id)
                .node_name("fibonacci_action_server");
            // Phase 104.A — bare-metal callers explicitly register the RMW
            // backend before `Executor::open`. POSIX hosts auto-register via
            // `.init_array`; this target doesn't walk that section.
            nros_rmw_zenoh::register().expect("Failed to register RMW backend");
            let mut executor = Executor::open(&exec_config)?;
            let _node = executor.create_node("fibonacci_action_server")?;

            nros_info!(&LOGGER, "Creating action server: /fibonacci (Fibonacci)");
            let handle = executor.register_action_server::<Fibonacci, _, _>(
                "/fibonacci",
                |_goal_id, goal: &FibonacciGoal| {
                    nros_info!(&LOGGER, "Goal request: order={}", goal.order);
                    if goal.order >= 0 {
                        GoalResponse::AcceptAndExecute
                    } else {
                        GoalResponse::Reject
                    }
                },
                |_goal_id, _status| CancelResponse::Ok,
            )?;
            nros_info!(&LOGGER, "Action server ready");
            nros_info!(&LOGGER, "");
            nros_info!(&LOGGER, "Waiting for goals...");

            let mut goals_handled = 0u32;

            loop {
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
                        if let Err(e) = handle.publish_feedback(&mut executor, &goal_id, &feedback)
                        {
                            nros_warn!(&LOGGER, "Feedback error: {:?}", e);
                        } else {
                            nros_info!(&LOGGER, "Feedback: {:?}", feedback.sequence);
                        }
                    }

                    let result = FibonacciResult {
                        sequence: sequence.clone(),
                    };
                    nros_info!(&LOGGER, "Goal completed: {:?}", result.sequence);
                    handle.complete_goal(&mut executor, &goal_id, GoalStatus::Succeeded, result);

                    goals_handled += 1;
                    if goals_handled >= 3 {
                        nros_info!(&LOGGER, "");
                        nros_info!(&LOGGER, "Handled 3 goals, exiting.");
                        return Ok::<(), NodeError>(());
                    }
                }
            }
        },
    )
}
