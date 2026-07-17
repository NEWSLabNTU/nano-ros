//! Age-contract subscriber for the contract-monitor parity fixture.
//!
//! Bakes a `max_age_ms` subscriber contract on `/cm_header` and subscribes
//! `std_msgs/Header`. When the received message's `header.stamp` is older
//! than the declared bound (the pub's `CM_STALE_MS` config), the executor's
//! age monitor fires `max-age-runtime`, which this bin drains through the
//! `nros-diagnostics` reporter and republishes on `/diagnostics`.

use std::time::{Duration, Instant};

use log::info;
use nros::{
    monitor::{AgeMonitorSpec, SubMonitorCell},
    prelude::*,
};
use nros_diagnostic_msgs::msg::DiagnosticArray;
use nros_diagnostics::DiagnosticReporter;
use nros_std_msgs_diag::msg::Header;

use contract_monitor::{DIAG_TOPIC, HEADER_TOPIC, MAX_AGE_MS, drain_and_report};

static AGE_CELL: SubMonitorCell = SubMonitorCell::new();
static AGE_MONITORS: &[AgeMonitorSpec] = &[AgeMonitorSpec {
    topic: HEADER_TOPIC,
    fqn: "/cm/sub/cm_header",
    max_age_ms: MAX_AGE_MS,
    cell: &AGE_CELL,
}];

fn env_u64(key: &str, default: u64) -> u64 {
    std::env::var(key)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

fn main() {
    env_logger::init();
    nros_rmw_zenoh::register().expect("register zenoh backend");
    info!("contract-monitor sub (max_age contract on {HEADER_TOPIC})");

    let ctx = nros::init_with_launch_auto().expect("nros init failed");
    let cfg = ctx.config("cm_sub");
    let mut executor: Executor = Executor::open(&cfg).expect("open session");
    // Install the baked age-monitor table BEFORE entity creation. The hosted
    // executor already carries a SystemTime epoch (W3b.5 default), so the age
    // hook attaches on the stamped Header subscription.
    executor.set_age_table(AGE_MONITORS);

    let diag_pub = {
        let mut node = executor
            .create_node("cm_sub_diag")
            .expect("create diag node");
        node.create_publisher::<DiagnosticArray>(DIAG_TOPIC)
            .expect("create diagnostics publisher")
    };

    let nid = executor.node_builder("cm_sub").build().expect("build node");
    executor
        .node_mut(nid)
        .subscription(HEADER_TOPIC)
        .typed::<Header>()
        .build(move |hdr: &Header| {
            info!("cm_sub: received header stamp.sec={}", hdr.stamp.sec);
        })
        .expect("create header subscription");

    let run_ms = env_u64("CM_RUN_MS", 16_000);
    let mut reporter = DiagnosticReporter::new(0);
    info!("cm_sub: subscribed; run={run_ms}ms");

    let started = Instant::now();
    while started.elapsed() < Duration::from_millis(run_ms) {
        let _ = executor.spin_once(Duration::from_millis(50));
        drain_and_report(&mut executor, &mut reporter, |arr| {
            let _ = diag_pub.publish(arr);
        });
    }
    info!("cm_sub: done");
}
