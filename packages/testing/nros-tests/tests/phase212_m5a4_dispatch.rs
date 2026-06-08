//! Phase 212.M.5.a.4 / N.7 — component dispatch path integration.
//!
//! Originally the test exercised the BSP-baker fn-pointer ABI: the
//! `nros::node!()` macro emitted four `__nros_component_<pkg>_*`
//! externs and the test called `register_dispatch_slot(...)` with those
//! symbols directly. Phase 212.N.7 step-6 dropped the global symbols —
//! the macro now emits ONE public item, `pub fn register(runtime)`, and
//! the four typed fns live as local items inside it. The
//! Node-pkg-facing entry point is `<pkg>::register(&mut RuntimeCtx)`.
//!
//! This rewrite preserves the original coverage (callback fires + the
//! publisher resolver routes the dispatched publish) by going through
//! the new path: build a real `ExecutorNodeRuntime`, wrap it in a
//! `RuntimeCtx`, invoke the macro-emitted `register(runtime)` wrapper,
//! then spin. Same end-state asserts as before.

#![cfg(feature = "component-runtime-test")]

use nros_rmw_zenoh as _;

use std::{
    sync::atomic::{AtomicU32, Ordering},
    time::Duration,
};

use nros::{
    Callback, CallbackCtx, CdrReader, CdrWriter, DeserError, Deserialize, ExecutableNode, Executor,
    ExecutorConfig, ExecutorNodeRuntime, Node, NodeContext, NodeOptions, NodeResult, SerError,
    Serialize, TickCtx,
};
use nros_platform::RuntimeCtx;
use nros_tests::fixtures::{ZenohRouter, require_zenohd, zenohd_unique};
use rstest::rstest;

// =============================================================================
// Test message
// =============================================================================

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct TestMsg {
    data: i32,
}
impl Serialize for TestMsg {
    fn serialize(&self, w: &mut CdrWriter) -> Result<(), SerError> {
        w.write_i32(self.data)
    }
}
impl Deserialize for TestMsg {
    fn deserialize(r: &mut CdrReader) -> Result<Self, DeserError> {
        Ok(Self {
            data: r.read_i32()?,
        })
    }
}
impl nros::RosMessage for TestMsg {
    const TYPE_NAME: &'static str = "test_msgs::msg::dds_::TestMsg_";
    const TYPE_HASH: &'static str =
        "RIHS01_0000000000000000000000000000000000000000000000000000000000000000";
}

// =============================================================================
// Talker — one publisher + one 50 ms timer. The dispatched
// `on_callback` body mutates a shared counter and publishes through
// `CallbackCtx::publish` so we exercise both the state-erasure path
// AND the per-cell publisher resolver.
// =============================================================================

static TALKER_FIRES: AtomicU32 = AtomicU32::new(0);
static TALKER_PUB_ERRORS: AtomicU32 = AtomicU32::new(0);

struct Talker;
impl Node for Talker {
    const NAME: &'static str = "m5a4_talker";
    fn register(ctx: &mut NodeContext<'_>) -> NodeResult<()> {
        let mut node = ctx.create_node(NodeOptions::new("m5a4_talker_node"))?;
        let _p = node.create_publisher_for_topic::<TestMsg>("/m5a4_chatter")?;
        let _t =
            node.create_timer_for_callback_name("on_tick", nros::TimerDuration::from_millis(50))?;
        Ok(())
    }
}
impl ExecutableNode for Talker {
    type State = i32;
    fn init() -> Self::State {
        0
    }
    fn on_callback(state: &mut i32, cb: Callback<'_>, ctx: &mut CallbackCtx<'_>) {
        if cb.as_str() == "on_tick" {
            let msg = TestMsg { data: *state };
            let r = ctx.publish_to_topic::<TestMsg, 64>("/m5a4_chatter", &msg);
            TALKER_FIRES.fetch_add(1, Ordering::SeqCst);
            if r.is_err() {
                TALKER_PUB_ERRORS.fetch_add(1, Ordering::SeqCst);
            }
            *state += 1;
        }
    }
    fn tick(_state: &mut Self::State, _ctx: &mut TickCtx<'_>) {}
}

// Phase 212.N.7 step-3.4 — the macro emits a single `pub fn register`
// wrapper at this file's scope. Bind it to a distinct local name so we
// can call it without conflicting with `Node::register` (the
// declarative-side method bound on the `Talker` type above).
nros::node!(Talker);
use self::register as talker_register;

// =============================================================================
// Tests
// =============================================================================

#[rstest]
fn dispatch_fires_timer_callback(zenohd_unique: ZenohRouter) {
    if !require_zenohd() {
        nros_tests::skip!("zenohd not found");
    }
    TALKER_FIRES.store(0, Ordering::SeqCst);
    TALKER_PUB_ERRORS.store(0, Ordering::SeqCst);

    let locator = zenohd_unique.locator();
    let cfg = ExecutorConfig::new(&locator)
        .node_name("m5a4_dispatch")
        .domain_id(180);
    let executor = Executor::open(&cfg).expect("Executor::open failed");
    let mut runtime = ExecutorNodeRuntime::from_executor(executor);

    // Phase 212.N.7 — register through the new `pub fn register(runtime)`
    // wrapper emitted by `nros::node!(Talker)`. The wrapper
    // transmutes the four typed local fns to the platform-layer opaque
    // aliases and forwards to `ExecutorNodeRuntime::register_dispatch_slot_dyn`
    // (the impl in `component_runtime.rs` transmutes them back). Same
    // dispatch path the previous test exercised via the four globally
    // mangled symbols — now via the wrapper-emitted local fns.
    {
        let mut ctx = RuntimeCtx::with_runtime(&mut runtime);
        talker_register(&mut ctx).expect("component register");
    }

    assert_eq!(runtime.component_count(), 1);

    // Spin for ~300 ms. The 50 ms-period timer must fire ≥ 2× and the
    // dispatched body must run.
    for _ in 0..15 {
        std::thread::sleep(Duration::from_millis(20));
        runtime
            .spin_once(Duration::from_millis(0))
            .expect("spin_once");
    }

    let fires = TALKER_FIRES.load(Ordering::SeqCst);
    let errs = TALKER_PUB_ERRORS.load(Ordering::SeqCst);
    assert!(
        fires >= 2,
        "on_callback fired {fires} times — expected ≥ 2 over 300 ms"
    );
    assert_eq!(
        errs, 0,
        "CallbackCtx::publish must resolve through the cell resolver (errs = {errs})"
    );
}

#[rstest]
fn dispatch_routes_publisher_resolver(zenohd_unique: ZenohRouter) {
    if !require_zenohd() {
        nros_tests::skip!("zenohd not found");
    }
    TALKER_FIRES.store(0, Ordering::SeqCst);
    TALKER_PUB_ERRORS.store(0, Ordering::SeqCst);

    let locator = zenohd_unique.locator();
    let cfg = ExecutorConfig::new(&locator)
        .node_name("m5a4_pub_resolver")
        .domain_id(181);
    let executor = Executor::open(&cfg).expect("Executor::open failed");
    let mut runtime = ExecutorNodeRuntime::from_executor(executor);

    {
        let mut ctx = RuntimeCtx::with_runtime(&mut runtime);
        talker_register(&mut ctx).expect("component register");
    }

    // Drive the spin long enough for at least one timer fire.
    for _ in 0..10 {
        std::thread::sleep(Duration::from_millis(20));
        runtime
            .spin_once(Duration::from_millis(0))
            .expect("spin_once");
    }

    let errs = TALKER_PUB_ERRORS.load(Ordering::SeqCst);
    let fires = TALKER_FIRES.load(Ordering::SeqCst);
    assert!(fires >= 1, "expected ≥ 1 timer fire (got {fires})");
    assert_eq!(
        errs, 0,
        "publisher resolver must route the dispatched publish (errs = {errs})"
    );
}
