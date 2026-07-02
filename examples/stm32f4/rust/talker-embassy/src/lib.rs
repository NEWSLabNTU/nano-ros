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
//! `std_msgs/String`), avoiding a dep on `std_msgs` (which would require
//! `nros ws sync` to materialise `generated/std_msgs/` before cross-
//! check). A follow-up that finishes the trampoline-registration story
//! swaps this for a typed `String` publish once `generated/` ships.

#![no_std]

use nros::{
    Callback, CallbackCtx, ExecutableNode, Node, NodeContext, NodeOptions, NodeResult,
    TimerDuration,
};

/// Talker component — counter state + chatter publish on every tick.
pub struct Talker;

impl Node for Talker {
    const NAME: &'static str = "talker";

    fn register(ctx: &mut NodeContext<'_>) -> NodeResult<()> {
        let mut node = ctx.create_node(NodeOptions::new("talker"))?;
        // Phase 216.C.4 placeholder — declare the publisher with a
        // type_name string that matches `std_msgs/String`'s wire shape
        // without pulling the codegen-only `std_msgs` rlib in. A
        // 216.C follow-up swaps this for the real typed
        // `create_publisher::<String>(...)` call once the
        // trampoline-registration story lands.
        let pub_chatter = node.create_publisher_for_topic::<PlaceholderString>("/chatter")?;
        let _timer =
            node.create_timer_for_callback_name("on_tick", TimerDuration::from_millis(1000))?;
        node.callback_for_name("on_tick")
            .publishes_entity(&pub_chatter)?;
        Ok(())
    }
}

impl ExecutableNode for Talker {
    /// Monotonic counter — the next sequence number to publish.
    type State = i32;

    fn init() -> Self::State {
        0
    }

    fn on_callback(state: &mut Self::State, callback: Callback<'_>, ctx: &mut CallbackCtx<'_>) {
        if callback.as_str() == "on_tick" {
            // Official ROS 2 demo behavior (phase-277 W4): "Hello World: N".
            *state = state.wrapping_add(1);
            let msg = PlaceholderString::hello(*state);
            let _ = ctx.publish_to_topic::<PlaceholderString, 64>("/chatter", &msg);
            defmt::info!("Publishing: '{=str}'", msg.as_str());
        }
    }
}

nros::node!(Talker);

// Phase 216.C.4 placeholder — minimal `RosMessage` stand-in so the
// declarative `create_publisher` call type-checks without dragging
// `std_msgs` (which is codegen-materialised under
// `generated/std_msgs/`) into this skeleton. The wire shape matches
// `std_msgs/String` (phase-277 W4: the chatter payload is the official
// demo `Hello World: N`). Follow-ups switch to the real type once
// `generated/` ships for this example.
struct PlaceholderString {
    data: [u8; 32],
    len: usize,
}

impl PlaceholderString {
    /// Build the official demo payload (`Hello World: N`).
    fn hello(n: i32) -> Self {
        let mut msg = Self {
            data: [0; 32],
            len: 0,
        };
        let _ = core::fmt::Write::write_fmt(&mut msg, format_args!("Hello World: {n}"));
        msg
    }

    fn as_str(&self) -> &str {
        core::str::from_utf8(&self.data[..self.len]).unwrap_or("")
    }
}

impl core::fmt::Write for PlaceholderString {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        let bytes = s.as_bytes();
        let room = self.data.len() - self.len;
        let n = core::cmp::min(room, bytes.len());
        self.data[self.len..self.len + n].copy_from_slice(&bytes[..n]);
        self.len += n;
        Ok(())
    }
}

impl nros::Serialize for PlaceholderString {
    fn serialize(&self, writer: &mut nros::CdrWriter) -> Result<(), nros::SerError> {
        writer.write_string(self.as_str())?;
        Ok(())
    }
}

impl nros::Deserialize for PlaceholderString {
    fn deserialize(reader: &mut nros::CdrReader) -> Result<Self, nros::DeserError> {
        let s = reader.read_string()?;
        let mut msg = Self {
            data: [0; 32],
            len: 0,
        };
        let _ = core::fmt::Write::write_str(&mut msg, s);
        Ok(msg)
    }
}

impl nros::RosMessage for PlaceholderString {
    const TYPE_NAME: &'static str = "std_msgs/msg/String";
    const TYPE_HASH: &'static str = "";
}
