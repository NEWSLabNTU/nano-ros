//! nros Zephyr DDS Service Client Example (Rust)
//!
//! ROS 2 / DDS-RTPS service client running on Zephyr RTOS, driven by
//! the cooperative `NrosPlatformRuntime<ZephyrPlatform>`. Uses the
//! Promise API: `client.call()` returns immediately, then
//! `promise.wait()` drives I/O and waits for the reply.

#![no_std]

use example_interfaces::srv::{AddTwoInts, AddTwoIntsRequest};
use log::{error, info};
use nros::{Executor, ExecutorConfig, NodeError};

#[no_mangle]
extern "C" fn rust_main() {
    unsafe {
        zephyr::set_logger().ok();
    }

    info!("nros Zephyr DDS Service Client");
    info!("Board: {}", zephyr::kconfig::CONFIG_BOARD);

    if let Err(e) = run() {
        error!("Error: {:?}", e);
    }
}

fn run() -> Result<(), NodeError> {
    let _ = nros::platform::zephyr::wait_for_network(2000);

    let config = ExecutorConfig::new("")
        .domain_id(0)
        .node_name("dds_service_client");
    let mut executor = Executor::open(&config)?;

    let mut node = executor.create_node("add_two_ints_client")?;
    let mut client = node.create_client::<AddTwoInts>("/add_two_ints")?;

    info!("Service client ready: /add_two_ints");
    info!("Sending service requests...");

    // Drive I/O for ~10 seconds while SPDP/SEDP discovery completes.
    // A blocking `zephyr::time::sleep` here would starve the
    // cooperative `NrosPlatformRuntime`: dust-dds's UDP sockets keep
    // queueing inbound discovery packets, the per-socket queue fills
    // up, and the Xilinx GEM driver runs out of `net_pkt` slots
    // ("RX packet buffer alloc failed: 110 bytes"). Interleave
    // `executor.spin_once` (drains the sockets) with brief
    // `zephyr::time::sleep` calls (yields to lower-prio threads /
    // tickless idle so wall-clock actually advances).
    for _ in 0..100 {
        executor.spin_once(core::time::Duration::from_millis(10));
        zephyr::time::sleep(zephyr::time::Duration::millis(100));
    }

    let mut count: i64 = 0;

    loop {
        let req = AddTwoIntsRequest {
            a: count,
            b: count + 1,
        };
        info!("[{}] Sending: {} + {}", count, req.a, req.b);

        let mut promise = client.call(&req)?;

        match promise.wait(&mut executor, core::time::Duration::from_millis(15000)) {
            Ok(resp) => {
                info!("[{}] Response: sum={}", count, resp.sum);
            }
            Err(e) => {
                error!("[{}] Call failed: {:?}", count, e);
            }
        }

        count += 1;
        zephyr::time::sleep(zephyr::time::Duration::secs(2));
    }
}
