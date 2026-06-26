//! STM32F4 Listener Node pkg — RTIC flavor — Phase 216.B.5.
//!
//! Board-agnostic Node pkg consumed by the sibling Entry pkg
//! (`listener-rtic`) and wired onto an RTIC framework via
//! `[package.metadata.nros.entry] deploy = "rtic-stm32f4"` plus the
//! `nros::main!()` macro.
//!
//! ## Sibling to `listener_pkg` (Embassy)
//!
//! The Embassy-flavored `listener_pkg` (Phase 216.C.5) holds an
//! `Option<embassy_executor::Spawner>` on `ListenerState` for the
//! spawn-from-sync escape — Embassy's executor exposes runtime spawn
//! through a value handle. **RTIC has no equivalent value handle**:
//! task spawn is `task_name::spawn()` — a static module path emitted by
//! the `#[rtic::app]` macro at the call site. There is no spawner
//! object to thread through `Self::State::init()`, so this sibling pkg
//! drops the `spawner` field and lets `on_callback` invoke
//! `app::sidekick::spawn().ok()` directly (once the trampoline-
//! registration story lands and the example is rewired against a
//! concrete RTIC `mod app`). Keeping the Embassy `listener_pkg`
//! untouched preserves the spawn-from-sync demo the C.5 commit landed.
//!
//! ## Deferred dispatch + tag-based subscription
//!
//! Same shape as the Embassy sibling — `DISPATCH = Deferred` so the
//! board-side dispatch runtime (RTIC: an SPSC ring + dispatch task)
//! enqueues signaled subscription deliveries onto a framework-owned
//! task instead of the spin task. The subscription is registered via
//! [`NodeContext::create_subscription_static`](nros::NodeContext::create_subscription_static),
//! which returns a [`SubscriptionTag`] the Node author stores on
//! `Self::State` and matches against the `Callback<'_>` delivered
//! to [`ExecutableNode::on_callback`].
//!
//! ## Placeholder message
//!
//! Phase 216.B.5 ships the trait-shaped scaffolding only. The body
//! deserialises a 4-byte little-endian `i32` (the wire shape of
//! `std_msgs/Int32`), avoiding a dep on `std_msgs` (which would
//! require `nros ws sync` to materialise `generated/std_msgs/`
//! before cross-check). A follow-up that finishes the trampoline-
//! registration story swaps this for a typed `Int32` subscribe once
//! `generated/` ships.

#![no_std]

use nros::{
    Callback, CallbackCtx, DispatchStrategy, ExecutableNode, Node, NodeContext, NodeOptions,
    NodeResult, SubscriptionTag,
};

/// Listener component — subscribes to `/chatter` under the RTIC
/// framework. Phase 216.B.5 skeleton.
pub struct Listener;

/// Per-instance mutable state. Carries the [`SubscriptionTag`]
/// returned from registration; **no spawner handle** (RTIC's
/// task-spawn is a static `task_name::spawn()` call emitted by
/// `#[rtic::app]`, not a runtime value).
pub struct ListenerState {
    /// Tag returned from `create_subscription_static::<PlaceholderInt32>("/chatter")`.
    /// Macro-emitted init bodies use [`SubscriptionTag::placeholder`]
    /// as a sentinel; the real tag is bound at register time by a
    /// follow-up wave of Phase 216.B.
    pub sub_chatter: SubscriptionTag,
}

impl Node for Listener {
    const NAME: &'static str = "listener";

    /// Phase 216.B.5 — declares Deferred dispatch. The RTIC board
    /// crate's `NodeDispatchRuntime::dispatch_strategy()` returns
    /// `Deferred`; `nros check` (Phase 216.D.1) accepts the
    /// `(RTIC, Deferred)` matrix cell.
    const DISPATCH: DispatchStrategy = DispatchStrategy::Deferred;

    fn register(ctx: &mut NodeContext<'_>) -> NodeResult<()> {
        let mut node = ctx.create_node(NodeOptions::new("listener"))?;
        // Phase 216.B.5 — tag-based subscription. The topic literal
        // becomes both the stable entity ID and the callback ID; the
        // returned `SubscriptionTag` is what `on_callback` matches
        // against the delivered `Callback<'_>`.
        let _sub_chatter = node.create_subscription_static::<PlaceholderInt32>("/chatter")?;
        Ok(())
    }
}

impl ExecutableNode for Listener {
    type State = ListenerState;

    fn init() -> Self::State {
        // Phase 216.B.5 — `sub_chatter` uses `SubscriptionTag::
        // placeholder()` as the macro-emit sentinel; the real tag
        // (returned by `create_subscription_static` in `register`) is
        // bound at register time by a follow-up wave of Phase 216.B.
        ListenerState {
            sub_chatter: SubscriptionTag::placeholder(),
        }
    }

    fn on_callback(state: &mut Self::State, callback: Callback<'_>, ctx: &mut CallbackCtx<'_>) {
        if state.sub_chatter == callback {
            // 4-byte LE decode == wire shape of `std_msgs/Int32`
            // (CDR-PL, header omitted — placeholder; the RTIC
            // dispatch path is not yet hooked up, so this never
            // reaches the wire on a real flash).
            let payload = ctx.payload();
            let msg = decode_placeholder_int32(payload);
            defmt::info!("Received: {}", msg.data);

            // The RTIC equivalent of Embassy's spawn-from-sync escape
            // is a static `app::sidekick::spawn().ok();` call here
            // — emitted directly because RTIC has no runtime spawner
            // handle. The line is commented out for the skeleton
            // (the Entry pkg's `mod app` isn't visible from a
            // board-agnostic Node pkg); the trampoline-registration
            // follow-up wires it via a `dispatch_*` extern emitted
            // by the `#[rtic::app]` body.
        }
    }
}

/// Decode a [`PlaceholderInt32`] from a raw CDR payload. 4-byte LE
/// `i32` (header omitted in the placeholder; mirrors the
/// `talker_pkg` encode path).
fn decode_placeholder_int32(payload: &[u8]) -> PlaceholderInt32 {
    let mut bytes = [0u8; 4];
    let n = core::cmp::min(payload.len(), 4);
    bytes[..n].copy_from_slice(&payload[..n]);
    PlaceholderInt32 {
        data: i32::from_le_bytes(bytes),
    }
}

nros::node!(Listener);

// Phase 216.B.5 placeholder — minimal `RosMessage` stand-in so the
// declarative `create_subscription_static` call type-checks without
// dragging `std_msgs` (which is codegen-materialised under
// `generated/std_msgs/`) into this skeleton. The wire shape matches
// `std_msgs/Int32`. Follow-ups switch to the real type once
// `generated/` ships for this example.
#[derive(Copy, Clone)]
pub struct PlaceholderInt32 {
    pub data: i32,
}

impl nros::Serialize for PlaceholderInt32 {
    fn serialize(&self, _writer: &mut nros::CdrWriter) -> Result<(), nros::SerError> {
        Ok(())
    }
}

impl nros::Deserialize for PlaceholderInt32 {
    fn deserialize(_reader: &mut nros::CdrReader) -> Result<Self, nros::DeserError> {
        Ok(Self { data: 0 })
    }
}

impl nros::RosMessage for PlaceholderInt32 {
    const TYPE_NAME: &'static str = "std_msgs/msg/Int32";
    const TYPE_HASH: &'static str = "";
}
