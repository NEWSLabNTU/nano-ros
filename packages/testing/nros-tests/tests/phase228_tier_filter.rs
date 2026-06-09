//! Phase 228 — per-tier callback-group filter + shared-session
//! acceptance test (228.C runtime gate + 228.E `session_ptr` /
//! `open_with_session`).
//!
//! Two executors run over **one** RMW session (the per-tier model: the
//! boot executor owns the session, a second borrows it via
//! [`Executor::open_with_session`] from [`Executor::session_ptr`]). Each
//! executor installs a distinct `active_groups` filter, then registers a
//! node carrying **two** group-labelled timers (`high` + `low`). The
//! gate in `ExecutorSink::create_entity` must register only the timer
//! whose group is active on that executor; the other is filtered out (no
//! RMW handle, no dispatch slot), so its callback never fires.
//!
//! Asserts:
//! - boot executor (`active = ["high"]`) fires only its `high` timer;
//!   its `low` timer was gated out (never fires).
//! - borrowed executor (`active = ["low"]`) fires only its `low` timer;
//!   its `high` timer was gated out. This also proves the borrowed
//!   session pointer yields a working executor that spins on the shared
//!   session.
//!
//! Needs real zenohd (the gate lives on the `rmw-cffi` live path);
//! gated by `component-runtime-test` like the M.5.a.2 suite.

#![cfg(feature = "component-runtime-test")]

// Force-link the zenoh-pico backend so its vtable self-registers before
// `Executor::open` (matches the component-runtime suite).
use nros_rmw_zenoh as _;

use std::{
    sync::{
        Arc,
        atomic::{AtomicU32, Ordering},
    },
    time::Duration,
};

use nros::{
    Callback, CallbackCtx, ExecutableNode, Executor, ExecutorConfig, ExecutorNodeRuntime, Node,
    NodeContext, NodeOptions, NodeResult,
};
use nros_tests::fixtures::{ZenohRouter, require_zenohd, zenohd_unique};
use rstest::rstest;

// Four counters: <node>_<timer>. The gate must keep the off-tier ones
// at zero.
fn high_node_high() -> &'static Arc<AtomicU32> {
    static I: std::sync::OnceLock<Arc<AtomicU32>> = std::sync::OnceLock::new();
    I.get_or_init(|| Arc::new(AtomicU32::new(0)))
}
fn high_node_low() -> &'static Arc<AtomicU32> {
    static I: std::sync::OnceLock<Arc<AtomicU32>> = std::sync::OnceLock::new();
    I.get_or_init(|| Arc::new(AtomicU32::new(0)))
}
fn low_node_high() -> &'static Arc<AtomicU32> {
    static I: std::sync::OnceLock<Arc<AtomicU32>> = std::sync::OnceLock::new();
    I.get_or_init(|| Arc::new(AtomicU32::new(0)))
}
fn low_node_low() -> &'static Arc<AtomicU32> {
    static I: std::sync::OnceLock<Arc<AtomicU32>> = std::sync::OnceLock::new();
    I.get_or_init(|| Arc::new(AtomicU32::new(0)))
}

/// Declare two timers, one labelled `high`, one labelled `low`, via the
/// sticky [`DeclaredNode::callback_group`] setter (Phase 228.C).
fn declare_dual_timers(ctx: &mut NodeContext<'_>, node_name: &str) -> NodeResult<()> {
    let mut node = ctx.create_node(NodeOptions::new(node_name))?;
    node.callback_group("high")?;
    node.create_timer_for_callback_name("on_high", nros::TimerDuration::from_millis(10))?;
    node.callback_group("low")?;
    node.create_timer_for_callback_name("on_low", nros::TimerDuration::from_millis(10))?;
    Ok(())
}

/// Node registered on the boot (high-tier) executor.
struct HighTierNode;
impl Node for HighTierNode {
    const NAME: &'static str = "tier_high_node";
    fn register(ctx: &mut NodeContext<'_>) -> NodeResult<()> {
        declare_dual_timers(ctx, "tier_high")
    }
}
impl ExecutableNode for HighTierNode {
    type State = ();
    fn init() -> Self::State {}
    fn on_callback(_s: &mut (), cb: Callback<'_>, _ctx: &mut CallbackCtx<'_>) {
        match cb.as_str() {
            "on_high" => high_node_high().fetch_add(1, Ordering::SeqCst),
            "on_low" => high_node_low().fetch_add(1, Ordering::SeqCst),
            _ => 0,
        };
    }
}

/// Node registered on the borrowed (low-tier) executor.
struct LowTierNode;
impl Node for LowTierNode {
    const NAME: &'static str = "tier_low_node";
    fn register(ctx: &mut NodeContext<'_>) -> NodeResult<()> {
        declare_dual_timers(ctx, "tier_low")
    }
}
impl ExecutableNode for LowTierNode {
    type State = ();
    fn init() -> Self::State {}
    fn on_callback(_s: &mut (), cb: Callback<'_>, _ctx: &mut CallbackCtx<'_>) {
        match cb.as_str() {
            "on_high" => low_node_high().fetch_add(1, Ordering::SeqCst),
            "on_low" => low_node_low().fetch_add(1, Ordering::SeqCst),
            _ => 0,
        };
    }
}

#[rstest]
fn tier_filter_gates_off_tier_callbacks_over_shared_session(zenohd_unique: ZenohRouter) {
    if !require_zenohd() {
        nros_tests::skip!("zenohd not found");
    }
    for c in [
        high_node_high(),
        high_node_low(),
        low_node_high(),
        low_node_low(),
    ] {
        c.store(0, Ordering::SeqCst);
    }

    let locator = zenohd_unique.locator();
    let cfg = ExecutorConfig::new(&locator)
        .node_name("p228_tier")
        .domain_id(178);

    // Boot executor owns the one session.
    let boot = Executor::open(&cfg).expect("Executor::open failed");
    let mut boot_rt = ExecutorNodeRuntime::from_executor(boot);

    // Second executor borrows the SAME session (per-tier model).
    let sptr = boot_rt.executor_mut().session_ptr();
    // SAFETY: `sptr` aliases `boot_rt`'s session, which outlives `low_rt`
    // (both live to the end of this test); access is serialized by the
    // backend.
    let borrowed = unsafe { Executor::open_with_session(sptr) };
    let mut low_rt = ExecutorNodeRuntime::from_executor(borrowed);

    // Install per-tier filters, then register the dual-timer nodes. The
    // gate drops the off-tier timer at registration.
    boot_rt.executor_mut().set_active_groups(&["high"]);
    boot_rt
        .register_node::<HighTierNode>()
        .expect("register HighTierNode");

    low_rt.executor_mut().set_active_groups(&["low"]);
    low_rt
        .register_node::<LowTierNode>()
        .expect("register LowTierNode");

    // Spin both for ~80 ms; 10 ms timers fire several times.
    for _ in 0..8 {
        std::thread::sleep(Duration::from_millis(10));
        boot_rt
            .spin_once(Duration::from_millis(0))
            .expect("boot spin");
        low_rt
            .spin_once(Duration::from_millis(0))
            .expect("low spin");
    }

    let hh = high_node_high().load(Ordering::SeqCst);
    let hl = high_node_low().load(Ordering::SeqCst);
    let lh = low_node_high().load(Ordering::SeqCst);
    let ll = low_node_low().load(Ordering::SeqCst);

    // Active-tier timers fire; off-tier timers were gated out entirely.
    assert!(hh >= 3, "boot/high timer should fire (got {hh})");
    assert_eq!(hl, 0, "boot/low timer must be gated out (got {hl})");
    assert!(ll >= 3, "borrowed/low timer should fire (got {ll})");
    assert_eq!(lh, 0, "borrowed/high timer must be gated out (got {lh})");
}
