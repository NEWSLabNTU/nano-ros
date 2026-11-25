//! phase-566 W4: the Rust metadata probe constructs a component on its launch
//! values, as the C/C++ probe does (issue 2640).
//!
//! A Rust component reads its launch `<param>` values in `register()` with
//! `ctx.param(name)`, and an Autoware-shaped one sizes entities from them
//! (here: a timer at `ctrl_period_ms`). Before W4 the cargo harness ran
//! `register()` with no launch values, so every such read answered `None` and
//! the sidecar missed the timer. The harness now calls
//! `record_node_metadata_seeded_watching_prose` with the values D2 chose.
//!
//! Driven on an executor over the transport-free `metadata` backend, the one
//! the harness opens.

use nros::{
    Component, DeclarativeNode, NodeContext, NodeOptions, NodeResult, TimerDuration,
    node_metadata::{EntityKind, MetadataRecorder},
};

// The posix C port DEFINES the `nros_platform_*` symbols the executor calls.
extern crate nros_platform_cffi as _;

/// What `register` saw: `ctx.param`, and what the typed declaration read.
static SEEN: std::sync::Mutex<Vec<(Option<String>, Option<i64>)>> =
    std::sync::Mutex::new(Vec::new());

/// The recorder behind a metadata executor is process-global; one test at a
/// time records.
static CENSUS: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Creates its timer only when the launch gave it a period, and also declares
/// the same name the typed rclrs way, with no default.
struct Controller;

impl Component for Controller {
    const NAME: &'static str = "controller";
    const ENTITY_BOUNDS: nros::EntityBounds = nros::EntityBounds::exact(0, 0, 1, 0, 0);

    fn register(ctx: &mut NodeContext<'_>) -> NodeResult<()> {
        let launch = ctx.param("ctrl_period_ms").map(str::to_owned);
        let mut node = ctx.create_node(NodeOptions::new("controller"))?;
        // The board's rule: with no parameter store before `register`, the
        // seeds are not in it, so this declaration is the name's first and
        // must not be refused. A seed in the store would refuse it
        // (`AlreadyDeclared`, issue 2969), failing the probe of a component
        // whose image boots.
        let typed = node
            .declare_parameter::<i64>("ctrl_period_ms")
            .expect("the store opens")
            .optional()
            .expect("ctrl_period_ms declares: a seed must not pre-empt it")
            .get();
        SEEN.lock().unwrap().push((launch.clone(), typed));
        if let Some(ms) = launch
            .and_then(|v| v.parse::<u64>().ok())
            .filter(|ms| *ms > 0)
        {
            let _timer =
                node.create_timer_for_callback_name("on_control", TimerDuration::from_millis(ms))?;
        }
        Ok(())
    }
}

fn probe(seeds: &[(&str, &str)]) -> (MetadataRecorder, (Option<String>, Option<i64>)) {
    let _census = CENSUS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let _ = nros_rmw_metadata::nros_rmw_metadata_register();
    let config = nros::ExecutorConfig::new("").rmw("metadata");
    let mut executor = nros::Executor::open(&config).expect("the metadata backend must open");
    let mut recorder = MetadataRecorder::default();
    SEEN.lock().unwrap().clear();
    nros::__macro::record_node_metadata_seeded_watching_prose::<Controller>(
        &mut recorder,
        &mut executor,
        seeds,
    )
    .expect("the component registers");
    let seen = SEEN.lock().unwrap().pop().expect("register ran");
    (recorder, seen)
}

fn timer_periods(recorder: &MetadataRecorder) -> Vec<Option<u64>> {
    recorder
        .entities()
        .iter()
        .filter(|e| e.kind == EntityKind::Timer)
        .map(|e| e.period_us)
        .collect()
}

#[test]
fn a_seeded_probe_constructs_the_node_on_its_launch_value() {
    let (recorder, (launch, typed)) = probe(&[("ctrl_period_ms", "20")]);
    assert_eq!(launch.as_deref(), Some("20"), "`ctx.param` reads the seed");
    assert_eq!(
        timer_periods(&recorder),
        vec![Some(20_000)],
        "the sidecar records the timer the launch value creates"
    );
    assert_eq!(
        typed, None,
        "no store existed before `register`, so as on a board without \
         `[param_services]` the seed is not in it"
    );
}

/// The control: no seeds is the probe before W4, and the rule for a component
/// no launch names. The read answers nothing, so no timer.
#[test]
fn an_unseeded_probe_runs_on_the_declaration_defaults() {
    let (recorder, (launch, typed)) = probe(&[]);
    assert_eq!(launch, None);
    assert_eq!(typed, None);
    assert!(
        timer_periods(&recorder).is_empty(),
        "no launch value, no timer: {:?}",
        recorder.entities()
    );
}
