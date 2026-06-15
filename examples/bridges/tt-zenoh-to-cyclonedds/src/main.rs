//! Issue #53 — time-triggered cyclic bridge demo, **Cyclone DDS egress**.
//!
//! Two RMW backends in one binary (the stock-cyclonedds sibling of
//! `tt-zenoh-to-xrce`):
//!
//! * **Zenoh** — ingress side. A raw subscription on `/chatter` receives bytes
//!   from any ROS 2 / nano-ros publisher reachable via zenohd.
//! * **Cyclone DDS** — egress side. A raw publisher on `/chatter` forwards the
//!   captured bytes onto the DDS databus, where a stock `rmw_cyclonedds_cpp`
//!   (e.g. an Autoware listener) or another nano-ros cyclonedds node receives
//!   them.
//!
//! The bridge runs under the same ARINC-653-style cyclic executive as the XRCE
//! variant (10 ms major frame; ingress 0..3 ms; egress 5..8 ms).
//!
//! ## The one structural difference from `tt-zenoh-to-xrce`
//!
//! Cyclone resolves a topic by a registered `dds_topic_descriptor_t` — a raw
//! publisher with only `(type_name, type_hash)` is rejected (`UNSUPPORTED`)
//! unless the type's descriptor already exists. XRCE registers lazily from the
//! name+hash; Cyclone is stricter. So the egress type's schema is staged via
//! [`nros_rmw::register_type_descriptor`] BEFORE creating the raw publisher.
//! The Cyclone backend installs the registrar from its own crate during
//! `nros_rmw_cyclonedds_sys::register()`; the schema below mirrors the
//! generated `std_msgs/msg/String` (`{ data: string }`).
//!
//! Usage (three terminals):
//!
//! ```bash
//! # 1. start zenohd (default tcp/127.0.0.1:7447)
//! zenohd
//!
//! # 2. run the bridge
//! cargo run -p native-rs-bridge-tt-zenoh-to-cyclonedds -- \
//!     # ZENOH_LOCATOR (ingress) + ROS_DOMAIN_ID (cyclone egress domain) via env
//!
//! # 3. subscribe on Cyclone DDS /chatter (stock ROS 2 or a nano-ros cyclone
//! #    listener on the same ROS_DOMAIN_ID) + publish on Zenoh /chatter from a
//! #    nano-ros / ROS 2 talker; observe the bridged samples on the DDS side.
//! ```

use std::{
    cell::RefCell,
    rc::Rc,
    time::{Duration, Instant},
};

use log::{info, warn};
use nros::{Executor, ExecutorConfig, TimeTriggeredSchedule, TimeTriggeredWindow, TimerDuration};
use nros_serdes::schema::{Field, FieldType};

const TYPE_NAME: &str = "std_msgs/msg/String";
const TYPE_HASH: &str = "RIHS01_df668c740482bbd48fb39d76a70dfd4bd59db1288021743503259e948f6b1a18";

/// Cyclone-side type schema for `std_msgs/msg/String` — byte-identical to the
/// generated `<std_msgs::msg::String as nros_serdes::schema::Message>::FIELDS`
/// (`{ data: string }`, offset 0). Staged into the descriptor seam so Cyclone's
/// `find_descriptor` resolves `/chatter`'s topic type before the raw publisher
/// is created.
static STRING_FIELDS: &[Field] = &[Field {
    name: "data\0",
    ty: FieldType::String,
    offset: 0,
}];

/// NUL-terminated registry key — `type_registry` hands `type_name.as_ptr()`
/// straight to the Cyclone C descriptor table, so the registered name MUST be
/// `\0`-terminated (the publish-side name is cffi-marshalled separately).
const REG_TYPE_NAME: &str = "std_msgs/msg/String\0";

/// 10 ms major frame; ingress 0..3 ms; egress 5..8 ms.
const MAJOR_FRAME_US: u32 = 10_000;
const INGRESS_OFFSET_US: u32 = 0;
const INGRESS_DURATION_US: u32 = 3_000;
const EGRESS_OFFSET_US: u32 = 5_000;
const EGRESS_DURATION_US: u32 = 3_000;

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    info!("=== Issue #53: zenoh → Cyclone DDS under TT schedule ===");

    // Backend registration. Both `register()` calls are required even though
    // each backend has a `#[used]` linkme distributed-slice entry (the rlib's
    // CGU isn't linked until something references its public symbols). The
    // Cyclone `register()` additionally installs the type-descriptor registrar
    // into the `nros_rmw` seam (consumed below).
    nros_rmw_zenoh::register().expect("register zenoh backend");
    nros_rmw_cyclonedds_sys::register().expect("register cyclonedds backend");

    let zenoh_locator =
        std::env::var("ZENOH_LOCATOR").unwrap_or_else(|_| "tcp/127.0.0.1:7447".into());
    let domain_id: u32 = std::env::var("ROS_DOMAIN_ID")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);

    let cfg = ExecutorConfig::new(&zenoh_locator)
        .domain_id(domain_id)
        .node_name("tt_bridge_primary")
        .namespace("/");
    let mut exec = Executor::open_with_rmw("zenoh", &cfg).expect("open zenoh primary session");
    info!("Primary session open (zenoh @ {zenoh_locator})");

    let node_in = exec
        .node_builder("ingress")
        .rmw("zenoh")
        .build()
        .expect("ingress Node");
    // Egress on Cyclone DDS — a second session on `domain_id` (DDS discovery,
    // no locator string). `cfg.domain_id` only sets the primary (zenoh) session;
    // the egress is an EXTRA session, so thread the domain here or the cyclone
    // participant opens on domain 0 and a listener on any other ROS_DOMAIN_ID
    // never matches.
    let node_out = exec
        .node_builder("egress")
        .rmw("cyclonedds")
        .domain_id(domain_id)
        .build()
        .expect("egress Node (cyclonedds session open)");
    info!("Nodes built: ingress (zenoh), egress (cyclonedds @ domain {domain_id})");

    // Stage the egress type descriptor BEFORE creating the raw publisher —
    // Cyclone's `find_descriptor` requires it (the crux that distinguishes this
    // from the XRCE variant). No-op on backends without a registrar.
    nros_rmw::register_type_descriptor(REG_TYPE_NAME, STRING_FIELDS)
        .expect("register std_msgs/String descriptor with cyclonedds");
    info!("Cyclone descriptor staged for {TYPE_NAME}");

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

    // Single-slot staging buffer shared between the ingress + egress windows.
    let staging: Rc<RefCell<Option<Vec<u8>>>> = Rc::new(RefCell::new(None));

    // Egress raw publisher on the Cyclone session.
    let pub_out = exec
        .with_node_try(node_out, |n| {
            n.create_publisher_raw("/chatter", TYPE_NAME, TYPE_HASH)
        })
        .expect("egress raw publisher");
    let pub_out = Rc::new(RefCell::new(pub_out));

    // Ingress subscription on zenoh: copy into the staging buffer.
    let staging_in = Rc::clone(&staging);
    let ingress_sub = exec
        .node_mut(node_in)
        .subscription("/chatter")
        .generic(TYPE_NAME, TYPE_HASH)
        .qos(Default::default())
        .rx_buffer::<1024>()
        .build(move |bytes: &[u8]| {
            staging_in.borrow_mut().replace(bytes.to_vec());
            info!("[ingress] captured {} bytes", bytes.len());
        })
        .expect("register ingress sub on zenoh");
    exec.bind_handle_to_sched_context(ingress_sub, ingress_sc)
        .expect("bind ingress sub to ingress SC");

    // Egress drain timer: 1 kHz tick, gated to the egress window by `egress_sc`.
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

    info!("Spinning. Publish on Zenoh /chatter; observe forwards on Cyclone DDS /chatter.");
    let started = Instant::now();
    while started.elapsed() < Duration::from_secs(60) {
        exec.spin_once(Duration::from_millis(1));
    }
    info!("Bridge stopped after 60 s.");
}
