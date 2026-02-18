//! Native Talker Example
//!
//! Demonstrates publishing messages using nros on native x86.
//! Uses the Executor API with timer callback for periodic publishing.
//!
//! # Without zenoh feature (simulation mode):
//! ```bash
//! cargo run -p native-rs-talker
//! ```
//!
//! # With zenoh feature (real transport):
//! ```bash
//! # Start zenoh router first:
//! zenohd --listen tcp/127.0.0.1:7447
//!
//! # Then run the talker:
//! cargo run -p native-rs-talker --features zenoh
//! ```
//!
//! # Enabling debug logs:
//! ```bash
//! RUST_LOG=debug cargo run -p native-rs-talker --features zenoh
//! ```

#[cfg(not(feature = "zenoh"))]
use log::{debug, error, info};
#[cfg(feature = "zenoh")]
use log::{error, info};
use nros::prelude::*;
use std_msgs::msg::Int32;

#[cfg(feature = "zenoh")]
fn main() {
    env_logger::init();

    info!("nros Native Talker (Zenoh Transport)");
    info!("=========================================");

    // Create executor from environment (reads ZENOH_LOCATOR, ROS_DOMAIN_ID, ZENOH_MODE)
    let config = ExecutorConfig::from_env().node_name("talker");
    let mut executor = Executor::<_, 4, 4096>::open(&config).expect("Failed to open session");

    // Register parameter services (when param-services feature is enabled)
    #[cfg(feature = "param-services")]
    {
        executor
            .register_parameter_services("/demo/talker")
            .expect("Failed to register parameter services");
        executor.declare_parameter("start_value", ParameterValue::Integer(0));
        info!("Parameter services registered for /demo/talker");
    }

    // Create publisher
    let mut node = executor
        .create_node("talker")
        .expect("Failed to create node");
    info!("Node created: talker");

    let publisher = node
        .create_publisher::<Int32>("/chatter")
        .expect("Failed to create publisher");
    info!("Publisher created for topic: /chatter");
    info!("Publishing Int32 messages...");

    // Get counter start value from parameters (if available)
    #[cfg(feature = "param-services")]
    let counter_start = {
        let v = executor.get_parameter_integer("start_value").unwrap_or(0) as i32;
        info!("Counter start value: {}", v);
        v
    };
    #[cfg(not(feature = "param-services"))]
    let counter_start = 0i32;

    // Manual publish loop: publish first, then pump transport, then sleep.
    // This ensures the first message is sent immediately (important for tests).
    let mut count: i32 = counter_start;
    loop {
        let msg = Int32 { data: count };
        match publisher.publish(&msg) {
            Ok(()) => info!("[{}] Published: data={}", count, msg.data),
            Err(e) => error!("Publish error: {:?}", e),
        }
        count = count.wrapping_add(1);

        // Pump transport I/O
        executor.spin_once(10);

        // Sleep 1 second between messages (like ROS 2 demo)
        std::thread::sleep(std::time::Duration::from_secs(1));
    }
}

#[cfg(not(feature = "zenoh"))]
fn main() {
    env_logger::init();

    info!("nros Native Talker (Simulation Mode)");
    info!("=========================================");
    info!("Note: Running without zenoh transport.");
    info!("To use real transport, run with: --features zenoh");

    // Create a node (without transport)
    let config = NodeConfig::new("talker", "/demo");
    let mut node = StandaloneNode::<4, 4>::new(config);

    info!("Node created: {}", node.fully_qualified_name());

    // Create a publisher for Int32 messages
    let publisher = node
        .create_publisher::<Int32>(PublisherOptions::new("/chatter"))
        .expect("Failed to create publisher");

    info!("Publisher created for topic: /chatter");
    debug!("Message type: {}", Int32::TYPE_NAME);

    // Simulate publishing loop
    for i in 0..10 {
        let msg = Int32 { data: i };

        // Serialize the message (but don't actually send it)
        match node.serialize_message(&publisher, &msg) {
            Ok(bytes) => {
                info!(
                    "[{}] Serialized: data={}, {} bytes: {:02x?}...",
                    i,
                    msg.data,
                    bytes.len(),
                    &bytes[..bytes.len().min(16)]
                );
            }
            Err(e) => {
                error!("Serialization error: {:?}", e);
            }
        }

        std::thread::sleep(std::time::Duration::from_millis(500));
    }

    info!("Talker finished (simulation mode).");
}
