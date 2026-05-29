//! nros Zephyr Action Client (Rust, Fibonacci) — Phase 168.3 collapsed shape.

#![no_std]

#[cfg(not(any(feature = "rmw-zenoh", feature = "rmw-xrce", feature = "rmw-cyclonedds")))]
compile_error!("Exactly one rmw-* feature must be enabled.");

#[cfg(any(
    all(feature = "rmw-zenoh", feature = "rmw-xrce"),
    all(feature = "rmw-zenoh", feature = "rmw-cyclonedds"),
    all(feature = "rmw-xrce", feature = "rmw-cyclonedds"),
))]
compile_error!("rmw-zenoh / rmw-xrce / rmw-cyclonedds are mutually exclusive.");

use example_interfaces::action::{Fibonacci, FibonacciGoal};
use log::{error, info, warn};
use nros::{Executor, ExecutorConfig, NodeError};

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
        .node_name("cyclonedds_action_client")
}

#[cfg(feature = "rmw-xrce")]
fn make_config() -> ExecutorConfig<'static> {
    use core::fmt::Write;
    static mut LOCATOR: heapless::String<48> = heapless::String::new();
    // SAFETY: single-threaded startup; this is the sole accessor of the
    // `LOCATOR` static, and `from_utf8_unchecked` is fed bytes written here
    // from formatted Kconfig string values (valid UTF-8).
    unsafe {
        LOCATOR.clear();
        let _ = write!(
            LOCATOR,
            "{}:{}",
            zephyr::kconfig::CONFIG_NROS_XRCE_AGENT_ADDR,
            zephyr::kconfig::CONFIG_NROS_XRCE_AGENT_PORT
        );
        let s: &'static str = core::str::from_utf8_unchecked(LOCATOR.as_bytes());
        ExecutorConfig::new(s).node_name("xrce_action_client")
    }
}

#[no_mangle]
extern "C" fn rust_main() {
    // SAFETY: installs the logger once during single-threaded startup, before any logging call.
    unsafe { zephyr::set_logger().ok(); }
    info!("nros Zephyr Action Client");
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
    let mut node = executor.create_node("fibonacci_action_client")?;
    let mut action_client = node.create_action_client::<Fibonacci>("/fibonacci")?;

    info!("Action client ready: /fibonacci");

    // DDS needs SPDP/SEDP discovery warmup; zenoh + xrce use brief sleep.
            zephyr::time::sleep(zephyr::time::Duration::secs(3));
        let goal = FibonacciGoal { order: 10 };
    info!("Sending goal: order={}", goal.order);

    let (goal_id, mut promise) = action_client.send_goal(&goal)?;
    let accepted = match promise.wait(&mut executor, core::time::Duration::from_millis(10000)) {
        Ok(a) => a,
        Err(e) => { error!("Goal acceptance failed: {:?}", e); return Err(e); }
    };
    if !accepted {
        warn!("Goal rejected"); return Ok(());
    }
    info!("Goal accepted! ID: {:02x}{:02x}{:02x}{:02x}...",
        goal_id.uuid[0], goal_id.uuid[1], goal_id.uuid[2], goal_id.uuid[3]);

    {
        let mut stream = action_client.feedback_stream_for(goal_id);
        let mut feedback_count: u32 = 0;
        for _ in 0..60 {
            match stream.wait_next(&mut executor, core::time::Duration::from_millis(1000)) {
                Ok(Some(feedback)) => {
                    feedback_count += 1;
                    info!("Feedback #{}: {:?}", feedback_count, feedback.sequence.as_slice());
                    if feedback.sequence.len() as i32 > goal.order { break; }
                }
                Ok(None) => {
                    if feedback_count == 0 { error!("No feedback yet, retrying..."); }
                }
                Err(e) => { error!("Feedback error: {:?}", e); break; }
            }
        }
    }

    let mut result_promise = action_client.get_result(&goal_id)?;
    match result_promise.wait(&mut executor, core::time::Duration::from_millis(30000)) {
        Ok((status, result)) => info!("Result: status={:?}, sequence={:?}",
            status, result.sequence.as_slice()),
        Err(e) => error!("Failed to get result: {:?}", e),
    }

    info!("Action client finished");
    loop { zephyr::time::sleep(zephyr::time::Duration::secs(10)); }
}
