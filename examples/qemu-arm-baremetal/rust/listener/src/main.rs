//! Simple QEMU Listener using nros-board-mps2-an385
//!
//! Subscribes to typed `std_msgs/Int32` messages on `/chatter`.

#![no_std]
#![no_main]

use nros::prelude::*;
use nros_board_mps2_an385::{Config, run};
use nros_log::{Logger, nros_info};

// Phase 88.16.C — diagnostics route through `nros-log`.
static LOGGER: Logger = Logger::new("listener");
use panic_semihosting as _;
use std_msgs::msg::Int32;

// Phase 213.E.1 — zenoh locator overridable at build time via `NROS_LOCATOR`
// env-var (compile-time, keeps `#![no_std]` clean). Falls back to the QEMU
// slirp fixture default. MAC/IP/gateway tuples stay literal for now —
// board-internal smoltcp tuning, not user-facing config.
// TODO(213.E later): move MAC/IP/gateway to
// [package.metadata.nros.deploy.<target>] once macro/board-crate plumbing
// lands.
const LOCATOR: &str = match option_env!("NROS_LOCATOR") {
    Some(s) => s,
    None => "tcp/10.0.2.2:7450",
};

#[nros_board_mps2_an385::entry]
fn main() -> ! {
    // Load config from nros.toml (different IP/MAC than talker)
    run(
        Config {
            mac: [0x02, 0x00, 0x00, 0x00, 0x00, 0x01],
            ip: [10, 0, 2, 11],
            prefix: 24,
            gateway: [10, 0, 2, 2],
            zenoh_locator: LOCATOR,
            domain_id: 0,
        },
        |config| {
            nros_log::register_logger(&LOGGER);
            nros_log::init(nros_log::sinks::default());

            let exec_config = ExecutorConfig::new(config.zenoh_locator)
                .domain_id(config.domain_id)
                .node_name("listener");
            // Phase 104.A — bare-metal callers explicitly register the RMW
            // backend before `Executor::open`. POSIX hosts auto-register via
            // `.init_array`; this target doesn't walk that section.
            nros_rmw_zenoh::register().expect("Failed to register RMW backend");
            let mut executor = Executor::open(&exec_config)?;
            let nid = executor.node_builder("listener").build()?;

            nros_info!(&LOGGER, "Subscribing to /chatter (std_msgs/Int32)");
            executor
                .node_mut(nid)
                .create_subscription::<Int32, _>("/chatter", |msg: &Int32| {
                    nros_info!(&LOGGER, "Received: {}", msg.data);
                })?;

            nros_info!(&LOGGER, "Subscriber declared");
            nros_info!(&LOGGER, "Waiting for messages...");

            loop {
                executor.spin_once(core::time::Duration::from_millis(10));
            }

            #[allow(unreachable_code)]
            Ok::<(), NodeError>(())
        },
    )
}
