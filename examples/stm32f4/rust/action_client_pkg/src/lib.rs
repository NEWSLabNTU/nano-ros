//! STM32F4 Action Client Node pkg — Phase 216.B.5.
//!
//! Board-agnostic Node pkg that the sibling Entry pkg
//! (`action-client-rtic` for the RTIC framework; a parallel
//! `action-client-embassy` migration will follow once the C-wave
//! catches up) wires onto a board + framework via its
//! `[package.metadata.nros.entry] deploy = "<board>"` key plus the
//! `nros::main!()` macro.
//!
//! ## Inline dispatch
//!
//! Unlike the sibling `action_server_pkg` (Deferred), this Node pkg
//! declares
//!
//! ```ignore
//! const DISPATCH: DispatchStrategy = DispatchStrategy::Inline;
//! ```
//!
//! The client side has no callbacks on the spin path: the legacy
//! Pattern A `examples/stm32f4/rust/action-client-rtic/src/main.rs`
//! drives goal acceptance, feedback, and result via
//! `try_recv()` / `try_recv_feedback()` loops from an `async fn`
//! task — the executor never delivers an action callback into the
//! Node body. Inline therefore matches the talker_pkg precedent
//! (Phase 216.C.4): `(RTIC, Inline)` is the same matrix cell
//! `nros check` (Phase 216.D.1) already accepts for `talker-rtic`,
//! and there is no Spawner / RTIC-`task::spawn()` handoff to
//! plumb here.
//!
//! ## PlaceholderAct reuse
//!
//! This pkg reuses `PlaceholderAct` from the sibling
//! [`stm32f4_action_server_pkg`] so the client + server wire
//! shapes stay aligned by construction. When the real
//! `example_interfaces::action::Fibonacci` ships (follow-up B wave
//! that finishes the trampoline-registration story), both pkgs flip
//! together. See [`stm32f4_action_server_pkg::PlaceholderAct`] for
//! the placeholder's rationale + wire shape.
//!
//! ## Skeleton status — TODO
//!
//! Phase 216.B.5 ships the trait-shaped scaffolding only. `register`
//! declares the action client via `create_action_client` so the
//! cross-check passes; the actual goal-send + try_recv loops the
//! legacy main.rs drove (see `examples/stm32f4/rust/action-client-rtic/
//! src/main.rs` pre-migration) move into a framework-owned
//! `#[task]` body added by a follow-up B wave once the
//! `nros::main!()` Entry pkg exposes a hook for Inline pkgs to
//! schedule their own one-shot bringup task. The macro emit + dep
//! graph compile clean today; a real flash will not yet send a goal.

#![no_std]

use nros::{
    DispatchStrategy, EntityId, ExecutableNode, Node, NodeContext, NodeOptions, NodeResult,
};
use stm32f4_action_server_pkg::PlaceholderAct;

/// Action client component — issues Fibonacci-shaped goals and
/// (eventually) polls for feedback + result. Phase 216.B.5 skeleton.
pub struct ActionClient;

impl Node for ActionClient {
    const NAME: &'static str = "fibonacci_client";

    /// Phase 216.B.5 — declares Inline dispatch. The client side has
    /// no callbacks on the spin path (`try_recv*` loops drive
    /// goal-acceptance, feedback, and result), so `(RTIC, Inline)`
    /// matches the matrix cell `nros check` (Phase 216.D.1) already
    /// accepts for `talker-rtic`. See the module doc for the
    /// rationale + Embassy-side parity story.
    const DISPATCH: DispatchStrategy = DispatchStrategy::Inline;

    fn register(ctx: &mut NodeContext<'_>) -> NodeResult<()> {
        let mut node = ctx.create_node(NodeOptions::new("fibonacci_client"))?;
        // Phase 216.B.5 — action client uses the `EntityId`-shaped
        // builder (no `create_action_client_static` exists: clients
        // need a USABLE handle, not just a tag, to dispatch goals —
        // see `DeclaredNode::create_action_static`'s doc comment).
        // The returned `NodeActionClient` is dropped because the
        // skeleton's send_goal / try_recv* bodies haven't been
        // threaded through yet; a follow-up B wave moves those
        // loops onto a framework-owned RTIC task, at which point the
        // handle is stashed on `Self::State` (Inline state grows
        // from `()` to `Option<NodeActionClient<'static, …>>`).
        let _client = node.create_action_client::<PlaceholderAct>(
            EntityId::new("client_fibonacci"),
            "/fibonacci",
        )?;
        Ok(())
    }
}

impl ExecutableNode for ActionClient {
    /// Phase 216.B.5 — Inline pkgs without per-tick state carry `()`.
    /// Follow-ups grow this into an
    /// `Option<NodeActionClient<'static, PlaceholderAct>>` (plus a
    /// goal-counter / state-machine field) once the framework
    /// exposes a hook for Inline pkgs to schedule their own one-shot
    /// bringup task — at which point `register` stashes the client
    /// handle here instead of dropping it.
    type State = ();

    fn init() -> Self::State {}

    fn on_callback(
        _state: &mut Self::State,
        _callback: nros::CallbackId<'_>,
        _ctx: &mut nros::CallbackCtx<'_>,
    ) {
        // Phase 216.B.5 — Inline client has no callbacks: goal-accept,
        // feedback, and result are driven by `try_recv*` loops from
        // an Entry-pkg-owned async task (TODO documented in the
        // module doc). The body is intentionally empty — the
        // framework should never invoke it for this Node.
        defmt::trace!("ActionClient::on_callback (unexpected — Inline client)");
    }
}

nros::node!(ActionClient);
