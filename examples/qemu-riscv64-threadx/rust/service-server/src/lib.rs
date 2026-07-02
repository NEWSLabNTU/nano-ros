//! ThreadX QEMU RISC-V Service Server — app-node logic.
//!
//! Handles `example_interfaces/AddTwoInts` requests on `/add_two_ints`. This is
//! an **app node** (it owns `main`, via `src/main.rs`'s `nros::main!()`), not a
//! workspace Node lib — but the *logic* is still platform/RMW-agnostic:
//! `register()` declares node + service server; `on_callback("on_add")` reads the
//! typed request, sums the two ints, and writes the typed reply. The board
//! (`nros-board-threadx-qemu-riscv64`, `BoardEntry::run`) owns `nros::init`,
//! executor open, RMW registration, and the spin loop. RMW selection
//! (zenoh / cyclonedds) lives in `Cargo.toml [features]`; the locator + domain in
//! `[package.metadata.nros.deploy.threadx-qemu-riscv64]` — never here.

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
use nros::{Callback, CallbackCtx, ExecutableNode, Node, NodeContext, NodeOptions, NodeResult};

/// AddTwoInts service server — sums the two request ints on every call.
pub struct AddTwoIntsServer;

impl Node for AddTwoIntsServer {
    const NAME: &'static str = "add_two_ints_server";

    fn register(ctx: &mut NodeContext<'_>) -> NodeResult<()> {
        let mut node = ctx.create_node(NodeOptions::new("add_two_ints_server"))?;
        let _srv = node.create_service_server_for_name_with_callback::<AddTwoInts>(
            "/add_two_ints",
            "on_add",
        )?;
        // Readiness marker the e2e harness greps before driving the client.
        log::info!("Waiting for service requests");
        Ok(())
    }
}

impl ExecutableNode for AddTwoIntsServer {
    /// Count of handled requests.
    type State = u32;

    fn init() -> Self::State {
        0
    }

    fn on_callback(state: &mut Self::State, callback: Callback<'_>, ctx: &mut CallbackCtx<'_>) {
        if callback.as_str() == "on_add"
            && let Ok(req) = ctx.message::<AddTwoIntsRequest>()
        {
            log::info!("Incoming request");
            log::info!("a: {} b: {}", req.a, req.b);
            let resp = AddTwoIntsResponse { sum: req.a + req.b };
            let _ = ctx.reply::<AddTwoIntsResponse, 64>(&resp);
            *state = state.wrapping_add(1);
        }
    }
}

nros::node!(AddTwoIntsServer);

// CycloneDDS / CMake firmware path: the C `startup.c::main` calls
// `tx_kernel_enter()` and dispatches to this `app_main` *inside* the ThreadX app
// thread — so the kernel is already running here. `run_app_thread` runs the
// post-kernel body (open executor + `register` + spin); it must NOT re-enter the
// kernel via `BoardEntry::run`. The zenoh/cargo path uses `src/main.rs`'s
// `nros::main!()` instead and never compiles this. Both are thin — the board owns
// executor open, RMW registration, and the spin loop; the `nros::node!()`-emitted
// `register` declares the server. No manual `Executor::open` / `register_rmw` /
// spin loop / hardcoded locator in the example.
#[unsafe(no_mangle)]
pub extern "C" fn app_main() -> ! {
    nros_board_threadx_qemu_riscv64::run_app_thread(register)
}
