//! issue 1426 — the four authoring shapes it measured, and what each one does
//! now. Precedence between an AUTHORED placement and an ALLOCATED one, per
//! FACT rather than per TABLE.
//!
//! The fixture is `examples/workspaces/derived-tiers-cpp` (phase-459 W0): four
//! C++ components in the shape of the Autoware Safety Island, one wall timer
//! each, two at 30 Hz and two at 10 Hz, `CALLBACK_GROUPS main` on every
//! registration. Every case below edits only its `system.toml`, which is the
//! whole authoring surface the issue is about.
//!
//! The rules are stated once, on
//! [`nros_orchestration_ir::derive::placement_is_unauthored`]; this file is the
//! gate that each of them is reachable from what a cmake image can write.
//!
//! # The bullet this file does NOT move
//!
//! `group_tiers` naming a tier with no `[tiers.<name>]` at all stays a
//! REFUSAL - rule 4's `None` arm. A typo in a tier name must not silently
//! become a derived tier, which is the same reason phase-459 W3 chose an
//! explicit marker over an undeclared name. What changed is that the author now
//! has somewhere to go: declaring the tier and writing NO platform sub-table is
//! rule 2, an allocation request, and needs no new key in the `[tiers.*]`
//! schema (`ros-launch-manifest`'s `TierDef` is `deny_unknown_fields`, so a new
//! key cannot land from this repository).

mod common;

use std::collections::BTreeMap;

use common::derived_tiers::{FAST, Fixture, SLOW, resolve_model, resolve_model_path};
use nros_cli_core::{
    codegen::entry::{Lang, Plan, emit, metadata, plan_from_model, resolve_plan_sched},
    orchestration::{
        cargo_metadata_schema::SystemToml, model_ingest, nros_config::NrosConfig,
        tier_resolver::collect_callback_groups,
    },
};
use nros_orchestration_ir::derive::{
    DerivedSchedule, derive_tiers_from_contracts, placement_is_unauthored,
};

/// The board these bakes are for. The fixture's `[image.zephyr]` names
/// `native_sim/native/64`, whose descriptor's platform gives this tier key.
const TARGET_RTOS: &str = "zephyr";

/// The Zephyr application pool this board's Kconfig resolves to (RFC-0079
/// section 4.1, phase-459 W4): the transport owns k_thread [0, 4], the
/// application [5, 14].
const POOL: (i64, i64) = (5, 14);

/// The fixture's own `system.toml`, as the bake loads it.
fn system_toml(fixture: &Fixture) -> SystemToml {
    let raw =
        std::fs::read_to_string(fixture.bringup().join("system.toml")).expect("read system.toml");
    toml::from_str(&raw).expect("system.toml parses")
}

/// What `codegen-system` does between collecting the groups and deriving.
fn bake(fixture: &Fixture, with_system: bool) -> (DerivedSchedule, bool) {
    let model = resolve_model(&fixture.bringup(), with_system);
    let system = system_toml(fixture);
    let cfg = NrosConfig::from_workspace(&fixture.bake_workspace()).expect("workspace");
    let groups = collect_callback_groups(&cfg, &system.components);
    let wanted = placement_is_unauthored(&model, TARGET_RTOS, &groups);
    (
        derive_tiers_from_contracts(&model, TARGET_RTOS, &groups),
        wanted,
    )
}

/// The whole bake, as `cmd::codegen_system` runs it: apply the model's
/// execution layer, then derive into what it left unauthored.
fn bake_into_system(fixture: &Fixture) -> eyre::Result<(SystemToml, usize)> {
    let model = resolve_model(&fixture.bringup(), true);
    let mut system = system_toml(fixture);
    let cfg = NrosConfig::from_workspace(&fixture.bake_workspace()).expect("workspace");
    let groups = collect_callback_groups(&cfg, &system.components);
    model_ingest::apply_model_execution(&mut system, &model, TARGET_RTOS, &groups)?;
    let mut n = 0;
    if placement_is_unauthored(&model, TARGET_RTOS, &groups) {
        let (derived, _) = model_ingest::derive_execution_from_contracts(
            &mut system,
            &model,
            TARGET_RTOS,
            &groups,
        )?;
        n = derived;
    }
    Ok((system, n))
}

/// `cmd::codegen::run_entry`'s own sequence for a typed C++ Zephyr entry.
fn entry_plan(fixture: &Fixture, with_system: bool) -> Plan {
    let meta = fixture.configure(&["main"]);
    let model = resolve_model_path(&fixture.bringup(), with_system);
    let mut plan =
        plan_from_model(&model, Some(TARGET_RTOS.to_string())).expect("plan from the model");
    let index = metadata::ComponentIndex::load(&meta).expect("the cmake metadata loads");
    metadata::enrich_plan(&mut plan, &index).expect("enrich from the cmake metadata");
    resolve_plan_sched(&mut plan, TARGET_RTOS).expect("resolve (and derive what is unauthored)");
    plan
}

/// `node -> the Zephyr priority its resolved tier carries`.
fn priorities(plan: &Plan) -> BTreeMap<String, i64> {
    let table = plan
        .resolved_tiers
        .as_ref()
        .expect("a tiered entry resolves a tier table");
    let mut out = BTreeMap::new();
    for tier in &table.tiers {
        for (node, _group) in &tier.members {
            out.insert(node.clone(), tier.priority);
        }
    }
    out
}

// ============================================================================
// bullet 1 - `group_tiers` naming an undeclared tier: still refused
// ============================================================================

/// issue 1426 bullet 1, UNCHANGED and deliberately so.
///
/// `group_tiers = { main = "ctrl" }` with no `[tiers.ctrl]` anywhere is a
/// binding to a tier that does not exist. It was refused before this fix and it
/// is refused after: the alternative is that `ctrll` becomes a derived tier and
/// the author never learns they misspelled `ctrl`.
///
/// The refusal may come from either half of the chain - `nros-launch-resolve`
/// reading the `system.toml` it was handed, or `apply_model_execution` reading
/// the model - so this asserts that ONE of them refuses and that the message
/// names the tier, rather than pinning which.
#[test]
fn a_binding_to_an_undeclared_tier_is_refused_by_name() {
    let fixture = Fixture::copy("prec-undeclared");
    fixture.configure(&["main"]);
    fixture.author("ctrl", &FAST, "");

    let resolver_refusal = std::process::Command::new(common::pinned_launch_resolver())
        .arg(fixture.bringup().join("launch/system.launch.xml"))
        .arg("--bringup-root")
        .arg(fixture.bringup())
        .arg("--system")
        .arg(fixture.bringup().join("system.toml"))
        .arg("-o")
        .arg(fixture.root.join("model.yaml"))
        .output()
        .expect("spawn nros-launch-resolve");

    let text = if resolver_refusal.status.success() {
        // The resolver accepted it; the bake must not.
        bake_into_system(&fixture)
            .expect_err("a binding to an undeclared tier must not bake")
            .to_string()
    } else {
        String::from_utf8_lossy(&resolver_refusal.stderr).to_string()
    };
    assert!(
        text.contains("ctrl"),
        "the refusal must name the tier the binding could not find, so a typo \
         reads as a typo: {text}"
    );
}

// ============================================================================
// bullet 2 - a DECLARED tier with no placement: the fix
// ============================================================================

/// issue 1426 bullet 2, FIXED. `[tiers.ctrl]` declared with no platform
/// sub-table is rule 2: an ALLOCATION REQUEST, not a table that disables the
/// derivation.
///
/// Before: `codegen_system.rs` derived only when `model.execution.tiers` was
/// empty, so declaring the tier a binding needs was exactly what turned the
/// derivation off - "a `group_tiers` binding needs a declared tier, and a
/// declared tier disables derivation", the dead end the issue describes.
#[test]
fn a_declared_tier_with_no_placement_has_its_priority_allocated() {
    let fixture = Fixture::copy("prec-unplaced");
    fixture.configure(&["main"]);
    // Every component binds its one group to `ctrl`; `ctrl` states a name and a
    // class and NO priority.
    let all: Vec<&str> = FAST.iter().chain(SLOW.iter()).copied().collect();
    fixture.author("ctrl", &all, "[tiers.ctrl]\nclass = \"real_time\"\n");

    let (derived, wanted) = bake(&fixture, true);
    assert!(
        wanted,
        "an authored tier with no `[tiers.ctrl.{TARGET_RTOS}]` is an unauthored \
         PLACEMENT, so the derivation must be wanted"
    );
    let spec = derived
        .placements
        .get("ctrl")
        .unwrap_or_else(|| panic!("`ctrl` must get an allocated placement: {derived:?}"));
    assert!(
        POOL.0 <= spec.priority && spec.priority <= POOL.1,
        "the allocated priority {} must be inside the board's application pool \
         {POOL:?} - a derived tier may not outrank the transport that feeds it \
         (issue 1427)",
        spec.priority
    );
    assert!(
        derived.groupless_notes.is_empty(),
        "every node declares `CALLBACK_GROUPS main`: {:?}",
        derived.groupless_notes
    );
    assert!(
        derived.shadowed.is_empty(),
        "nothing authored a placement, so nothing was shadowed: {:?}",
        derived.shadowed
    );

    // Four nodes over two ranks bound to ONE tier: the tier is one thread at one
    // priority, so it takes the more urgent rank and REPORTS the split it lost.
    let collapsed: Vec<&str> = derived
        .degradations
        .iter()
        .filter(|d| d.reason.contains("do not share a rank"))
        .map(|d| d.node.as_str())
        .collect();
    assert!(
        !collapsed.is_empty(),
        "the 30 Hz and 10 Hz pairs rank differently, so collapsing them into one \
         authored tier loses the rate-monotonic split and must be recorded: {:?}",
        derived.degradations
    );

    // And the whole bake installs it on the tier the author wrote, leaving the
    // head alone.
    let (system, n) = bake_into_system(&fixture).expect("the bake succeeds");
    assert_eq!(n, 1, "one tier had a priority allocated for it");
    let def = system.tiers.get("ctrl").expect("`ctrl` survives the merge");
    assert_eq!(
        def.class.as_deref(),
        Some("real_time"),
        "the authored head is untouched: {def:?}"
    );
    assert_eq!(
        def.zephyr.as_ref().map(|z| z.priority),
        Some(spec.priority),
        "the allocated placement is installed on the authored tier: {def:?}"
    );
}

/// The negative control for rule 2, and the measurement of what the
/// table-shaped guard did with this exact input: with no allocation,
/// `[tiers.ctrl]` has no `[tiers.ctrl.<rtos>]` and `resolve_tiers` REFUSES.
///
/// So before this fix the acceptance image was not "built with the wrong
/// priority" - it could not be built at all. `codegen entry` saw a non-empty
/// `model.execution.tiers`, skipped the derivation, and handed `resolve_tiers` a
/// tier with a binding and no placement.
#[test]
fn without_the_allocation_an_unplaced_tier_cannot_resolve() {
    use std::collections::BTreeSet;

    use nros_orchestration_ir::{TierResolveError, resolve_tiers, tier_from_model};

    let fixture = Fixture::copy("prec-unplaced-refused");
    fixture.configure(&["main"]);
    let all: Vec<&str> = FAST.iter().chain(SLOW.iter()).copied().collect();
    fixture.author("ctrl", &all, "[tiers.ctrl]\n");

    let model = resolve_model(&fixture.bringup(), true);
    let system = system_toml(&fixture);
    let cfg = NrosConfig::from_workspace(&fixture.bake_workspace()).expect("workspace");
    // The groups as the bake sees them, with each one bound to `ctrl` by
    // `group_tiers` - i.e. exactly what `resolve_tiers` is given, minus the
    // allocation.
    let mut groups = collect_callback_groups(&cfg, &system.components);
    for decls in groups.values_mut() {
        for d in decls.iter_mut() {
            d.tier = "ctrl".to_string();
        }
    }
    let tiers = model
        .execution
        .tiers
        .iter()
        .map(|(name, t)| (name.clone(), tier_from_model(t, TARGET_RTOS)))
        .collect();
    let names: BTreeSet<&str> = all.iter().copied().collect();
    let err = resolve_tiers(&tiers, &[], &names, &groups, TARGET_RTOS)
        .expect_err("an unplaced tier cannot resolve without an allocation");
    assert!(
        matches!(err, TierResolveError::MissingRtosSpec { .. }),
        "the refusal is the missing sub-table, not something else: {err}"
    );
    let text = err.to_string();
    assert!(
        text.contains("allocated from the contract"),
        "and it must name the other way out, or the author has no route from \
         here: {text}"
    );
}

/// The same shape, through the ENTRY - which is where issue 1426 says a fix has
/// to land, because a derived tier that reaches only `nros-plan.json` reaches no
/// image. The generated C++ must end in `run_tiers` over the allocated table.
#[test]
fn the_entry_runs_an_allocated_authored_tier() {
    let fixture = Fixture::copy("prec-unplaced-entry");
    let all: Vec<&str> = FAST.iter().chain(SLOW.iter()).copied().collect();
    fixture.author("ctrl", &all, "[tiers.ctrl]\n");

    let plan = entry_plan(&fixture, true);
    let prio = priorities(&plan);
    let mut placed: Vec<&str> = prio.keys().map(String::as_str).collect();
    placed.sort_unstable();
    let mut want = all.clone();
    want.sort_unstable();
    assert_eq!(placed, want, "all four components are placed: {prio:?}");
    for (node, p) in &prio {
        assert!(
            POOL.0 <= *p && *p <= POOL.1,
            "{node} at {p} is outside the application pool {POOL:?}: {prio:?}"
        );
    }

    let src = emit::emit_typed(Lang::Cpp, &plan).expect("the typed C++ entry emits");
    assert!(
        src.contains("::nros::board::ZephyrBoard::run_tiers("),
        "an allocated tier must reach the IMAGE, not just the plan; src:\n{src}"
    );
    assert!(
        !src.contains("run_components("),
        "a tiered entry must not fall back to run_components; src:\n{src}"
    );
    assert!(
        src.contains("\"ctrl\""),
        "the baked table names the tier the author declared; src:\n{src}"
    );
}

// ============================================================================
// bullet 3 - groups with no bindings, model resolved without `--system`
// ============================================================================

/// issue 1426 bullet 3. A `system.toml` carrying `group_tiers`, resolved into a
/// model WITHOUT `--system`, produces a model with no bindings at all - and the
/// bake then refuses, because the `group_tiers` the author wrote reached no
/// node.
///
/// The refusal is CORRECT and stays: silently ignoring a binding is issue 0398,
/// and `nros sync` passes `--system` whenever the file exists (`cmd/ws.rs`), so
/// no supported road produces this pairing. It is asserted here because the
/// issue lists it, and because the message has to keep naming the component -
/// that is the only thing distinguishing it from "your tier is misspelled".
#[test]
fn a_binding_that_reached_no_node_is_refused_naming_the_component() {
    let fixture = Fixture::copy("prec-no-system");
    fixture.configure(&["main"]);
    fixture.author("ctrl", &["mrm_handler"], "[tiers.ctrl]\n");

    // `with_system: false` - the pairing the issue measured.
    let model = resolve_model(&fixture.bringup(), false);
    assert!(
        model.execution.bindings.is_empty(),
        "without `--system` the model carries no bindings: {:?}",
        model.execution.bindings
    );
    let mut system = system_toml(&fixture);
    let cfg = NrosConfig::from_workspace(&fixture.bake_workspace()).expect("workspace");
    let groups = collect_callback_groups(&cfg, &system.components);
    let err = model_ingest::apply_model_execution(&mut system, &model, TARGET_RTOS, &groups)
        .expect_err("a `group_tiers` that reached no node must be refused");
    let text = err.to_string();
    assert!(
        text.contains("mrm_handler"),
        "the refusal names the component whose binding was lost: {text}"
    );
}

// ============================================================================
// bullet 4 - an authored placement wins, and says what it beat
// ============================================================================

/// issue 1426 bullet 4. `[tiers.ctrl.zephyr] priority = 5` is rule 1: AUTHORED
/// wins verbatim. What changed is that it no longer wins SILENTLY, and no longer
/// wins for nodes that did not author it.
#[test]
fn an_authored_placement_wins_and_names_the_rank_it_beat() {
    let fixture = Fixture::copy("prec-authored");
    fixture.configure(&["main"]);
    fixture.author(
        "ctrl",
        &["mrm_handler"],
        "[tiers.ctrl]\n[tiers.ctrl.zephyr]\npriority = 9\n",
    );

    let (derived, wanted) = bake(&fixture, true);
    assert!(
        wanted,
        "three of the four nodes have no authored placement, so the derivation \
         is still wanted - which is the whole of issue 1426's second bullet"
    );

    // Rule 1: the authored number wins, and the rank it beat is recorded.
    let shadow = derived
        .shadowed
        .iter()
        .find(|s| s.node == "mrm_handler")
        .unwrap_or_else(|| {
            panic!(
                "an authored priority that beat a rank must be recorded: {:?}",
                derived.shadowed
            )
        });
    assert_eq!(shadow.tier, "ctrl");
    assert_eq!(
        shadow.authored, 9,
        "the recorded number is the one the author wrote: {shadow:?}"
    );
    assert!(
        POOL.0 <= shadow.allocated && shadow.allocated <= POOL.1,
        "and the rank it beat is a real pool address, so the two are comparable: \
         {shadow:?}"
    );
    assert!(
        !derived.placements.contains_key("ctrl"),
        "a PLACED tier is never given a second placement: {:?}",
        derived.placements
    );

    // Rule 4: the other three are allocated, which is what the table-shaped
    // guard prevented.
    let mut names: Vec<String> = derived.overrides.iter().map(|o| o.name.clone()).collect();
    names.sort();
    let mut want: Vec<String> = FAST
        .iter()
        .chain(SLOW.iter())
        .filter(|n| **n != "mrm_handler")
        .map(|n| n.to_string())
        .collect();
    want.sort();
    assert_eq!(
        names, want,
        "every node the author said nothing about gets its own derived tier: \
         {:?}",
        derived.overrides
    );
}

/// The negative control for the whole file: a workspace whose every node is
/// bound to an authored, PLACED tier derives NOTHING and prints nothing.
///
/// This is what keeps "authored wins" a rule rather than a preference. It is
/// also the byte-identical guarantee for every workspace that authors its
/// schedule today (`examples/workspaces/realtime-cpp` is the in-tree one).
#[test]
fn a_fully_authored_workspace_derives_nothing() {
    let fixture = Fixture::copy("prec-fully-authored");
    fixture.configure(&["main"]);
    let all: Vec<&str> = FAST.iter().chain(SLOW.iter()).copied().collect();
    fixture.author(
        "ctrl",
        &all,
        "[tiers.ctrl]\n[tiers.ctrl.zephyr]\npriority = 8\n",
    );

    let (derived, wanted) = bake(&fixture, true);
    assert!(
        !wanted,
        "every group names a tier with an authored `[tiers.ctrl.{TARGET_RTOS}]`, \
         so there is no unauthored placement and the derivation must not run"
    );
    assert!(
        derived.tiers.is_empty() && derived.overrides.is_empty(),
        "nothing may be derived beside a fully authored table: {:?}",
        derived.tiers.keys().collect::<Vec<_>>()
    );
    assert!(
        derived.placements.is_empty(),
        "and nothing may be allocated into it: {:?}",
        derived.placements
    );

    // The entry agrees: one authored tier, its authored number, all four nodes.
    let plan = entry_plan(&fixture, true);
    let prio = priorities(&plan);
    for node in &all {
        assert_eq!(
            prio.get(*node),
            Some(&8),
            "{node} runs at the authored priority: {prio:?}"
        );
    }
    assert_eq!(
        plan.tiers.keys().collect::<Vec<_>>(),
        vec!["ctrl"],
        "the authored table is the whole table when it authors every placement"
    );
}
