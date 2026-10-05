//! ReliableTalker Node pkg — publishes `std_msgs/Int32` on `/qos_chatter` with
//! an EXPLICIT, non-default QoS profile (phase-263 B4).
//!
//! The nano-ros QoS differentiator in the declarative shape: instead of
//! `create_publisher_for_topic` (which takes `QoSProfile::default()` =
//! reliable + volatile + keep-last), this declares the publisher via
//! `create_publisher_for_topic_with_qos` with `reliable() + transient_local() +
//! depth(1)`. TRANSIENT_LOCAL durability is the visible behaviour: a
//! late-joining subscriber with matching QoS still receives the last sample
//! published before it joined. The matching `rust_qos_listener_pkg` subscribes with
//! the same profile (QoS must match for the endpoints to connect).

#![no_std]

use nros::{
    Callback, CallbackCtx, ExecutableNode, Node, NodeContext, NodeOptions, NodeResult, QoSProfile,
    TimerDuration,
};
use std_msgs::msg::Int32;

/// The shared QoS contract both endpoints declare. RELIABLE delivery,
/// TRANSIENT_LOCAL durability (the broker holds history for late joiners),
/// KEEP_LAST(1) history depth.
///
/// Depth 1 because it is what the zenoh backend SERVES, and a stock peer reads
/// the served profile (issue 1687): a transient-local publisher retains one
/// sample (`TL_RETAIN_DEPTH`), so KEEP_LAST(10) was granted down to 1 and
/// `ros2 topic info -v` showed 1 — a declaration the image did not keep. It is
/// still distinct from the default (KEEP_LAST(10), VOLATILE) in two policies.
pub fn qos_profile() -> QoSProfile {
    QoSProfile::default().reliable().transient_local().depth(1)
}

/// Reliable talker — monotonic counter published on `/qos_chatter` at 1 Hz.
pub struct ReliableTalker;

impl Node for ReliableTalker {
    const NAME: &'static str = "reliable_talker";

    // issue 0857 — the cell registries this class fills, exactly: (publishers,
    // service servers, service clients, action clients, action servers). Undeclared
    // means `NROS_RUNTIME_MAX_CELL_ENTITIES` per kind, in `.bss`, twice over.
    const ENTITY_BOUNDS: nros::EntityBounds = nros::EntityBounds::exact(1, 0, 0, 0, 0);

    fn register(ctx: &mut NodeContext<'_>) -> NodeResult<()> {
        let mut node = ctx.create_node(NodeOptions::new("reliable_talker"))?;
        let pub_chatter =
            node.create_publisher_for_topic_with_qos::<Int32>("/qos_chatter", qos_profile())?;
        let _timer =
            node.create_timer_for_callback_name("on_tick", TimerDuration::from_millis(1000))?;
        node.callback_for_name("on_tick")
            .publishes_entity(&pub_chatter)?;
        Ok(())
    }
}

impl ExecutableNode for ReliableTalker {
    /// Monotonic counter — the next int32 to publish.
    type State = i32;

    fn init() -> Self::State {
        0
    }

    fn on_callback(state: &mut Self::State, callback: Callback<'_>, ctx: &mut CallbackCtx<'_>) {
        if callback.as_str() == "on_tick" {
            let msg = Int32 { data: *state };
            let _ = ctx.publish_to_topic::<Int32, 8>("/qos_chatter", &msg);
            *state = state.wrapping_add(1);
        }
    }
}

nros::node!(ReliableTalker);
