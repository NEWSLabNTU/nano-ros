//! PX4 XRCE-DDS companion — Phase 233.3 (RFC-0039 Track B).
//!
//! A nano-ros node that joins the *same* Micro XRCE-DDS Agent PX4's
//! `uxrce_dds_client` connects to, and speaks `px4_msgs` over it — the
//! mainstream PX4 ↔ ROS 2 integration, peer-side. It:
//!
//!   * subscribes `/fmu/out/vehicle_odometry`  (PX4 → companion), and
//!   * publishes  `/fmu/in/offboard_control_mode`  (companion → PX4).
//!
//! Both endpoints use the PX4 QoS profile ([`QosSettings::px4`] —
//! `BEST_EFFORT` + `TRANSIENT_LOCAL` + `KEEP_LAST(1)`); the default
//! reliable+volatile profile will *not* match PX4's endpoints.
//!
//! # Bring-up
//!
//! ```bash
//! # 1. Start the agent PX4 talks to (UDP on the default PX4 port):
//! MicroXRCEAgent udp4 -p 8888
//!
//! # 2. Start PX4 SITL (or real firmware) so /fmu/out/* flows.
//! #    (see docs/reference/px4-xrce-companion.md)
//!
//! # 3. Run this companion against the same agent:
//! NROS_LOCATOR=127.0.0.1:8888 cargo run -p px4-offboard-companion
//! ```
//!
//! Environment:
//!   NROS_LOCATOR        — agent `host:port` (default `127.0.0.1:8888`, PX4's default)
//!   ROS_DOMAIN_ID       — DDS domain shared with PX4 (default `0`)
//!   PX4_COMPANION_TICKS — run N setpoint ticks then exit (default: stream forever)

use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

use log::info;
use nros::prelude::*;
use px4_msgs::msg::{OffboardControlMode, VehicleOdometry};

// The companion is the peer of PX4's uxrce_dds_client; no platform scope.
extern crate nros_platform_cffi as _;

// Phase 248 C6 — force-link the xrce backend rlib so its `RMW_INIT_ENTRIES`
// self-register section survives pruning (the board-less app owns its backend,
// no `nros/rmw-xrce` umbrella feature). This is the only RMW this example
// supports, so the backend dep (and this force-link) are unconditional
// (phase-277 W3.b).
#[doc(hidden)]
#[used]
static __FORCE_LINK_XRCE: fn() -> Result<(), nros_rmw_xrce_cffi::RegisterError> =
    nros_rmw_xrce_cffi::register;

fn main() {
    env_logger::init();

    // PX4's default agent port is 8888 (`MicroXRCEAgent udp4 -p 8888`).
    let locator = std::env::var("NROS_LOCATOR").unwrap_or_else(|_| "127.0.0.1:8888".to_string());
    let domain_id: u32 = std::env::var("ROS_DOMAIN_ID")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);

    info!("PX4 XRCE companion: agent={locator} domain={domain_id}");

    let config = ExecutorConfig::new(&locator)
        .domain_id(domain_id)
        .node_name("px4_offboard_companion");
    let mut executor: Executor = Executor::open(&config).expect("Failed to open XRCE session");

    let nid = executor
        .node_builder("px4_offboard_companion")
        .build()
        .expect("Failed to build node");

    // PX4 → companion: telemetry on /fmu/out/*. Count what arrives so a host
    // test (Phase 233.4) can assert the round-trip flowed.
    let rx = Arc::new(AtomicU64::new(0));
    let rx_cb = rx.clone();
    executor
        .node_mut(nid)
        .subscription("/fmu/out/vehicle_odometry")
        .typed::<VehicleOdometry>()
        .qos(QosSettings::px4())
        .build(move |msg: &VehicleOdometry| {
            let n = rx_cb.fetch_add(1, Ordering::SeqCst) + 1;
            info!(
                "[{n}] odometry: t={} pos=[{:.2} {:.2} {:.2}]",
                msg.timestamp, msg.position[0], msg.position[1], msg.position[2]
            );
        })
        .expect("Failed to subscribe /fmu/out/vehicle_odometry");

    // companion → PX4: offboard setpoint stream on /fmu/in/*. PX4 requires a
    // steady OffboardControlMode stream (>2 Hz) to stay in offboard mode.
    let setpoint = executor
        .node_mut(nid)
        .publisher("/fmu/in/offboard_control_mode")
        .qos(QosSettings::px4())
        .typed::<OffboardControlMode>()
        .build()
        .expect("Failed to advertise /fmu/in/offboard_control_mode");

    info!("companion wired; streaming offboard_control_mode + listening for odometry");

    // Warm-up: drive the XRCE session so the subscription's create_datareader
    // + request_data handshake flushes and the agent matches PX4's writer
    // before we start the setpoint stream.
    for _ in 0..50u32 {
        executor.spin_once(core::time::Duration::from_millis(10));
    }

    // Position-control offboard mode: position=true, the rest false.
    let mut tick: u64 = 0;
    loop {
        // Spin first so inbound odometry drains and the session stays live
        // before each publish.
        for _ in 0..10u32 {
            executor.spin_once(core::time::Duration::from_millis(10));
        }

        let mode = OffboardControlMode {
            timestamp: tick.wrapping_mul(100_000), // synthetic µs clock
            position: true,
            velocity: false,
            acceleration: false,
            attitude: false,
            body_rate: false,
            thrust_and_torque: false,
            direct_actuator: false,
        };
        if let Err(e) = setpoint.publish(&mode) {
            log::warn!("publish offboard_control_mode failed: {e:?}");
        }
        tick = tick.wrapping_add(1);

        // Bounded run when a host test sets a tick budget; else stream forever.
        if let Some(max) = std::env::var("PX4_COMPANION_TICKS")
            .ok()
            .and_then(|s| s.parse::<u64>().ok())
            .filter(|&max| tick >= max)
        {
            info!(
                "tick budget {max} reached, rx={}",
                rx.load(Ordering::SeqCst)
            );
            break;
        }
    }

    let _ = executor.close();
}
