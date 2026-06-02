//! Zephyr Fibonacci action client — Phase 212.M.3 / Phase 212.L Component pkg.
//!
//! Declarative: node + action client. Like the service-client variant
//! the imperative goal-sending sequence has no `TickCtx` seam yet —
//! action-client send_goal + feedback-stream wiring is a follow-up
//! wave for the generated runtime. The declarative metadata above is
//! the stable contract.

#![no_std]

use example_interfaces::action::Fibonacci;
use nros::{
    CallbackCtx, CallbackId, Component, ComponentContext, ComponentResult, EntityId,
    ExecutableComponent, NodeId, NodeOptions,
};

pub struct FibonacciClient;

impl Component for FibonacciClient {
    const NAME: &'static str = "fibonacci_action_client";

    fn register(ctx: &mut ComponentContext<'_>) -> ComponentResult<()> {
        let mut node = ctx.create_node(
            NodeId::new("node"),
            NodeOptions::new("fibonacci_action_client"),
        )?;
        let _client =
            node.create_action_client::<Fibonacci>(EntityId::new("client_fib"), "/fibonacci")?;
        Ok(())
    }
}

impl ExecutableComponent for FibonacciClient {
    type State = ();

    fn init() -> Self::State {}

    fn on_callback(
        _state: &mut Self::State,
        _callback: CallbackId<'_>,
        _ctx: &mut CallbackCtx<'_>,
    ) {
        // Action-client send_goal / feedback-stream wiring is a follow-up
        // runtime wave; codegen-system will own the imperative driver
        // once the seam ships.
    }
}

nros::component!(FibonacciClient);

/// Phase 212.N.7 step-2 — codegen-facing `register` entry point.
///
/// Zephyr is the §212.N.2 carve-out: `nros-board-zephyr` is
/// `NetworkWait`-only, and Kconfig + DTS own the C `main()` boot
/// path (a Rust staticlib can't take over `main` on Zephyr). There
/// is therefore **no Entry pkg sibling** for Zephyr Component pkgs;
/// the existing `zephyr.exe`-from-`west build` shape stays.
///
/// This wrapper exists so a future Zephyr-side codegen layer can
/// call `<this-pkg>::register(runtime)?` from inside the C
/// `main()`'s `nros_app_rust_entry` hook — the same stable surface
/// signature as the other §212.N.7 Component pkgs, just driven from
/// C rather than a Rust Entry pkg.
///
/// The 212.N runtime plumbing that lets this function reach into
/// the executor + register the [`FibonacciClient`] component lands
/// in a follow-up step. For now the body is intentionally a no-op
/// (the existing `nros::component!(FibonacciClient)` macro still
/// owns the symbol-export path the M.5.b Zephyr baker consumes).
pub fn register(runtime: &mut nros_platform::RuntimeCtx<'_>) -> Result<(), &'static str> {
    let _ = runtime;
    Ok(())
}
