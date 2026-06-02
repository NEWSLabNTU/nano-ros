//! Zephyr AddTwoInts service client — Phase 212.M.3 / Phase 212.L Component pkg.
//!
//! Declarative metadata: node + service client + driver timer. The
//! component model expresses *what* entities exist; the imperative call
//! sequencing (issue request → await reply) currently has no `TickCtx`
//! seam — service-client invocation is a follow-up wave for the
//! generated runtime. For now the body is a declarative no-op; codegen-
//! system will own the call-loop once that seam lands.

#![no_std]

use example_interfaces::srv::AddTwoInts;
use nros::{
    CallbackCtx, CallbackId, Component, ComponentContext, ComponentResult, EntityId,
    ExecutableComponent, NodeId, NodeOptions, TimerDuration,
};

pub struct AddTwoIntsClient;

impl Component for AddTwoIntsClient {
    const NAME: &'static str = "add_two_ints_client";

    fn register(ctx: &mut ComponentContext<'_>) -> ComponentResult<()> {
        let mut node =
            ctx.create_node(NodeId::new("node"), NodeOptions::new("add_two_ints_client"))?;
        let _client =
            node.create_service_client::<AddTwoInts>(EntityId::new("client_add"), "/add_two_ints")?;
        let _timer = node.create_timer(
            EntityId::new("timer_call"),
            CallbackId::new("issue_call"),
            TimerDuration::from_secs(1),
        )?;
        Ok(())
    }
}

impl ExecutableComponent for AddTwoIntsClient {
    /// Index into the canned test-case table for the next call.
    type State = u8;

    fn init() -> Self::State {
        0
    }

    fn on_callback(
        _state: &mut Self::State,
        _callback: CallbackId<'_>,
        _ctx: &mut CallbackCtx<'_>,
    ) {
        // The runtime-side service-client call seam is a follow-up wave
        // (TickCtx today exposes publish + action ops only). Codegen-
        // system will wire the imperative call loop here once the seam
        // ships; the declarative metadata above is the stable contract.
    }
}

nros::component!(AddTwoIntsClient);

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
/// the executor + register the [`AddTwoIntsClient`] component lands
/// in a follow-up step. For now the body is intentionally a no-op
/// (the existing `nros::component!(AddTwoIntsClient)` macro still
/// owns the symbol-export path the M.5.b Zephyr baker consumes).
pub fn register(runtime: &mut nros_platform::RuntimeCtx<'_>) -> Result<(), &'static str> {
    let _ = runtime;
    Ok(())
}
