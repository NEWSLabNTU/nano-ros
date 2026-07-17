//! Rate-contract publisher for the contract-monitor parity fixture.
//!
//! Bakes a `min_rate_hz` publisher contract on `/cm_header` and publishes a
//! `std_msgs/Header` there whose stamp is aged by `CM_STALE_MS`. In the
//! violating configuration (`CM_PERIOD_MS=500` → 2 Hz < the 10 Hz declared
//! minimum) the executor's rate monitor fires `rate-hierarchy-runtime`,
//! which this bin drains through the `nros-diagnostics` reporter and
//! republishes on `/diagnostics`. Compliant config (`CM_PERIOD_MS=50` →
//! 20 Hz, `CM_STALE_MS=0`) stays silent.

use std::time::{Duration, Instant};

use log::info;
use nros::{
    monitor::{MonitorSpec, PubMonitorCell},
    prelude::*,
};
use nros_builtin_interfaces_diag::msg::Time;
use nros_diagnostic_msgs::msg::DiagnosticArray;
use nros_diagnostics::DiagnosticReporter;
use nros_std_msgs_diag::msg::Header;

use contract_monitor::{DIAG_TOPIC, HEADER_TOPIC, MIN_RATE_HZ_MILLI, drain_and_report, now_us};

static PUB_CELL: PubMonitorCell = PubMonitorCell::new();
static MONITORS: &[MonitorSpec] = &[MonitorSpec {
    topic: HEADER_TOPIC,
    fqn: "/cm/pub/cm_header",
    min_rate_hz_milli: MIN_RATE_HZ_MILLI,
    max_latency_ms: 0,
    cell: &PUB_CELL,
}];

fn env_u64(key: &str, default: u64) -> u64 {
    std::env::var(key)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

fn epoch_us() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_micros() as u64)
        .unwrap_or(0)
}

fn main() {
    env_logger::init();
    nros_rmw_zenoh::register().expect("register zenoh backend");
    info!("contract-monitor pub (rate contract on {HEADER_TOPIC})");

    let ctx = nros::init_with_launch_auto().expect("nros init failed");
    let cfg = ctx.config("cm_pub");
    let mut executor: Executor = Executor::open(&cfg).expect("open session");
    // Install the baked rate-monitor table BEFORE entity creation so the
    // publisher attaches PUB_CELL by exact topic match.
    executor.set_monitor_table(MONITORS);

    let header_pub = {
        let mut node = executor.create_node("cm_pub").expect("create node");
        node.create_publisher::<Header>(HEADER_TOPIC)
            .expect("create header publisher")
    };
    let diag_pub = {
        let mut node = executor
            .create_node("cm_pub_diag")
            .expect("create diag node");
        node.create_publisher::<DiagnosticArray>(DIAG_TOPIC)
            .expect("create diagnostics publisher")
    };

    let period_ms = env_u64("CM_PERIOD_MS", 500);
    let stale_ms = env_u64("CM_STALE_MS", 0);
    let run_ms = env_u64("CM_RUN_MS", 16_000);
    let mut reporter = DiagnosticReporter::new(0);

    info!("cm_pub: period={period_ms}ms stale={stale_ms}ms run={run_ms}ms");
    let started = Instant::now();
    let mut last_pub = Instant::now() - Duration::from_millis(period_ms);
    let mut seq: u32 = 0;
    while started.elapsed() < Duration::from_millis(run_ms) {
        let _ = executor.spin_once(Duration::from_millis(20));
        if last_pub.elapsed() >= Duration::from_millis(period_ms) {
            last_pub = Instant::now();
            let stamp_us = epoch_us().saturating_sub(stale_ms * 1000);
            let mut frame_id = heapless::String::<256>::new();
            let _ = frame_id.push_str("cm");
            let hdr = Header {
                stamp: Time {
                    sec: (stamp_us / 1_000_000) as i32,
                    nanosec: ((stamp_us % 1_000_000) * 1_000) as u32,
                },
                frame_id,
            };
            let _ = header_pub.publish(&hdr);
            seq = seq.wrapping_add(1);
            if seq % 4 == 0 {
                info!("cm_pub: published {seq} headers");
            }
        }
        let _ = now_us(); // prime the monotonic base early
        drain_and_report(&mut executor, &mut reporter, |arr| {
            let _ = diag_pub.publish(arr);
        });
    }
    info!("cm_pub: done ({seq} headers)");
}
