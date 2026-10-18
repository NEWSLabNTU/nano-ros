//! phase-462 W4 -- a contract MODE lowers to table variants, never rebuilt at
//! run time.
//!
//! The phase's gate, followed through the one derivation every road shares
//! (`nros_orchestration_ir::mode_variants`):
//!
//! 1. an image with two modes bakes two variants of its monitor table, the
//!    base's rows with each mode's numbers (W4a/W4b);
//! 2. and two SchedContext deadline sets: each mode's derived tier deadlines,
//!    the base's first, so entering the base again restores it (W4c);
//! 3. what a second table COSTS is its spec rows and its deadline rows --
//!    never a cell, which every variant shares with the base -- so
//!    `mem-report` prices a mode at the rows it re-states;
//! 4. a mode override on an entity the base does not have refuses at resolve
//!    (the negative control).
//!
//! The executor half -- a switch applies the deadline at the next spin and
//! `mode-switch-runtime` judges a slow one -- is proved against a real
//! executor in `nros-node`
//! (`a_mode_switch_moves_the_tier_deadline_and_judges_its_latency`); this
//! crate builds no executor under the command the phase names.

use std::collections::BTreeMap;

use nros_orchestration_ir::{
    CallbackGroupDecl,
    contract_monitors::{RenderPaths, render_monitor_rs_full},
    mode_variants::{
        ModeError, ModeExtras, all_variants, deadline_rows, mode_tier_deadlines,
        render_mode_tables_rs,
    },
};
use ros_launch_manifest_model::SystemModel;

/// Two timer nodes, one contracted publisher, and `modes` under
/// `contracts.modes`.
fn model(modes: &str) -> SystemModel {
    let yaml = format!(
        r#"
meta:
  version: 1
structure:
  nodes:
    /control_node:
      scope: /
    /telem_node:
      scope: /
  topics:
    /cmd:
      type: std_msgs/Int32
      pub: [/control_node/cmd]
    /status:
      type: std_msgs/Int32
      pub: [/telem_node/status]
contracts:
  pub_endpoints:
    /control_node/cmd:
      min_rate_hz: 100.0
    /telem_node/status: {{}}
  node_paths:
    /control_node/loop:
      output: [/control_node/cmd]
      trigger: {{ kind: timer, value: {{ rate_hz: 100.0 }} }}
      max_latency_ms: 5.0
    /telem_node/loop:
      output: [/telem_node/status]
      trigger: {{ kind: timer, value: {{ rate_hz: 10.0 }} }}
      max_latency_ms: 100.0
  modes:
{modes}
"#
    );
    SystemModel::from_yaml_str(&yaml).expect("fixture model")
}

fn ov(table: &str, key: &str, field: &str, value: &str) -> String {
    format!(
        "      - target: x\n        value: '{value}'\n        resolved: \
         {{ table: {table}, key: '{key}', field: {field} }}\n"
    )
}

/// `degraded` halves the control rate and relaxes its path; `tight` keeps
/// the rate and tightens the path.
fn two_modes() -> SystemModel {
    model(&format!(
        "    degraded:\n      overrides:\n{}{}    tight:\n      overrides:\n{}",
        ov("pub_endpoints", "/control_node/cmd", "min_rate_hz", "50"),
        ov("node_paths", "/control_node/loop", "max_latency", "8ms"),
        ov("node_paths", "/control_node/loop", "max_latency", "2ms"),
    ))
}

fn groups() -> BTreeMap<String, Vec<CallbackGroupDecl>> {
    let g = |id: &str| {
        vec![CallbackGroupDecl {
            id: id.to_string(),
            r#type: "MutuallyExclusive".to_string(),
            tier: id.to_string(),
        }]
    };
    BTreeMap::from([
        ("control_node".to_string(), g("ctrl")),
        ("telem_node".to_string(), g("telem")),
    ])
}

#[test]
fn two_modes_bake_two_monitor_variants_and_two_deadline_sets() {
    let m = two_modes();
    let variants = all_variants(&m).expect("both modes lower");
    let names: Vec<&str> = variants.iter().map(|(v, _, _)| v.name.as_str()).collect();
    assert_eq!(names, ["degraded", "tight"]);
    let rate = |i: usize| variants[i].1[0].min_rate_hz_milli;
    assert_eq!((rate(0), rate(1)), (50_000, 100_000), "each mode's numbers");

    let d = mode_tier_deadlines(&m, "posix", &groups()).expect("tiers lower");
    assert_eq!(d.tiers, ["derived-control_node"], "only the moved tier");
    assert_eq!(
        d.deadlines,
        [vec![5_000], vec![8_000], vec![2_000]],
        "the base's deadline, then each mode's"
    );
}

/// What a mode adds to the image: its re-stated spec rows and one deadline
/// row per moved context, each naming the base's cell, never a cell of its
/// own -- so the variant is priced at its rows.
#[test]
fn a_variant_shares_the_base_cells_and_adds_only_rows() {
    let m = two_modes();
    let variants = all_variants(&m).unwrap();
    let base_rows = nros_orchestration_ir::contract_monitors::monitor_rows(&m).unwrap();
    let mut text = render_monitor_rs_full(&base_rows, &[], &[], RenderPaths::NROS_NODE);
    let slices: Vec<_> = variants
        .iter()
        .map(|(_, r, a)| (r.as_slice(), a.as_slice()))
        .collect();
    let d = mode_tier_deadlines(&m, "posix", &groups()).unwrap();
    let extras = ModeExtras {
        deadlines: deadline_rows(&d, |t| (t == "derived-control_node").then_some(0)),
        switch: Vec::new(),
    };
    text.push_str(&render_mode_tables_rs(
        &slices,
        &extras,
        RenderPaths::NROS_NODE,
    ));
    let count = |needle: &str| text.matches(needle).count();
    assert_eq!(
        count("static NROS_MONITOR_CELL_"),
        base_rows.len(),
        "{text}"
    );
    assert_eq!(
        count("cell: &NROS_MONITOR_CELL_0"),
        3,
        "the base and both modes name one cell:\n{text}"
    );
    assert_eq!(count("pub static NROS_MODE_DEADLINES_"), 3, "{text}");
}

/// The negative control: an override naming an endpoint the base has not
/// declared is refused before anything bakes.
#[test]
fn a_mode_override_on_an_entity_the_base_lacks_refuses() {
    let m = model(&format!(
        "    ghost:\n      overrides:\n{}",
        ov("pub_endpoints", "/control_node/absent", "min_rate_hz", "5")
    ));
    let e = all_variants(&m).unwrap_err();
    assert!(
        matches!(&e, ModeError::MissingEntity { key, .. } if key == "/control_node/absent"),
        "{e}"
    );
    assert!(e.to_string().contains("cannot add one"), "{e}");
}
