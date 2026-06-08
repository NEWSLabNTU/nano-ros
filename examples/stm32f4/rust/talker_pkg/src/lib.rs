//! STM32F4 Talker Node pkg — Phase 216.C.4.
//!
//! Board-agnostic Node pkg that the sibling Entry pkg
//! (`talker-embassy` for the Embassy framework, the forthcoming
//! `talker-rtic` migration scheduled under Phase 216.B.5 for the RTIC
//! framework) wires onto a board + framework via its
//! `[package.metadata.nros.entry] deploy = "<board>"` key plus the
//! `nros::main!()` macro.
//!
//! Node pkg shape (mirrors the qemu-arm-freertos / threadx-linux Node
//! pkgs): `register()` declares node + publisher + timer; the
//! `ExecutableNode::on_callback("on_tick")` body bumps a counter and
//! publishes raw CDR bytes through the `/chatter` topic. The Entry
//! pkg's macro-generated runtime owns `nros::init`, executor open, RMW
//! registration, and the spin / yield loop. Authors of this pkg touch
//! only the declarative + body bits.
//!
//! ## Placeholder publish
//!
//! Phase 216.C.4 ships the trait-shaped scaffolding only — the spec
//! locks "Node pkg can compile + cross-check; runtime publish requires
//! the integration work scheduled for follow-ups". The body emits raw
//! CDR for a 4-byte little-endian `i32` (the wire shape of
//! `std_msgs/Int32`), avoiding a dep on `std_msgs` (which would require
//! `nros ws sync` to materialise `generated/std_msgs/` before cross-
//! check). A follow-up that finishes the trampoline-registration story
//! swaps this for a typed `Int32` publish once `generated/` ships.

#![no_std]

use nros::{
    CallbackCtx, CallbackId, EntityId, ExecutableNode, Node, NodeContext, NodeOptions, NodeResult,
    TimerDuration,
};

/// Talker component — counter state + chatter publish on every tick.
pub struct Talker;

impl Node for Talker {
    const NAME: &'static str = "talker";

    fn register(ctx: &mut NodeContext<'_>) -> NodeResult<()> {
        let mut node = ctx.create_node(NodeOptions::new("talker"))?;
        // Phase 216.C.4 placeholder — declare the publisher with a
        // type_name string that matches `std_msgs/Int32`'s wire shape
        // without pulling the codegen-only `std_msgs` rlib in. A
        // 216.C follow-up swaps this for the real typed
        // `create_publisher::<Int32>(...)` call once the
        // trampoline-registration story lands.
        let _pub =
            node.create_publisher::<PlaceholderInt32>(EntityId::new("pub_chatter"), "/chatter")?;
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

impl ExecutableNode for Talker {
    /// Monotonic counter — the next int32 to publish.
    type State = i32;

    fn init() -> Self::State {
        0
    }

    fn on_callback(state: &mut Self::State, callback: CallbackId<'_>, ctx: &mut CallbackCtx<'_>) {
        if callback.as_str() == "on_tick" {
            // 4-byte LE encoding == wire shape of `std_msgs/Int32`
            // (CDR-PL, header omitted — placeholder; the Embassy
            // dispatch path is not yet hooked up, so this never
            // reaches the wire on a real flash).
            let bytes = state.to_le_bytes();
            let _ = ctx.publish_raw(EntityId::new("pub_chatter"), &bytes);
            *state = state.wrapping_add(1);
        }
    }
}

nros::node!(Talker);

// Phase 216.C.4 placeholder — minimal `RosMessage` stand-in so the
// declarative `create_publisher` call type-checks without dragging
// `std_msgs` (which is codegen-materialised under
// `generated/std_msgs/`) into this skeleton. The wire shape matches
// `std_msgs/Int32`. Follow-ups switch to the real type once
// `generated/` ships for this example.
struct PlaceholderInt32;

impl nros::Serialize for PlaceholderInt32 {
    fn serialize(&self, _writer: &mut nros::CdrWriter) -> Result<(), nros::SerError> {
        Ok(())
    }
}

impl nros::Deserialize for PlaceholderInt32 {
    fn deserialize(_reader: &mut nros::CdrReader) -> Result<Self, nros::DeserError> {
        Ok(Self)
    }
}

impl nros::RosMessage for PlaceholderInt32 {
    const TYPE_NAME: &'static str = "std_msgs/msg/Int32";
    const TYPE_HASH: &'static str = "";
}
