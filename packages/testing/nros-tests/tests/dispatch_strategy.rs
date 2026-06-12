//! Phase 216.A — dispatch substrate integration tests.
//!
//! Spec §216 Acceptance #6: cover the substrate primitives that the
//! board crates + `nros check` matrix downstream consume.
//!
//! Three layers exercised here:
//!
//! * `nros_platform::DispatchStrategy` (216.A.1) — enum + `to_u8` /
//!   `from_u8` round-trip + `DEFAULT == Inline`.
//! * `nros::Node::DISPATCH` (216.A.3) — defaults to `Inline`, explicit
//!   `Deferred` override is observable on the trait associated const.
//! * `nros_platform::NodeDispatchRuntime` (216.A.2) — default
//!   `dispatch_strategy()` returns `Inline`, default `signal_callback`
//!   panics with the documented diagnostic when invoked on an Inline
//!   runtime (the `NullNodeRuntime` sink).
//!
//! Plus a Phase 216.A.4 tag-equality sanity check
//! (`SubscriptionTag::new("x") == CallbackId::new("x")`); the deeper
//! tag-type coverage already lives in
//! `packages/core/nros/src/dispatch_tag.rs` unit tests — the check
//! here just guards against the cross-crate re-export drifting.

use std::panic;

use nros::{
    Callback, CallbackId, DispatchStrategy, Node, NodeContext, NodeResult, SubscriptionTag,
};
use nros_platform::{NodeDispatchRuntime, NullNodeRuntime, SignaledCallback};

// ---------------------------------------------------------------------------
// 216.A.1 — DispatchStrategy enum + u8 round-trip
// ---------------------------------------------------------------------------

#[test]
fn dispatch_strategy_round_trips_u8() {
    for s in [
        DispatchStrategy::Inline,
        DispatchStrategy::Deferred,
        DispatchStrategy::FromIsr,
    ] {
        assert_eq!(
            DispatchStrategy::from_u8(s.to_u8()),
            Some(s),
            "from_u8(to_u8({s:?})) must round-trip",
        );
    }

    // Unknown discriminants must surface `None`, not a silent
    // mis-classification to `Inline`.
    assert_eq!(DispatchStrategy::from_u8(3), None);
    assert_eq!(DispatchStrategy::from_u8(255), None);
}

#[test]
fn dispatch_strategy_default_const_is_inline() {
    assert_eq!(DispatchStrategy::DEFAULT, DispatchStrategy::Inline);
    assert_eq!(DispatchStrategy::DEFAULT.to_u8(), 0);
}

// ---------------------------------------------------------------------------
// 216.A.3 — Node::DISPATCH defaults to Inline; explicit override is
// observable.
// ---------------------------------------------------------------------------

struct DefaultDispatchNode;

impl Node for DefaultDispatchNode {
    const NAME: &'static str = "default_dispatch_node";
    fn register(_: &mut NodeContext<'_>) -> NodeResult<()> {
        Ok(())
    }
}

struct DeferredDispatchNode;

impl Node for DeferredDispatchNode {
    const NAME: &'static str = "deferred_dispatch_node";
    const DISPATCH: DispatchStrategy = DispatchStrategy::Deferred;
    fn register(_: &mut NodeContext<'_>) -> NodeResult<()> {
        Ok(())
    }
}

#[test]
fn dispatch_strategy_default_is_inline() {
    assert_eq!(
        <DefaultDispatchNode as Node>::DISPATCH,
        DispatchStrategy::Inline,
    );
}

#[test]
fn dispatch_strategy_explicit_deferred_overrides_default() {
    assert_eq!(
        <DeferredDispatchNode as Node>::DISPATCH,
        DispatchStrategy::Deferred,
    );
    // And the explicit override is distinct from the default — the
    // matrix downstream relies on observing the difference.
    assert_ne!(
        <DefaultDispatchNode as Node>::DISPATCH,
        <DeferredDispatchNode as Node>::DISPATCH,
    );
}

// ---------------------------------------------------------------------------
// 216.A.2 — NodeDispatchRuntime defaults
// ---------------------------------------------------------------------------

#[test]
fn dispatch_runtime_default_strategy_is_inline() {
    let runtime = NullNodeRuntime;
    assert_eq!(runtime.dispatch_strategy(), DispatchStrategy::Inline);
}

#[test]
fn signal_callback_on_inline_runtime_panics() {
    // The default `NodeDispatchRuntime::signal_callback` impl is
    // documented to `panic!("signal_callback not implemented for
    // Inline runtime")`. Calling it on the `NullNodeRuntime` (which
    // inherits the default) MUST surface the panic — silent drops
    // would mask a mis-wired Deferred-strategy Node pkg.
    let result = panic::catch_unwind(|| {
        // catch_unwind requires UnwindSafe; NullNodeRuntime is
        // trivially safe (no interior state). Build a fresh sink +
        // a stub `SignaledCallback` whose `ctx_ptr` is never
        // dereferenced because the default impl panics first.
        let mut runtime = NullNodeRuntime;
        let cb = SignaledCallback {
            cb_id: "test_cb",
            ctx_ptr: core::ptr::null_mut(),
        };
        runtime.signal_callback(cb);
    });
    let payload = result.expect_err("default signal_callback must panic");
    let msg = payload
        .downcast_ref::<&'static str>()
        .map(|s| (*s).to_string())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_default();
    assert!(
        msg.contains("signal_callback not implemented for Inline runtime"),
        "panic payload should carry the documented diagnostic, got: {msg:?}",
    );
}

// ---------------------------------------------------------------------------
// 216.A.4 — SubscriptionTag / CallbackId equality re-export sanity check
// ---------------------------------------------------------------------------

#[test]
fn tag_eq_callback_id_matches() {
    let tag = SubscriptionTag::new("/chatter");
    let id = CallbackId::new("/chatter");
    // SubscriptionTag implements `PartialEq<Callback<'_>>` — the
    // dispatch path relies on this comparison to route a signaled
    // CallbackId back to the originating tag.
    assert!(tag == id);

    // And the inverse identifier must NOT match.
    let other = CallbackId::new("/other_topic");
    assert!(!(tag == other));
}
