//! ThreadX QEMU RISC-V Talker — Phase 245 app-node logic.
//!
//! Publishes `std_msgs/Int32` on `/chatter` once per second. This is an **app
//! node** (it owns `main`, via `src/main.rs`'s `nros::main!()`), not a workspace
//! Node lib — but the *logic* is still platform/RMW-agnostic: `register()`
//! declares node + publisher + timer; `on_callback("on_tick")` runs the body. The
//! board (`nros-board-threadx-qemu-riscv64`, `BoardEntry::run`) owns `nros::init`,
//! executor open, RMW registration, and the spin loop. RMW selection
//! (zenoh / cyclonedds / xrce) lives in `Cargo.toml [features]`; the locator +
//! domain in `[package.metadata.nros.deploy.threadx-qemu-riscv64]` — never here.

#![no_std]

// Keep the board crate linked into the `staticlib`/`rlib` firmware artifact even
// when no path explicitly names it (the zenoh/cargo path enters through
// `main.rs`'s `nros::main!()`, not this lib). The board owns the `#[panic_handler]`
// + global allocator; without this `extern crate`, the standalone `staticlib`
// target has no panic handler and fails to compile.
extern crate nros_board_threadx_qemu_riscv64 as _;

#[cfg(feature = "rmw-cyclonedds")]
extern crate alloc;
#[cfg(feature = "rmw-cyclonedds")]
extern crate nros_platform_critical_section as _;

use nros::{
    Callback, CallbackCtx, ExecutableNode, Node, NodeContext, NodeOptions, NodeResult,
    TimerDuration,
};
use std_msgs::msg::Int32;

/// Talker node — counter state + chatter publish on every tick.
pub struct Talker;

impl Node for Talker {
    const NAME: &'static str = "talker";

    fn register(ctx: &mut NodeContext<'_>) -> NodeResult<()> {
        let mut node = ctx.create_node(NodeOptions::new("talker"))?;
        let pub_chatter = node.create_publisher_for_topic::<Int32>("/chatter")?;
        let _timer =
            node.create_timer_for_callback_name("on_tick", TimerDuration::from_millis(1000))?;
        node.callback_for_name("on_tick")
            .publishes_entity(&pub_chatter)?;
        Ok(())
    }
}

impl ExecutableNode for Talker {
    /// Monotonic counter — the next int32 to publish.
    type State = i32;

    fn init() -> Self::State {
        0
    }

    fn on_callback(state: &mut Self::State, callback: Callback<'_>, ctx: &mut CallbackCtx<'_>) {
        if callback.as_str() == "on_tick" {
            let msg = Int32 { data: *state };
            let _ = ctx.publish_to_topic::<Int32, 64>("/chatter", &msg);
            *state = state.wrapping_add(1);
        }
    }
}

nros::node!(Talker);

// CycloneDDS / CMake firmware path: the C `startup.c::main` calls
// `tx_kernel_enter()` and dispatches to this `app_main` *inside* the ThreadX app
// thread — so the kernel is already running here. `run_app_thread` runs the
// post-kernel body (open executor + `register` + spin); it must NOT re-enter the
// kernel via `BoardEntry::run`. The zenoh/cargo path uses `src/main.rs`'s
// `nros::main!()` instead and never compiles this. Both are thin — the board owns
// executor open, RMW registration, and the spin loop; the `nros::node!()`-emitted
// `register` declares the Talker. No manual `Executor::open` / `register_rmw` /
// spin loop / hardcoded locator in the example (Phase 245 / issue 0049 P1/P3/P4/P6).
#[cfg(feature = "rmw-cyclonedds")]
#[unsafe(no_mangle)]
pub extern "C" fn app_main() -> ! {
    nros_board_threadx_qemu_riscv64::run_app_thread(register)
}
