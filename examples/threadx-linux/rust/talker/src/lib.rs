//! ThreadX Linux Talker — Phase 212.L Component pkg.
//!
//! Publishes `std_msgs/Int32` on `/chatter` once per second.
//!
//! Component pkg shape: `register()` declares node + publisher + timer;
//! `ExecutableComponent::on_callback("on_tick")` runs the timer body
//! (bump counter, publish). The generated runtime — emitted by
//! `nros codegen-system` via the H.4 ThreadX adapter shim — owns
//! `nros::init`, executor open, RMW registration, and the spin loop.

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

/// Phase 212.N.7 step-2 — Entry-pkg-callable wrapper.
///
/// The codegen-emitted `run_plan(runtime)` body (see
/// `nros-build::generate_run_plan`, §212.N.4) dispatches one
/// `<pkg>::register(runtime)?` call per launch-XML `<node>` entry.
/// This wrapper is the stable per-Component-pkg API the Entry pkg
/// links against — board-agnostic, no `nros::init` / executor /
/// spin (those live in `BoardEntry::run`).
///
/// Today the wrapper is a stub: the per-component declarative
/// registration (publisher / subscription / timer / service /
/// action) still flows through the `nros::component!`-emitted
/// trampoline that `Executor::add_components` invokes after
/// `BoardEntry::run` opens the executor. Once the §212.N.4
/// codegen lands the full `RuntimeCtx`-aware launch overlay
/// (param / remap / env application), this body will bridge
/// `runtime` into the component's `ComponentContext` before
/// dispatch. For now we accept the `runtime` arg, hand it to
/// `_`, and return `Ok(())` so the Entry pkg `main.rs` reaches
/// `Executor::spin`.
pub fn register(runtime: &mut nros_platform::RuntimeCtx<'_>) -> Result<(), &'static str> {
    let _ = runtime;
    Ok(())
}
