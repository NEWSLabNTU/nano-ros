//! NuttX QEMU ARM Action Client Example
//!
//! Sends a Fibonacci goal to `/fibonacci`, receives feedback.
//! Uses NuttX QEMU ARM virt (Cortex-A7 + virtio-net).

use example_interfaces::action::{Fibonacci, FibonacciGoal};
use nros::prelude::*;
use nros_board_nuttx_qemu_arm::{Config, run};

fn main() {
    run(Config::from_toml(include_str!("../config.toml")), |config| {
        let exec_config = ExecutorConfig::new(config.zenoh_locator)
            .domain_id(config.domain_id)
            .node_name("fibonacci_action_client");
        let mut executor = Executor::open(&exec_config)?;
        let mut node = executor.create_node("fibonacci_action_client")?;

        println!("Creating action client: /fibonacci (Fibonacci)");
        let mut client = node.create_action_client::<Fibonacci>("/fibonacci")?;
        println!("Client created — waiting for action server discovery...");

        // Race-3 fix (action variant): probe the server's send_goal queryable
        // via liveliness before the first send_goal. See the service-client
        // example for the full rationale.
        let server_seen = client.wait_for_action_server(
            &mut executor,
            core::time::Duration::from_secs(10),
        )?;
        if !server_seen {
            eprintln!("Action server /fibonacci not visible after 10s — bailing");
            return Err(NodeError::Timeout);
        }
        println!("Action server discovered — sending goal");
        println!();

        let goal = FibonacciGoal { order: 10 };
        println!("Sending goal: order={}", goal.order);

        let (goal_id, mut promise) = client.send_goal(&goal)?;
        let accepted = promise.wait(&mut executor, core::time::Duration::from_millis(10000))?;

        if !accepted {
            println!("Goal rejected!");
            return Ok(());
        }
        println!("Goal accepted! ID: {:?}", goal_id);
        println!();
        println!("Waiting for feedback...");

        let mut stream = client.feedback_stream_for(goal_id);
        let mut feedback_count = 0;
        for _ in 0..30 {
            match stream.wait_next(&mut executor, core::time::Duration::from_millis(1000)) {
                Ok(Some(feedback)) => {
                    feedback_count += 1;
                    println!("Feedback #{}: {:?}", feedback_count, feedback.sequence);

                    if feedback.sequence.len() as i32 > goal.order {
                        println!();
                        println!("All feedback received!");
                        println!("Final sequence: {:?}", feedback.sequence);
                        break;
                    }
                }
                Ok(None) => {}
                Err(e) => {
                    eprintln!("Feedback error: {:?}", e);
                    break;
                }
            }
        }

        println!();
        println!("Action client finished.");
        Ok::<(), NodeError>(())
    })
}
