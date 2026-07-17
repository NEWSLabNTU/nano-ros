//! `/diagnostics` observer for the contract-monitor parity fixture.
//!
//! Subscribes the `DiagnosticArray` topic both monitor sides publish to and
//! prints one `DIAG rule=<rule-id> hw=<endpoint>` line per status. The test
//! greps these lines: a violating pair must surface `rate-hierarchy-runtime`
//! and `max-age-runtime`; a compliant twin must stay silent.

use std::time::{Duration, Instant};

use log::info;
use nros::prelude::*;
use nros_diagnostic_msgs::msg::DiagnosticArray;

use contract_monitor::DIAG_TOPIC;

/// Stable marker the e2e test greps for.
pub const DIAG_MARKER: &str = "DIAG rule=";

fn env_u64(key: &str, default: u64) -> u64 {
    std::env::var(key)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

fn main() {
    env_logger::init();
    nros_rmw_zenoh::register().expect("register zenoh backend");
    // Banner contains "Listener" so the e2e spawn helper's readiness wait
    // keys off it, like the other sink fixtures.
    info!("contract-monitor diagsink Listener (observing {DIAG_TOPIC})");

    let ctx = nros::init_with_launch_auto().expect("nros init failed");
    let cfg = ctx.config("cm_diagsink");
    let mut executor: Executor = Executor::open(&cfg).expect("open session");

    let nid = executor
        .node_builder("cm_diagsink")
        .build()
        .expect("build node");
    executor
        .node_mut(nid)
        .subscription(DIAG_TOPIC)
        .typed::<DiagnosticArray>()
        .build(move |arr: &DiagnosticArray| {
            for st in arr.status.iter() {
                // `DIAG rule=<name> hw=<hardware_id>` — the test's grep key.
                info!(
                    "DIAG rule={} hw={} level={}",
                    st.name.as_str(),
                    st.hardware_id.as_str(),
                    st.level
                );
            }
        })
        .expect("create diagnostics subscription");

    let run_ms = env_u64("CM_RUN_MS", 18_000);
    info!("cm_diagsink: subscribed; run={run_ms}ms");
    let started = Instant::now();
    while started.elapsed() < Duration::from_millis(run_ms) {
        let _ = executor.spin_once(Duration::from_millis(50));
    }
    info!("cm_diagsink: done");
}
