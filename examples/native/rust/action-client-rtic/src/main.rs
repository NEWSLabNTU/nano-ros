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
//! This is the native equivalent of `examples/stm32f4/rust/rtic-action-client/`.

use example_interfaces::action::{Fibonacci, FibonacciGoal};
use nros::prelude::*;
use nros_log::{Logger, nros_error, nros_info};

// Phase 88.16.B — diagnostics route through `nros-log`.
static LOGGER: Logger = Logger::new("action-client-rtic");

extern crate nros_platform_cffi as _;

fn main() {
    nros_log::register_logger(&LOGGER);
    nros_log::init(nros_log::sinks::default());

    nros_info!(&LOGGER, "nros RTIC-pattern Action Client (native)");

    let config = ExecutorConfig::from_env().node_name("fibonacci_client");
    // Phase 227.3 (unified RMW) — no explicit `register()` call. The RMW is
    // declared via the `nros/rmw-zenoh` build feature; `nros`'s `#[used]
    // __FORCE_LINK_ZENOH` static keeps the backend's self-register section in
    // the link graph, and it fires inside `Executor::open` via the cffi-rmw
    // walker.
    let mut executor = Executor::open(&config).expect("Failed to open session");

    let mut node = executor
        .create_node("fibonacci_client")
        .expect("Failed to create node");
    let mut client = node
        .create_action_client::<Fibonacci>("/fibonacci")
        .expect("Failed to create action client");

    nros_info!(
        &LOGGER,
        "Action client created for /fibonacci (RTIC pattern)"
    );

    // Stabilization delay
    for _ in 0..300 {
        executor.spin_once(core::time::Duration::from_millis(0));
        std::thread::sleep(std::time::Duration::from_millis(10));
    }

    let goal = FibonacciGoal { order: 5 };
    nros_info!(&LOGGER, "Sending goal: order={}", goal.order);

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
        nros_error!(&LOGGER, "Goal not accepted (timeout)");
        std::process::exit(1);
    }
    nros_info!(&LOGGER, "Goal accepted: {:?}", goal_id);

    // Receive feedback via try_recv_feedback() loop
    let mut feedback_count = 0u32;
    for _ in 0..500 {
        executor.spin_once(core::time::Duration::from_millis(0));

        if let Ok(Some((id, feedback))) = client.try_recv_feedback()
            && id.uuid == goal_id.uuid
        {
            feedback_count += 1;
            nros_info!(
                &LOGGER,
                "Feedback #{}: {:?}",
                feedback_count,
                &feedback.sequence[..]
            );
            if feedback.sequence.len() as i32 > goal.order {
                break;
            }
        }

        std::thread::sleep(std::time::Duration::from_millis(10));
    }

    nros_info!(
        &LOGGER,
        "Done. Got {} feedback messages, goal accepted",
        feedback_count
    );
}
