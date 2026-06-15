//! Issue #53 — minimal mixed-RMW bridge fixture, **Cyclone DDS egress**.
//!
//! Forwards bytes from a zenoh raw subscription on `/chatter` to a raw
//! publisher on the same topic, on a Cyclone DDS egress session (DDS
//! discovery on `ROS_DOMAIN_ID`). The stock-cyclonedds sibling of
//! `bridge-zenoh-to-xrce-fwd`; same Int32 type as the `native-rs-talker` /
//! `native-rs-listener` fixtures, no TT scheduling.
//!
//! **The one cyclonedds-specific step:** Cyclone rejects a raw publisher whose
//! topic type has no registered `dds_topic_descriptor_t`, so the Int32 schema
//! is staged via [`nros_rmw::register_type_descriptor`] (NUL-terminated key —
//! it is handed straight to Cyclone's C descriptor table) before the egress
//! publisher is created. The Cyclone backend installs the registrar during
//! `nros_rmw_cyclonedds_sys::register()`.
//!
//! ## Env vars
//!
//! * `ZENOH_LOCATOR` — zenoh primary session. Default `tcp/127.0.0.1:7447`.
//! * `ROS_DOMAIN_ID` — Cyclone DDS egress domain. Default `0`.

use log::{info, warn};
use nros::{Executor, ExecutorConfig};
use nros_serdes::schema::{Field, FieldType};

// DDS-mangled type name — matches the zenoh keyexpr the `native-rs-talker`
// (typed `Int32`) publishes under AND the Cyclone descriptor's `m_typename`.
const TYPE_NAME: &str = "std_msgs::msg::dds_::Int32_";
const TYPE_HASH: &str = "TypeHashNotSupported";

// Schema for the Cyclone descriptor (ROS form + NUL-terminated key/field — the
// registry hands the pointer straight to C). Mirrors std_msgs/msg/Int32.
const REG_TYPE_NAME: &str = "std_msgs/msg/Int32\0";
static INT32_FIELDS: &[Field] = &[Field {
    name: "data\0",
    ty: FieldType::Int32,
    offset: 0,
}];

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    info!("=== Issue #53 bridge: zenoh → Cyclone DDS (Int32) ===");

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
        .node_name("bridge_zenoh_to_cyclonedds")
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
        .rmw("cyclonedds")
        // The egress is an EXTRA session — `cfg.domain_id` only sets the primary
        // (zenoh) session. Thread the domain here or the cyclone participant opens
        // on domain 0 (`resolve_session_slot`'s `domain_id.unwrap_or(0)`), so a
        // listener on any other `ROS_DOMAIN_ID` never matches.
        .domain_id(domain_id)
        .build()
        .expect("egress Node (cyclonedds session open)");
    info!("Nodes built: ingress (zenoh), egress (cyclonedds @ domain {domain_id})");

    // Stage the Int32 descriptor BEFORE the raw publisher (the cyclonedds crux).
    nros_rmw::register_type_descriptor(REG_TYPE_NAME, INT32_FIELDS)
        .expect("register std_msgs/Int32 descriptor with cyclonedds");

    let pub_out = exec
        .with_node_try(node_out, |n| {
            n.create_publisher_raw("/chatter", TYPE_NAME, TYPE_HASH)
        })
        .expect("egress raw publisher");
    let pub_out = std::rc::Rc::new(std::cell::RefCell::new(pub_out));

    let pub_for_cb = std::rc::Rc::clone(&pub_out);
    let _ingress_sub = exec
        .node_mut(node_in)
        .subscription("/chatter")
        .generic(TYPE_NAME, TYPE_HASH)
        .qos(Default::default())
        .rx_buffer::<1024>()
        .build(move |bytes: &[u8]| {
            let p = pub_for_cb.borrow();
            match p.publish_raw(bytes) {
                Ok(()) => info!("forwarded {} bytes zenoh→cyclonedds", bytes.len()),
                Err(e) => warn!("forward publish failed: {e:?}"),
            }
        })
        .expect("register ingress sub on zenoh");

    info!("Spinning. Publish on zenoh /chatter; observe forwards on Cyclone DDS /chatter.");
    loop {
        let _ = exec.spin_once(core::time::Duration::from_millis(10));
    }
}
