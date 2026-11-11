//! issue 2432 - one model, one contract, three roads: do they derive the SAME
//! schedule?
//!
//! Three roads run the rate-monotonic derivation of RFC-0079 over one resolved
//! SystemModel, all through the shared core `nros_orchestration_ir::derive`:
//!
//! * `codegen entry` (C/C++): `plan_from_model` -> `enrich_plan` ->
//!   `resolve_plan_sched`, i.e. `cmd::codegen::run_entry`'s sequence;
//! * `codegen-system` (the Zephyr bake): `collect_callback_groups` ->
//!   `apply_model_execution` -> `derive_execution_from_contracts` ->
//!   `resolve_system_sched_tasks`;
//! * `nros::main!` (Rust): the callback-group map from
//!   `entry_groups::model_node_groups` - the function the macro itself calls -
//!   then the same `declined` / derive / `resolve_sched_tasks` steps the macro
//!   takes.
//!
//! The core was shared and its INPUT was not: issue 2432 measured C and C++
//! deriving two sched tasks from one workspace while Rust derived none, silently.
//! This file compares the three resolved tables (`sched task -> (priority,
//! members)`) for the fixture `examples/workspaces/derived-sched-tasks-cpp`:
//! four components, two at 30 Hz and two at 10 Hz, one callback group `main`
//! each, and a `system.toml` that authors no `[tasks.*]`.
//!
//! Nothing here compiles anything. The cmake metadata and the model are
//! written / resolved the way `common::derived_sched_tasks` does for its
//! siblings.

mod common;

use std::collections::BTreeMap;

use common::derived_sched_tasks::{FAST, Fixture, SLOW, resolve_model, resolve_model_path};
use nros_cli_core::{
    codegen::entry::{metadata, plan_from_model, resolve_plan_sched},
    orchestration::{
        cargo_metadata_schema::SystemToml,
        model_ingest,
        nros_config::NrosConfig,
        sched_task_resolver::{collect_callback_groups, resolve_system_sched_tasks},
    },
};
use nros_orchestration_ir::{
    ResolvedSchedTaskTable,
    entry_groups::{Declined, declined, model_node_groups},
};

/// The board these bakes are for (the fixture's `[image.zephyr]`).
const TARGET_RTOS: &str = "zephyr";

/// `sched task -> (priority, sorted member node names)`.
type Table = BTreeMap<String, (i64, Vec<String>)>;

fn table(t: &ResolvedSchedTaskTable) -> Table {
    t.sched_tasks
        .iter()
        .map(|s| {
            let mut members: Vec<String> = s.members.iter().map(|(n, _)| n.clone()).collect();
            members.sort();
            members.dedup();
            (s.name.clone(), (s.priority, members))
        })
        .collect()
}

/// The C/C++ entry road.
fn c_entry(fixture: &Fixture) -> Table {
    let meta = fixture.configure(&["main"]);
    let model = resolve_model_path(&fixture.bringup(), true);
    let mut plan =
        plan_from_model(&model, Some(TARGET_RTOS.to_string())).expect("plan from the model");
    let index = metadata::ComponentIndex::load(&meta).expect("the cmake metadata loads");
    metadata::enrich_plan(&mut plan, &index).expect("enrich from the cmake metadata");
    resolve_plan_sched(&mut plan, TARGET_RTOS).expect("resolve (and derive)");
    table(
        plan.resolved_sched_tasks
            .as_ref()
            .expect("the entry resolves a sched task table"),
    )
}

/// The `codegen-system` road.
fn codegen_system(fixture: &Fixture) -> Table {
    fixture.configure(&["main"]);
    let model = resolve_model(&fixture.bringup(), true);
    let raw =
        std::fs::read_to_string(fixture.bringup().join("system.toml")).expect("read system.toml");
    let mut system: SystemToml = toml::from_str(&raw).expect("system.toml parses");
    let cfg = NrosConfig::from_workspace(&fixture.bake_workspace()).expect("workspace");
    let groups = collect_callback_groups(&cfg, &system.components);
    model_ingest::apply_model_execution(&mut system, &model, TARGET_RTOS, &groups)
        .expect("apply the model's execution layer");
    if declined(&model, TARGET_RTOS, &groups).is_none() {
        model_ingest::derive_execution_from_contracts(&mut system, &model, TARGET_RTOS, &groups)
            .expect("derive");
    }
    table(&resolve_system_sched_tasks(&system, &groups, TARGET_RTOS).expect("resolve"))
}

/// The `nros::main!` road: its group map, and either its decline or its table.
fn rust_entry(fixture: &Fixture) -> Result<Table, Declined> {
    let model = resolve_model(&fixture.bringup(), true);
    let keep = |fqn: &str| nros_orchestration_ir::overlay::placed_here(&model, fqn);
    let groups = model_node_groups(&model, keep);
    if let Some(why) = declined(&model, TARGET_RTOS, &groups) {
        return Err(why);
    }
    let derived = nros_orchestration_ir::derive::derive_sched_tasks_from_contracts(
        &model,
        TARGET_RTOS,
        &groups,
    );
    let mut sched_tasks: BTreeMap<String, nros_orchestration_ir::SchedTaskDef> = model
        .execution
        .tasks
        .iter()
        .map(|(n, t)| {
            (
                n.clone(),
                nros_orchestration_ir::sched_task_from_model(t, TARGET_RTOS),
            )
        })
        .collect();
    sched_tasks.extend(derived.sched_tasks);
    for (name, spec) in derived.placements {
        if let Some(def) = sched_tasks.get_mut(&name) {
            nros_orchestration_ir::install_placement(def, TARGET_RTOS, spec);
        }
    }
    let names: Vec<String> = model
        .structure
        .nodes
        .keys()
        .filter(|f| keep(f))
        .map(|f| f.rsplit('/').next().unwrap_or(f).to_string())
        .collect();
    let component_names = names.iter().map(String::as_str).collect();
    let resolved = nros_orchestration_ir::resolve_sched_tasks(
        &sched_tasks,
        &derived.overrides,
        &component_names,
        &groups,
        TARGET_RTOS,
    )
    .expect("resolve");
    Ok(table(&resolved))
}

#[test]
fn every_road_derives_the_same_schedule_from_one_model() {
    let fixture = Fixture::copy("parity");
    let c = c_entry(&fixture);
    let system = codegen_system(&fixture);

    // The C/C++ answer, pinned: one derived sched task per component, the
    // 30 Hz pair more urgent (Zephyr: the smaller number) than the 10 Hz pair.
    let derived: Vec<(&String, &(i64, Vec<String>))> = c
        .iter()
        .filter(|(n, _)| n.starts_with("derived-"))
        .collect();
    assert!(
        !derived.is_empty(),
        "codegen entry derived no sched task from a contract that states rates: {c:#?}"
    );
    let prio = |node: &str| {
        c.values()
            .find(|(_, m)| m.iter().any(|n| n == node))
            .map(|(p, _)| *p)
            .unwrap_or_else(|| panic!("{node} is in no sched task: {c:#?}"))
    };
    for fast in FAST {
        for slow in SLOW {
            assert!(
                prio(fast) < prio(slow),
                "30 Hz {fast} must outrank 10 Hz {slow} on Zephyr: {c:#?}"
            );
        }
    }

    assert_eq!(
        system, c,
        "codegen-system and codegen entry derived different tables from one model"
    );

    // KNOWN DIVERGENCE - issue 2432. The Rust road builds its group map from
    // the model's bindings only, and this workspace (like the issue's) binds
    // no group: the groups live in the code, which only the cmake keyword
    // carries to the C/C++ roads. So the macro declines and the image runs on
    // one task. Asserted EXPLICITLY so this goes red when either side moves:
    // when the roads agree, replace it with `assert_eq!(rust, Ok(c))`.
    let rust = rust_entry(&fixture);
    assert_eq!(
        rust,
        Err(Declined::NoCallbackGroups),
        "issue 2432's divergence changed - if nros::main! now derives, assert it \
         derives THIS table: {c:#?}"
    );
}
