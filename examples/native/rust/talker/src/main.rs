//! Native Talker Example
//!
//! Demonstrates publishing messages using nros on native x86.
//! Uses the Executor API with timer callback for periodic publishing.
//!
//! # Usage
//!
//! ```bash
//! # Start zenoh router first:
//! zenohd --listen tcp/127.0.0.1:7447
//!
//! # Then run the talker:
//! cargo run -p native-rs-talker
//! ```
//!
//! # UDP transport
//!
//! zenoh-pico supports UDP on native/POSIX without any extra features.
//! Start zenohd with a UDP listener and set `ZENOH_LOCATOR`:
//!
//! ```bash
//! zenohd --listen udp/0.0.0.0:7447
//! ZENOH_LOCATOR=udp/127.0.0.1:7447 cargo run -p native-rs-talker
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
//! # Run talker with TLS (--features link-tls)
//! ZENOH_LOCATOR=tls/localhost:7447 \
//!   ZENOH_TLS_ROOT_CA_CERTIFICATE=cert.pem \
//!   cargo run -p native-rs-talker --features link-tls
//! ```
//!
//! # Enabling debug logs:
//! ```bash
//! RUST_LOG=debug cargo run -p native-rs-talker
//! ```

use log::{error, info};
use nros::prelude::*;
use std_msgs::msg::Int32;

// Phase 118 — RMW selection is build-time via the mutually
// exclusive `rmw-{zenoh,dds,xrce}` features.

#[cfg(not(any(feature = "rmw-zenoh", feature = "rmw-dds", feature = "rmw-xrce")))]
compile_error!(
    "examples/native/rust/talker requires exactly one of \
     `rmw-zenoh`, `rmw-dds`, or `rmw-xrce` to be enabled. \
     The default feature set picks `rmw-zenoh`; pass \
     `--no-default-features --features rmw-X` to switch.",
);

fn register_rmw() -> Result<(), &'static str> {
    // Phase 128.B.1 — on stable Rust the backend rlib is NOT pulled
    // into the link line unless something references one of its
    // symbols, even though its `RMW_INIT_ENTRIES` entry is `#[used]`.
    // The one-line `register()` call below doubles as both the
    // (idempotent) backend registration trigger AND the symbol
    // reference that drags the rlib's CGU into the binary. C/C++
    // builds avoid this because `--whole-archive` semantics for
    // static libs pulls every section entry unconditionally.
    #[cfg(feature = "rmw-zenoh")]
    {
        nros_rmw_zenoh::register().map_err(|_| "zenoh register failed")?;
    }
    #[cfg(feature = "rmw-dds")]
    {
        nros_rmw_dds::register().map_err(|_| "dds register failed")?;
    }
    #[cfg(feature = "rmw-xrce")]
    {
        nros_rmw_xrce_cffi::register().map_err(|_| "xrce register failed")?;
    }
    Ok(())
}

/// Per-RMW default locator. Overridden at runtime via
/// `NROS_LOCATOR` env var (then `ZENOH_LOCATOR` for zenoh-specific
/// back-compat; ignored on DDS / XRCE).
fn default_locator() -> &'static str {
    #[cfg(feature = "rmw-zenoh")]
    {
        "tcp/127.0.0.1:7447"
    }
    #[cfg(feature = "rmw-dds")]
    {
        "" // brokerless RTPS — locator ignored
    }
    #[cfg(feature = "rmw-xrce")]
    {
        "127.0.0.1:2019"
    }
}

const ACTIVE_RMW_NAME: &str = if cfg!(feature = "rmw-zenoh") {
    "Zenoh"
} else if cfg!(feature = "rmw-dds") {
    "DDS"
} else if cfg!(feature = "rmw-xrce") {
    "XRCE-DDS"
} else {
    "(none)"
};

fn main() {
    env_logger::init();

    info!("nros Native Talker ({} Transport)", ACTIVE_RMW_NAME);
    info!("=========================================");

    register_rmw().expect("Failed to register RMW backend");

    // Locator precedence: NROS_LOCATOR → ZENOH_LOCATOR → RMW-default.
    let locator = std::env::var("NROS_LOCATOR")
        .or_else(|_| std::env::var("ZENOH_LOCATOR"))
        .unwrap_or_else(|_| default_locator().to_string());
    let config = ExecutorConfig::new(&locator).node_name("talker");
    let mut executor: Executor = Executor::open(&config).expect("Failed to open session");

    // Register parameter services (when param-services feature is enabled)
    #[cfg(feature = "param-services")]
    {
        executor
            .register_parameter_services()
            .expect("Failed to register parameter services");
        executor.declare_parameter("start_value", ParameterValue::Integer(0));
        info!("Parameter services registered for /talker");
    }

    // Create publisher (scoped so the node drops; publisher is owned).
    let publisher = {
        let mut node = executor
            .create_node("talker")
            .expect("Failed to create node");
        info!("Node created: talker");
        let pub_ = node
            .create_publisher::<Int32>("/chatter")
            .expect("Failed to create publisher");
        info!("Publisher created for topic: /chatter");
        pub_
    };

    // Get counter start value from parameters (if available)
    #[cfg(feature = "param-services")]
    let counter_start = {
        let v = executor.get_parameter_integer("start_value").unwrap_or(0) as i32;
        info!("Counter start value: {}", v);
        v
    };
    #[cfg(not(feature = "param-services"))]
    let counter_start = 0i32;

    // Phase 122.4 — L2 timer-driven publish. Timer fires every 1 s;
    // closure owns the publisher + counter.
    let mut count: i32 = counter_start;
    executor
        .register_timer(nros::TimerDuration::from_millis(1000), move || {
            let msg = Int32 { data: count };
            match publisher.publish(&msg) {
                Ok(()) => info!("Published: {}", count),
                Err(e) => error!("Publish error: {:?}", e),
            }
            count = count.wrapping_add(1);
        })
        .expect("Failed to register publish timer");
    info!("Publishing Int32 messages every 1s...");

    executor
        .spin_blocking(SpinOptions::default())
        .expect("spin_blocking error");
}
