//! Phase 211.I — minimal mixed-RMW bridge fixture.
//!
//! Forwards bytes from a zenoh raw subscription on `/chatter` to a raw
//! publisher on the same topic, on an XRCE-DDS egress session opened
//! against the agent at `XRCE_LOCATOR`.
//!
//! Differs from the Phase 110.G `tt-zenoh-to-xrce` example in two ways:
//!
//! 1. **Type name + hash match `std_msgs::msg::String`**, the type the
//!    `native-rs-talker` + `native-rs-listener` fixtures use (phase-277
//!    flipped the demos to the official String chatter). A type mismatch
//!    here carries from the publisher's `RawSubscription` keyexpr into
//!    the XRCE side and nothing on the listener matches.
//! 2. **No TT scheduling** — a single zenoh-side callback republishes
//!    directly. The TT gate is irrelevant for the 211.I assertion
//!    (that a sample crosses the RMW boundary) and adds variance to
//!    the e2e timeout window.
//!
//! ## Env vars
//!
//! * `ZENOH_LOCATOR` — locator for the zenoh primary session. Default
//!   `tcp/127.0.0.1:7447`.
//! * `XRCE_LOCATOR` — `host:port` (or `udp/host:port`) for the XRCE
//!   agent. Default `127.0.0.1:8888`. The XRCE backend strips the
//!   `udp/` prefix in `session.c::locator_strip_udp_prefix`.

use log::{info, warn};
use nros::{Executor, ExecutorConfig};

const TYPE_NAME: &str = "std_msgs::msg::dds_::String_";
const TYPE_HASH: &str = "TypeHashNotSupported";

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    info!("=== Phase 211.I bridge: zenoh → XRCE (String) ===");

    // Both backends must `register()` so their CGUs link in — Phase
    // 128/129 `linkme` discovery alone isn't enough when the rlib has
    // no other reference into its CGU.
    nros_rmw_zenoh::register().expect("register zenoh backend");
    nros_rmw_xrce_cffi::register().expect("register xrce backend");

    let zenoh_locator =
        std::env::var("ZENOH_LOCATOR").unwrap_or_else(|_| "tcp/127.0.0.1:7447".into());
    let xrce_locator = std::env::var("XRCE_LOCATOR").unwrap_or_else(|_| "127.0.0.1:8888".into());

    let cfg = ExecutorConfig::new(&zenoh_locator)
        .node_name("bridge_zenoh_to_xrce")
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
                Ok(()) => info!("forwarded {} bytes zenoh→xrce", bytes.len()),
                Err(e) => warn!("forward publish failed: {e:?}"),
            }
        })
        .expect("register ingress sub on zenoh");

    info!("Spinning. Publish on zenoh /chatter; observe forwards on XRCE /chatter.");
    loop {
        let _ = exec.spin_once(core::time::Duration::from_millis(10));
    }
}
