//! Talker Node pkg — publishes `std_msgs/Int32` on `/chatter`.
//!
//! Board-agnostic Node pkg. The sibling Entry pkg (`robot_entry`)
//! wires it onto a board via `[package.metadata.nros.entry]` + the
//! `nros::main!(launch = "demo_bringup:...")` macro, which emits one
//! `talker_pkg::register(runtime)?;` call per `<node>` entry in the
//! launch file.
//!
//! `register()` declares the node + a 1 Hz publisher + timer; the
//! `ExecutableNode::on_callback("on_tick")` body bumps a counter and
//! publishes raw CDR bytes. The Entry pkg's macro-generated runtime
//! owns `nros::init`, executor open, RMW registration, and the spin
//! loop.
//!
//! ## Placeholder message
//!
//! The body emits raw CDR for a 4-byte little-endian `i32` (the wire
//! shape of `std_msgs/Int32`) via a tiny `PlaceholderInt32`, avoiding
//! a dep on `std_msgs` (which would need `nros ws sync` to materialise
//! `generated/std_msgs/`). Swap it for a typed `Int32` publish once
//! you run `nros generate-rust` for this pkg.

#![no_std]

use nros::{
    CallbackCtx, CallbackId, EntityId, ExecutableNode, Node, NodeContext, NodeId, NodeOptions,
    NodeResult, TimerDuration,
};

/// Talker — counter state + chatter publish on every tick.
pub struct Talker;

impl Node for Talker {
    const NAME: &'static str = "talker";

    fn register(ctx: &mut NodeContext<'_>) -> NodeResult<()> {
        let mut node = ctx.create_node(NodeId::new("node"), NodeOptions::new("talker"))?;
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
            // 4-byte LE encoding == wire shape of `std_msgs/Int32`.
            let bytes = state.to_le_bytes();
            let _ = ctx.publish_raw(EntityId::new("pub_chatter"), &bytes);
            *state = state.wrapping_add(1);
        }
    }
}

nros::node!(Talker);

// Placeholder `RosMessage` stand-in so `create_publisher` type-checks
// without dragging `std_msgs` (codegen-materialised under
// `generated/std_msgs/`) into the template. Wire shape matches
// `std_msgs/Int32`.
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
