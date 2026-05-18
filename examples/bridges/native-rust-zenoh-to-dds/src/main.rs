//! Phase 104.C — bridge demo: forward raw CDR bytes Zenoh → DDS.
//!
//! Demonstrates the rclcpp-aligned multi-Node + multi-RMW pattern:
//!
//! * Two RMW backends (zenoh-pico + dust-DDS) linked in one binary.
//!   Both self-register under their canonical names via POSIX ctor
//!   (Phase 104.A).
//! * One `Executor` holds two `Node`s. The primary Node ("ingress")
//!   binds to whichever backend is named in
//!   `Executor::open_with_rmw("zenoh", ...)`; the egress Node opts
//!   in to DDS via `node_builder.rmw("dds")`.
//! * The Executor opens a second session via `CffiRmw::open_with_rmw`
//!   under the hood (Phase 104.C.3), stores it in `extra_sessions`,
//!   and drives both via `spin_once`.
//!
//! Topic flow (untyped raw bytes — keeps the example free of any
//! generated message-crate dependency):
//!
//!   Zenoh "/chatter" ── ingress sub (raw) ──┐
//!                                            ├─ bridge ─ publish ──> DDS "/chatter"
//!
//! For a typed-message bridge, swap the raw sub/pub for
//! `create_publisher::<M>` / `create_subscription_buffered::<M>`;
//! see `examples/native/rust/zenoh/talker/` for the codegen setup.
//!
//! Usage:
//!
//! ```bash
//! zenohd --listen tcp/127.0.0.1:7447 &
//! cargo run -p native-rs-bridge-zenoh-to-dds
//! ```

use std::{cell::RefCell, rc::Rc};

use log::{info, warn};
use nros::ExecutorConfig;

const TYPE_NAME: &str = "std_msgs/msg/dds_/String_";
const TYPE_HASH: &str = "RIHS01_df668c740482bbd48fb39d76a70dfd4bd59db1288021743503259e948f6b1a18";

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    info!("=== Phase 104 bridge: Zenoh → DDS ===");

    // Phase 156 — explicit `register()` calls. Phase 128.B.1
    // notes that on stable Rust the backend rlib is NOT
    // pulled into the link line unless something references
    // one of its symbols, even though its `RMW_INIT_ENTRIES`
    // entry is `#[used]`. The two `register()` calls below
    // double as (a) idempotent registration triggers + (b)
    // the symbol references that drag each rlib's CGU into
    // the binary so the linkme walker actually finds both
    // names. C/C++ builds avoid this because
    // `--whole-archive` pulls every section entry
    // unconditionally; pure-Rust bridges have to do it
    // manually. Without these, `Executor::open_with_rmw(...)`
    // returns `Transport(ConnectionFailed)` because the
    // registry's `"zenoh"` slot is empty.
    nros_rmw_zenoh::register().expect("Failed to register zenoh RMW backend");
    nros_rmw_dds::register().expect("Failed to register dds RMW backend");

    // Phase 104.D.3 — read locator + mode from env so callers
    // (the E2E test, ad-hoc users) can point the bridge at any
    // zenohd instance without rebuilding. `default()` would
    // hard-code `tcp/127.0.0.1:7447` and miss NROS_LOCATOR.
    let cfg = ExecutorConfig::from_env()
        .node_name("bridge_primary")
        .namespace("/");

    // Open against the Zenoh backend explicitly. Both backend ctors
    // (zenoh + dds) fire at lib-load, so without a name the
    // first-registered slot wins. `open_with_rmw` removes that
    // ambiguity.
    let mut exec = nros::Executor::open_with_rmw("zenoh", &cfg)
        .expect("failed to open executor with zenoh backend");
    info!("Executor opened (primary RMW: zenoh)");

    let node_in = exec
        .node_builder("ingress")
        .rmw("zenoh")
        .build()
        .expect("ingress Node");
    let node_out = exec
        .node_builder("egress")
        .rmw("dds")
        .domain_id(0)
        .build()
        .expect("egress Node — DDS session open");

    info!(
        "Nodes registered: ingress(session_idx={}), egress(session_idx={})",
        exec.node(node_in).unwrap().session_idx,
        exec.node(node_out).unwrap().session_idx,
    );

    // Egress publisher (raw bytes) via `with_node_try` — flat
    // `Result` shape (Phase 104.C.3.3.d).
    let pub_out = exec
        .with_node_try(node_out, |n| {
            n.create_publisher_raw("/chatter", TYPE_NAME, TYPE_HASH)
        })
        .expect("create egress raw publisher");
    let pub_out = Rc::new(RefCell::new(pub_out));
    info!("Egress raw publisher created on DDS /chatter");

    // Ingress raw subscription on the zenoh session — Phase
    // 104.C.3.2 `register_subscription_buffered_raw_on(node_id, ...)`
    // routes through the named Node's session. Callback fires
    // inside `spin_once` whenever a new message arrives on Zenoh
    // /chatter and republishes verbatim on the DDS publisher.
    let pub_out_cb = Rc::clone(&pub_out);
    exec.register_subscription_buffered_raw_on::<_, 1024>(
        node_in,
        "/chatter",
        TYPE_NAME,
        TYPE_HASH,
        Default::default(),
        move |bytes: &[u8]| {
            let p = pub_out_cb.borrow();
            match p.publish_raw(bytes) {
                Ok(()) => info!("forwarded {} bytes", bytes.len()),
                Err(e) => warn!("publish failed: {:?}", e),
            }
        },
    )
    .expect("register ingress raw sub on zenoh");
    info!("Ingress raw subscription registered on Zenoh /chatter");

    info!("Spinning. Publish on Zenoh /chatter; listen on DDS /chatter.");
    exec.spin_default();
}
