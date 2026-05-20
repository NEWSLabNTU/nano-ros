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

// Phase 118 — RMW selection is build-time via mutually exclusive
// `rmw-{zenoh,cyclonedds,xrce}` features. `register_rmw()` fans out under
// `#[cfg(feature)]`; the rest of the file stays RMW-agnostic.

#[cfg(not(any(
    feature = "rmw-zenoh",
    feature = "rmw-cyclonedds",
    feature = "rmw-xrce"
)))]
compile_error!(
    "examples/native/rust/listener requires exactly one of \
     `rmw-zenoh`, `rmw-cyclonedds`, or `rmw-xrce` to be enabled.",
);

fn register_rmw() -> Result<(), &'static str> {
    #[cfg(feature = "rmw-zenoh")]
    {
        nros_rmw_zenoh::register().map_err(|_| "zenoh register failed")?;
    }
    #[cfg(feature = "rmw-cyclonedds")]
    {
        nros_rmw_cyclonedds_sys::register().map_err(|_| "cyclonedds register failed")?;
    }
    #[cfg(feature = "rmw-xrce")]
    {
        nros_rmw_xrce_cffi::register().map_err(|_| "xrce register failed")?;
    }
    Ok(())
}

/// Safety-e2e listener: validates CRC and tracks sequence gaps.
#[cfg(feature = "safety-e2e")]
fn main() {
    env_logger::init();

    info!("nros Native Listener (Zenoh Transport, Safety E2E)");
    info!("=====================================================");

    let config = ExecutorConfig::from_env().node_name("listener");
    // Phase 115.L.5 — install zenoh-pico C-vtable backend.

    // Phase 104.A — explicit RMW backend registration. The auto-ctor
    // in `.init_array` doesn't survive Rust's archive-walk linkage
    // when no symbol from the rlib is otherwise referenced.
    register_rmw().expect("Failed to register RMW backend");
    let mut executor: Executor = Executor::open(&config).expect("Failed to open session");

    let mut count: u64 = 0;
    executor
        .register_subscription_with_safety::<Int32, _>("/chatter", move |msg, status| {
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
    // Phase 104.A — explicit RMW backend registration. The auto-ctor
    // in `.init_array` doesn't survive Rust's archive-walk linkage
    // when no symbol from the rlib is otherwise referenced.
    register_rmw().expect("Failed to register RMW backend");
    let mut executor: Executor = Executor::open(&config).expect("Failed to open session");

    executor
        .register_subscription_with_info::<Int32, _>("/chatter", move |msg, info| {
            info!("Received: {}", msg.data);
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
