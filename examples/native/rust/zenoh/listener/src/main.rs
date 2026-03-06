//! Native Listener Example
//!
//! Demonstrates subscribing to messages using nros on native x86.
//! Uses the Executor API with callback-based subscriptions.
//!
//! # Usage
//!
//! ```bash
//! # Start zenoh router first:
//! zenohd --listen tcp/127.0.0.1:7447
//!
//! # Then run the listener:
//! cargo run -p native-rs-listener
//! ```
//!
//! # UDP transport
//!
//! zenoh-pico supports UDP on native/POSIX without any extra features.
//! Start zenohd with a UDP listener and set `ZENOH_LOCATOR`:
//!
//! ```bash
//! zenohd --listen udp/0.0.0.0:7447
//! ZENOH_LOCATOR=udp/127.0.0.1:7447 cargo run -p native-rs-listener
//! ```
//!
//! # TLS transport
//!
//! TLS requires system mbedTLS (`sudo apt install libmbedtls-dev`) and the
//! `link-tls` feature. Generate a self-signed certificate, start zenohd with
//! a TLS listener, then connect with `ZENOH_TLS_ROOT_CA_CERTIFICATE` pointing
//! to the CA certificate:
//!
//! ```bash
//! # Generate test certificate
//! openssl req -x509 -newkey ec -pkeyopt ec_paramgen_curve:prime256v1 \
//!   -keyout key.pem -out cert.pem -days 365 -nodes -subj "/CN=localhost"
//!
//! # Start zenohd with TLS
//! zenohd --no-multicast-scouting --listen tls/localhost:7447 \
//!   --cfg 'transport/link/tls/listen_certificate:"cert.pem"' \
//!   --cfg 'transport/link/tls/listen_private_key:"key.pem"'
//!
//! # Run listener with TLS (--features link-tls)
//! ZENOH_LOCATOR=tls/localhost:7447 \
//!   ZENOH_TLS_ROOT_CA_CERTIFICATE=cert.pem \
//!   cargo run -p native-rs-listener --features link-tls
//! ```
//!
//! # Enabling debug logs:
//! ```bash
//! RUST_LOG=debug cargo run -p native-rs-listener
//! ```

use log::{error, info};
use nros::prelude::*;
use std_msgs::msg::Int32;

/// Safety-e2e listener: validates CRC and tracks sequence gaps.
#[cfg(feature = "safety-e2e")]
fn main() {
    env_logger::init();

    info!("nros Native Listener (Zenoh Transport, Safety E2E)");
    info!("=====================================================");

    let config = ExecutorConfig::from_env().node_name("listener");
    let mut executor: Executor<_> = Executor::open(&config).expect("Failed to open session");

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
#[cfg(not(feature = "safety-e2e"))]
fn main() {
    env_logger::init();

    info!("nros Native Listener (Zenoh Transport)");
    info!("==========================================");

    let config = ExecutorConfig::from_env().node_name("listener");
    let mut executor: Executor<_> = Executor::open(&config).expect("Failed to open session");

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
