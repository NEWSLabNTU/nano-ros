//! ThreadX QEMU RISC-V Talker — Phase 245 app-node logic.
//!
//! Publishes `std_msgs/String` (`Hello World: N`) on `/chatter` once per
//! second, matching the official ROS 2 `demo_nodes_cpp` talker. This is an **app
//! node** (it owns the image's entry, via `src/app_main.rs`), not a workspace
//! Node lib — but the *logic* is still platform/RMW-agnostic: `register()`
//! declares node + publisher + timer; `on_callback("on_tick")` runs the body. The
//! board (`nros-board-threadx-qemu-riscv64`, `BoardEntry::run`) owns `nros::init`,
//! executor open, RMW registration, and the spin loop. RMW selection
//! (zenoh / cyclonedds / xrce) lives in `Cargo.toml [features]`; the locator +
//! domain in `[package.metadata.nros.deploy.threadx-qemu-riscv64]` — never here.

#![no_std]

mod app_main;

// Keep the board crate (panic handler + allocator + critical-section impl)
// linked into the `staticlib`. phase-369 — `app_main!` names it on both RMW
// paths now, so this anchor is belt-and-braces rather than load-bearing
// (issue #205 — the per-example critical-section anchor moved into the board
// crate).

use core::fmt::Write as _;
use nros::{
    Callback, CallbackCtx, ExecutableNode, Node, NodeContext, NodeOptions, NodeResult,
    TimerDuration,
};

use std_msgs::msg::String as StringMsg;

/// Talker node — counter state + chatter publish on every tick.
pub struct Talker;

impl Node for Talker {
    const NAME: &'static str = "talker";

    // phase-391 W5-endgame (issue 0857) — exact bounds: one publisher.
    // The per-class cell is a STATIC sized by these; the knob default
    // (8 slots x ~1.35 KiB) collapsed the esp32 single-image stack.
    const ENTITY_BOUNDS: nros::EntityBounds = nros::EntityBounds::exact(1, 0, 0, 0, 0);

    fn register(ctx: &mut NodeContext<'_>) -> NodeResult<()> {
        let mut node = ctx.create_node(NodeOptions::new("talker"))?;
        let pub_chatter = node.create_publisher_for_topic::<StringMsg>("/chatter")?;
        let _timer =
            node.create_timer_for_callback_name("on_tick", TimerDuration::from_millis(1000))?;
        node.callback_for_name("on_tick")
            .publishes_entity(&pub_chatter)?;
        Ok(())
    }
}

impl ExecutableNode for Talker {
    /// Monotonic counter — the next sequence number to publish.
    type State = i32;

    fn init() -> Self::State {
        0
    }

    fn on_callback(state: &mut Self::State, callback: Callback<'_>, ctx: &mut CallbackCtx<'_>) {
        if callback.as_str() == "on_tick" {
            // Official ROS 2 demo behavior (phase-277 W4): payload
            // "Hello World: N" (N from 1) + the canonical `Publishing:` log
            // line the e2e harness counts.
            *state = state.wrapping_add(1);
            let mut msg = StringMsg::default();
            let _ = write!(msg.data, "Hello World: {}", *state);
            let _ = ctx.publish_to_topic::<StringMsg, 64>("/chatter", &msg);
            log::info!("Publishing: '{}'", msg.data);
        }
    }
}

nros::node!(Talker);

// CycloneDDS / CMake firmware path: the C `startup.c::main` calls
// `tx_kernel_enter()` and dispatches to this `app_main` *inside* the ThreadX app
// thread — so the kernel is already running here. `run_app_thread` runs the
// post-kernel body (open executor + `register` + spin); it must NOT re-enter the
// kernel via `BoardEntry::run`. phase-369 — BOTH RMWs build through CMake and
// compile this; the cargo `main.rs` path is gone. It is thin — the board owns
// executor open, RMW registration, and the spin loop; the `nros::node!()`-emitted
// `register` declares the Talker. No manual `Executor::open` / `register_rmw` /
// spin loop / hardcoded locator in the example (Phase 245 / issue 0049 P1/P3/P4/P6).

