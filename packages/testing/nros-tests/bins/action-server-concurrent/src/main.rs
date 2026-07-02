//! Concurrent Fibonacci action-server fixture.
//!
//! Extracted from the `examples/native/rust/action-server`
//! `NROS_ACTION_CONCURRENT` escape hatch (phase-277 W5), when the example
//! slimmed to the official single-goal `fibonacci_action_server` demo.
//!
//! Accepts and advances several goals at once, draining `get_result` every
//! spin so multiple early get_result requests are held (deferred)
//! simultaneously — exercises the backends' seq-keyed reply tables under
//! real concurrent load. Consumed by `tests/rmw_interop.rs::
//! test_action_concurrent_nano_server_ros2_clients` (Zenoh build) and
//! `tests/xrce_ros2_interop.rs::test_xrce_action_ros2_concurrent`
//! (XRCE build, `--no-default-features --features rmw-xrce`).

use example_interfaces::action::{Fibonacci, FibonacciFeedback, FibonacciGoal, FibonacciResult};
use log::info;
use nros::prelude::*;

extern crate nros_platform_cffi as _;

fn main() -> ! {
    // Register the RMW backend the build linked (idempotent; must run before
    // the executor opens).
    nros_board_native::register_linked_rmw();

    env_logger::init();

    let ctx = nros::init_with_launch_auto().expect("nros init failed");
    let cfg = ctx.config("fibonacci_action_server");
    let mut executor = Executor::open(&cfg).expect("Failed to open session");

    let mut node = executor
        .create_node("fibonacci_action_server")
        .expect("Failed to create node");
    let mut server = node
        .create_action_server::<Fibonacci>("/fibonacci")
        .expect("Failed to create action server");
    // Both ready markers the consuming tests wait for.
    info!("Action server ready (concurrent mode)");
    info!("Waiting for action goals...");

    struct Tracked {
        id: GoalId,
        order: i32,
        seq: heapless::Vec<i32, 64>,
    }
    let mut tracked: heapless::Vec<Tracked, 4> = heapless::Vec::new();
    loop {
        let _ = executor.spin_once(core::time::Duration::from_millis(10));

        // Accept a new goal without blocking the in-flight ones.
        if let Ok(Some(goal_id)) = server.try_accept_goal(|_id, goal: &FibonacciGoal| {
            info!("Received goal request: order={}", goal.order);
            GoalResponse::AcceptAndExecute
        }) && let Some(ag) = server.get_goal(&goal_id)
        {
            let order = ag.goal.order;
            server.set_goal_status(&goal_id, GoalStatus::Executing);
            let _ = tracked.push(Tracked {
                id: goal_id,
                order,
                seq: heapless::Vec::new(),
            });
            info!("Goal accepted (concurrent): {goal_id}");
        }

        // Advance every tracked goal one Fibonacci step.
        let mut i = 0;
        while i < tracked.len() {
            let n = tracked[i].seq.len();
            let val = if n == 0 {
                0
            } else if n == 1 {
                1
            } else {
                tracked[i].seq[n - 1] + tracked[i].seq[n - 2]
            };
            let _ = tracked[i].seq.push(val);
            let fb = FibonacciFeedback {
                sequence: tracked[i].seq.clone(),
            };
            let _ = server.publish_feedback(&tracked[i].id, &fb);

            if tracked[i].seq.len() as i32 > tracked[i].order {
                let id = tracked[i].id;
                let result = FibonacciResult {
                    sequence: tracked[i].seq.clone(),
                };
                info!("Goal completed (concurrent): {:?}", result.sequence);
                // Flushes any get_result held for this goal.
                server.complete_goal(&id, GoalStatus::Succeeded, result);
                let _ = tracked.swap_remove(i);
            } else {
                i += 1;
            }
        }

        // Drain get_result: requests for still-active goals are held;
        // completed goals reply immediately.
        let _ = server.try_handle_get_result();
        std::thread::sleep(core::time::Duration::from_millis(100));
    }
}
