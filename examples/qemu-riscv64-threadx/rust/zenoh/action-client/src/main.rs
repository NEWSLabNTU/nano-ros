//! ThreadX QEMU RISC-V Action Client
//!
//! Sends a `example_interfaces/Fibonacci` goal to `/fibonacci`.

#![no_std]
#![no_main]

use example_interfaces::action::{Fibonacci, FibonacciGoal};
use nros::prelude::*;
use nros_board_threadx_qemu_riscv64::{Config, println, run};

#[unsafe(no_mangle)]
extern "C" fn main() -> ! {
    run(Config::from_toml(include_str!("../config.toml")), |config| {
        let exec_config = ExecutorConfig::new(config.zenoh_locator)
            .domain_id(config.domain_id)
            .node_name("fibonacci_action_client");
        let mut executor = Executor::open(&exec_config)?;
        let mut node = executor.create_node("fibonacci_action_client")?;

        let mut client = node.create_action_client::<Fibonacci>("/fibonacci")?;
        println!("Action client ready for /fibonacci");

        // Wait for server to be available
        for _ in 0..500 {
            executor.spin_once(core::time::Duration::from_millis(10));
        }

        let goal = FibonacciGoal { order: 5 };
        println!("Sending goal: order={}", goal.order);

        // Phase 120.3: retry the full send_goal + accept-poll cycle
        // on send_goal failure OR transient query errors during
        // try_recv. Matches the C action client's retry loop —
        // discovery may need more time after the 5 s prelude on
        // multi-threaded RTOS, and transient zenoh-pico query
        // errors (Phase 120.1 left NoData as Ok(None); other
        // backend errors still surface as ServiceRequestFailed
        // and shouldn't kill the example).
        let mut accepted_goal_id: Option<nros::GoalId> = None;
        'outer: for attempt in 0..5 {
            match client.send_goal(&goal) {
                Ok((gid, mut promise)) => {
                    let mut got_response = false;
                    for _ in 0..5000 {
                        executor.spin_once(core::time::Duration::from_millis(10));
                        match promise.try_recv() {
                            Ok(Some(result)) => {
                                if result {
                                    accepted_goal_id = Some(gid);
                                }
                                got_response = true;
                                break;
                            }
                            Ok(None) => {}
                            Err(_) => {
                                // transient backend error; drop this
                                // promise and re-send_goal next outer.
                                break;
                            }
                        }
                    }
                    if got_response {
                        break 'outer;
                    }
                    println!("Goal accept timed out (attempt {})", attempt + 1);
                }
                Err(e) => {
                    println!("Goal attempt {} failed: {:?}, retrying...", attempt + 1, e);
                    for _ in 0..500 {
                        executor.spin_once(core::time::Duration::from_millis(10));
                    }
                }
            }
        }
        let goal_id = match accepted_goal_id {
            Some(g) => g,
            None => {
                println!("Goal was rejected or timed out");
                return Ok(());
            }
        };
        println!("Goal accepted: {:?}", goal_id);

        // Poll for result
        println!("Requesting result...");
        let mut result_promise = client.get_result(&goal_id)?;
        for _ in 0..10000u32 {
            executor.spin_once(core::time::Duration::from_millis(10));
            if let Some((status, result)) = result_promise.try_recv()? {
                println!("Result status: {:?}", status);
                println!("Fibonacci sequence: {:?}", result.sequence);
                println!("Action completed successfully.");
                return Ok(());
            }
        }

        println!("Timeout waiting for result.");
        Ok::<(), NodeError>(())
    })
}
