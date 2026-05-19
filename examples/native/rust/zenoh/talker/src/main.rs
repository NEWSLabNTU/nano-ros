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
//! to the CA certificate.
//!
//! # Diagnostics
//!
//! All diagnostic output flows through `nros-log`. The default
//! `PlatformSink` renders `[<LEVEL>] talker: <message>` on stderr.
//! Drop the runtime threshold to silence Info / Debug:
//!
//! ```ignore
//! LOGGER.set_level(nros_log::Severity::Warn);
//! ```

use nros::prelude::*;
use nros_log::{nros_debug, nros_error, nros_info, nros_trace, nros_warn, Logger, Severity};
use std_msgs::msg::Int32;

// Phase 88.16.B — pre-register the logger so `[INFO] talker: …`
// shows the expected name, not the catch-all `nros` default.
static LOGGER: Logger = Logger::new("talker");

// Force the nros-platform-cffi crate's `posix-c-port` C build into
// the link graph so `nros_platform_log_write` resolves.
extern crate nros_platform_cffi as _;

fn main() {
    nros_log::register_logger(&LOGGER);
    nros_log::init(nros_log::sinks::default());

    nros_info!(&LOGGER, "nros Native Talker (Zenoh Transport)");

    // Phase 128.B.1 — explicit register() drags nros-rmw-zenoh's
    // CGU into the binary on stable Rust.
    nros_rmw_zenoh::register().expect("Failed to register RMW backend");

    let config = ExecutorConfig::from_env().node_name("talker");
    let mut executor: Executor = Executor::open(&config).expect("Failed to open session");

    #[cfg(feature = "param-services")]
    {
        executor
            .register_parameter_services()
            .expect("Failed to register parameter services");
        executor.declare_parameter("start_value", ParameterValue::Integer(0));
        nros_info!(&LOGGER, "Parameter services registered for /talker");
    }

    let publisher = {
        let mut node = executor
            .create_node("talker")
            .expect("Failed to create node");
        nros_info!(&LOGGER, "Node created: talker");
        let pub_ = node
            .create_publisher::<Int32>("/chatter")
            .expect("Failed to create publisher");
        nros_info!(&LOGGER, "Publisher created for topic: /chatter");
        pub_
    };

    #[cfg(feature = "param-services")]
    let counter_start = {
        let v = executor.get_parameter_integer("start_value").unwrap_or(0) as i32;
        nros_info!(&LOGGER, "Counter start value: {}", v);
        v
    };
    #[cfg(not(feature = "param-services"))]
    let counter_start = 0i32;

    let mut count: i32 = counter_start;
    executor
        .register_timer(nros::TimerDuration::from_millis(1000), move || {
            let msg = Int32 { data: count };
            match publisher.publish(&msg) {
                Ok(()) => nros_info!(&LOGGER, "Published: {}", count),
                Err(e) => nros_error!(&LOGGER, "Publish error: {:?}", e),
            }
            count = count.wrapping_add(1);
        })
        .expect("Failed to register publish timer");
    nros_info!(&LOGGER, "Publishing Int32 messages every 1s...");

    // Reference Severity so the unused-import lint stays quiet if a
    // user wires set_level() into their own variant later.
    let _ = Severity::Info;

    executor
        .spin_blocking(SpinOptions::default())
        .expect("spin_blocking error");
}
