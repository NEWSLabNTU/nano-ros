//! ThreadX QEMU RISC-V Service Server — app-node logic.
//!
//! Handles `example_interfaces/AddTwoInts` requests on `/add_two_ints`. This is
//! an **app node** (it owns `main`, via `src/main.rs`'s `nros::main!()`), not a
//! workspace Node lib — but the *logic* is still platform/RMW-agnostic:
//! `register()` declares node + service server; `on_callback("handle_add")` reads the
//! typed request, sums the two ints, and writes the typed reply. The board
//! (`nros-board-threadx-qemu-riscv64`, `BoardEntry::run`) owns `nros::init`,
//! executor open, RMW registration, and the spin loop. RMW selection
//! (zenoh / cyclonedds) lives in `Cargo.toml [features]`; the locator + domain in
//! `[package.metadata.nros.deploy.threadx-qemu-riscv64]` — never here.

#![no_std]

mod app_main;

// Keep the board crate (panic handler + allocator + critical-section impl)
// linked into the standalone `staticlib` even on the zenoh/cargo path, where
// only `main.rs`'s `nros::main!()` names it (issue #205 — the per-example
// critical-section anchor moved into the board crate).

use example_interfaces::srv::{AddTwoInts, AddTwoIntsRequest, AddTwoIntsResponse};
use nros::{Callback, CallbackCtx, ExecutableNode, Node, NodeContext, NodeOptions, NodeResult};

pub struct AddTwoIntsServer;

impl Node for AddTwoIntsServer {
    const NAME: &'static str = "add_two_ints_server";

    fn register(ctx: &mut NodeContext<'_>) -> NodeResult<()> {
        let mut node = ctx.create_node(NodeOptions::new("add_two_ints_server"))?;
        let _srv = node.create_service_server_for_name_with_callback::<AddTwoInts>(
            "/add_two_ints",
            "handle_add",
        )?;
        log::info!("Waiting for service requests");
        Ok(())
    }
}

impl ExecutableNode for AddTwoIntsServer {
    type State = ();

    fn init() -> Self::State {}

    fn on_callback(_state: &mut Self::State, callback: Callback<'_>, ctx: &mut CallbackCtx<'_>) {
        if callback.as_str() == "handle_add" {
            if let Ok(req) = ctx.message::<AddTwoIntsRequest>() {
                log::info!("Incoming request");
                log::info!("a: {} b: {}", req.a, req.b);
                let reply = AddTwoIntsResponse { sum: req.a + req.b };
                let _ = ctx.reply::<AddTwoIntsResponse, 64>(&reply);
            }
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

// phase-359 W8.e / issue 0585 — the backend requires the heap; it does not
// silently enable it. Build as `--features rmw-cyclonedds,alloc`.
#[cfg(all(feature = "rmw-cyclonedds", not(any(feature = "alloc", feature = "std"))))]
compile_error!("`rmw-cyclonedds` allocates: add \"alloc\" (--features rmw-cyclonedds,alloc)");
