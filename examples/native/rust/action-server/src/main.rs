//! Native Action Server — Phase 212.L.2 Application pkg shape.
//!
//! Serves the `Fibonacci` action on `/fibonacci`, publishing feedback as
//! it computes. Single-file `[[bin]]`: explicit
//! [`nros::init_with_launch_auto`] (Pattern 2) then a user-owned spin
//! loop.
//!
//! ```bash
//! cargo run -p native-rs-action-server   # then native-rs-action-client
//! ```

use example_interfaces::action::{Fibonacci, FibonacciFeedback, FibonacciGoal, FibonacciResult};
use log::{error, info};
use nros::prelude::*;

// RMW selection is build/config, never application logic (RFC-0031): the backend
// is the one `nros-rmw-*` optional dep activated by the config-lowered
// `rmw-{zenoh,xrce,cyclonedds}` feature. The `#[used]` static(s) below are a pure
// LINK-FORCE — they reference the backend's `register` symbol so the rlib's
// linkme `RMW_INIT_ENTRIES` self-register section is pulled into the link graph
// (rlib archive linking drops unreferenced objects, so this reference is
// required, NOT a `register()` call). The cffi walker in `nros::init` then
// discovers + registers the backend. Accepted link-force pattern (cf.
// `extern crate nros_platform_cffi as _`), not an RMW leak.
#[cfg(feature = "rmw-zenoh")]
#[used]
static __FORCE_LINK_ZENOH: fn() -> Result<(), nros_rmw_zenoh::RegisterError> =
    nros_rmw_zenoh::register;
#[cfg(feature = "rmw-xrce")]
#[used]
static __FORCE_LINK_XRCE: fn() -> Result<(), nros_rmw_xrce_cffi::RegisterError> =
    nros_rmw_xrce_cffi::register;
#[cfg(feature = "rmw-cyclonedds")]
#[used]
static __FORCE_LINK_CYCLONEDDS_SYS: fn() -> Result<(), nros_rmw_cyclonedds_sys::RegisterError> =
    nros_rmw_cyclonedds_sys::register;

fn main() -> ! {
    env_logger::init();
    info!("nros Action Server Example");
    info!("================================");

    // Phase 244 E3 — the action's cancel-service + status-publisher protocol
    // types (`action_msgs/srv/CancelGoal_{Request,Response}`,
    // `action_msgs/msg/GoalStatusArray`) are now registered by the framework via
    // `RosAction::register_protocol_types` (generated impl), no example-side
    // registration needed.
    let ctx = nros::init_with_launch_auto().expect("nros init failed");
    let cfg = ctx.config("fibonacci_action_server");
    let mut executor = Executor::open(&cfg).expect("Failed to open session");

    let mut node = executor
        .create_node("fibonacci_action_server")
        .expect("Failed to create node");
    info!("Node created: fibonacci_action_server");
    let mut server = node
        .create_action_server::<Fibonacci>("/fibonacci")
        .expect("Failed to create action server");
    info!("Action server created: /fibonacci");
    info!("Waiting for action goals...");

    // Phase 237 — concurrent mode: accept and advance several goals at once,
    // draining get_result every spin so multiple early get_result requests are
    // held (deferred) simultaneously. Exercises the backends' seq-keyed reply
    // tables under real concurrent load. Opt-in via NROS_ACTION_CONCURRENT;
    // the default path below stays the simple one-goal-at-a-time demo.
    if std::env::var("NROS_ACTION_CONCURRENT").is_ok() {
        info!("Concurrent action server mode");
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

    loop {
        let _ = executor.spin_once(core::time::Duration::from_millis(10));
        match server.try_accept_goal(|_goal_id, goal: &FibonacciGoal| {
            info!("Received goal request: order={}", goal.order);
            GoalResponse::AcceptAndExecute
        }) {
            Ok(Some(goal_id)) => {
                info!("Goal accepted: {}", goal_id);
                let _ = executor.spin_once(core::time::Duration::from_millis(10));
                if let Some(active_goal) = server.get_goal(&goal_id) {
                    let order = active_goal.goal.order;
                    server.set_goal_status(&goal_id, GoalStatus::Executing);
                    let _ = executor.spin_once(core::time::Duration::from_millis(10));
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
                        if server.publish_feedback(&goal_id, &feedback).is_ok() {
                            info!("Feedback: {:?}", feedback.sequence);
                        }
                        let _ = executor.spin_once(core::time::Duration::from_millis(10));
                        // Drain get_result queries during execution. A real
                        // `rcl_action` client (rclcpp_action) sends get_result
                        // right after acceptance; the server defers the reply
                        // until the goal terminates (Phase 237).
                        let _ = server.try_handle_get_result();
                        std::thread::sleep(std::time::Duration::from_millis(500));
                    }
                    let result = FibonacciResult { sequence };
                    info!("Goal completed: {:?}", result.sequence);
                    // `complete_goal` flushes any deferred get_result replies.
                    server.complete_goal(&goal_id, GoalStatus::Succeeded, result);
                    let _ = executor.spin_once(core::time::Duration::from_millis(10));
                    // Also answer a get_result that arrives only after completion
                    // (it hits the completed-results path → immediate reply).
                    let _ = server.try_handle_get_result();
                    let _ = executor.spin_once(core::time::Duration::from_millis(10));
                }
            }
            Ok(None) => std::thread::sleep(std::time::Duration::from_millis(100)),
            Err(e) => error!("Error accepting goal: {:?}", e),
        }
    }
}
