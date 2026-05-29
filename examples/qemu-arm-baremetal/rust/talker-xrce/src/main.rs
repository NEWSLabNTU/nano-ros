//! Phase 207.3 — bare-metal XRCE talker (QEMU MPS2-AN385 + CMSDK UART0).
//!
//! Publishes `std_msgs/Int32` over an XRCE custom transport bound to UART0.
//! QEMU exposes UART0 as a host PTY (`-serial pty`); connect it to
//! `MicroXRCEAgent` over the same PTY (e.g. via `socat`) to bridge into the
//! ROS 2 network.
//!
//! Build / run:
//! ```sh
//! cargo run --release
//! # → QEMU prints the PTY path; on the host:
//! # MicroXRCEAgent serial --dev /dev/pts/<N> --baudrate 115200
//! ```

#![no_std]
#![no_main]

use nros::prelude::*;
use nros_board_mps2_an385::{Config, run, xrce_transport};
use nros_log::{Logger, nros_error, nros_info};
use nros_rmw_xrce_cffi as xrce;

static LOGGER: Logger = Logger::new("talker-xrce");
use panic_semihosting as _;
use std_msgs::msg::Int32;

#[nros_board_mps2_an385::entry]
fn main() -> ! {
    run(Config::from_toml(include_str!("../nros.toml")), |config| {
        nros_log::register_logger(&LOGGER);
        nros_log::init(nros_log::sinks::default());

        nros_info!(&LOGGER, "XRCE locator: {}", config.zenoh_locator);

        let exec_config = ExecutorConfig::new(config.zenoh_locator)
            .domain_id(config.domain_id)
            .node_name("talker_xrce");

        // Phase 207 — install the XRCE custom-transport vtable BEFORE
        // registering the backend / opening the executor. The shim wraps
        // the board's existing CMSDK UART0; `framing = true` selects
        // XRCE's HDLC framing for a byte-stream link.
        let ops = xrce_transport::xrce_transport_ops();
        // SAFETY: `ops`' fn pointers are static; XRCE's custom transport
        // contract (no concurrent read/write, no ISR invocation) is
        // satisfied by the single-threaded bare-metal executor.
        unsafe { xrce::set_custom_transport_ops(&ops, true) }
            .expect("install XRCE custom transport");

        // Phase 104.A — bare-metal callers explicitly register the RMW
        // backend before `Executor::open` (linkme `RMW_INIT_ENTRIES` is a
        // stub on `target_os = "none"`, Phase 142).
        xrce::register().expect("Failed to register XRCE backend");

        let mut executor = Executor::open(&exec_config)?;
        let mut node = executor.create_node("talker_xrce")?;

        nros_info!(&LOGGER, "Declaring publisher on /chatter (std_msgs/Int32)");
        let publisher = node.create_publisher::<Int32>("/chatter")?;
        nros_info!(&LOGGER, "Publisher declared");

        nros_info!(&LOGGER, "Publishing messages over XRCE serial...");

        let mut count: i32 = 0;
        loop {
            match publisher.publish(&Int32 { data: count }) {
                Ok(()) => nros_info!(&LOGGER, "Published: {}", count),
                Err(e) => nros_error!(&LOGGER, "Publish failed: {:?}", e),
            }
            count = count.wrapping_add(1);

            // Poll to drive the XRCE session (drain UART RX, send
            // keepalives, ~1 s between publishes).
            for _ in 0..100u32 {
                executor.spin_once(core::time::Duration::from_millis(10));
            }
        }

        #[allow(unreachable_code)]
        Ok::<(), NodeError>(())
    })
}
