//! Issues 2984 and 2985: every timer and guard condition Rust code creates
//! reaches the census, once.
//!
//! Timers and guard conditions never touch the RMW, so the census counts a
//! callback slot only when something reports it. The executor reports each
//! one from its registration funnel, which every route crosses; no API layer
//! reports it again. Before, each API layer called a hook after its own
//! registration, and the imperative Rust routes (`Node::create_guard_condition`
//! and the `Node::create_timer_*` family, in `nros-node`, below those hooks)
//! recorded nothing, so a census of Rust code that created one that way was a
//! slot short: `ExecutorFull` at boot on an image sized from it.
//!
//! The first test drives every one of those `Node` methods on an executor
//! opened over the transport-free `metadata` backend and reads the sidecar the
//! census is built from: each is one `timers[]` row, of its own kind. The
//! second installs a declarative component and checks that its one timer is
//! one row, not two.

use nros::{DeclarativeNode as _, TimerClockSource, TimerDuration};

// The posix C port DEFINES the `nros_platform_*` symbols the executor calls.
extern crate nros_platform_cffi as _;

/// The `"kind"` of every row of every node's `"timers"` array, in order.
fn timer_kinds(json: &str) -> Vec<String> {
    let mut out = Vec::new();
    for (at, _) in json.match_indices("\"timers\":[") {
        let rest = &json[at + "\"timers\":[".len()..];
        // The array ends at the first `]` at depth 0.
        let mut depth = 0i32;
        let mut end = rest.len();
        for (i, c) in rest.char_indices() {
            match c {
                '{' | '[' => depth += 1,
                '}' => depth -= 1,
                ']' if depth == 0 => {
                    end = i;
                    break;
                }
                ']' => depth -= 1,
                _ => {}
            }
        }
        out.extend(
            rest[..end]
                .split("\"kind\":\"")
                .skip(1)
                .map(|s| s.split('"').next().unwrap_or("").to_owned()),
        );
    }
    assert!(
        !out.is_empty() || json.contains("\"timers\":["),
        "no timers array: {json}"
    );
    out
}

#[test]
fn imperative_rust_timers_and_guards_reach_the_census() {
    let _ = nros_rmw_metadata::nros_rmw_metadata_register();
    nros::metadata_mode::reset();
    // What `begin_node` does on a census run's first node; this test has no
    // hooked node, so it arms the observers itself.
    nros::metadata_mode::listen_for_registrations();

    // Two executors of four slots each (the default `NROS_EXECUTOR_MAX_CBS`
    // this test builds with), one node each, between them every `Node` method.
    let config = nros::ExecutorConfig::new("").rmw("metadata");
    let ms = TimerDuration::from_millis;

    let mut first = nros::Executor::open(&config).expect("the metadata backend must open");
    let id = first
        .node_builder("clocked")
        .build()
        .expect("node clocked registers");
    {
        let mut node = first.node_mut(id);
        let group = node.create_callback_group("ctrl");
        node.create_timer_repeating(ms(10), || {})
            .expect("repeating");
        node.create_timer_oneshot(ms(20), || {}).expect("oneshot");
        node.create_timer_inert(ms(30), || {}).expect("inert");
        node.create_timer_in_group(&group, ms(40), || {})
            .expect("in group");
    }

    let mut second = nros::Executor::open(&config).expect("the metadata backend must open");
    let id = second
        .node_builder("woken")
        .build()
        .expect("node woken registers");
    {
        let mut node = second.node_mut(id);
        let group = node.create_callback_group("ctrl");
        node.create_timer_on_clock(ms(50), TimerClockSource::System, || {})
            .expect("on clock");
        node.create_timer_on_clock_in_group(&group, ms(60), TimerClockSource::Steady, || {})
            .expect("on clock in group");
        node.create_timer_with_info(ms(70), TimerClockSource::Ros, true, |_info| {})
            .expect("with info");
        let _ = node.create_guard_condition(|| {}).expect("guard");
    }

    let export = nros::node_metadata::SourceMetadataExport::new("fixture_pkg", "imperative")
        .executable("imperative")
        .language("rust");
    let json = nros::metadata_mode::to_json(&export).expect("serialize");

    let mut kinds = timer_kinds(&json);
    kinds.sort();
    // The kind is what the SLOT is, read at the executor funnel, not which
    // method made it: a steady-clock timer in a group is `in_group` however it
    // was spelled, and a timer on another clock is `clock`.
    let mut want = vec![
        "clock",
        "clock",
        "guard_condition",
        "in_group",
        "in_group",
        "oneshot",
        "wall",
        "wall",
    ];
    want.sort_unstable();
    assert_eq!(
        kinds, want,
        "one row per Rust timer and guard condition, each of its own kind: {json}"
    );
}

// Issue 2984 -- the declarative route too, counted once.
struct Ticker;

impl nros::Component for Ticker {
    const NAME: &'static str = "ticker";
    const ENTITY_BOUNDS: nros::EntityBounds = nros::EntityBounds::exact(0, 0, 0, 0, 0);

    fn register(ctx: &mut nros::NodeContext<'_>) -> nros::NodeResult<()> {
        let mut node = ctx.create_node(nros::NodeOptions::new("ticker"))?;
        let _t = node.create_timer_for_callback_name("on_tick", TimerDuration::from_millis(15))?;
        Ok(())
    }
}

impl nros::ExecutableNode for Ticker {
    type State = ();
    fn init() -> Self::State {}
    fn on_callback(_state: &mut (), _cb: nros::Callback<'_>, _ctx: &mut nros::CallbackCtx<'_>) {}
    fn tick(_state: &mut (), _ctx: &mut nros::TickCtx<'_>) {}
    const HAS_TICK: bool = false;
}

nros::node!(Ticker);

/// A declared timer is one row: the executor funnel reports it, and the
/// declarative install path no longer reports it again. Before, that path
/// called the census hook itself; with the funnel also reporting, a hook left
/// there would make this two rows.
#[test]
fn a_declared_rust_timer_is_one_census_row() {
    let _ = nros_rmw_metadata::nros_rmw_metadata_register();
    nros::metadata_mode::reset();
    nros::metadata_mode::listen_for_registrations();

    let config = nros::ExecutorConfig::new("").rmw("metadata");
    let executor = nros::Executor::open(&config).expect("the metadata backend must open");
    let mut runtime = nros::__macro::ExecutorNodeRuntime::from_executor(executor);
    {
        let mut ctx = nros::__macro_support::nros_platform::RuntimeCtx::with_runtime(&mut runtime);
        register(&mut ctx).expect("the component registers");
    }

    let export = nros::node_metadata::SourceMetadataExport::new("fixture_pkg", "declarative")
        .executable("declarative")
        .language("rust");
    let json = nros::metadata_mode::to_json(&export).expect("serialize");
    assert_eq!(
        timer_kinds(&json),
        ["wall"],
        "one row for one declared timer: {json}"
    );
    nros::metadata_mode::reset();
}
