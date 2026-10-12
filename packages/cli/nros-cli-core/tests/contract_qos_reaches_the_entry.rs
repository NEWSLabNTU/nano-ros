//! issue 1854 -- a contract-only QoS change reaches the generated C++ entry.
//!
//! The pinned resolver derives the contract's runtime-only QoS (deadline,
//! liveliness, lease) into `qos_overrides.<topic>.<role>.<policy>` parameters
//! on the node, the way rclcpp's `QosOverridingOptions` would read them. The
//! entry lowers those into its baked override table. Until issue 1854 the
//! C++ pack rendered that table only for a `configure`-shape node: an
//! `rclcpp`-shape component (the Autoware Safety Island's three nodes are all
//! that shape) got a handle with no table, so a contract QoS that differed
//! from the code's was silently not applied.
//!
//! One launch file, two contracts that differ ONLY in the subscriber's
//! `liveliness:` line. The test runs `nros codegen entry --lang cpp --typed`'s
//! own sequence in process (`plan_from_model` -> `emit::emit_typed`) on both
//! models and asserts the entries differ by exactly the baked table, on the
//! node the contract names.
//!
//! Run with:
//! `cargo test --manifest-path packages/cli/Cargo.toml --test contract_qos_reaches_the_entry`

mod common;

use std::{
    fs,
    path::{Path, PathBuf},
};

use nros_cli_core::codegen::entry::{Lang, Plan, emit, plan_from_model};

/// Resolve `launch/<stem>.launch.xml` with its `<stem>.contract.yaml` sidecar
/// through the pinned resolver, then build the entry plan from the model.
fn plan_for(stem: &str) -> Plan {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .expect("repo root");
    let resolver = common::pinned_launch_resolver();
    let bringup = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/contract_qos_entry");
    let out = temp_output(repo, stem);
    fs::create_dir_all(&out).expect("create model out dir");
    let model = out.join("system_model.yaml");
    let output = std::process::Command::new(&resolver)
        .arg(bringup.join(format!("launch/{stem}.launch.xml")))
        .arg("--bringup-root")
        .arg(&bringup)
        .arg("-o")
        .arg(&model)
        .output()
        .expect("spawn nros-launch-resolve");
    assert!(
        output.status.success(),
        "nros-launch-resolve failed for {stem}:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let plan = plan_from_model(&model, Some("native".to_string()));
    let _ = fs::remove_dir_all(&out);
    let mut plan = plan.expect("plan from the resolved model");
    // What `metadata::enrich_plan` would fill from the cmake metadata: both
    // components are `SHAPE rclcpp` C++ classes, as the island's are.
    for n in &mut plan.nodes {
        n.lang = Some(Lang::Cpp);
        n.shape = Some("rclcpp".into());
        n.class_name = Some(format!("demo_pkg::{}", n.exec.to_uppercase()));
        n.class_header = Some(format!("demo_pkg/{}.hpp", n.exec));
    }
    plan
}

fn temp_output(repo: &Path, name: &str) -> PathBuf {
    let stamp = common::unique_stamp();
    let dir = repo.join("tmp").join(format!(
        "contract-qos-entry-{name}-{}-{stamp}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    dir
}

fn handler(plan: &Plan) -> &nros_cli_core::codegen::entry::PlanNode {
    plan.nodes
        .iter()
        .find(|n| n.exec == "handler")
        .expect("the handler is planned")
}

/// The baseline contract derives nothing: no table on any node, and the
/// entry hands every `rclcpp` component the 3-argument handle.
#[test]
fn a_contract_restating_the_code_bakes_no_table() {
    let plan = plan_for("code_qos");
    assert!(
        plan.nodes.iter().all(|n| n.qos_overrides.is_empty()),
        "{:#?}",
        plan.nodes
    );
    let src = emit::emit_typed(Lang::Cpp, &plan).expect("typed C++ emit");
    assert!(src.contains("::rclcpp::NodeHandle __h("), "{src}");
    assert!(!src.contains("nros_cpp_qos_override_t"), "{src}");
}

/// The contract-only liveliness line changes the baked table, on the handler
/// and nowhere else, and the table reaches the handler's handle.
#[test]
fn a_contract_only_qos_line_changes_the_baked_table() {
    let base = plan_for("code_qos");
    let plan = plan_for("contract_liveliness");

    // The precondition, asserted rather than assumed: the derivation must have
    // put the override on the plan, or the entry assertions below would be
    // vacuous.
    let rows = &handler(&plan).qos_overrides;
    assert_eq!(rows.len(), 1, "{rows:#?}");
    assert_eq!(rows[0].topic, "/availability");
    assert_eq!(
        rows[0].role,
        nros_orchestration_ir::qos_override::role::SUBSCRIPTION
    );
    assert_eq!(
        rows[0].policy,
        nros_orchestration_ir::qos_override::policy::LIVELINESS
    );
    let talker = plan.nodes.iter().find(|n| n.exec == "talker").unwrap();
    assert!(talker.qos_overrides.is_empty(), "{talker:#?}");

    let before = emit::emit_typed(Lang::Cpp, &base).expect("baseline emit");
    let after = emit::emit_typed(Lang::Cpp, &plan).expect("contract emit");
    assert_ne!(
        before, after,
        "a contract-only QoS change must move the entry"
    );

    let row = format!(
        "{{ \"/availability\", {}, {}, {} }},",
        rows[0].role, rows[0].policy, rows[0].value
    );
    assert!(after.contains(&row), "{after}");
    let idx = plan.nodes.iter().position(|n| n.exec == "handler").unwrap();
    assert!(
        after.contains(&format!(
            "static const ::nros_cpp_qos_override_t __nros_qos_{idx}[] = {{"
        )),
        "{after}"
    );
    assert!(
        after.contains(&format!(", __nros_qos_{idx}, 1);")),
        "the table must reach the handler's handle:\n{after}"
    );
    // Exactly one table: the talker's contract line did not change.
    assert_eq!(
        after.matches("nros_cpp_qos_override_t __nros_qos_").count(),
        1
    );
}

/// issue 1855 -- the contract's DURATION policies reach the entry in the
/// wire's unit. The resolver derives `lease_duration: 500ms` and
/// `deadline: 4.3s` into nanosecond parameters (rclcpp's unit); the lowering
/// must bake 500 and 4300 milliseconds. Before the fix the lease baked as
/// 500,000,000 ms and the deadline's 4,300,000,000 overflowed the `u32` and
/// the plan was refused.
#[test]
fn a_contract_lease_and_deadline_bake_in_milliseconds() {
    use nros_orchestration_ir::qos_override::{policy, role};
    let plan = plan_for("contract_durations");
    let rows = &handler(&plan).qos_overrides;
    let value_of = |p: u8| {
        rows.iter()
            .find(|r| r.topic == "/availability" && r.role == role::SUBSCRIPTION && r.policy == p)
            .unwrap_or_else(|| panic!("policy {p} must be derived and lowered: {rows:#?}"))
            .value
    };
    assert_eq!(value_of(policy::LIVELINESS_LEASE), 500, "{rows:#?}");
    assert_eq!(value_of(policy::DEADLINE), 4300, "{rows:#?}");

    let src = emit::emit_typed(Lang::Cpp, &plan).expect("typed C++ emit");
    for (p, v) in [(policy::LIVELINESS_LEASE, 500), (policy::DEADLINE, 4300)] {
        let row = format!("{{ \"/availability\", {}, {p}, {v} }},", role::SUBSCRIPTION);
        assert!(src.contains(&row), "{row} must be baked:\n{src}");
    }
}
