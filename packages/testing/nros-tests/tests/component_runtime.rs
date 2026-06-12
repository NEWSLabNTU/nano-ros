//! Phase 212.M.5.a.2 — `ExecutorNodeRuntime` integration tests.
//!
//! Exercises the executor-backed component runtime added in
//! `packages/core/nros/src/component_runtime.rs`. The runtime binds
//! the [`Node`] / [`ExecutableNode`] traits to a live
//! [`nros::Executor`] — registering components materialises their
//! nodes / pubs / subs / timers on the real executor, and fired
//! callbacks dispatch back into the component's `on_callback` body.
//!
//! Unit tests need a real RMW backend (the in-tree `MockSession`
//! flips off when `rmw-cffi` is on; the runtime is `rmw-cffi`-gated),
//! so they ride the same `trigger-test` / `nros-rmw-zenoh` fixture as
//! the wake-latency suite.

#![cfg(feature = "component-runtime-test")]

// Force-link the zenoh-pico backend so its `.init_array` ctor
// registers the vtable before `Executor::open` runs (matches the
// `wake_latency` test).
use nros_rmw_zenoh as _;

use std::{
    sync::{
        Arc,
        atomic::{AtomicU32, Ordering},
    },
    time::Duration,
};

use nros::{
    Callback, CallbackCtx, CdrReader, CdrWriter, DeserError, Deserialize, ExecutableNode, Executor,
    ExecutorConfig, ExecutorNodeRuntime, Node, NodeContext, NodeDeclError, NodeOptions, NodeResult,
    SerError, Serialize,
};
use nros_tests::fixtures::{ZenohRouter, require_zenohd, zenohd_unique};
use rstest::rstest;

// =============================================================================
// Test message: a single i32 ("Int32"-equivalent without depending on
// the in-tree std_msgs codegen).
// =============================================================================

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct TestMsg {
    data: i32,
}

impl Serialize for TestMsg {
    fn serialize(&self, writer: &mut CdrWriter) -> Result<(), SerError> {
        writer.write_i32(self.data)
    }
}

impl Deserialize for TestMsg {
    fn deserialize(reader: &mut CdrReader) -> Result<Self, DeserError> {
        Ok(Self {
            data: reader.read_i32()?,
        })
    }
}

impl nros::RosMessage for TestMsg {
    const TYPE_NAME: &'static str = "test_msgs::msg::dds_::TestMsg_";
    const TYPE_HASH: &'static str =
        "RIHS01_0000000000000000000000000000000000000000000000000000000000000000";
}

// =============================================================================
// Components used across the unit tests.
// =============================================================================

/// `TimerOnly` — one node, one timer firing every 10 ms. The callback
/// bumps a static counter via the component `State`, which we then
/// read out post-spin.
struct TimerOnly;
impl Node for TimerOnly {
    const NAME: &'static str = "timer_only";
    fn register(ctx: &mut NodeContext<'_>) -> NodeResult<()> {
        let mut node = ctx.create_node(NodeOptions::new("timer_only"))?;
        let _t =
            node.create_timer_for_callback_name("on_tick", nros::TimerDuration::from_millis(10))?;
        Ok(())
    }
}
impl ExecutableNode for TimerOnly {
    /// Use an `Arc<AtomicU32>` so the test can observe the count even
    /// though the runtime never surfaces a typed slot borrow.
    type State = Arc<AtomicU32>;
    fn init() -> Self::State {
        timer_only_count().clone()
    }
    fn on_callback(state: &mut Self::State, callback: Callback<'_>, _ctx: &mut CallbackCtx<'_>) {
        if callback.as_str() == "on_tick" {
            state.fetch_add(1, Ordering::SeqCst);
        }
    }
}

fn timer_only_count() -> &'static Arc<AtomicU32> {
    static INIT: std::sync::OnceLock<Arc<AtomicU32>> = std::sync::OnceLock::new();
    INIT.get_or_init(|| Arc::new(AtomicU32::new(0)))
}

/// Talker: one node, one publisher on `/chatter`, one timer at 100 ms
/// that publishes `TestMsg { data: state }` then bumps state.
struct Talker;
impl Node for Talker {
    const NAME: &'static str = "talker";
    fn register(ctx: &mut NodeContext<'_>) -> NodeResult<()> {
        let mut node = ctx.create_node(NodeOptions::new("talker_node"))?;
        let _p = node.create_publisher_for_topic::<TestMsg>("/m5a2_chatter")?;
        let _t =
            node.create_timer_for_callback_name("on_tick", nros::TimerDuration::from_millis(100))?;
        Ok(())
    }
}
impl ExecutableNode for Talker {
    type State = i32;
    fn init() -> Self::State {
        0
    }
    fn on_callback(state: &mut i32, callback: Callback<'_>, ctx: &mut CallbackCtx<'_>) {
        if callback.as_str() == "on_tick" {
            let msg = TestMsg { data: *state };
            let r = ctx.publish_to_topic::<TestMsg, 64>("/m5a2_chatter", &msg);
            TALKER_FIRES.fetch_add(1, Ordering::SeqCst);
            if r.is_err() {
                TALKER_PUB_ERRORS.fetch_add(1, Ordering::SeqCst);
            }
            *state += 1;
        }
    }
}

static TALKER_FIRES: AtomicU32 = AtomicU32::new(0);
static TALKER_PUB_ERRORS: AtomicU32 = AtomicU32::new(0);

/// Node whose declarative `register` always errors — used to
/// verify the runtime rolls back on init failure.
struct FailingComp;
impl Node for FailingComp {
    const NAME: &'static str = "failing";
    fn register(_ctx: &mut NodeContext<'_>) -> NodeResult<()> {
        Err(NodeDeclError::Runtime)
    }
}
impl ExecutableNode for FailingComp {
    type State = ();
    fn init() -> Self::State {}
    fn on_callback(_s: &mut (), _cb: Callback<'_>, _ctx: &mut CallbackCtx<'_>) {}
}

// =============================================================================
// Unit-style tests (need real zenohd; tagged by the `component-runtime-test`
// required-features gate).
// =============================================================================

#[rstest]
fn runtime_registers_single_component_and_spins_once(zenohd_unique: ZenohRouter) {
    if !require_zenohd() {
        nros_tests::skip!("zenohd not found");
    }
    timer_only_count().store(0, Ordering::SeqCst);

    let locator = zenohd_unique.locator();
    let config = ExecutorConfig::new(&locator)
        .node_name("m5a2_timer_only")
        .domain_id(174);
    let executor = Executor::open(&config).expect("Executor::open failed");
    let mut runtime = ExecutorNodeRuntime::from_executor(executor);
    let handle = runtime.register_node::<TimerOnly>().expect("register_node");
    assert_eq!(handle.slot(), 0);
    assert_eq!(runtime.component_count(), 1);

    // Spin for ~80 ms in 10 ms chunks. A 10 ms-period timer should
    // fire at least 3 times across that window. `spin_once` credits
    // real wall-clock — we need to actually elapse it.
    for _ in 0..8 {
        std::thread::sleep(Duration::from_millis(10));
        runtime
            .spin_once(Duration::from_millis(0))
            .expect("spin_once");
    }

    let fires = timer_only_count().load(Ordering::SeqCst);
    assert!(
        fires >= 3,
        "timer fired {fires} times under runtime — expected ≥ 3"
    );
}

/// Verifies that a component's declared publisher materialises as a
/// real executor publisher and that the [`CallbackCtx::publish_raw`]
/// path resolves through the per-component resolver.
///
/// Note (Phase 212.M.5.a.2): a true round-trip pub→sub test sits in the
/// out-of-process suite (zenoh-pico has a documented "in-process pub/sub
/// doesn't work due to write filter limitations" caveat — see
/// `tests/trigger_conditions.rs` header). The runtime's dispatch path is
/// exercised by `runtime_registers_single_component_and_spins_once`
/// (timer-fire dispatch); this test seals the publisher half: a Talker
/// publish through the runtime's resolver returns `Ok` and surfaces no
/// runtime error even though no subscriber is matched.
#[rstest]
fn runtime_creates_publisher_for_declared_entity(zenohd_unique: ZenohRouter) {
    if !require_zenohd() {
        nros_tests::skip!("zenohd not found");
    }
    TALKER_FIRES.store(0, Ordering::SeqCst);
    TALKER_PUB_ERRORS.store(0, Ordering::SeqCst);

    let locator = zenohd_unique.locator();
    let cfg = ExecutorConfig::new(&locator)
        .node_name("m5a2_pub_only")
        .domain_id(175);
    let executor = Executor::open(&cfg).expect("Executor::open failed");
    let mut runtime = ExecutorNodeRuntime::from_executor(executor);
    runtime.register_node::<Talker>().expect("register Talker");

    // Spin for ~250 ms; the talker's 100 ms timer should fire 2× and
    // each fire publishes via the resolver-backed `CallbackCtx::publish`.
    for _ in 0..15 {
        std::thread::sleep(Duration::from_millis(20));
        runtime
            .spin_once(Duration::from_millis(0))
            .expect("spin_once");
    }

    let fires = TALKER_FIRES.load(Ordering::SeqCst);
    let errs = TALKER_PUB_ERRORS.load(Ordering::SeqCst);
    assert!(
        fires >= 1,
        "talker timer should fire at least once across 300 ms (fires = {fires})"
    );
    assert_eq!(
        errs, 0,
        "CallbackCtx::publish must resolve through the runtime's publisher map (errs = {errs})"
    );
}

#[rstest]
fn runtime_propagates_init_failure(zenohd_unique: ZenohRouter) {
    if !require_zenohd() {
        nros_tests::skip!("zenohd not found");
    }

    let locator = zenohd_unique.locator();
    let config = ExecutorConfig::new(&locator)
        .node_name("m5a2_failing")
        .domain_id(176);
    let executor = Executor::open(&config).expect("Executor::open failed");
    let mut runtime = ExecutorNodeRuntime::from_executor(executor);

    let r = runtime.register_node::<FailingComp>();
    assert!(matches!(r, Err(NodeDeclError::Runtime)));
    assert_eq!(
        runtime.component_count(),
        0,
        "runtime should roll back the failed component slot"
    );
}

// =============================================================================
// Note on absent e2e pub→sub test
// =============================================================================
//
// The task RFC names an `executor_component_runtime_drives_talker_listener_e2e`
// integration test. zenoh-pico has a "write filter" caveat documented inline
// at `tests/trigger_conditions.rs:5`: in-process zenoh pub/sub doesn't work
// regardless of whether the endpoints share a session or run in distinct
// `Executor`s within the same OS process. The companion `loan_e2e` test
// (which IS designed around two-executor-one-zenohd) currently link-fails on
// `main` for a separate reason (rust-lld undefined `nros_platform_*` symbols
// — pre-existing, not introduced here). A true e2e Talker / Listener
// exchange therefore needs an out-of-process fixture pair, which the runtime
// API supports the same way the existing `examples/native/rust/{talker,
// listener}` consume `Executor::open` directly — the runtime adds the
// wrap-each-binary-in-`ExecutorNodeRuntime` step without changing the
// pub/sub data path. See the M.5.a.3 BSP baker wave for that lifecycle test.
//
// What this file does prove for M.5.a.2:
// * `runtime_registers_single_component_and_spins_once` — the
//   `ExecutableNode::on_callback` dispatch fires for a real
//   live-executor timer.
// * `runtime_creates_publisher_for_declared_entity` — the publisher
//   resolver path is materialised and `CallbackCtx::publish` resolves
//   without error.
// * `runtime_propagates_init_failure` — the runtime rolls back a failed
//   component registration cleanly.
