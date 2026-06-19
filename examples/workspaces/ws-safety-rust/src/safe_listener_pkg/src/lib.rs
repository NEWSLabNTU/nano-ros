//! SafeListener Node pkg — E2E message-integrity (CRC) subscriber on `/chatter`.
//!
//! Declares a SAFETY subscription via
//! `create_subscription_for_callback_name_with_safety` (ungated — works with or
//! without the `safety-e2e` build feature; off ⇒ a basic subscription). When the
//! system declares `[system].features = ["safety"]`, the zenoh backend attaches a
//! CRC + sequence number on publish, the runtime validates it on receive, and the
//! callback reads the per-message `CallbackCtx::integrity()` — CRC ok, sequence
//! gap, or duplicate — alongside the payload. The first WORKSPACE example of the
//! E2E-safety differentiator (phase-263 B1; the protocol itself is RFC-0028).

#![no_std]

use nros::{Callback, CallbackCtx, ExecutableNode, Node, NodeContext, NodeOptions, NodeResult};
use std_msgs::msg::Int32;

/// SafeListener — counts received messages + (when built with `safety-e2e`) CRC
/// failures / sequence gaps seen.
pub struct SafeListener;

#[derive(Default)]
pub struct Counts {
    received: u32,
    integrity_faults: u32,
}

impl Node for SafeListener {
    const NAME: &'static str = "safe_listener";

    fn register(ctx: &mut NodeContext<'_>) -> NodeResult<()> {
        let mut node = ctx.create_node(NodeOptions::new("safe_listener"))?;
        let _sub = node
            .create_subscription_for_callback_name_with_safety::<Int32>("on_chatter", "/chatter")?;
        Ok(())
    }
}

impl ExecutableNode for SafeListener {
    type State = Counts;

    fn init() -> Self::State {
        Counts::default()
    }

    fn on_callback(state: &mut Self::State, callback: Callback<'_>, ctx: &mut CallbackCtx<'_>) {
        if callback.as_str() == "on_chatter" {
            if ctx.message::<Int32>().is_ok() {
                state.received = state.received.wrapping_add(1);
            }
            // The integrity status describes the message just received; present
            // only when the safety axis is compiled in (the `.safety()` opt-in +
            // the `safety-e2e` build feature). A non-ok CRC / sequence gap / dup
            // bumps the fault counter.
            #[cfg(feature = "safety-e2e")]
            if let Some(status) = ctx.integrity() {
                if !status.is_valid() {
                    state.integrity_faults = state.integrity_faults.wrapping_add(1);
                }
            }
        }
    }
}

nros::node!(SafeListener);
