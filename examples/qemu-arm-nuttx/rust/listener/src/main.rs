//! NuttX QEMU ARM Listener Example
//!
//! Subscribes to `std_msgs/Int32` messages on `/chatter`.
//! Uses NuttX QEMU ARM virt (Cortex-A7 + virtio-net).

use nros::prelude::*;
use nros_board_nuttx_qemu_arm::{Config, run};
use nros_log::{Logger, nros_info};

// Phase 88.16.D — diagnostics route through `nros-log`.
static LOGGER: Logger = Logger::new("listener");
use std_msgs::msg::Int32;

fn main() {
    run(Config::from_toml(include_str!("../nros.toml")), |config| {
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
    })
}
