//! ThreadX QEMU RISC-V Fibonacci Action Client — Phase 245 app-node logic.
//!
//! Sends an `example_interfaces/Fibonacci` goal on `/fibonacci`. This is an
//! **app node** (it owns `main`, via `src/main.rs`'s `nros::main!()`), not a
//! workspace Node lib — but the *logic* is still platform/RMW-agnostic:
//! `register()` declares node + action client; `tick()` issues a one-shot
//! `send_goal` (then stays idempotent); feedback/result callbacks land via
//! `on_callback` once codegen wires the result-future + feedback-stream +
//! `GoalStatusArray` subscribers through to dispatch. The board
//! (`nros-board-threadx-qemu-riscv64`, `BoardEntry::run`) owns `nros::init`,
//! executor open, RMW registration, and the spin loop. RMW selection
//! (zenoh / cyclonedds) lives in `Cargo.toml [features]`; the locator + domain
//! in `[package.metadata.nros.deploy.threadx-qemu-riscv64]` — never here.

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

use example_interfaces::action::{Fibonacci, FibonacciGoal};
use nros::{
    Callback, CallbackCtx, ExecutableNode, Node, NodeContext, NodeOptions, NodeResult, TickCtx,
};

/// Fibonacci action client — declares the client, then issues a single goal
/// (`order = 5`) on the first `tick`.
pub struct FibonacciClient;

impl Node for FibonacciClient {
    const NAME: &'static str = "fibonacci_action_client";

    fn register(ctx: &mut NodeContext<'_>) -> NodeResult<()> {
        let mut node = ctx.create_node(NodeOptions::new("fibonacci_action_client"))?;
        let _client = node.create_action_client_for_name::<Fibonacci>("/fibonacci")?;
        Ok(())
    }
}

pub struct State {
    /// Set once the goal has been sent — keeps `tick` idempotent.
    sent: bool,
}

impl ExecutableNode for FibonacciClient {
    type State = State;

    fn init() -> Self::State {
        State { sent: false }
    }

    fn on_callback(_state: &mut Self::State, _callback: Callback<'_>, _ctx: &mut CallbackCtx<'_>) {
        // Feedback / result callbacks land here once codegen wires the
        // `GoalStatusArray` + feedback-stream + result-future subscribers.
        // The id-driven dispatch is the M-F.4.a + N runtime plumbing; this
        // body is the seam.
    }

    fn tick(state: &mut Self::State, ctx: &mut TickCtx<'_>) {
        if state.sent {
            return;
        }
        let goal = FibonacciGoal { order: 5 };
        // 32 B is more than enough for one `i32` + CDR header.
        if ctx
            .send_goal_for_name::<FibonacciGoal, 32>("/fibonacci", &goal)
            .is_ok()
        {
            state.sent = true;
        }
        // On a `Runtime` stub error, `sent` stays false — the next tick
        // retries. Once the real dispatch ships, the first successful send
        // flips the flag.
    }
}

nros::node!(FibonacciClient);

// CycloneDDS / CMake firmware path: the C `startup.c::main` calls
// `tx_kernel_enter()` and dispatches to this `app_main` *inside* the ThreadX app
// thread — so the kernel is already running here. `run_app_thread` runs the
// post-kernel body (open executor + `register` + spin); it must NOT re-enter the
// kernel via `BoardEntry::run`. The zenoh/cargo path uses `src/main.rs`'s
// `nros::main!()` instead and never compiles this. Both are thin — the board owns
// executor open, RMW registration, and the spin loop; the `nros::node!()`-emitted
// `register` declares the FibonacciClient. No manual `Executor::open` /
// `register_rmw` / spin loop / hardcoded locator in the example
// (Phase 245 / issue 0049 P1/P3/P4/P6).
#[cfg(feature = "rmw-cyclonedds")]
#[unsafe(no_mangle)]
pub extern "C" fn app_main() -> ! {
    nros_board_threadx_qemu_riscv64::run_app_thread(register)
}
