//! STM32F4 Listener Node pkg — Phase 216.C.5.
//!
//! Board-agnostic Node pkg that the sibling Entry pkg
//! (`listener-embassy` for the Embassy framework, the forthcoming
//! `listener-rtic` migration scheduled under Phase 216.B.5 for the
//! RTIC framework) wires onto a board + framework via its
//! `[package.metadata.nros.entry] deploy = "<board>"` key plus the
//! `nros::main!()` macro.
//!
//! ## Deferred dispatch + tag-based subscription
//!
//! Unlike the sibling `talker_pkg` (which keeps the default
//! [`DispatchStrategy::Inline`]), this pkg declares
//!
//! ```ignore
//! const DISPATCH: DispatchStrategy = DispatchStrategy::Deferred;
//! ```
//!
//! so the board-side dispatch runtime (Embassy: a
//! `Channel<CallbackId>`; RTIC: an SPSC ring) enqueues signaled
//! subscription deliveries onto a framework-owned task instead of
//! the spin task. The subscription is registered via the tag-shaped
//! [`NodeContext::create_subscription_static`](nros::NodeContext::create_subscription_static)
//! helper landed in 216.A.4-followup, which returns a
//! [`SubscriptionTag`] the Node author stores on `Self::State` and
//! matches against the `&CallbackId<'_>` delivered to
//! [`ExecutableNode::on_callback`].
//!
//! ## Spawn-from-sync escape
//!
//! `on_callback` is a sync function: callbacks fire from the
//! framework's deferred-dispatch task (still a sync stack frame,
//! just not the spin task). To run `async` work in response to a
//! subscription delivery, the body uses the **spawn-from-sync
//! escape**: it calls `state.spawner.spawn(handle_downstream(msg))`
//! which enqueues a fresh `#[embassy_executor::task]` onto the
//! Embassy executor. The async sidekick task ([`handle_downstream`])
//! is where `.await`-shaped work (a timer wait, an I2C transaction,
//! a publish that yields, …) lives.
//!
//! ## Spawner plumbing — TODO
//!
//! [`ExecutableNode::init`] has no arguments, but the
//! [`embassy_executor::Spawner`] only exists once
//! `EmbassyBoardEntry::init_hardware(spawner)` runs (Phase 216.C.2).
//! Threading the spawner from `init_hardware` into `Self::State` is
//! part of the trampoline-registration story that lands in a follow-
//! up wave of Phase 216.C. For this commit, `init()` parks
//! `spawner: None` and the on_callback body documents the escape
//! shape in a `if let Some(spawner) = state.spawner.as_ref()`
//! branch. A flash today will simply skip the spawn (the dispatch
//! runtime still cross-checks); once the plumbing lands, the same
//! body fires the async task without source change.
//!
//! ## Placeholder message
//!
//! Phase 216.C.5 ships the trait-shaped scaffolding only. The body
//! deserialises a 4-byte little-endian `i32` (the wire shape of
//! `std_msgs/Int32`), avoiding a dep on `std_msgs` (which would
//! require `nros ws sync` to materialise `generated/std_msgs/`
//! before cross-check). A follow-up that finishes the trampoline-
//! registration story swaps this for a typed `Int32` subscribe once
//! `generated/` ships.

#![no_std]

use embassy_executor::Spawner;
use nros::{
    CallbackCtx, CallbackId, DispatchStrategy, ExecutableNode, Node, NodeContext, NodeOptions,
    NodeResult, SubscriptionTag,
};

/// Listener component — subscribes to `/chatter` and (eventually)
/// spawns an `async` sidekick per delivery via the spawn-from-sync
/// escape. Phase 216.C.5 skeleton.
pub struct Listener;

/// Per-instance mutable state. Holds the [`SubscriptionTag`] returned
/// from registration (used in `on_callback` to match incoming
/// callbacks) and an optional [`Spawner`] handle for the
/// spawn-from-sync escape. See the module doc for the Spawner
/// plumbing TODO.
pub struct ListenerState {
    /// Tag returned from `create_subscription_static::<PlaceholderInt32>("/chatter")`.
    /// Macro-emitted init bodies use [`SubscriptionTag::placeholder`]
    /// as a sentinel; the real tag is bound at register time by a
    /// follow-up wave of Phase 216.C.
    pub sub_chatter: SubscriptionTag,
    /// Embassy executor handle for the spawn-from-sync escape.
    /// `None` for the C.5 skeleton; a follow-up threads the live
    /// `Spawner` through from `EmbassyBoardEntry::init_hardware`.
    pub spawner: Option<Spawner>,
}

impl Node for Listener {
    const NAME: &'static str = "listener";

    /// Phase 216.C.5 — declares Deferred dispatch. The Embassy board
    /// crate's `NodeDispatchRuntime::dispatch_strategy()` returns
    /// `Deferred`; `nros check` (Phase 216.D.1) accepts the
    /// `(Embassy, Deferred)` matrix cell.
    const DISPATCH: DispatchStrategy = DispatchStrategy::Deferred;

    fn register(ctx: &mut NodeContext<'_>) -> NodeResult<()> {
        let mut node = ctx.create_node(NodeOptions::new("listener"))?;
        // Phase 216.C.5 — tag-based subscription. The topic literal
        // becomes both the stable entity ID and the callback ID; the
        // returned `SubscriptionTag` is what `on_callback` matches
        // against the delivered `&CallbackId<'_>`. See the module
        // doc for the Deferred dispatch rationale.
        let _sub_chatter = node.create_subscription_static::<PlaceholderInt32>("/chatter")?;
        Ok(())
    }
}

impl ExecutableNode for Listener {
    type State = ListenerState;

    fn init() -> Self::State {
        // Phase 216.C.5 — Spawner plumbing TODO. `init()` has no
        // args, so the live `embassy_executor::Spawner` (only
        // available after `EmbassyBoardEntry::init_hardware(spawner)`
        // runs) cannot be threaded here yet. A follow-up wave of
        // Phase 216.C extends the trampoline-registration story to
        // pass the spawner into `Self::State::init(spawner)` (or an
        // equivalent shape). For now, `None` keeps the escape path
        // a documented no-op; the body below is shape-correct so
        // the eventual plumbing is a one-field swap.
        //
        // The `sub_chatter` tag uses `SubscriptionTag::placeholder()`
        // as the macro-emit sentinel; the real tag (returned by
        // `create_subscription_static` in `register`) is bound at
        // register time by the same follow-up wave.
        ListenerState {
            sub_chatter: SubscriptionTag::placeholder(),
            spawner: None,
        }
    }

    fn on_callback(state: &mut Self::State, callback: CallbackId<'_>, ctx: &mut CallbackCtx<'_>) {
        if state.sub_chatter == callback {
            // 4-byte LE decode == wire shape of `std_msgs/Int32`
            // (CDR-PL, header omitted — placeholder; the Embassy
            // dispatch path is not yet hooked up, so this never
            // reaches the wire on a real flash).
            let payload = ctx.payload();
            let msg = decode_placeholder_int32(payload);

            // Spawn-from-sync escape: enqueue async work. `on_callback`
            // is a sync stack frame (the framework's deferred-
            // dispatch task drives it synchronously); to do `.await`-
            // shaped work in response to a delivery, hand it off to
            // a fresh `#[embassy_executor::task]` via the Spawner.
            //
            // The `if let Some(...)` is the Spawner-plumbing TODO
            // guard (see module doc); once the spawner is threaded
            // through, this becomes an unconditional `.spawn(...)
            // .unwrap()`.
            if let Some(spawner) = state.spawner.as_ref() {
                let _ = spawner.spawn(handle_downstream(msg));
            }
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

/// Async sidekick driven by the spawn-from-sync escape. The body is
/// a stub today (`defmt::info!` log only); a real listener would do
/// I/O, a publish that yields, etc.
#[embassy_executor::task]
async fn handle_downstream(msg: PlaceholderInt32) {
    defmt::info!("Received: {}", msg.data);
    // Real bodies put `.await`-shaped work here, e.g.
    // `embassy_time::Timer::after_secs(1).await;`.
}

nros::node!(Listener);

// Phase 216.C.5 placeholder — minimal `RosMessage` stand-in so the
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
