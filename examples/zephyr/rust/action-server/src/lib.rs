//! nros Zephyr Action Server (Rust, Fibonacci) — Phase 168.3 collapsed shape.

#![no_std]

#[cfg(not(any(feature = "rmw-zenoh", feature = "rmw-xrce", feature = "rmw-cyclonedds")))]
compile_error!("Exactly one rmw-* feature must be enabled.");

#[cfg(any(
    all(feature = "rmw-zenoh", feature = "rmw-xrce"),
    all(feature = "rmw-zenoh", feature = "rmw-cyclonedds"),
    all(feature = "rmw-xrce", feature = "rmw-cyclonedds"),
))]
compile_error!("rmw-zenoh / rmw-xrce / rmw-cyclonedds are mutually exclusive.");

use example_interfaces::action::{Fibonacci, FibonacciFeedback, FibonacciResult};
use log::{error, info};
use nros::{CancelResponse, Executor, ExecutorConfig, GoalResponse, GoalStatus, NodeError};

fn register_rmw() -> Result<(), &'static str> {
    #[cfg(feature = "rmw-zenoh")]
    { nros_rmw_zenoh::register().map_err(|_| "zenoh register failed")?; }
    #[cfg(feature = "rmw-xrce")]
    { nros_rmw_xrce_cffi::register().map_err(|_| "xrce register failed")?; }
    #[cfg(feature = "rmw-cyclonedds")]
    { nros_rmw_cyclonedds_sys::register().map_err(|_| "cyclonedds register failed")?; }
    Ok(())
}

#[cfg(feature = "rmw-zenoh")]
fn make_config() -> ExecutorConfig<'static> {
    ExecutorConfig::new("tcp/127.0.0.1:7476")
}

#[cfg(feature = "rmw-cyclonedds")]
fn make_config() -> ExecutorConfig<'static> {
    // Domain from Kconfig (CONFIG_NROS_DOMAIN_ID) — compile-time, embedded-style.
    // Test fixtures build distinct domains per role-set via -DCONFIG_NROS_DOMAIN_ID
    // so the native_sim Cyclone tests run in parallel (distinct RTPS ports).
    ExecutorConfig::new("")
        .domain_id(zephyr::kconfig::CONFIG_NROS_DOMAIN_ID as u32)
        .node_name("cyclonedds_action_server")
}

#[cfg(feature = "rmw-xrce")]
fn make_config() -> ExecutorConfig<'static> {
    use core::fmt::Write;
    static mut LOCATOR: heapless::String<48> = heapless::String::new();
    // SAFETY: single-threaded startup; this is the sole accessor of the
    // `LOCATOR` static, and `from_utf8_unchecked` is fed bytes written here
    // from formatted Kconfig string values (valid UTF-8).
    unsafe {
        let loc = core::ptr::addr_of_mut!(LOCATOR);
        (*loc).clear();
        let _ = write!(
            *loc,
            "{}:{}",
            zephyr::kconfig::CONFIG_NROS_XRCE_AGENT_ADDR,
            zephyr::kconfig::CONFIG_NROS_XRCE_AGENT_PORT
        );
        let s: &'static str = core::str::from_utf8_unchecked((*loc).as_bytes());
        ExecutorConfig::new(s).node_name("xrce_action_server")
    }
}

#[no_mangle]
extern "C" fn rust_main() {
    // SAFETY: installs the logger once during single-threaded startup, before any logging call.
    unsafe { zephyr::set_logger().ok(); }
    info!("nros Zephyr Action Server");
    info!("Board: {}", zephyr::kconfig::CONFIG_BOARD);
    if let Err(e) = run() {
        error!("Error: {:?}", e);
    }
}

fn run() -> Result<(), NodeError> {
    let _ = nros::platform::zephyr::wait_for_network(2000);
    register_rmw().expect("Failed to register RMW backend");

    let config = make_config();
    let mut executor = Executor::open(&config)?;
    let mut node = executor.create_node("fibonacci_action_server")?;
    let mut action_server = node.create_action_server::<Fibonacci>("/fibonacci")?;

    info!("Action server ready: /fibonacci");

    loop {
        executor.spin_once(core::time::Duration::from_millis(100));

        let _ = action_server.try_handle_cancel(|_goal_id, status| {
            if status == GoalStatus::Executing || status == GoalStatus::Accepted {
                CancelResponse::Ok
            } else {
                CancelResponse::GoalTerminated
            }
        });

        let _ = action_server.try_handle_get_result();

        let accepted = action_server.try_accept_goal(|_goal_id, goal| {
            info!("Goal request: order={}", goal.order);
            if goal.order >= 0 { GoalResponse::AcceptAndExecute } else { GoalResponse::Reject }
        })?;

        if let Some(goal_id) = accepted {
            let order = match action_server.get_goal(&goal_id) {
                Some(g) => g.goal.order,
                None => continue,
            };

            info!("Executing goal: order={}", order);
            action_server.set_goal_status(&goal_id, GoalStatus::Executing);

            let mut sequence: heapless::Vec<i32, 64> = heapless::Vec::new();
            let mut cancelled = false;

            for i in 0..=order {
                executor.spin_once(core::time::Duration::from_millis(10));
                let _ = action_server.try_handle_cancel(|_cid, status| {
                    if status == GoalStatus::Executing || status == GoalStatus::Accepted {
                        CancelResponse::Ok
                    } else {
                        CancelResponse::GoalTerminated
                    }
                });

                if let Some(g) = action_server.get_goal(&goal_id) {
                    if g.status == GoalStatus::Canceling {
                        cancelled = true;
                        break;
                    }
                }

                let next_val = if i == 0 {
                    0
                } else if i == 1 {
                    1
                } else {
                    let len = sequence.len();
                    sequence[len - 1] + sequence[len - 2]
                };
                let _ = sequence.push(next_val);

                let feedback = FibonacciFeedback { sequence: sequence.clone() };
                let _ = action_server.publish_feedback(&goal_id, &feedback);

                zephyr::time::sleep(zephyr::time::Duration::millis(500));
            }

            let result = FibonacciResult { sequence };
            if cancelled {
                action_server.complete_goal(&goal_id, GoalStatus::Canceled, result);
            } else {
                action_server.complete_goal(&goal_id, GoalStatus::Succeeded, result);
            }
        }
    }
}
