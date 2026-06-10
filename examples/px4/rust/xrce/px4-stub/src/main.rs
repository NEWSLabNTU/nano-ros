//! Fake-PX4 XRCE-DDS stub — Phase 233.4 (RFC-0039 Track B).
//!
//! Stands in for PX4's `uxrce_dds_client` so the companion example can be
//! driven without a full PX4 SITL: publishes `VehicleOdometry` on
//! `/fmu/out/vehicle_odometry` with the PX4 QoS profile — exactly what a real
//! PX4 advertises.
//!
//! NOTE: a *bare* `MicroXRCEAgent` (no px4_msgs typesupport) registers DDS
//! types by name only; two such clients match for built-in ROS types but not
//! for `px4_msgs::msg::dds_::*`. A real PX4 runs the agent with px4_msgs
//! typesupport, so the companion round-trip works there. See
//! `docs/issues/` (px4-xrce bare-agent type matching) and
//! `docs/reference/px4-xrce-companion.md`.
//!
//! ```bash
//! MicroXRCEAgent udp4 -p 8888
//! NROS_LOCATOR=127.0.0.1:8888 cargo run -p px4-stub
//! ```
//!
//! Environment:
//!   NROS_LOCATOR    — agent `host:port` (default `127.0.0.1:8888`)
//!   ROS_DOMAIN_ID   — DDS domain (default `0`)
//!   PX4_STUB_TICKS  — publish N samples then exit (default: stream forever)

use log::info;
use nros::prelude::*;
use px4_msgs::msg::VehicleOdometry;

extern crate nros_platform_cffi as _;

fn main() {
    env_logger::init();

    let locator = std::env::var("NROS_LOCATOR").unwrap_or_else(|_| "127.0.0.1:8888".to_string());
    let domain_id: u32 = std::env::var("ROS_DOMAIN_ID")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);

    info!("PX4 stub: agent={locator} domain={domain_id}");

    let config = ExecutorConfig::new(&locator)
        .domain_id(domain_id)
        .node_name("px4_stub");
    let mut executor: Executor = Executor::open(&config).expect("Failed to open XRCE session");

    let nid = executor
        .node_builder("px4_stub")
        .build()
        .expect("Failed to build node");

    let odom = executor
        .node_mut(nid)
        .publisher("/fmu/out/vehicle_odometry")
        .qos(QosSettings::px4())
        .typed::<VehicleOdometry>()
        .build()
        .expect("Failed to advertise /fmu/out/vehicle_odometry");

    info!("publishing VehicleOdometry on /fmu/out/vehicle_odometry");

    let max = std::env::var("PX4_STUB_TICKS")
        .ok()
        .and_then(|s| s.parse::<u64>().ok());

    let mut tick: u64 = 0;
    loop {
        let t = tick.wrapping_mul(100_000);
        let f = tick as f32;
        let msg = VehicleOdometry {
            timestamp: t,
            timestamp_sample: t,
            position: [f, f * 2.0, f * 3.0],
            q: [1.0, 0.0, 0.0, 0.0],
            ..Default::default()
        };
        if let Err(e) = odom.publish(&msg) {
            log::warn!("publish vehicle_odometry failed: {e:?}");
        }
        info!("published odometry tick={tick}");
        tick = tick.wrapping_add(1);

        for _ in 0..10u32 {
            executor.spin_once(core::time::Duration::from_millis(10));
        }

        if let Some(max) = max.filter(|&max| tick >= max) {
            info!("tick budget {max} reached");
            break;
        }
    }

    let _ = executor.close();
}
