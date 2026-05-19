//! Phase 110.G.bridge — time-triggered cyclic bridge demo.
//!
//! Two RMW backends in one binary:
//!
//! * **Zenoh** — ingress side. A raw subscription on `/chatter`
//!   receives bytes from any ROS 2 / nano-ros publisher reachable
//!   via zenohd.
//! * **XRCE-DDS** — egress side. A raw publisher on `/chatter`
//!   forwards the captured bytes to a Micro-XRCE-DDS Agent.
//!
//! The bridge runs under an ARINC-653-style cyclic executive
//! (Phase 110.G):
//!
//! ```text
//! 10 ms major frame
//! ┌────────────────────────────────────────────────┐
//! │ ingress window     │ idle │ egress window │ idle│
//! │ 0..3 ms            │      │ 5..8 ms       │     │
//! └────────────────────────────────────────────────┘
//! ```
//!
//! * Ingress window (0..3 ms): the zenoh subscription callback is
//!   the only handle eligible for dispatch. It copies the latest
//!   payload into a shared buffer.
//! * Egress window (5..8 ms): a 1 kHz periodic timer drains the
//!   shared buffer and republishes the bytes on XRCE.
//! * Idle gaps (3..5 ms, 8..10 ms): every TT-bound handle is
//!   suppressed; only handles without a TT-gated SC (none here)
//!   would dispatch.
//!
//! Determinism property: under sustained ingress traffic, the
//! egress timer never publishes during the ingress window (and
//! vice versa). The `spin_once` TT gate at
//! `executor/spin.rs:4007+` reads each handle's bound SC and
//! suppresses dispatch outside the SC's
//! `[offset, offset + duration) mod major_frame` slot.
//!
//! Usage (in three terminals):
//!
//! ```bash
//! # 1. start zenohd (default tcp/127.0.0.1:7447)
//! zenohd
//!
//! # 2. start the Micro-XRCE-DDS Agent
//! MicroXRCEAgent udp4 -p 8888
//!
//! # 3. run the bridge
//! cargo run -p native-rs-bridge-tt-zenoh-to-xrce -- \
//!     --zenoh tcp/127.0.0.1:7447 --xrce 127.0.0.1:8888
//!
//! # 4. publish on Zenoh /chatter from any ROS 2 / nano-ros
//! #    talker; observe the bridged messages reach the XRCE
//! #    agent (e.g. via the agent's verbose log).
//! ```

use std::{
    cell::RefCell,
    rc::Rc,
    time::{Duration, Instant},
};

use log::{info, warn};
use nros::{Executor, ExecutorConfig, TimeTriggeredSchedule, TimeTriggeredWindow, TimerDuration};

const TYPE_NAME: &str = "std_msgs/msg/String";
const TYPE_HASH: &str = "RIHS01_df668c740482bbd48fb39d76a70dfd4bd59db1288021743503259e948f6b1a18";

/// 10 ms major frame; ingress 0..3 ms; egress 5..8 ms.
const MAJOR_FRAME_US: u32 = 10_000;
const INGRESS_OFFSET_US: u32 = 0;
const INGRESS_DURATION_US: u32 = 3_000;
const EGRESS_OFFSET_US: u32 = 5_000;
const EGRESS_DURATION_US: u32 = 3_000;

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    info!("=== Phase 110.G.bridge: zenoh → XRCE under TT schedule ===");

    // Backend registration. See `examples/bridges/native-rust-zenoh-to-dds`
    // for why both `register()` calls are required even though
    // each backend has a `#[used]` linkme distributed-slice entry
    // (the rlib's CGU isn't linked until something references its
    // public symbols).
    nros_rmw_zenoh::register().expect("register zenoh backend");
    nros_rmw_xrce_cffi::register().expect("register xrce backend");

    let zenoh_locator =
        std::env::var("ZENOH_LOCATOR").unwrap_or_else(|_| "tcp/127.0.0.1:7447".into());
    let xrce_locator = std::env::var("XRCE_LOCATOR").unwrap_or_else(|_| "127.0.0.1:8888".into());

    let cfg = ExecutorConfig::new(&zenoh_locator)
        .node_name("tt_bridge_primary")
        .namespace("/");
    let mut exec = Executor::open_with_rmw("zenoh", &cfg).expect("open zenoh primary session");
    info!("Primary session open (zenoh @ {zenoh_locator})");

    let node_in = exec
        .node_builder("ingress")
        .rmw("zenoh")
        .build()
        .expect("ingress Node");
    let node_out = exec
        .node_builder("egress")
        .rmw("xrce")
        .locator(&xrce_locator)
        .build()
        .expect("egress Node (XRCE session open)");
    info!("Nodes built: ingress (zenoh), egress (xrce @ {xrce_locator})");

    // Apply the TT schedule. `apply_time_triggered_schedule` sets
    // `major_frame_us` on the executor and creates one auto-Fifo
    // SC per window with the per-handle TT-gate fields populated.
    let schedule = TimeTriggeredSchedule::<2>::new_full(
        MAJOR_FRAME_US,
        [
            TimeTriggeredWindow::new(INGRESS_OFFSET_US, INGRESS_DURATION_US, "ingress"),
            TimeTriggeredWindow::new(EGRESS_OFFSET_US, EGRESS_DURATION_US, "egress"),
        ],
    );
    let [ingress_sc, egress_sc] = exec
        .apply_time_triggered_schedule(&schedule)
        .expect("TT schedule should validate");
    info!(
        "TT schedule applied: major_frame={}us, ingress=[{}, {})us, egress=[{}, {})us",
        MAJOR_FRAME_US,
        INGRESS_OFFSET_US,
        INGRESS_OFFSET_US + INGRESS_DURATION_US,
        EGRESS_OFFSET_US,
        EGRESS_OFFSET_US + EGRESS_DURATION_US,
    );

    // Shared buffer between the ingress and egress windows. A
    // single-slot `Option<Vec<u8>>` keeps the example simple; a
    // ring buffer would let the example tolerate ingress bursts
    // longer than the egress drain rate.
    let staging: Rc<RefCell<Option<Vec<u8>>>> = Rc::new(RefCell::new(None));

    // Egress raw publisher on the XRCE session.
    let pub_out = exec
        .with_node_try(node_out, |n| {
            n.create_publisher_raw("/chatter", TYPE_NAME, TYPE_HASH)
        })
        .expect("egress raw publisher");
    let pub_out = Rc::new(RefCell::new(pub_out));

    // Ingress subscription on zenoh: copy into the staging buffer.
    let staging_in = Rc::clone(&staging);
    let ingress_sub = exec
        .register_subscription_buffered_raw_on::<_, 1024>(
            node_in,
            "/chatter",
            TYPE_NAME,
            TYPE_HASH,
            Default::default(),
            move |bytes: &[u8]| {
                staging_in.borrow_mut().replace(bytes.to_vec());
                info!("[ingress] captured {} bytes", bytes.len());
            },
        )
        .expect("register ingress sub on zenoh");
    exec.bind_handle_to_sched_context(ingress_sub, ingress_sc)
        .expect("bind ingress sub to ingress SC");

    // Egress drain timer: 1 kHz tick. Only fires during the
    // egress window thanks to the TT gate on `egress_sc`.
    let staging_out = Rc::clone(&staging);
    let pub_for_drain = Rc::clone(&pub_out);
    let egress_timer = exec
        .register_timer(TimerDuration::from_millis(1), move || {
            if let Some(bytes) = staging_out.borrow_mut().take() {
                let p = pub_for_drain.borrow();
                match p.publish_raw(&bytes) {
                    Ok(()) => info!("[egress] forwarded {} bytes", bytes.len()),
                    Err(e) => warn!("[egress] publish failed: {:?}", e),
                }
            }
        })
        .expect("register egress drain timer");
    exec.bind_handle_to_sched_context(egress_timer, egress_sc)
        .expect("bind egress timer to egress SC");

    info!("Spinning. Publish on Zenoh /chatter; observe forwards on XRCE /chatter.");
    let started = Instant::now();
    // Bound the demo at 60 s so the example terminates cleanly
    // under `cargo test` style runs; remove the cap for ad-hoc
    // exploration.
    while started.elapsed() < Duration::from_secs(60) {
        exec.spin_once(Duration::from_millis(1));
    }
    info!("Bridge stopped after 60 s.");
}
