//! Native Listener Example — Phase 212.L.2 Application pkg shape.
//!
//! Subscribes to `std_msgs/Int32` on `/chatter` and logs each message.
//! Single-file `[[bin]]`: explicit [`nros::init_with_launch_auto`]
//! (Pattern 2 — picks up launch overlay env vars from the environment)
//! then a user-owned spin loop.
//!
//! # Usage
//!
//! ```bash
//! # Zenoh path (default):
//! zenohd --listen tcp/127.0.0.1:7447 &
//! cargo run -p native-rs-listener
//!
//! # XRCE path:
//! cargo run -p native-rs-listener --no-default-features --features rmw-xrce
//!
//! # Cyclone DDS path (Phase 212.K pure-cargo):
//! cargo run -p native-rs-listener --no-default-features --features rmw-cyclonedds
//! ```
//!
//! Override the locator with `NROS_LOCATOR` (or legacy `ZENOH_LOCATOR`);
//! `RUST_LOG=debug` for debug logs.

use log::{error, info};
use nros::prelude::*;
use std_msgs::msg::Int32;

// Phase 248 C6d — board-LESS APP owns + force-links its selected backend rlib.
// The `nros` umbrella no longer carries `rmw-*`, so its `__FORCE_LINK_*` statics
// are inert here; this `#[used]` static keeps the backend rlib (and its linkme
// `RMW_INIT_ENTRIES` self-register section) in the link graph so the backend
// auto-registers on POSIX. Mirrors `packages/core/nros/src/lib.rs`.
#[cfg(feature = "rmw-zenoh")]
#[used]
static __FORCE_LINK_ZENOH: fn() -> Result<(), nros_rmw_zenoh::RegisterError> =
    nros_rmw_zenoh::register;
#[cfg(feature = "rmw-xrce")]
#[used]
static __FORCE_LINK_XRCE: fn() -> Result<(), nros_rmw_xrce_cffi::RegisterError> =
    nros_rmw_xrce_cffi::register;
#[cfg(feature = "rmw-cyclonedds")]
#[used]
static __FORCE_LINK_CYCLONEDDS_SYS: fn() -> Result<(), nros_rmw_cyclonedds_sys::RegisterError> =
    nros_rmw_cyclonedds_sys::register;

// Phase 244 D3 — RMW selection is build/config, not source: the backend is
// chosen by the mutually-exclusive `rmw-{zenoh,cyclonedds,xrce}` Cargo features
// (default `rmw-zenoh`) and self-registers via the `nros` umbrella's
// `#[used] __FORCE_LINK_*` statics + the cffi walker in `nros::init`. No
// `register()` call and no RMW name baked into the source.

/// Safety-e2e listener (zenoh-specific): validates CRC + tracks seq gaps.
#[cfg(feature = "safety-e2e")]
fn main() {
    env_logger::init();
    info!("nros Native Listener (Safety E2E)");

    let ctx = nros::init_with_launch_auto().expect("nros init failed");
    let cfg = ctx.config("listener");
    let mut executor: Executor = Executor::open(&cfg).expect("Failed to open session");

    let nid = executor
        .node_builder("listener")
        .build()
        .expect("Failed to build node");
    let mut count: u64 = 0;
    executor
        .node_mut(nid)
        .subscription("/chatter")
        .typed::<Int32>()
        .safety()
        .build(move |msg, status| {
            count += 1;
            let crc_str = match status.crc_valid {
                Some(true) => "ok",
                Some(false) => "FAIL",
                None => "n/a",
            };
            info!(
                "[{}] Received: data={} [SAFETY] seq_gap={} dup={} crc={}",
                count, msg.data, status.gap, status.duplicate, crc_str
            );
        })
        .expect("Failed to add safety subscription");
    info!("Safety subscriber created for topic: /chatter");
    if let Err(e) = executor.spin_blocking(SpinOptions::default()) {
        error!("Spin error: {:?}", e);
    }
}

/// Standard listener — subscribe to `std_msgs/Int32` on `/chatter` and
/// log each message.
#[cfg(not(feature = "safety-e2e"))]
fn main() {
    env_logger::init();
    info!("nros Native Listener");
    info!("==========================================");

    // Phase 212.L.5 Pattern 2 — launch-aware init.
    let ctx = nros::init_with_launch_auto().expect("nros init failed");
    let cfg = ctx.config("listener");
    let mut executor: Executor = Executor::open(&cfg).expect("Failed to open session");

    let nid = executor
        .node_builder("listener")
        .build()
        .expect("Failed to build node");
    executor
        .node_mut(nid)
        .subscription("/chatter")
        .typed::<Int32>()
        .message_info()
        .build(move |msg, info| {
            info!("Received: {}", msg.data);
            if let Some(info) = info {
                let gid = info.publisher_gid();
                log::trace!(
                    "seq={} gid={:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x} ",
                    info.publication_sequence_number(),
                    gid[0],
                    gid[1],
                    gid[2],
                    gid[3],
                    gid[4],
                    gid[5],
                    gid[6],
                    gid[7],
                );
            }
        })
        .expect("Failed to add subscription");
    info!("Subscriber created for topic: /chatter");
    info!("Waiting for Int32 messages on /chatter...");

    if let Err(e) = executor.spin_blocking(SpinOptions::default()) {
        error!("Spin error: {:?}", e);
    }
}
