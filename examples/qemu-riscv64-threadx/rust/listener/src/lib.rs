//! ThreadX QEMU RISC-V Listener — Phase 245 app-node logic.
//!
//! Subscribes to `std_msgs/String` on `/chatter` and logs each message
//! (`I heard: [Hello World: N]`). This is an **app node** (it owns `main`, via `src/main.rs`'s
//! `nros::main!()`), not a workspace Node lib — but the *logic* is still
//! platform/RMW-agnostic: `register()` declares node + subscription;
//! `on_callback("on_chatter")` runs the body. The board
//! (`nros-board-threadx-qemu-riscv64`, `BoardEntry::run`) owns `nros::init`,
//! executor open, RMW registration, and the spin loop. RMW selection
//! (zenoh / cyclonedds / xrce) lives in `Cargo.toml [features]`; the locator +
//! domain in `[package.metadata.nros.deploy.threadx-qemu-riscv64]` — never here.

#![no_std]

extern crate alloc;
// Keep the board crate (panic handler + allocator + critical-section impl)
// linked into the standalone `staticlib` even on the zenoh/cargo path, where
// only `main.rs`'s `nros::main!()` names it (issue #205 — the per-example
// critical-section anchor moved into the board crate).
extern crate nros_board_threadx_qemu_riscv64 as _;

use nros::{Callback, CallbackCtx, ExecutableNode, Node, NodeContext, NodeOptions, NodeResult};
use std_msgs::msg::String as StringMsg;

/// Listener node — tracks the last value seen on `/chatter`.
pub struct Listener;

impl Node for Listener {
    const NAME: &'static str = "listener";

    fn register(ctx: &mut NodeContext<'_>) -> NodeResult<()> {
        let mut node = ctx.create_node(NodeOptions::new("listener"))?;
        let _sub =
            node.create_subscription_for_callback_name::<StringMsg>("on_chatter", "/chatter")?;
        Ok(())
    }
}

impl ExecutableNode for Listener {
    /// Number of messages seen on `/chatter`.
    type State = i32;

    fn init() -> Self::State {
        0
    }

    fn on_callback(state: &mut Self::State, callback: Callback<'_>, ctx: &mut CallbackCtx<'_>) {
        if callback.as_str() == "on_chatter" {
            if let Ok(msg) = ctx.message::<StringMsg>() {
                *state = state.wrapping_add(1);
                // Canonical delivery line (phase-277 W4) — the rtos e2e
                // harness counts `I heard:` lines; without it a working
                // listener looked silent (pre-existing gap found in T4).
                log::info!("I heard: [{}]", msg.data);
            }
        }
    }
}

nros::node!(Listener);

// CycloneDDS / CMake firmware path: the C `startup.c::main` calls
// `tx_kernel_enter()` and dispatches to this `app_main` *inside* the ThreadX app
// thread — so the kernel is already running here. `run_app_thread` runs the
// post-kernel body (open executor + `register` + spin); it must NOT re-enter the
// kernel via `BoardEntry::run`. The zenoh/cargo path uses `src/main.rs`'s
// `nros::main!()` instead and never compiles this. Both are thin — the board owns
// executor open, RMW registration, and the spin loop; the `nros::node!()`-emitted
// `register` declares the Listener. No manual `Executor::open` / `register_rmw` /
// spin loop / hardcoded locator in the example (Phase 245 / issue 0049 P1/P3/P4/P6).
nros_board_threadx_qemu_riscv64::cyclonedds_app_main!(register);
