//! nros Zephyr Service Server Example (Rust)
//!
//! A ROS 2 compatible service server running on Zephyr RTOS using the nros API.
//! The server responds to AddTwoInts service requests.

#![no_std]

use example_interfaces::srv::{AddTwoInts, AddTwoIntsResponse};
use log::{error, info};
#[allow(deprecated)]
use nros::{
    EmbeddedExecutor, EmbeddedNodeError, SessionMode, Transport, TransportConfig,
    internals::ShimTransport,
};

#[unsafe(no_mangle)]
extern "C" fn rust_main() {
    unsafe {
        zephyr::set_logger().ok();
    }

    info!("nros Zephyr Service Server");
    info!("Board: {}", zephyr::kconfig::CONFIG_BOARD);

    if let Err(e) = run() {
        error!("Error: {:?}", e);
    }
}

fn run() -> Result<(), EmbeddedNodeError> {
    let config = TransportConfig {
        locator: Some("tcp/192.0.2.2:7447"),
        mode: SessionMode::Client,
        properties: &[],
    };
    let session = ShimTransport::open(&config)
        .map_err(|_| EmbeddedNodeError::Transport(nros::TransportError::ConnectionFailed))?;
    let mut executor = EmbeddedExecutor::from_session(session);
    let mut node = executor.create_node("add_two_ints_server")?;
    let mut service = node.create_service::<AddTwoInts>("/add_two_ints")?;

    info!("Service server ready: /add_two_ints");
    info!("Waiting for service requests...");

    loop {
        let _ = executor.drive_io(100);
        let _ = service.handle_request(|req| {
            let sum = req.a + req.b;
            info!("{} + {} = {}", req.a, req.b, sum);
            AddTwoIntsResponse { sum }
        });
    }
}
