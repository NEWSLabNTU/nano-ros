//! Supervised-route handler for the contract-monitor fixture (phase-503).
//!
//! Subscribes the route's trigger (`/cm_header`, fed by `contract-monitor-pub`)
//! and owns the route's output publisher (`/cm_route_out`), which it NEVER
//! publishes on: the shape of a handler whose output stopped. The monitor row
//! supervises that route with a `CM_ROUTE_DEADLINE_MS` deadline (0 = the
//! unsupervised twin) and reaction id 7. The executor's reaction hook prints
//! one `CM_REACTION ...` line per call, and every drained verdict prints one
//! `CM_VIOLATION <rule>` line.
//!
//! `CM_ROUTE_LATE_MS` > 0 switches to the other shape: the trigger's callback
//! runs that long (a sleep) and then publishes the output, so the route is
//! answered late by ONE long callback, under `CM_MECHANISM` (0 executor,
//! 1 timer; default 1).

use std::time::{Duration, Instant};

use log::info;
use nros::{
    monitor::{MECHANISM_TIMER, MonitorSpec, PubMonitorCell, ReactionEvent},
    prelude::*,
};
use nros_std_msgs::msg::Header;

use contract_monitor::{HEADER_TOPIC, ROUTE_OUT_TOPIC, ROUTE_REACTION_ID};

static ROUTE_CELL: PubMonitorCell = PubMonitorCell::new();

fn env_u64(key: &str, default: u64) -> u64 {
    std::env::var(key)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

/// The reaction hook: what an image's fallback would start, printed.
unsafe extern "C" fn on_reaction(ev: *const ReactionEvent, _ctx: *mut core::ffi::c_void) {
    // SAFETY: the executor passes a valid event for the call's duration.
    let ev = unsafe { *ev };
    println!(
        "CM_REACTION id={} row={} rule={} measured_ms={} declared_ms={} mechanism={}",
        ev.reaction_id, ev.row, ev.rule_code, ev.measured_ms, ev.declared_ms, ev.mechanism
    );
}

fn main() {
    env_logger::init();
    nros_rmw_zenoh::register().expect("register zenoh backend");
    let deadline_ms = env_u64("CM_ROUTE_DEADLINE_MS", 150) as u32;
    let run_ms = env_u64("CM_RUN_MS", 8_000);
    let late_ms = env_u64("CM_ROUTE_LATE_MS", 0);
    let mechanism = env_u64("CM_MECHANISM", MECHANISM_TIMER as u64) as u8;
    // The table is built at run time (the deadline comes from the
    // environment) and leaked: the executor keeps it for its whole life.
    let table: &'static [MonitorSpec] = Box::leak(Box::new([MonitorSpec {
        topic: ROUTE_OUT_TOPIC,
        fqn: "/cm/route/out",
        min_rate_hz_milli: 0,
        max_latency_ms: 0,
        route_trigger: HEADER_TOPIC,
        route_max_latency_ms: 150,
        route_deadline_ms: deadline_ms,
        reaction_id: ROUTE_REACTION_ID,
        mechanism,
        cell: &ROUTE_CELL,
    }]));
    info!(
        "contract-monitor route (deadline {deadline_ms} ms on {HEADER_TOPIC} -> {ROUTE_OUT_TOPIC})"
    );

    let ctx = nros::init().expect("nros init failed");
    let cfg = ctx.config("cm_route");
    let mut executor: Executor = Executor::open(&cfg).expect("open session");
    executor.set_monitor_table(table);
    executor.set_reaction_hook(Some((on_reaction, core::ptr::null_mut())));

    let nid = executor
        .node_builder("cm_route")
        .build()
        .expect("build node");
    // The output exists and, unless `CM_ROUTE_LATE_MS` is set, is never
    // published: every take misses.
    let out = executor
        .node_mut(nid)
        .create_publisher::<Header>(ROUTE_OUT_TOPIC)
        .expect("create route output");
    executor
        .node_mut(nid)
        .subscription(HEADER_TOPIC)
        .typed::<Header>()
        .build(move |hdr: &Header| {
            if late_ms > 0 {
                std::thread::sleep(Duration::from_millis(late_ms));
                let _ = out.publish(hdr);
            }
        })
        .expect("create trigger subscription");
    info!("cm_route: subscribed; run={run_ms}ms");

    let started = Instant::now();
    while started.elapsed() < Duration::from_millis(run_ms) {
        let _ = executor.spin_once(Duration::from_millis(50));
        executor.drain_violations(|v| {
            println!(
                "CM_VIOLATION {} {} measured={} declared={}",
                v.rule, v.fqn, v.measured, v.declared
            );
        });
    }
    info!("cm_route: done");
}
