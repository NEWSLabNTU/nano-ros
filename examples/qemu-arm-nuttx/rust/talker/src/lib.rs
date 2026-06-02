//! NuttX QEMU ARM Talker — Phase 212.L Component pkg.
//!
//! Publishes `std_msgs/Int32` on `/chatter` once per second.
//!
//! Component pkg shape: `register()` declares node + publisher + timer;
//! `ExecutableComponent::on_callback("on_tick")` runs the timer body
//! (bump counter, publish). The generated runtime — emitted by
//! `nros codegen-system --pkg <this-dir>` via the H.2 NuttX adapter
//! shim — owns `nros::init`, executor open, RMW registration, and the
//! spin loop. The user authors *only* the declarative + body bits.

#![no_std]

use nros::{
    CallbackCtx, CallbackId, Component, ComponentContext, ComponentResult, EntityId,
    ExecutableComponent, NodeId, NodeOptions, TimerDuration,
};
use std_msgs::msg::Int32;

/// Talker component — counter state + chatter publish on every tick.
pub struct Talker;

impl Component for Talker {
    const NAME: &'static str = "talker";

    fn register(ctx: &mut ComponentContext<'_>) -> ComponentResult<()> {
        let mut node = ctx.create_node(NodeId::new("node"), NodeOptions::new("talker"))?;
        let _pub = node.create_publisher::<Int32>(EntityId::new("pub_chatter"), "/chatter")?;
        let _timer = node.create_timer(
            EntityId::new("timer_tick"),
            CallbackId::new("on_tick"),
            TimerDuration::from_millis(1000),
        )?;
        node.callback(CallbackId::new("on_tick"))
            .publishes(EntityId::new("pub_chatter"))?;
        Ok(())
    }
}

impl ExecutableComponent for Talker {
    /// Monotonic counter — the next int32 to publish.
    type State = i32;

    fn init() -> Self::State {
        0
    }

    fn on_callback(state: &mut Self::State, callback: CallbackId<'_>, ctx: &mut CallbackCtx<'_>) {
        if callback.as_str() == "on_tick" {
            let msg = Int32 { data: *state };
            let _ = ctx.publish::<Int32, 64>(EntityId::new("pub_chatter"), &msg);
            *state = state.wrapping_add(1);
        }
    }
}

nros::component!(Talker);

/// Phase 212.N.7 step-2 — codegen-facing `register` entry point.
///
/// The sibling Entry pkg's codegen-emitted `run_plan(runtime)` body
/// calls `<this-pkg>::register(runtime)?` once per launch-XML `<node>`
/// row that names this Component pkg. `runtime` is the
/// `nros_platform::RuntimeCtx<'_>` overlay (params / remaps / env
/// from the launch file or `--ros-args -p k:=v`).
///
/// ## Why generic over `R: ?Sized`
///
/// The Phase 212.N.7 step-2 contract says **touch only `src/lib.rs`**
/// of the Component pkg. Naming `nros_platform::RuntimeCtx` directly
/// would require adding `nros-platform` to `Cargo.toml`. The generic
/// `&mut R` defers that — the Entry pkg passes a
/// `&mut nros_platform::RuntimeCtx<'_>` and `R` monomorphises to that
/// type. Step-3+ can tighten the signature (and add the dep) once
/// the runtime plumbing lands.
///
/// ## Body is a no-op (TODO step-3+)
///
/// The 212.N runtime plumbing that lets this function reach into the
/// executor + register the [`Talker`] component (i.e. close the
/// `ComponentContext` ↔ `RuntimeCtx` gap — see
/// `tmp/wave4/N.7.qemu-arm-freertos.md` "Design choices") is
/// follow-up work. The macro-emitted `nros::component!(Talker)`
/// symbol still owns the live registration path the NuttX M.5.a
/// baker discovers at link time, so the Component pkg's behaviour is
/// unchanged.
pub fn register<R: ?Sized>(_runtime: &mut R) -> Result<(), &'static str> {
    Ok(())
}
