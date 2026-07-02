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
//! framework-owned queue; RTIC: an SPSC ring) enqueues signaled
//! subscription deliveries onto a framework-owned task instead of
//! the spin task. The subscription is registered via the tag-shaped
//! [`NodeContext::create_subscription_static`](nros::NodeContext::create_subscription_static)
//! helper landed in 216.A.4-followup, which returns a
//! [`SubscriptionTag`] the Node author stores on `Self::State` and
//! matches against the `Callback<'_>` delivered to
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
//! `std_msgs/String`), avoiding a dep on `std_msgs` (which would
//! require `nros ws sync` to materialise `generated/std_msgs/`
//! before cross-check). A follow-up that finishes the trampoline-
//! registration story swaps this for a typed `String` subscribe once
//! `generated/` ships.

#![no_std]

use embassy_executor::Spawner;
use nros::{
    Callback, CallbackCtx, DispatchStrategy, ExecutableNode, Node, NodeContext, NodeOptions,
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
    /// Tag returned from `create_subscription_static::<PlaceholderString>("/chatter")`.
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
        // against the delivered `Callback<'_>`. See the module
        // doc for the Deferred dispatch rationale.
        let _sub_chatter = node.create_subscription_static::<PlaceholderString>("/chatter")?;
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

    fn on_callback(state: &mut Self::State, callback: Callback<'_>, ctx: &mut CallbackCtx<'_>) {
        if state.sub_chatter == callback {
            // CDR string decode == wire shape of `std_msgs/String`
            // (CDR-PL, header omitted — placeholder; the Embassy
            // dispatch path is not yet hooked up, so this never
            // reaches the wire on a real flash).
            let payload = ctx.payload();
            let msg = decode_placeholder_string(payload);

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

/// Decode a [`PlaceholderString`] from a raw CDR payload: u32 LE
/// length (includes the NUL) + bytes (header omitted in the
/// placeholder; mirrors the placeholder encode path).
fn decode_placeholder_string(payload: &[u8]) -> PlaceholderString {
    let mut msg = PlaceholderString {
        data: [0; 32],
        len: 0,
    };
    if payload.len() >= 4 {
        let strlen = u32::from_le_bytes([payload[0], payload[1], payload[2], payload[3]]) as usize;
        let text = strlen.saturating_sub(1); // drop the trailing NUL
        let avail = payload.len() - 4;
        let n = text.min(avail).min(msg.data.len());
        msg.data[..n].copy_from_slice(&payload[4..4 + n]);
        msg.len = n;
    }
    msg
}

/// Async sidekick driven by the spawn-from-sync escape. The body is
/// a stub today (`defmt::info!` log only); a real listener would do
/// I/O, a publish that yields, etc.
#[embassy_executor::task]
async fn handle_downstream(msg: PlaceholderString) {
    defmt::info!("I heard: [{=str}]", msg.as_str());
    // Real bodies put `.await`-shaped work here, e.g.
    // `embassy_time::Timer::after_secs(1).await;`.
}

nros::node!(Listener);

// Phase 216.C.5 placeholder — minimal `RosMessage` stand-in so the
// declarative `create_subscription_static` call type-checks without
// dragging `std_msgs` (which is codegen-materialised under
// `generated/std_msgs/`) into this skeleton. The wire shape matches
// `std_msgs/String` (phase-277 W4: the chatter payload is the official
// demo `Hello World: N`). Follow-ups switch to the real type once
// `generated/` ships for this example.
#[derive(Copy, Clone)]
pub struct PlaceholderString {
    pub data: [u8; 32],
    pub len: usize,
}

impl PlaceholderString {
    pub fn as_str(&self) -> &str {
        core::str::from_utf8(&self.data[..self.len]).unwrap_or("")
    }
}

impl nros::Serialize for PlaceholderString {
    fn serialize(&self, _writer: &mut nros::CdrWriter) -> Result<(), nros::SerError> {
        Ok(())
    }
}

impl nros::Deserialize for PlaceholderString {
    fn deserialize(_reader: &mut nros::CdrReader) -> Result<Self, nros::DeserError> {
        Ok(Self {
            data: [0; 32],
            len: 0,
        })
    }
}

impl nros::RosMessage for PlaceholderString {
    const TYPE_NAME: &'static str = "std_msgs/msg/String";
    const TYPE_HASH: &'static str = "";
}
