//! phase-459 W1 (issue 1426) - the cmake road reaches the rate-monotonic
//! derivation.
//!
//! The fixture is `examples/workspaces/derived-tiers-cpp` (phase-459 W0): four
//! `SHAPE rclcpp` C++ components in the shape of the Autoware Safety Island,
//! one wall timer each, two at 30 Hz and two at 10 Hz, `CALLBACK_GROUPS main`
//! on every registration, and a `system.toml` that authors no `group_tiers`
//! and no `[tiers.*]`.
//!
//! What issue 1426 measured on exactly that shape: the derivation is complete
//! code with tests, and no authored input reaches it. `codegen-system`
//! collected groups from `[[component]].group_tiers` (which a workspace
//! deriving its schedule does not write) and from `cfg.component_packages`
//! (empty for every workspace with no root `Cargo.toml`), so all four nodes
//! arrived at `derive_tiers_from_contracts` groupless, every one of them went
//! into `groupless_notes`, and the derived schedule was empty. W1 adds the
//! third source - the `nros-metadata.json` a configure writes from the cmake
//! keyword - and this asserts the before and the after on one fixture.
//!
//! The fixture and its `nros-metadata.json` writer live in
//! `common::derived_tiers` (issue 1426 folded three copies into one).
//!
//! # What is asserted
//!
//! RANKS, MEMBERSHIP, and - since phase-459 W4 landed (issue 1427) - the
//! PRIORITY NUMBERS, against the plan the island's own Kconfig resolves rather
//! than as literals. This header used to say the opposite, and its reason was
//! good while it held: `rank_to_priority` mapped dense rank 0 to Zephyr
//! priority 0, above the transport threads that feed the application, so a
//! test pinning 0 and 1 would have read as if 0 were the intended answer.
//! W4 made the allocation come out of the board's application pool, and
//! `the_derived_table_lands_below_the_transport_band` is the end-to-end half of
//! that: the unit tests prove the realizer allocates inside a pool, this proves
//! the numbers a BAKE of the W0 fixture produces are in the pool the island's
//! `.config` implies, and below the band its transport threads sit in.

mod common;

use std::{collections::BTreeMap, fs, path::Path};

use common::derived_tiers::{FAST, Fixture, SLOW, resolve_model};
use nros_cli_core::orchestration::{
    cargo_metadata_schema::SystemToml, model_ingest, nros_config::NrosConfig,
    tier_resolver::collect_callback_groups,
};
use nros_orchestration_ir::{
    derive::{DerivedSchedule, derive_tiers_from_contracts},
    priority_plan::{Band, PriorityPlan},
};
use ros_launch_manifest_model::SystemModel;

/// The board this bake is for. The fixture's `[image.zephyr]` names
/// `native_sim/native/64`, whose descriptor's platform gives this tier key.
const TARGET_RTOS: &str = "zephyr";

/// The fixture's own `system.toml`, as the bake loads it.
fn system_toml(bringup: &Path) -> SystemToml {
    let raw = fs::read_to_string(bringup.join("system.toml")).expect("read system.toml");
    toml::from_str(&raw).expect("system.toml parses")
}

/// What `codegen-system` does between the two, with nothing in between: collect
/// the groups from the workspace, then derive.
fn derive(fixture: &Fixture, model: &SystemModel, system: &SystemToml) -> DerivedSchedule {
    let cfg = NrosConfig::from_workspace(&fixture.bake_workspace())
        .expect("the fixture's src/ tree loads as a workspace");
    let groups = collect_callback_groups(&cfg, &system.components);
    derive_tiers_from_contracts(model, TARGET_RTOS, &groups)
}

/// `node name -> the Zephyr priority its derived tier carries`, from the
/// schedule's overrides and tiers. Read as an ORDER, never as a number.
fn priorities(derived: &DerivedSchedule) -> BTreeMap<String, i64> {
    let mut out = BTreeMap::new();
    for ov in &derived.overrides {
        let tier_name = &ov
            .callback_groups
            .first()
            .expect("a derived override binds at least one group")
            .tier;
        let tier = derived
            .tiers
            .get(tier_name)
            .unwrap_or_else(|| panic!("override names tier `{tier_name}`, which is not derived"));
        let spec = tier
            .zephyr
            .as_ref()
            .unwrap_or_else(|| panic!("tier `{tier_name}` has no [tiers.*.zephyr] sub-table"));
        out.insert(ov.name.clone(), spec.priority);
    }
    out
}

/// The wave, on the fixture: the keyword reaches the bake, every node is
/// placed, and the placement is rate-monotonic.
#[test]
fn the_cmake_keyword_makes_the_fixture_derive_a_schedule() {
    let fixture = Fixture::copy("bake-keyword");
    fixture.configure(&["main"]);
    let model = resolve_model(&fixture.bringup(), false);
    let system = system_toml(&fixture.bringup());
    let derived = derive(&fixture, &model, &system);

    assert!(
        derived.groupless_notes.is_empty(),
        "every node declares `CALLBACK_GROUPS main`, so none may be groupless: {:?}",
        derived.groupless_notes
    );
    assert_eq!(
        derived.overrides.len(),
        4,
        "four components, four placements: {:?}",
        derived.overrides
    );

    let prio = priorities(&derived);
    let mut placed: Vec<&str> = prio.keys().map(String::as_str).collect();
    placed.sort_unstable();
    let mut want: Vec<&str> = FAST.iter().chain(SLOW.iter()).copied().collect();
    want.sort_unstable();
    assert_eq!(placed, want, "the four members of the derived schedule");

    // RANKS, not numbers (see the module header). Zephyr is
    // `low_number_is_high`, so "more urgent" is a SMALLER priority.
    let fast: Vec<i64> = FAST.iter().map(|n| prio[*n]).collect();
    let slow: Vec<i64> = SLOW.iter().map(|n| prio[*n]).collect();
    assert_eq!(
        fast[0], fast[1],
        "equal periods share a rank (the pinned ranker gives one `fine_group` \
         one rank); the two 30 Hz nodes must land together: {prio:?}"
    );
    assert_eq!(
        slow[0], slow[1],
        "the two 10 Hz nodes must land together: {prio:?}"
    );
    assert!(
        fast[0] < slow[0],
        "rate-monotonic: 30 Hz outranks 10 Hz. Zephyr counts down, so the fast \
         pair must carry the smaller number: {prio:?}"
    );

    // Exactly two ranks over the four nodes - the island's projection, and the
    // shape every later wave's gate reads.
    let ranks: std::collections::BTreeSet<i64> = prio.values().copied().collect();
    assert_eq!(ranks.len(), 2, "two ranks over four nodes: {prio:?}");

    // Every derived tier is a real `[tiers.*]` row the rest of the pipeline can
    // consume: one per node, each carrying the Zephyr sub-table the target
    // selects. (W3 is the wave that names them by rank instead.)
    assert_eq!(
        derived.tiers.len(),
        4,
        "`derive_tiers_from_contracts` names a tier per NODE: {:?}",
        derived.tiers.keys().collect::<Vec<_>>()
    );
}

/// The bake's own count, through the wrapper `codegen-system` calls - the line
/// an operator sees, and the mutation the rest of the pipeline consumes.
#[test]
fn the_bake_reports_the_derived_tiers_and_binds_every_node() {
    let fixture = Fixture::copy("bake-count");
    fixture.configure(&["main"]);
    let model = resolve_model(&fixture.bringup(), false);
    let mut system = system_toml(&fixture.bringup());
    assert!(
        system.tiers.is_empty(),
        "the fixture authors no tier; derivation only runs on an empty table"
    );

    let cfg = NrosConfig::from_workspace(&fixture.bake_workspace()).expect("workspace");
    let groups = collect_callback_groups(&cfg, &system.components);
    let (derived, warnings) =
        model_ingest::derive_execution_from_contracts(&mut system, &model, TARGET_RTOS, &groups)
            .expect("the derived bake succeeds");

    assert_eq!(derived, 4, "`derived {derived} scheduling tier(s)`");
    assert!(
        warnings.is_empty(),
        "no path declares a deadline or a budget, so the realizer weakens no \
         guarantee: {warnings:?}"
    );
    // The bake writes the derivation back as ordinary rows, which is what makes
    // `resolve_system_tiers` -> `run_tiers` consume it unchanged.
    assert_eq!(system.tiers.len(), 4);
    assert_eq!(system.node_overrides.len(), 4);
    for ov in &system.node_overrides {
        assert_eq!(
            ov.callback_groups.len(),
            1,
            "`CALLBACK_GROUPS main` declares one group: {ov:?}"
        );
        assert_eq!(ov.callback_groups[0].id, "main");
    }
}

/// The island's Zephyr `.config`, as the fixture mirrors it: 15 preemptive
/// priorities, both Kconfig gates on. The two transport bands are the ones the
/// image creates its zenoh read and lease tasks at - Kconfig's default 200, and
/// the 255 the island raises the lease to.
///
/// Written out here rather than read from a build tree on purpose: this repo
/// does not compile inside tests, so there is no `.config` to read, and the
/// checker that DOES read one (`scripts/check-tier-priority-plan-image.py`,
/// run by `just zephyr build-fixtures`) is the half that judges a built image.
/// Two implementations of one arithmetic, one of them a test - RFC-0079 §4.1.
const ISLAND_DOTCONFIG: &str = "CONFIG_NUM_PREEMPT_PRIORITIES=15\n\
     CONFIG_NUM_COOP_PRIORITIES=16\n\
     CONFIG_POSIX_PRIORITY_SCHEDULING=y\n\
     CONFIG_PREEMPT_ENABLED=y\n";
const ISLAND_TRANSPORT_BANDS: [i64; 2] = [200, 255];

/// issue 1427, end to end: the numbers a bake of the W0 fixture produces sit
/// INSIDE the application pool the island's image resolves, and therefore below
/// the k_thread priorities its transport threads run at.
///
/// The two unit-test halves (`priority_plan_allocates_inside_the_application_pool`,
/// `an_empty_application_pool_derives_no_tier_rather_than_a_reserved_one`) prove
/// the realizer's rule. This proves the rule survives the whole road the
/// operator drives - launch file -> resolved model -> callback groups ->
/// `derive_tiers_from_contracts` -> `[tiers.*.zephyr] priority` - which is where
/// issue 1427 was measured, and where a green unit test would not have caught
/// a bake that passed a different plan.
#[test]
fn the_derived_table_lands_below_the_transport_band() {
    let fixture = Fixture::copy("band");
    fixture.configure();
    let model = resolve_model(&fixture.bringup());
    let system = system_toml(&fixture.bringup());
    let derived = derive(&fixture, &model, &system);
    let prio = priorities(&derived);

    // The plan the island's image resolves, by the same arithmetic the bake
    // projects from Kconfig's defaults.
    let plan = PriorityPlan::from_zephyr_dotconfig(ISLAND_DOTCONFIG, &ISLAND_TRANSPORT_BANDS)
        .expect("the island's .config resolves a plan");
    let transport = plan.reserved["transport"];
    assert_eq!(transport, Band::new(0, 4), "the measured island band");
    assert_eq!(plan.app, Band::new(5, 14), "the measured island pool");

    for (node, p) in &prio {
        assert!(
            plan.app.contains(*p),
            "{node} derived priority {p} is outside the island's pool.app {:?} \
             (transport {transport:?}) - this is issue 1427's inversion: {prio:?}",
            plan.app
        );
        // Zephyr counts DOWN, so "below the transport band" is a LARGER number.
        assert!(
            *p > transport.hi,
            "{node} at {p} is at least as urgent as the transport's least urgent \
             thread ({}) - a derived tier must never preempt the link it publishes \
             over (issues 0623, 1427)",
            transport.hi
        );
    }
    // The measured allocation, for the record and for the next reader: the two
    // 30 Hz components take the pool's most urgent address, the two 10 Hz ones
    // the next. Not 0 and 1, which is what this bake produced before W4.
    let fast: Vec<i64> = FAST.iter().map(|n| prio[*n]).collect();
    let slow: Vec<i64> = SLOW.iter().map(|n| prio[*n]).collect();
    assert_eq!(
        fast,
        vec![5, 5],
        "30 Hz rank -> pool.app's urgent end: {prio:?}"
    );
    assert_eq!(
        slow,
        vec![6, 6],
        "10 Hz rank -> the next one down: {prio:?}"
    );
    assert!(
        !derived.degradations.iter().any(|d| d.dim == "priority"),
        "two ranks fit a ten-wide pool with nothing compressed: {:?}",
        derived.degradations
    );
}

/// The negative control: the same fixture with the keyword removed - which,
/// for a source the cmake road carries only in `nros-metadata.json`, is a
/// workspace that never configured one.
///
/// This is the state issue 1426 measured. It must still be a groupless note
/// per node and an empty schedule, because the fix is a new SOURCE and not a
/// new default: a node that declares no group has nothing for the gating
/// executor to bind, and inventing a group for it would place code on a tier
/// nobody asked for.
#[test]
fn without_the_keyword_every_node_is_groupless_and_nothing_derives() {
    let fixture = Fixture::copy("bake-no-keyword");
    let model = resolve_model(&fixture.bringup(), false);
    let system = system_toml(&fixture.bringup());
    let derived = derive(&fixture, &model, &system);

    assert!(
        derived.tiers.is_empty() && derived.overrides.is_empty(),
        "nothing may be derived: {:?}",
        derived.tiers.keys().collect::<Vec<_>>()
    );
    let mut notes: Vec<&str> = derived
        .groupless_notes
        .iter()
        .map(|n| n.rsplit('/').next().unwrap_or(n))
        .collect();
    notes.sort_unstable();
    let mut want: Vec<&str> = FAST.iter().chain(SLOW.iter()).copied().collect();
    want.sort_unstable();
    assert_eq!(
        notes, want,
        "one groupless note per node (issue 1371's persisted form)"
    );
}
