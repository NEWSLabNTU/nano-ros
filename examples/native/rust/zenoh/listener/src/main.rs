//! Native Listener Example
//!
//! Demonstrates subscribing to messages using nros on native x86.
//! Uses the Executor API with callback-based subscriptions.
//!
//! # Without zenoh feature (simulation mode):
//! ```bash
//! cargo run -p native-rs-listener
//! ```
//!
//! # With zenoh feature (real transport):
//! ```bash
//! # Start zenoh router first:
//! zenohd --listen tcp/127.0.0.1:7447
//!
//! # Then run the listener:
//! cargo run -p native-rs-listener --features zenoh
//! ```
//!
//! # Enabling debug logs:
//! ```bash
//! RUST_LOG=debug cargo run -p native-rs-listener --features zenoh
//! ```

#[cfg(not(feature = "zenoh"))]
use log::{debug, error, info};
#[cfg(feature = "zenoh")]
use log::{error, info};
use nros::prelude::*;
use std_msgs::msg::Int32;

/// Safety-e2e listener: validates CRC and tracks sequence gaps.
#[cfg(all(feature = "zenoh", feature = "safety-e2e"))]
fn main() {
    env_logger::init();

    info!("nros Native Listener (Zenoh Transport, Safety E2E)");
    info!("=====================================================");

    let config = ExecutorConfig::from_env().node_name("listener");
    let mut executor = Executor::<_, 4, 4096>::open(&config).expect("Failed to open session");

    let mut count: u64 = 0;
    executor
        .add_subscription_with_safety::<Int32, _>("/chatter", move |msg, status| {
            count += 1;
            let crc_str = match status.crc_valid {
                Some(true) => "ok",
                Some(false) => "FAIL",
                None => "n/a",
            };
            info!(
                "[{}] Received: data={} [SAFETY] seq_gap={} dup={} crc={}",
                count, msg.data, status.gap, status.duplicate, crc_str
            );
        })
        .expect("Failed to add safety subscription");
    info!("Safety subscriber created for topic: /chatter");

    info!("Waiting for Int32 messages on /chatter...");
    info!("(Press Ctrl+C to exit)");

    if let Err(e) = executor.spin_blocking(SpinOptions::default()) {
        error!("Spin error: {:?}", e);
    }
}

/// Standard listener with MessageInfo: logs seq/gid at trace level.
/// When unstable-zenoh-api is enabled, the zero-copy path is used transparently.
#[cfg(all(feature = "zenoh", not(feature = "safety-e2e")))]
fn main() {
    env_logger::init();

    info!("nros Native Listener (Zenoh Transport)");
    info!("==========================================");

    let config = ExecutorConfig::from_env().node_name("listener");
    let mut executor = Executor::<_, 4, 4096>::open(&config).expect("Failed to open session");

    let mut count: u64 = 0;
    executor
        .add_subscription_with_info::<Int32, _>("/chatter", move |msg, info| {
            count += 1;
            info!("[{}] Received: data={}", count, msg.data);
            if let Some(info) = info {
                let gid = info.publisher_gid();
                log::trace!(
                    "seq={} gid={:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x} ",
                    info.publication_sequence_number(),
                    gid[0],
                    gid[1],
                    gid[2],
                    gid[3],
                    gid[4],
                    gid[5],
                    gid[6],
                    gid[7],
                );
            }
        })
        .expect("Failed to add subscription");
    info!("Subscriber created for topic: /chatter");

    info!("Waiting for Int32 messages on /chatter...");
    info!("(Press Ctrl+C to exit)");

    if let Err(e) = executor.spin_blocking(SpinOptions::default()) {
        error!("Spin error: {:?}", e);
    }
}

#[cfg(not(feature = "zenoh"))]
fn main() {
    env_logger::init();

    info!("nros Native Listener (Simulation Mode)");
    info!("==========================================");
    info!("Note: Running without zenoh transport.");
    info!("To use real transport, run with: --features zenoh");

    // Create a node (without transport)
    let config = NodeConfig::new("listener", "/demo");
    let mut node = StandaloneNode::<4, 4>::new(config);

    info!("Node created: {}", node.fully_qualified_name());

    // Create a subscriber for Int32 messages
    let subscriber = node
        .create_subscriber::<Int32>(SubscriberOptions::new("/chatter"))
        .expect("Failed to create subscriber");

    info!("Subscriber created for topic: /chatter");
    debug!("Message type: {}", Int32::TYPE_NAME);

    // Simulate receiving messages (in real implementation, bytes come from transport)
    // Here we create some test CDR data to demonstrate deserialization
    info!("Simulating received messages...");

    for i in 0..10 {
        // Create test CDR data (header + i32)
        // CDR header: [0x00, 0x01, 0x00, 0x00] (little-endian)
        // i32 value: little-endian bytes
        let value: i32 = i * 10;
        let mut test_data = vec![0x00, 0x01, 0x00, 0x00]; // CDR header
        test_data.extend_from_slice(&value.to_le_bytes()); // i32 payload

        // Deserialize the message
        match node.deserialize_message::<Int32>(&subscriber, &test_data) {
            Ok(msg) => {
                info!(
                    "[{}] Received (simulated): data={}, from {} bytes",
                    i,
                    msg.data,
                    test_data.len()
                );
            }
            Err(e) => {
                error!("Deserialization error: {:?}", e);
            }
        }

        std::thread::sleep(std::time::Duration::from_millis(500));
    }

    info!("Listener finished (simulation mode).");
}
