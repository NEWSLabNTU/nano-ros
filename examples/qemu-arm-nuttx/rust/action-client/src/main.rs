//! NuttX QEMU ARM Action Client Example
//!
//! Sends a Fibonacci goal to `/fibonacci`, receives feedback.
//! Uses NuttX QEMU ARM virt (Cortex-A7 + virtio-net).

use example_interfaces::action::{Fibonacci, FibonacciGoal};
use nros::prelude::*;
use nros_board_nuttx_qemu_arm::{Config, run};
use nros_log::{Logger, nros_error, nros_info, nros_warn};

// Phase 88.16.D — diagnostics route through `nros-log`.
static LOGGER: Logger = Logger::new("action-client");

fn main() {
    run(
        Config::from_toml(include_str!("../config.toml")),
        |config| {
            nros_log::register_logger(&LOGGER);
            nros_log::init(nros_log::sinks::default());

            let exec_config = ExecutorConfig::new(config.zenoh_locator)
                .domain_id(config.domain_id)
                .node_name("fibonacci_action_client");
            // Phase 104.A — bare-metal callers explicitly register the RMW
            // backend before `Executor::open`. POSIX hosts auto-register via
            // `.init_array`; this target doesn't walk that section.
            nros_rmw_zenoh::register().expect("Failed to register RMW backend");
            let mut executor = Executor::open(&exec_config)?;
            let mut node = executor.create_node("fibonacci_action_client")?;

            nros_info!(&LOGGER, "Creating action client: /fibonacci (Fibonacci)");
            let mut client = node.create_action_client::<Fibonacci>("/fibonacci")?;
            nros_info!(
                &LOGGER,
                "Client created — waiting for action server discovery..."
            );

            // Race-3 fix (action variant): probe the server's send_goal queryable
            // via liveliness before the first send_goal. See the service-client
            // example for the full rationale.
            let server_seen = client
                .wait_for_action_server(&mut executor, core::time::Duration::from_secs(10))?;
            if !server_seen {
                nros_warn!(
                    &LOGGER,
                    "Action server /fibonacci not visible after 10s — bailing"
                );
                return Err(NodeError::Timeout);
            }
            nros_info!(&LOGGER, "Action server discovered — sending goal");
            nros_info!(&LOGGER, "");

            let goal = FibonacciGoal { order: 10 };
            nros_info!(&LOGGER, "Sending goal: order={}", goal.order);

            let (goal_id, mut promise) = client.send_goal(&goal)?;
            let accepted = promise.wait(&mut executor, core::time::Duration::from_millis(10000))?;

            if !accepted {
                nros_info!(&LOGGER, "Goal rejected!");
                return Ok(());
            }
            nros_info!(&LOGGER, "Goal accepted! ID: {:?}", goal_id);
            nros_info!(&LOGGER, "");
            nros_info!(&LOGGER, "Waiting for feedback...");

            let mut stream = client.feedback_stream_for(goal_id);
            let mut feedback_count = 0;
            for _ in 0..30 {
                match stream.wait_next(&mut executor, core::time::Duration::from_millis(1000)) {
                    Ok(Some(feedback)) => {
                        feedback_count += 1;
                        nros_info!(
                            &LOGGER,
                            "Feedback #{}: {:?}",
                            feedback_count,
                            feedback.sequence
                        );

                        if feedback.sequence.len() as i32 > goal.order {
                            nros_info!(&LOGGER, "");
                            nros_info!(&LOGGER, "All feedback received!");
                            nros_info!(&LOGGER, "Final sequence: {:?}", feedback.sequence);
                            break;
                        }
                    }
                    Ok(None) => {}
                    Err(e) => {
                        nros_warn!(&LOGGER, "Feedback error: {:?}", e);
                        break;
                    }
                }
            }

            nros_info!(&LOGGER, "");
            nros_info!(&LOGGER, "Action client finished.");
            Ok::<(), NodeError>(())
        },
    )
}
