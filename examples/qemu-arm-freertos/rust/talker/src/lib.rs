//! FreeRTOS QEMU MPS2-AN385 Talker — Phase 212.L Component pkg.
//!
//! Publishes `std_msgs/Int32` on `/chatter` once per second.
//!
//! Component pkg shape: `register()` declares node + publisher + timer;
//! `ExecutableComponent::on_callback("on_tick")` runs the timer body
//! (bump counter, publish). The BSP-generated runtime (M.5.a.3+4 owns
//! `nros::init`, executor open, RMW registration, and the spin loop.
//! The user authors *only* the declarative + body bits.

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

/// Phase 212.N.7 step-2 — Entry-pkg-facing register wrapper.
///
/// **Current status: TODO stub.** The codegen-emitted `run_plan(runtime)`
/// body invokes `<pkg>::register(runtime)` once per launch-XML `<node>`
/// entry, passing `runtime: &mut nros_platform::RuntimeCtx<'_>`. The
/// trait surface in 212.N.1 (`RuntimeCtx`) exposes overlay knobs only
/// (params / remaps / env) — it does NOT carry a `ComponentRuntime`
/// (executor + metadata sink). The existing
/// `<Talker as Component>::register(ctx)` machinery wired by
/// `nros::component!(Talker)` therefore can't be driven from this
/// wrapper without first either:
///
/// 1. extending `RuntimeCtx` with a `&mut dyn ComponentRuntime` slot
///    populated by `BoardEntry::run` before the setup closure fires, or
/// 2. having the Entry pkg's `main.rs` build its own `Executor` +
///    `ComponentExecutorRuntime` and pass that through, bypassing
///    `RuntimeCtx`.
///
/// Both options touch 212.N.1 + the per-board `BoardEntry` impls; out
/// of scope for the 212.N.7 step-2 sweep. For now this is a no-op so
/// the Entry pkg compiles; the live registration path remains the
/// `nros::component!()`-emitted `nros_component_register` extern that
/// the FreeRTOS BSP baker discovers at link time.
///
/// The signature is generic over the runtime so this Component pkg
/// does not need to add a direct `nros-platform` dependency just for
/// the type name — `step-2` constraint kept `Cargo.toml` untouched.
/// The Entry pkg passes `&mut nros_platform::RuntimeCtx<'_>` which
/// monomorphises `R` accordingly.
pub fn register<R>(_runtime: &mut R) -> Result<(), &'static str> {
    // TODO(212.N.7 step-3+): wire to <Talker as Component>::register
    // once RuntimeCtx exposes a ComponentRuntime sink.
    Ok(())
}
