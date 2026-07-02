//! STM32F4 Action Server Node pkg.
//!
//! Board-agnostic Node pkg that the sibling Entry pkg
//! (`action-server-rtic` for the RTIC framework; a parallel
//! `action-server-embassy` migration will follow once the C-wave
//! catches up) wires onto a board + framework via its
//! `[package.metadata.nros.entry] deploy = "<board>"` key plus the
//! `nros::main!()` macro.
//!
//! ## Deferred dispatch + tag-based action server
//!
//! Like the sibling `listener_pkg`, this pkg declares
//!
//! ```ignore
//! const DISPATCH: DispatchStrategy = DispatchStrategy::Deferred;
//! ```
//!
//! because action callbacks (goal / cancel / accepted) are exactly
//! the Deferred-dispatch use case — the body wants to do async-shaped
//! work (publish feedback in a loop, await a long-running goal)
//! without blocking the spin task. The action server is registered
//! via the tag-shaped
//! [`NodeContext::create_action_static`](nros::NodeContext::create_action_static)
//! helper, which returns an
//! [`ActionTag`] the Node author stores on `Self::State` and matches
//! against the `Callback<'_>` delivered to
//! [`ExecutableNode::on_callback`]. Note: the action variant fans the
//! synthesized callback ID across the goal, cancel, and accepted
//! slots — `state.act_fibonacci == cb` matches all three.
//!
//! ## RTIC-side dispatch — TODO
//!
//! The RTIC Entry pkg's macro emit hands Deferred dispatch off via an
//! `rtic::Mutex`-guarded SPSC ring (not Embassy's `Spawner`), so the
//! escape from `on_callback` to async-shaped work is the RTIC
//! `task::spawn()` API exposed via the board crate's runtime handle.
//! That handle's type only exists in the Entry pkg's `#[rtic::app]`
//! scope, so for now the placeholder body stores `()` and a follow-up
//! wave threads the handle through.
//!
//! ## Placeholder action
//!
//! This example ships the trait-shaped scaffolding only. The action
//! type is a local `PlaceholderAct: RosAction` with
//! Goal/Result/Feedback all aliased to a 4-byte little-endian `i32`
//! shape (mirrors `example_interfaces/action/Fibonacci`'s `order`
//! field width), avoiding deps on `example_interfaces` +
//! `action_msgs` + `unique_identifier_msgs` + `builtin_interfaces`
//! (which would require `nros ws sync` to materialise their
//! `generated/` trees before cross-check). A follow-up that finishes
//! the trampoline-registration story swaps this for the real
//! `Fibonacci` action once `generated/` ships.

#![no_std]

use nros::{
    ActionTag, Callback, CallbackCtx, CdrReader, CdrWriter, DeserError, Deserialize,
    DispatchStrategy, ExecutableNode, Node, NodeContext, NodeOptions, NodeResult, RosAction,
    RosMessage, SerError, Serialize,
};

/// Action server component — accepts Fibonacci-shaped goals and
/// (eventually) publishes feedback per iteration. Skeleton.
pub struct ActionServer;

/// Per-instance mutable state. Holds the [`ActionTag`] returned from
/// registration (used in `on_callback` to match incoming callbacks
/// against the goal / cancel / accepted slots, all three of which
/// fan out to the same synthesized callback ID per
/// `create_action_static`). See the module doc for the RTIC-side
/// dispatch-handle plumbing TODO.
pub struct ActionServerState {
    /// Tag returned from `create_action_static::<PlaceholderAct>("/fibonacci")`.
    /// Macro-emitted init bodies use [`ActionTag::placeholder`]
    /// as a sentinel; the real tag is bound at register time by a
    /// follow-up wave.
    pub act_fibonacci: ActionTag,
}

impl Node for ActionServer {
    const NAME: &'static str = "fibonacci_action_server";

    /// Declares Deferred dispatch. Action callbacks are exactly the
    /// Deferred-dispatch use case; the RTIC board crate's
    /// `NodeDispatchRuntime::dispatch_strategy()` returns `Deferred`;
    /// `nros check` accepts the `(RTIC, Deferred)` matrix cell.
    const DISPATCH: DispatchStrategy = DispatchStrategy::Deferred;

    fn register(ctx: &mut NodeContext<'_>) -> NodeResult<()> {
        let mut node = ctx.create_node(NodeOptions::new("fibonacci_action_server"))?;
        // Tag-based action server. The action-name literal becomes
        // both the stable entity ID and the (single, fanned-out)
        // callback ID; the returned `ActionTag` is what `on_callback`
        // matches against the delivered `Callback<'_>` for goal,
        // cancel, and accepted deliveries. See the module doc for the
        // Deferred dispatch rationale.
        let _act_fibonacci = node.create_action_static::<PlaceholderAct>("/fibonacci")?;
        defmt::info!("Waiting for action goals...");
        Ok(())
    }
}

impl ExecutableNode for ActionServer {
    type State = ActionServerState;

    fn init() -> Self::State {
        // The `act_fibonacci` tag uses `ActionTag::placeholder()` as
        // the macro-emit sentinel; the real tag (returned by
        // `create_action_static` in `register`) is bound at register
        // time by a follow-up wave. The RTIC-side dispatch handle TODO
        // (see module doc) means there is no Spawner / runtime-handle
        // field here yet — once the plumbing lands, this struct grows
        // a `dispatch: RticDispatchHandle` field initialised from
        // `RticBoardEntry::init_hardware`'s return shape.
        ActionServerState {
            act_fibonacci: ActionTag::placeholder(),
        }
    }

    fn on_callback(state: &mut Self::State, callback: Callback<'_>, _ctx: &mut CallbackCtx<'_>) {
        if state.act_fibonacci == callback {
            // Goal / cancel / accepted all share the synthesized
            // callback ID (per `create_action_static`'s fan-out).
            // The trampoline-registration story (a follow-up wave)
            // will split these into per-slot dispatch entries once the
            // runtime exposes a discriminator on `CallbackCtx`. For
            // now, log the delivery as a placeholder so a real flash
            // surfaces the wiring.
            defmt::info!("Fibonacci action callback fired");

            // RTIC spawn-from-sync escape (see module doc) lands
            // here once the dispatch handle is threaded through:
            //
            //     if let Some(spawn) = state.dispatch.as_ref() {
            //         let _ = spawn.run_fibonacci(goal);
            //     }
        }
    }
}

nros::node!(ActionServer);

// Placeholder — minimal `RosAction` stand-in so the declarative
// `create_action_static` call type-checks without dragging
// `example_interfaces` (and its transitive `action_msgs` +
// `unique_identifier_msgs` + `builtin_interfaces` deps, all codegen-
// materialised under `generated/`) into this skeleton. Goal /
// Result / Feedback all share a 4-byte little-endian `i32` wire
// shape (mirrors `Fibonacci`'s `order` field width); the five
// envelope types (`SendGoalRequest` / `SendGoalResponse` /
// `GetResultRequest` / `GetResultResponse` / `FeedbackMessage`) are
// aliased to the same placeholder. Follow-ups switch to the real
// type once `generated/example_interfaces/` ships for this example.
#[derive(Copy, Clone, Default)]
pub struct PlaceholderInt32 {
    pub data: i32,
}

impl Serialize for PlaceholderInt32 {
    fn serialize(&self, _writer: &mut CdrWriter) -> Result<(), SerError> {
        Ok(())
    }
}

impl Deserialize for PlaceholderInt32 {
    fn deserialize(_reader: &mut CdrReader) -> Result<Self, DeserError> {
        Ok(Self { data: 0 })
    }
}

impl RosMessage for PlaceholderInt32 {
    const TYPE_NAME: &'static str = "example_interfaces/msg/Int32";
    const TYPE_HASH: &'static str = "";
}

/// Placeholder action — Fibonacci-shaped, all envelope slots
/// aliased to [`PlaceholderInt32`]. See the module doc for the
/// rationale.
pub struct PlaceholderAct;

impl RosAction for PlaceholderAct {
    type Goal = PlaceholderInt32;
    type Result = PlaceholderInt32;
    type Feedback = PlaceholderInt32;
    type SendGoalRequest = PlaceholderInt32;
    type SendGoalResponse = PlaceholderInt32;
    type GetResultRequest = PlaceholderInt32;
    type GetResultResponse = PlaceholderInt32;
    type FeedbackMessage = PlaceholderInt32;

    const ACTION_NAME: &'static str = "example_interfaces/action/Fibonacci";
    const ACTION_HASH: &'static str = "";
}
