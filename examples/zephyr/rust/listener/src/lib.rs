//! Zephyr Listener — Phase 212.M.3 / Phase 212.L Component pkg.
//!
//! Subscribes to `std_msgs/Int32` on `/chatter` and tracks the last seen
//! value. The generated runtime — emitted by `nros codegen-system` via
//! the H.1 Zephyr adapter shim once L.7 self-pkg lands — owns init,
//! executor open, RMW registration, and the spin loop.

#![no_std]

use nros::{
    CallbackCtx, CallbackId, Component, ComponentContext, ComponentResult, EntityId,
    ExecutableComponent, NodeId, NodeOptions,
};
use std_msgs::msg::Int32;

pub struct Listener;

impl Component for Listener {
    const NAME: &'static str = "listener";

    fn register(ctx: &mut ComponentContext<'_>) -> ComponentResult<()> {
        let mut node = ctx.create_node(NodeId::new("node"), NodeOptions::new("listener"))?;
        let _sub = node.create_subscription::<Int32>(
            EntityId::new("sub_chatter"),
            CallbackId::new("on_chatter"),
            "/chatter",
        )?;
        Ok(())
    }
}

impl ExecutableComponent for Listener {
    /// Last value seen on `/chatter` (state shared across callback ticks).
    type State = i32;

    fn init() -> Self::State {
        0
    }

    fn on_callback(state: &mut Self::State, callback: CallbackId<'_>, ctx: &mut CallbackCtx<'_>) {
        if callback.as_str() == "on_chatter" {
            if let Ok(msg) = ctx.message::<Int32>() {
                *state = msg.data;
            }
        }
    }
}

nros::component!(Listener);

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
/// the executor + register the [`Listener`] component lands in a
/// follow-up step. For now the body is intentionally a no-op (the
/// existing `nros::component!(Listener)` macro still owns the symbol-
/// export path the M.5.b Zephyr baker consumes).
pub fn register(runtime: &mut nros_platform::RuntimeCtx<'_>) -> Result<(), &'static str> {
    let _ = runtime;
    Ok(())
}
