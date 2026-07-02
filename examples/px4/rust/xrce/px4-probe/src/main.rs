//! PX4 SITL XRCE probe — Phase 233.4 (RFC-0039 Track B, real-PX4 e2e).
//!
//! Subscribes a real PX4 `/fmu/out/*` topic over a `MicroXRCEAgent` to validate
//! the companion path against **actual PX4 firmware** (SITL), not the
//! `px4-stub`. Defaults to `/fmu/out/timesync_status` — published continuously
//! by PX4's `uxrce_dds_client` regardless of simulator/EKF state, so a headless
//! `make px4_sitl none` boot is enough.
//!
//! ```bash
//! MicroXRCEAgent udp4 -p 8888 &
//! make px4_sitl none          # in PX4-Autopilot; uxrce_dds_client autoconnects
//! NROS_LOCATOR=127.0.0.1:8888 cargo run -p px4-probe
//! ```
//!
//! Environment:
//!   NROS_LOCATOR   — agent `host:port` (default `127.0.0.1:8888`)
//!   ROS_DOMAIN_ID  — DDS domain shared with PX4 (default `0`)
//!   PX4_PROBE_MAX  — exit after N received samples (default: run forever)

use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

use log::info;
use nros::prelude::*;
use px4_msgs::msg::TimesyncStatus;

extern crate nros_platform_cffi as _;

// Phase 248 C6 — force-link the xrce backend (board-less app owns it; no
// `nros/rmw-xrce`). This is the only RMW this example supports, so the
// backend dep (and this force-link) are unconditional (phase-277 W3.b).
#[doc(hidden)]
#[used]
static __FORCE_LINK_XRCE: fn() -> Result<(), nros_rmw_xrce_cffi::RegisterError> =
    nros_rmw_xrce_cffi::register;

fn main() {
    env_logger::init();

    let locator = std::env::var("NROS_LOCATOR").unwrap_or_else(|_| "127.0.0.1:8888".to_string());
    let domain_id: u32 = std::env::var("ROS_DOMAIN_ID")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    let max: u64 = std::env::var("PX4_PROBE_MAX")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);

    info!("PX4 probe: agent={locator} domain={domain_id} topic=/fmu/out/timesync_status");

    let config = ExecutorConfig::new(&locator)
        .domain_id(domain_id)
        .node_name("px4_probe");
    let mut executor: Executor = Executor::open(&config).expect("Failed to open XRCE session");

    let nid = executor
        .node_builder("px4_probe")
        .build()
        .expect("Failed to build node");

    let rx = Arc::new(AtomicU64::new(0));
    let rx_cb = rx.clone();
    executor
        .node_mut(nid)
        .subscription("/fmu/out/timesync_status")
        .typed::<TimesyncStatus>()
        .qos(QosSettings::px4())
        .build(move |m: &TimesyncStatus| {
            let n = rx_cb.fetch_add(1, Ordering::SeqCst) + 1;
            info!(
                "probe rx[{n}]: timestamp={} ts_offset={}",
                m.timestamp, m.estimated_offset
            );
        })
        .expect("Failed to subscribe /fmu/out/timesync_status");

    info!("probe wired; waiting for PX4 timesync_status");

    loop {
        executor.spin_once(core::time::Duration::from_millis(20));
        if max != 0 && rx.load(Ordering::SeqCst) >= max {
            info!("probe budget {max} reached");
            break;
        }
    }

    let _ = executor.close();
}
