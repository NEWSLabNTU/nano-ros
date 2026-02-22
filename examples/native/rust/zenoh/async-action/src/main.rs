//! Async Action Client Example — tokio background spin + StreamExt
//!
//! Demonstrates the async action client pattern:
//! 1. Create executor → create action client (owned, no lifetime to executor)
//! 2. Move executor to a background `spin_async()` task
//! 3. `.await` the goal acceptance Promise directly
//! 4. Stream feedback with `futures::StreamExt` combinators
//! 5. `.await` the get_result Promise
//!
//! # Usage
//!
//! ```bash
//! # Terminal 1: Start zenoh router
//! zenohd --listen tcp/127.0.0.1:7447
//!
//! # Terminal 2: Start the action server
//! cargo run -p native-rs-action-server
//!
//! # Terminal 3: Run this async client
//! cargo run -p native-rs-async-action
//! ```

use example_interfaces::action::{Fibonacci, FibonacciGoal};
use futures::StreamExt;
use log::{error, info, warn};
use nros::prelude::*;

#[tokio::main(flavor = "current_thread")]
async fn main() {
    env_logger::init();

    info!("nros Async Action Client Example (tokio + StreamExt)");
    info!("=====================================================");

    // Create executor
    let config = ExecutorConfig::from_env().node_name("async_fibonacci_client");
    let mut executor = Executor::<_, 8, 8192>::open(&config).expect("Failed to open session");

    // Create action client — owned type, no lifetime tied to node or executor.
    // The node is dropped at the end of this block, freeing the executor.
    let mut client = {
        let mut node = executor
            .create_node("async_fibonacci_client")
            .expect("Failed to create node");
        node.create_action_client::<Fibonacci>("/fibonacci")
            .expect("Failed to create action client")
    };

    info!("Action client created: /fibonacci");

    let goal = FibonacciGoal { order: 10 };
    let order = goal.order;
    info!("Sending goal: order={}", order);

    // LocalSet enables spawn_local (single-threaded, no Send bound needed)
    let local = tokio::task::LocalSet::new();
    local
        .run_until(async move {
            // Spawn spin_async() as a background task on the same thread.
            // This drives I/O so action messages can arrive.
            tokio::task::spawn_local(async move {
                executor.spin_async().await;
            });

            // ── Step 1: Send goal and await acceptance ──────────────
            let (goal_id, promise) = match client.send_goal(&goal) {
                Ok(pair) => pair,
                Err(e) => {
                    error!("Failed to send goal: {:?}", e);
                    return;
                }
            };

            let accepted = match promise.await {
                Ok(accepted) => accepted,
                Err(e) => {
                    error!("Goal acceptance failed: {:?}", e);
                    return;
                }
            };

            if !accepted {
                warn!("Goal was rejected by the server");
                return;
            }
            info!("Goal accepted! ID: {:?}", goal_id);

            // ── Step 2: Stream feedback with StreamExt ──────────────
            info!("Streaming feedback...");
            {
                let mut stream = client.feedback_stream_for(goal_id);
                let mut feedback_count = 0;

                // StreamExt::next() drives the stream one item at a time.
                // The background spin_async() task processes I/O concurrently.
                while let Some(result) = stream.next().await {
                    match result {
                        Ok(feedback) => {
                            feedback_count += 1;
                            info!("Feedback #{}: {:?}", feedback_count, feedback.sequence);

                            if feedback.sequence.len() as i32 > order {
                                info!("Received all feedback, action completed!");
                                break;
                            }
                        }
                        Err(e) => {
                            error!("Feedback error: {:?}", e);
                            break;
                        }
                    }
                }
            } // stream dropped — releases &mut client

            // ── Step 3: Get final result ────────────────────────────
            match client.get_result(&goal_id) {
                Ok(promise) => match promise.await {
                    Ok((status, result)) => {
                        info!(
                            "Result: status={:?}, sequence={:?}",
                            status, result.sequence
                        );
                    }
                    Err(e) => error!("get_result failed: {:?}", e),
                },
                Err(e) => error!("get_result failed: {:?}", e),
            }

            info!("Async action client finished");
        })
        .await;
}
