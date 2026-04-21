//! Native Lifecycle Node Example (REP-2002)
//!
//! Demonstrates the `lifecycle-services` feature: registers the five ROS 2
//! lifecycle services on a node, wires up transition callbacks, and then
//! spins indefinitely so `ros2 lifecycle set|get` can drive the state
//! machine from another terminal.
//!
//! # Usage
//!
//! ```bash
//! # Start the router:
//! zenohd --listen tcp/127.0.0.1:7447
//!
//! # Run the node:
//! cargo run -p native-rs-lifecycle-node
//!
//! # From another terminal (requires a ROS 2 install + rmw_zenoh):
//! ros2 lifecycle nodes
//! ros2 lifecycle get /lifecycle_demo
//! ros2 lifecycle set /lifecycle_demo configure
//! ros2 lifecycle set /lifecycle_demo activate
//! ros2 lifecycle set /lifecycle_demo deactivate
//! ros2 lifecycle set /lifecycle_demo cleanup
//! ros2 lifecycle list /lifecycle_demo
//! ```
//!
//! The transition callbacks log their invocation and return success; they
//! are written as `extern "C" fn` so this path exercises exactly the same
//! FFI surface the C API uses.

use core::ffi::c_void;
use core::time::Duration;

use log::info;
use nros::lifecycle::{LifecycleCallbackSlot, TransitionResult};
use nros::{Executor, ExecutorConfig};

unsafe extern "C" fn on_configure(_ctx: *mut c_void) -> u8 {
    info!("[callback] on_configure — allocating resources");
    TransitionResult::Success as u8
}

unsafe extern "C" fn on_activate(_ctx: *mut c_void) -> u8 {
    info!("[callback] on_activate — starting work");
    TransitionResult::Success as u8
}

unsafe extern "C" fn on_deactivate(_ctx: *mut c_void) -> u8 {
    info!("[callback] on_deactivate — pausing work");
    TransitionResult::Success as u8
}

unsafe extern "C" fn on_cleanup(_ctx: *mut c_void) -> u8 {
    info!("[callback] on_cleanup — releasing resources");
    TransitionResult::Success as u8
}

unsafe extern "C" fn on_shutdown(_ctx: *mut c_void) -> u8 {
    info!("[callback] on_shutdown — finalizing");
    TransitionResult::Success as u8
}

fn main() {
    env_logger::Builder::from_default_env()
        .filter_level(log::LevelFilter::Info)
        .init();
    info!("Lifecycle demo starting…");

    let config = ExecutorConfig::from_env().node_name("lifecycle_demo");
    let mut executor: Executor = Executor::open(&config).expect("Failed to open session");

    executor
        .register_lifecycle_services()
        .expect("Failed to register lifecycle services");
    info!("Lifecycle services registered on /lifecycle_demo");

    // Register transition callbacks on the executor-owned state machine.
    let sm = executor
        .lifecycle_state_machine_mut()
        .expect("services were registered above");
    sm.register(LifecycleCallbackSlot::Configure, Some(on_configure));
    sm.register(LifecycleCallbackSlot::Activate, Some(on_activate));
    sm.register(LifecycleCallbackSlot::Deactivate, Some(on_deactivate));
    sm.register(LifecycleCallbackSlot::Cleanup, Some(on_cleanup));
    sm.register(LifecycleCallbackSlot::Shutdown, Some(on_shutdown));

    info!(
        "Ready. Drive the lifecycle with `ros2 lifecycle set /lifecycle_demo configure`, etc."
    );

    // Spin indefinitely. Each spin_once drains the lifecycle services so
    // `ros2 lifecycle` queries round-trip.
    loop {
        let _ = executor.spin_once(Duration::from_millis(100));
    }
}
