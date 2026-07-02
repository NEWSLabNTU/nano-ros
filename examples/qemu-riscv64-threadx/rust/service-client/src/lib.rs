//! ThreadX QEMU RISC-V Service Client — Phase 245 app-node logic.
//!
//! Calls `example_interfaces/AddTwoInts` on `/add_two_ints`. This is an **app
//! node** (it owns `main`, via `src/main.rs`'s `nros::main!()`), not a workspace
//! Node lib — but the *logic* is still platform/RMW-agnostic: `register()`
//! declares node + service client + a 1 Hz driver timer; `on_callback("issue_call")`
//! arms the next request (bumps the operand counter), and `tick` dispatches the
//! call — `TickCtx` is the only place `&mut Executor` is free (see `TickCtx`
//! docs). The board (`nros-board-threadx-qemu-riscv64`, `BoardEntry::run`) owns
//! `nros::init`, executor open, RMW registration, and the spin loop. RMW
//! selection (zenoh / cyclonedds) lives in `Cargo.toml [features]`; the locator +
//! domain in `[package.metadata.nros.deploy.threadx-qemu-riscv64]` — never here.
//!
//! The pre-245 `src/main.rs` issued a fixed sweep of test cases
//! (`(5,3)`, `(10,20)`, `(100,200)`, `(-5,10)`) inside a manual `Executor::open`
//! + spin loop; the app-node form replaces that with a periodic call driven by
//! the timer + a monotonic operand counter.

#![no_std]

// Keep the board crate linked into the `staticlib`/`rlib` firmware artifact even
// when no path explicitly names it (the zenoh/cargo path enters through
// `main.rs`'s `nros::main!()`, not this lib). The board owns the `#[panic_handler]`
// + global allocator; without this `extern crate`, the standalone `staticlib`
// target has no panic handler and fails to compile.
extern crate nros_board_threadx_qemu_riscv64 as _;

extern crate alloc;
extern crate nros_platform_critical_section as _;

use example_interfaces::srv::{AddTwoInts, AddTwoIntsRequest, AddTwoIntsResponse};
use nros::{
    Callback, CallbackCtx, ExecutableNode, Node, NodeContext, NodeOptions, NodeResult, TickCtx,
    TimerDuration,
};

/// AddTwoInts service client — issues a call once per second.
pub struct AddTwoIntsClient;

impl Node for AddTwoIntsClient {
    const NAME: &'static str = "add_two_ints_client";

    fn register(ctx: &mut NodeContext<'_>) -> NodeResult<()> {
        let mut node = ctx.create_node(NodeOptions::new("add_two_ints_client"))?;
        let _client = node.create_service_client_for_name::<AddTwoInts>("/add_two_ints")?;
        let _timer =
            node.create_timer_for_callback_name("issue_call", TimerDuration::from_secs(1))?;
        Ok(())
    }
}

pub struct State {
    /// Set by `on_callback` when the timer fires; drained by `tick`
    /// after dispatching the call.
    pending: bool,
    /// Monotonic counter used as the request operands.
    counter: i64,
}

impl ExecutableNode for AddTwoIntsClient {
    type State = State;

    fn init() -> Self::State {
        State {
            pending: false,
            counter: 0,
        }
    }

    fn on_callback(state: &mut Self::State, callback: Callback<'_>, _ctx: &mut CallbackCtx<'_>) {
        if callback.as_str() == "issue_call" {
            state.pending = true;
            state.counter = state.counter.wrapping_add(1);
        }
    }

    fn tick(state: &mut Self::State, ctx: &mut TickCtx<'_>) {
        if !state.pending {
            return;
        }
        state.pending = false;
        let req = AddTwoIntsRequest {
            a: state.counter,
            b: state.counter.wrapping_add(1),
        };
        // Stack-buf sizes: AddTwoInts request = 2 × i64 + CDR header = 24 B;
        // response = 1 × i64 + header = 16 B. 64 each is generous.
        let _: nros::NodeResult<AddTwoIntsResponse> = ctx
            .call_for_name::<AddTwoIntsRequest, AddTwoIntsResponse, 64, 64>("/add_two_ints", &req);
    }
}

nros::node!(AddTwoIntsClient);

// CycloneDDS / CMake firmware path: the C `startup.c::main` calls
// `tx_kernel_enter()` and dispatches to this `app_main` *inside* the ThreadX app
// thread — so the kernel is already running here. `run_app_thread` runs the
// post-kernel body (open executor + `register` + spin); it must NOT re-enter the
// kernel via `BoardEntry::run`. The zenoh/cargo path uses `src/main.rs`'s
// `nros::main!()` instead and never compiles this. Both are thin — the board owns
// executor open, RMW registration, and the spin loop; the `nros::node!()`-emitted
// `register` declares the client. No manual `Executor::open` / `register_rmw` /
// spin loop / hardcoded locator in the example (Phase 245 / issue 0049 P1/P3/P4/P6).
#[unsafe(no_mangle)]
pub extern "C" fn app_main() -> ! {
    nros_board_threadx_qemu_riscv64::run_app_thread(register)
}
