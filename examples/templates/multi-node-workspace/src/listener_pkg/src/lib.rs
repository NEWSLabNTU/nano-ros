//! Listener Node pkg — subscribes to `std_msgs/Int32` on `/chatter`.
//!
//! Board-agnostic Node pkg. The sibling Entry pkg (`robot_entry`)
//! wires it onto a board via the `nros::main!(launch = "demo_bringup:...")`
//! macro, which emits one `listener_pkg::register(runtime)?;` call per
//! matching `<node>` entry in the launch file.
//!
//! `register()` declares the node + a subscription whose `on_message`
//! callback decodes the incoming payload. Like the talker, the message
//! type is a `PlaceholderInt32` (4-byte LE `i32`, the wire shape of
//! `std_msgs/Int32`) so the pkg compiles without `nros ws sync`
//! materialising `generated/std_msgs/`. Swap it for a typed `Int32`
//! once you run `nros generate-rust` for this pkg.

#![no_std]

use nros::{
    CallbackCtx, CallbackId, EntityId, ExecutableNode, Node, NodeContext, NodeId, NodeOptions,
    NodeResult,
};

/// Listener — counts the int32 messages seen on `/chatter`.
pub struct Listener;

impl Node for Listener {
    const NAME: &'static str = "listener";

    fn register(ctx: &mut NodeContext<'_>) -> NodeResult<()> {
        let mut node = ctx.create_node(NodeId::new("node"), NodeOptions::new("listener"))?;
        let _sub = node.create_subscription::<PlaceholderInt32>(
            EntityId::new("sub_chatter"),
            CallbackId::new("on_message"),
            "/chatter",
        )?;
        node.callback(CallbackId::new("on_message"))
            .reads(EntityId::new("sub_chatter"))?;
        Ok(())
    }
}

impl ExecutableNode for Listener {
    /// Count of messages received so far.
    type State = u32;

    fn init() -> Self::State {
        0
    }

    fn on_callback(state: &mut Self::State, callback: CallbackId<'_>, ctx: &mut CallbackCtx<'_>) {
        if callback.as_str() == "on_message" {
            // Raw CDR payload of the triggering message. A typed
            // build would `ctx.message::<Int32>()` here; the
            // placeholder just counts deliveries.
            let _payload = ctx.payload();
            *state = state.wrapping_add(1);
        }
    }
}

nros::node!(Listener);

// Placeholder `RosMessage` stand-in — wire shape matches
// `std_msgs/Int32`. See talker_pkg for the rationale.
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
