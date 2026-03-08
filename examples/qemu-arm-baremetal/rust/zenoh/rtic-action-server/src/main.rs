//! RTIC Action Server Example for nros on QEMU MPS2-AN385
//!
//! Handles Fibonacci action goals using RTIC v2's hardware-scheduled async tasks
//! with LAN9118 Ethernet networking.
//!
//! - `#[init]` calls `board::init_hardware()` and creates nano-ros handles
//! - `net_poll` task drives transport I/O via `spin_once(0)`
//! - `action_serve` task polls for goals, publishes feedback, completes goals,
//!   and handles get_result requests — all via manual polling
//! - All nano-ros handles are `#[local]` — no locks required
//!
//! # Running
//!
//! ```bash
//! cargo nano-ros generate
//! cargo run --release
//! ```

#![no_std]
#![no_main]

use panic_semihosting as _;

use example_interfaces::action::{Fibonacci, FibonacciFeedback, FibonacciGoal, FibonacciResult};
use nros::prelude::*;
use nros_mps2_an385::{Config, println};

use rtic_monotonics::systick::prelude::*;

systick_monotonic!(Mono, 1000);

// Type aliases for RTIC Local struct annotations
type RmwSrvServer = nros::internals::RmwServiceServer;
type RmwPub = nros::internals::RmwPublisher;
type NrosExecutor = Executor<nros::internals::RmwSession, 0, 0>;
type NrosActionServer = nros::ActionServer<Fibonacci, RmwSrvServer, RmwPub>;

#[rtic::app(device = mps2_an385_pac, dispatchers = [UARTRX0, UARTTX0])]
mod app {
    use super::*;

    #[shared]
    struct Shared {}

    #[local]
    struct Local {
        executor: NrosExecutor,
        server: NrosActionServer,
    }

    #[init]
    fn init(cx: init::Context) -> (Shared, Local) {
        let config = Config::default();
        nros_mps2_an385::init_hardware(&config);

        Mono::start(cx.core.SYST, 25_000_000);

        let exec_config = ExecutorConfig::new(config.zenoh_locator)
            .domain_id(config.domain_id)
            .node_name("fibonacci_server");
        let mut executor = Executor::<_, 0, 0>::open(&exec_config).unwrap();
        let mut node = executor.create_node("fibonacci_server").unwrap();
        let server = node
            .create_action_server::<Fibonacci>("/fibonacci")
            .unwrap();

        net_poll::spawn().unwrap();
        action_serve::spawn().unwrap();

        (Shared {}, Local { executor, server })
    }

    /// Drive transport I/O — equivalent to rclcpp spin_some().
    #[task(local = [executor], priority = 1)]
    async fn net_poll(cx: net_poll::Context) {
        loop {
            cx.local.executor.spin_once(0);
            Mono::delay(10.millis()).await;
        }
    }

    /// Handle action goals, feedback, and results.
    #[task(local = [server], priority = 1)]
    async fn action_serve(cx: action_serve::Context) {
        // Wait for zenoh session establishment
        Mono::delay(2000.millis()).await;

        println!("Action server ready: /fibonacci");

        loop {
            // Try to accept new goals
            match cx
                .local
                .server
                .try_accept_goal(|_goal_id, goal: &FibonacciGoal| {
                    println!("Goal request: order={}", goal.order);
                    GoalResponse::AcceptAndExecute
                }) {
                Ok(Some(goal_id)) => {
                    println!("Goal accepted");

                    if let Some(active_goal) = cx.local.server.get_goal(&goal_id) {
                        let order = active_goal.goal.order;

                        cx.local
                            .server
                            .set_goal_status(&goal_id, GoalStatus::Executing);

                        // Compute Fibonacci with feedback
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
                            let _ = cx.local.server.publish_feedback(&goal_id, &feedback);

                            Mono::delay(100.millis()).await;
                        }

                        let result = FibonacciResult { sequence };
                        cx.local
                            .server
                            .complete_goal(&goal_id, GoalStatus::Succeeded, result);

                        println!("Goal complete");
                    }

                    // Handle get_result requests after completing the goal
                    for _ in 0..200 {
                        let _ = cx.local.server.try_handle_get_result();
                        Mono::delay(10.millis()).await;
                    }
                }
                Ok(None) => {}
                Err(e) => println!("Accept error: {:?}", e),
            }

            // Handle cancel requests
            let _ = cx
                .local
                .server
                .try_handle_cancel(|_id, _status| nros::CancelResponse::Ok);

            Mono::delay(10.millis()).await;
        }
    }
}
