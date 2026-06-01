//! Phase 212.M.5.a.4 — BSP dispatch path integration.
//!
//! M.5.a.4 lifts the BSP-path callback gap M.5.a.2 documented: the
//! `nros::component!()` macro now emits parallel `_init` / `_dispatch`
//! / `_tick` extern symbols alongside `_register`, and
//! `ExecutorComponentRuntime::register_dispatch_slot` pairs them into
//! a `BspDispatchSlot` so the BSP-launched component's `on_callback`
//! body fires from the spin loop — same as the typed
//! `register_component::<C>()` path, just type-erased through
//! `*mut ()`.
//!
//! Coverage:
//!
//! * `bsp_dispatch_fires_timer_callback` — register a Talker through
//!   the BSP fn-pointer ABI; spin; assert the `on_callback` body
//!   mutated state.
//! * `bsp_dispatch_publisher_resolves` — same path; assert
//!   `CallbackCtx::publish` (which depends on the per-cell resolver
//!   being wired) returns `Ok` from the dispatched body.

#![cfg(feature = "component-runtime-test")]

use nros_rmw_zenoh as _;

use std::{
    sync::{
        Arc,
        atomic::{AtomicU32, Ordering},
    },
    time::Duration,
};

use nros::{
    CallbackCtx, CdrReader, CdrWriter, ComponentContext, ComponentResult, DeserError, Deserialize,
    Executor, ExecutorComponentRuntime, ExecutorConfig, SerError, Serialize, TickCtx,
    component::{Component, ExecutableComponent, NodeOptions},
    component_metadata::{CallbackId, EntityId, NodeId as MetaNodeId},
};
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
// Talker — one publisher + one 100 ms timer. The dispatched
// `on_callback` body mutates a shared counter and publishes through
// `CallbackCtx::publish` so we exercise both the state-erasure path
// AND the per-cell publisher resolver.
// =============================================================================

static TALKER_FIRES: AtomicU32 = AtomicU32::new(0);
static TALKER_PUB_ERRORS: AtomicU32 = AtomicU32::new(0);

struct Talker;
impl Component for Talker {
    const NAME: &'static str = "m5a4_talker";
    fn register(ctx: &mut ComponentContext<'_>) -> ComponentResult<()> {
        let mut node = ctx.create_node(
            MetaNodeId::new("node"),
            NodeOptions::new("m5a4_talker_node"),
        )?;
        let _p = node.create_publisher::<TestMsg>(EntityId::new("pub_chatter"), "/m5a4_chatter")?;
        let _t = node.create_timer(
            EntityId::new("tick"),
            CallbackId::new("on_tick"),
            nros::TimerDuration::from_millis(50),
        )?;
        Ok(())
    }
}
impl ExecutableComponent for Talker {
    type State = i32;
    fn init() -> Self::State {
        0
    }
    fn on_callback(state: &mut i32, cb: CallbackId<'_>, ctx: &mut CallbackCtx<'_>) {
        if cb.as_str() == "on_tick" {
            let msg = TestMsg { data: *state };
            let r = ctx.publish::<TestMsg, 64>(EntityId::new("pub_chatter"), &msg);
            TALKER_FIRES.fetch_add(1, Ordering::SeqCst);
            if r.is_err() {
                TALKER_PUB_ERRORS.fetch_add(1, Ordering::SeqCst);
            }
            *state += 1;
        }
    }
    fn tick(_state: &mut Self::State, _ctx: &mut TickCtx<'_>) {}
}

nros::component!(Talker);

// The `nros::component!` macro emits four `__nros_component_<pkg>_*`
// symbols at top-level. Because this integration test compiles
// under `nros-tests`, the sanitised pkg suffix is `nros_tests`. We
// reference the locally defined symbols directly (declaring them
// again as `extern "Rust"` would E0428 against the macro emit).

// =============================================================================
// Tests
// =============================================================================

#[rstest]
fn bsp_dispatch_fires_timer_callback(zenohd_unique: ZenohRouter) {
    if !require_zenohd() {
        nros_tests::skip!("zenohd not found");
    }
    TALKER_FIRES.store(0, Ordering::SeqCst);
    TALKER_PUB_ERRORS.store(0, Ordering::SeqCst);

    let locator = zenohd_unique.locator();
    let cfg = ExecutorConfig::new(&locator)
        .node_name("m5a4_bsp_dispatch")
        .domain_id(180);
    let executor = Executor::open(&cfg).expect("Executor::open failed");
    let mut runtime = ExecutorComponentRuntime::from_executor(executor);

    // Phase 212.M.5.a.4 — register through the BSP-shape API. This is
    // the exact same shape the FreeRTOS BSP baker uses
    // (`register_dispatch_slot` paired with the four mangled symbols).
    runtime
        .register_dispatch_slot(
            __nros_component_nros_tests_register,
            __nros_component_nros_tests_init,
            __nros_component_nros_tests_dispatch,
            __nros_component_nros_tests_tick,
        )
        .expect("register_dispatch_slot");

    assert_eq!(runtime.component_count(), 1);

    // Spin for ~250 ms. The 50 ms-period timer must fire ≥ 2× and the
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
        "BSP-path on_callback fired {fires} times — expected ≥ 2 over 300 ms"
    );
    assert_eq!(
        errs, 0,
        "CallbackCtx::publish must resolve through the cell resolver (errs = {errs})"
    );
}

#[rstest]
fn bsp_dispatch_routes_publisher_resolver(zenohd_unique: ZenohRouter) {
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
    let mut runtime = ExecutorComponentRuntime::from_executor(executor);
    runtime
        .register_dispatch_slot(
            __nros_component_nros_tests_register,
            __nros_component_nros_tests_init,
            __nros_component_nros_tests_dispatch,
            __nros_component_nros_tests_tick,
        )
        .expect("register_dispatch_slot");

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
        "publisher resolver must route the BSP-dispatched publish (errs = {errs})"
    );
    // Silence the unused-warning on the Arc/Ordering imports under
    // `--no-default-features` builds that still want this test file
    // to compile.
    let _ = Arc::new(AtomicU32::new(0));
}
