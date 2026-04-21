//! Native RTIC-pattern Action Client
//!
//! Validates the RTIC action client pattern on native x86:
//! - `Executor<_, 0, 0>` (zero callback arena)
//! - `spin_once(0)` (non-blocking I/O drive)
//! - `client.send_goal()` + `promise.try_recv()` for acceptance
//! - `client.try_recv_feedback()` for feedback
//!
//! Note: `Promise::wait()` and `FeedbackStream::wait_next()` are NOT usable
//! in RTIC because they require `&mut Executor`. Use `try_recv()` loops instead.
//!
//! This is the native equivalent of `examples/stm32f4/rust/zenoh/rtic-action-client/`.

use example_interfaces::action::{Fibonacci, FibonacciGoal};
use log::info;
use nros::prelude::*;

fn main() {
    env_logger::init();

    info!("nros RTIC-pattern Action Client (native)");

    let config = ExecutorConfig::from_env().node_name("fibonacci_client");
    let mut executor = Executor::open(&config).expect("Failed to open session");

    let mut node = executor
        .create_node("fibonacci_client")
        .expect("Failed to create node");
    let mut client = node
        .create_action_client::<Fibonacci>("/fibonacci")
        .expect("Failed to create action client");

    info!("Action client created for /fibonacci (RTIC pattern)");

    // Stabilization delay
    for _ in 0..300 {
        executor.spin_once(core::time::Duration::from_millis(0));
        std::thread::sleep(std::time::Duration::from_millis(10));
    }

    let goal = FibonacciGoal { order: 5 };
    info!("Sending goal: order={}", goal.order);

    let (goal_id, mut promise) = client.send_goal(&goal).expect("Failed to send goal");

    // Poll for goal acceptance (~10s timeout)
    let mut accepted = false;
    for _ in 0..1000 {
        executor.spin_once(core::time::Duration::from_millis(0));
        if let Ok(Some(result)) = promise.try_recv() {
            accepted = result;
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }

    if !accepted {
        log::error!("Goal not accepted (timeout)");
        std::process::exit(1);
    }
    info!("Goal accepted: {:?}", goal_id);

    // Receive feedback via try_recv_feedback() loop
    let mut feedback_count = 0u32;
    for _ in 0..500 {
        executor.spin_once(core::time::Duration::from_millis(0));

        if let Ok(Some((id, feedback))) = client.try_recv_feedback()
            && id.uuid == goal_id.uuid
        {
            feedback_count += 1;
            info!("Feedback #{}: {:?}", feedback_count, &feedback.sequence[..]);
            if feedback.sequence.len() as i32 > goal.order {
                break;
            }
        }

        std::thread::sleep(std::time::Duration::from_millis(10));
    }

    info!(
        "Done. Got {} feedback messages, goal accepted",
        feedback_count
    );
}
