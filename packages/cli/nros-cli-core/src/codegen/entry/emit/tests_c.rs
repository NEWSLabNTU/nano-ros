//! The C pack's behavioural tests — `emit_c.rs`'s, moved here when phase-494
//! W3 deleted that emitter. Every body is unchanged: each renders a plan
//! through the C pack, now via the generic renderer, and asserts on the TU.
//! The one shim below is the old entry point's name.

use crate::codegen::entry::{Lang, Plan, PlanNode};
use std::path::PathBuf;

/// `emit_c::emit_typed`, which is now the generic renderer asked for C.
fn emit_typed(plan: &Plan) -> Result<String, String> {
    super::emit_typed(Lang::C, plan)
}

fn fixture_plan(nodes: &[(&str, &str)]) -> Plan {
    Plan {
        board: "linux".into(),
        nodes: nodes
            .iter()
            .map(|(pkg, exec)| PlanNode {
                pkg: (*pkg).into(),
                exec: (*exec).into(),
                name: None,
                namespace: None,
                class_name: None,
                class_header: None,
                lang: Some(Lang::C),
                shape: None,
                qos_overrides: Vec::new(),
                params: Vec::new(),
                remaps: Vec::new(),
                callback_groups: Vec::new(),
                sched_context: None,
                group_tiers: std::collections::BTreeMap::new(),
                stack_bytes: None,
            })
            .collect(),
        depfile_paths: Vec::new(),
        bringup: "demo_bringup".into(),
        launch_file: PathBuf::from("/tmp/system.launch.xml"),
        lifecycle: None,
        param_services: false,
        safety: None,
        tiers: Default::default(),
        node_overrides: Vec::new(),
        resolved_tiers: None,
        session: Default::default(),
    }
}

/// Issue #52 — a C node carrying qos_overrides emits the static
/// `nros_cpp_qos_override_t[]` table + the `nros_cpp_node_set_qos_overrides`
/// call, BEFORE the component's configure registers entities. Until
/// phase-296's residue sweep the C emitter dropped overrides entirely, so a
/// model configured QoS on a C++ image and silently did not on a C one.
#[test]
fn typed_emit_bakes_qos_overrides_before_configure() {
    let mut plan = fixture_plan(&[("talker_pkg", "talker")]);
    // Built through the shared lowering, so the test cannot assert codes
    // the real bake would never produce.
    plan.nodes[0].qos_overrides = nros_orchestration_ir::qos_override::lower_all([
        (
            "qos_overrides./chatter.publisher.reliability",
            "best_effort",
        ),
        ("qos_overrides./chatter.subscription.depth", "7"),
    ])
    .expect("fixture overrides lower");
    let src = emit_typed(&plan).expect("typed C emit ok");
    assert!(
        src.contains("static const nros_cpp_qos_override_t __nros_qos_0[] = {"),
        "{src}"
    );
    // (topic, role=0 publisher, policy=0 reliability, value=0 best_effort)
    assert!(src.contains("{ \"/chatter\", 0, 0, 0 },"), "{src}");
    // (topic, role=1 subscription, policy=3 depth, value=7)
    assert!(src.contains("{ \"/chatter\", 1, 3, 7 },"), "{src}");
    assert!(
        src.contains("nros_cpp_node_set_qos_overrides(&__nros_node_0, __nros_qos_0, 2)"),
        "{src}"
    );
    let set_at = src.find("nros_cpp_node_set_qos_overrides").unwrap();
    // The CALL site, not the forward declaration at the top of the TU.
    let cfg_at = src.find("crc = __nros_c_component").unwrap();
    assert!(
        set_at < cfg_at,
        "overrides must be installed before configure"
    );
}

/// Issue 1443 — the node is created at the namespace the PLAN gives it, and
/// the `nros_cpp_bind_node_name_sched` sibling spends the same value.
///
/// The three inputs and their one answer: a real namespace renders itself;
/// `None` and `Some("")` both render `"/"`, never `""`. The last is the case
/// RFC-0045 cares about — the C edge is a `const char*` and cannot carry the
/// difference between "unset" and "configured to nothing", so it is
/// normalised before it gets here.
#[test]
fn typed_emit_creates_each_node_at_its_plan_namespace() {
    for (declared, rendered) in [
        (Some("/island"), "/island"),
        (Some("/a/b"), "/a/b"),
        (None, "/"),
        (Some(""), "/"),
    ] {
        let mut plan = fixture_plan(&[("talker_pkg", "talker")]);
        plan.nodes[0].namespace = declared.map(str::to_string);
        let src = emit_typed(&plan).expect("typed C emit ok");
        assert!(
            src.contains(&format!(
                "nros_cpp_node_create(executor, \"talker\", \"{rendered}\", &__nros_node_0)"
            )),
            "namespace {declared:?} must render as {rendered:?}; src:\n{src}"
        );
        assert!(
            !src.contains("nros_cpp_node_create(executor, \"talker\", \"\","),
            "an empty namespace must never reach the C edge; src:\n{src}"
        );
    }
}

/// A C node with no overrides emits no table and no call.
#[test]
fn typed_emit_no_qos_overrides_no_table() {
    let plan = fixture_plan(&[("talker_pkg", "talker")]);
    let src = emit_typed(&plan).expect("typed C emit ok");
    assert!(!src.contains("nros_cpp_qos_override_t"));
    assert!(!src.contains("set_qos_overrides"));
}

#[test]
fn typed_emit_creates_node_and_configures_component() {
    let plan = fixture_plan(&[("talker_pkg", "talker")]);
    let src = emit_typed(&plan).expect("typed C emit ok");
    // factory + configure seam forward-decls.
    assert!(src.contains("extern void* __nros_c_component_talker_pkg_create(void);"));
    assert!(src.contains(
        "extern int32_t __nros_c_component_talker_pkg_configure(const nros_cpp_node_t* node, void* executor, void* self);"
    ));
    // setup creates the node + configures on the executor.
    assert!(src.contains("nros_cpp_node_create(executor, \"talker\", \"/\", &__nros_node_0)"));
    assert!(
        src.contains("__nros_c_component_talker_pkg_configure(&__nros_node_0, executor, self)")
    );
    // main drives the real-executor lifecycle via the named runner.
    // Issue 1597 — over the entry's own `.bss` executor storage.
    assert!(src.contains("nros_board_native_run_components_named_in(nros_boot_config_node_name(&NROS_BOOT_CONFIG), nros_boot_config_namespace(&NROS_BOOT_CONFIG), &__nros_entry_setup, __nros_executor_storage, sizeof(__nros_executor_storage))"));
    assert!(src.contains(
        "static uint64_t __nros_executor_storage[(NROS_CPP_EXECUTOR_STORAGE_SIZE + 7u) / 8u];"
    ));
    // single-node: boot config has the node name baked in.
    assert!(src.contains("NROS_BOOT_SET_NODE_NAME"));
    assert!(src.contains(".node_name  = \"talker\""));
}

#[test]
fn typed_emit_preserves_launch_order() {
    let plan = fixture_plan(&[("talker_pkg", "talker"), ("listener_pkg", "listener")]);
    let src = emit_typed(&plan).expect("typed C emit ok");
    let pos_talker = src.find("__nros_c_component_talker_pkg_create").unwrap();
    let pos_listener = src.find("__nros_c_component_listener_pkg_create").unwrap();
    assert!(pos_talker < pos_listener);
}

#[test]
fn typed_emit_dashed_pkg_names_sanitised() {
    let plan = fixture_plan(&[("dashy-pkg", "talker")]);
    let src = emit_typed(&plan).expect("typed C emit ok");
    assert!(src.contains("__nros_c_component_dashy_pkg_create"));
    assert!(!src.contains("dashy-pkg_create"));
}

#[test]
fn typed_emit_rejects_non_c_node() {
    let mut plan = fixture_plan(&[("talker_pkg", "talker")]);
    plan.nodes[0].lang = Some(Lang::Cpp);
    let err = emit_typed(&plan).unwrap_err();
    assert!(err.contains("not `c`"), "{err}");
    assert!(err.contains("talker_pkg"), "{err}");
}

#[test]
fn typed_emit_has_required_includes() {
    let plan = fixture_plan(&[("talker_pkg", "talker")]);
    let src = emit_typed(&plan).expect("typed C emit ok");
    assert!(src.contains("#include <rclc/detail/boot_config.h>"));
    assert!(src.contains("#include <rclc/detail/main.h>"));
    assert!(src.contains("#include <rclcpp/detail/nros_cpp_ffi.h>"));
    assert!(!src.contains("#include <rclc/component.h>"));
    assert!(src.contains("int main(int argc, char** argv)"));
}

#[test]
fn typed_emit_multi_node_boot_config_unset() {
    // Multi-node entry → all-unset boot config (node name not baked).
    let plan = fixture_plan(&[("talker_pkg", "talker"), ("listener_pkg", "listener")]);
    let src = emit_typed(&plan).expect("typed C emit ok");
    assert!(src.contains("NROS_BOOT_CONFIG"));
    // Multi-node: set_flags must be 0 (not NROS_BOOT_SET_NODE_NAME).
    assert!(src.contains(".set_flags  = 0,"));
    assert!(!src.contains("NROS_BOOT_SET_NODE_NAME"));
}

// Issue 1971 (RFC-0107 step 1) — launch `<remap>` rules bake as ONE flash
// table, installed in the entry point before the runner (so before any setup
// builds a node). No per-rule `nros_cpp_declare_remap` call is emitted.
#[test]
fn typed_emit_remaps_are_one_flash_table() {
    let mut plan = fixture_plan(&[("talker_pkg", "talker")]);
    plan.nodes[0].remaps = vec![("~/out".into(), "/wire".into())];
    let src = emit_typed(&plan).expect("typed C emit ok");
    assert!(
        src.contains("{\"talker\", \"/\", \"~/out\", \"/wire\"},"),
        "expected the table row; src:\n{src}"
    );
    assert!(!src.contains("nros_cpp_declare_remap"));
    assert_launch_table_installed_before_the_runner(&src, "run_components");
}

#[test]
fn typed_emit_tiered_remaps_are_installed_before_every_tier_setup() {
    let mut plan = fixture_plan_with_tiers();
    plan.nodes[0].remaps = vec![("a".into(), "/x".into())];
    plan.nodes[1].remaps = vec![("b".into(), "/y".into())];
    let src = emit_typed(&plan).expect("typed C tier emit ok");
    assert!(
        src.contains("__nros_launch_remaps[2]"),
        "one table for both tiers; src:\n{src}"
    );
    assert!(!src.contains("nros_cpp_declare_remap"));
    // phase-509 P10 -- the native tier runner is its levels runner.
    assert_launch_table_installed_before_the_runner(&src, "run_levels");
}

#[test]
fn typed_emit_no_remaps_no_declare_calls() {
    // Guard: remap-free plans produce byte-identical output (no table, no
    // install).
    let plan = fixture_plan(&[("talker_pkg", "talker")]);
    let src = emit_typed(&plan).expect("typed C emit ok");
    assert!(!src.contains("nros_cpp_declare_remap"));
    assert!(!src.contains("__nros_launch_remaps"));
}

/// Issue 1971 -- the launch remap table is installed in the entry point,
/// before the runner, so before the runner calls any (tier) setup.
fn assert_launch_table_installed_before_the_runner(src: &str, runner: &str) {
    let install_at = src
        .find("nros_cpp_set_launch_remaps(__nros_launch_remaps")
        .unwrap_or_else(|| panic!("expected the launch table install; src:\n{src}"));
    let table_at = src
        .find("static const nros_cpp_launch_remap_t __nros_launch_remaps[")
        .expect("the launch table is emitted");
    assert!(table_at < install_at, "the table precedes its install");
    let runner_at = src[install_at..]
        .find(runner)
        .unwrap_or_else(|| panic!("expected `{runner}` after the install; src:\n{src}"));
    assert!(runner_at > 0);
    // Every setup is a function DEFINED above and CALLED by the runner; none
    // is called directly from the entry point ahead of the install.
    let entry_at = src[..install_at].rfind("main(").expect("an entry point");
    assert!(
        !src[entry_at..install_at].contains("__nros_entry_setup"),
        "nothing runs a setup before the install; src:\n{src}"
    );
    assert!(src.contains("if (lrc != NROS_CPP_RET_OK) return (int)lrc;"));
}

#[test]
fn typed_emit_param_services_block_present_when_enabled() {
    // Phase 269 W1 — when param_services is true, the post-configure block emits
    // nros_cpp_register_parameter_services + one seed per node param.
    let mut plan = fixture_plan(&[("param_talker_pkg", "param_talker")]);
    plan.param_services = true;
    plan.nodes[0].params = vec![("publish_period_ms".into(), "250".into())];
    let src = emit_typed(&plan).expect("typed C emit ok");
    assert!(src.contains("nros_cpp_register_parameter_services(executor)"));
    assert!(src.contains(
        "nros_cpp_declare_node_param(executor, \"param_talker\", \"publish_period_ms\", \"250\")"
    ));
    // must appear after the configure loop, before return 0
    let reg_at = src.find("nros_cpp_register_parameter_services").unwrap();
    let ret_at = src.rfind("return 0;").unwrap();
    assert!(reg_at < ret_at, "param block must precede return 0");
}

#[test]
fn typed_emit_param_services_registration_is_non_fatal_both_paths() {
    // Issue 0745 — registration builds six service servers, and a backend can
    // refuse the whole set. The stated cause here was WRONG and issue 1268
    // corrected it: cyclonedds has service-server support and serves an image's
    // own services. What it refused was the rcl_interfaces TYPES, which nothing
    // registered a descriptor for; it now registers them with the rest. The
    // non-fatal shape below is unchanged and still right — a backend that cannot
    // serve the six must cost the runtime get/set RPC and nothing else. Launch
    // params are
    // seeded pre-construction by `emit_declare_params`, so a failed registration
    // must cost the runtime get/set RPC and nothing else. The C++ emitter moved to
    // the non-fatal shape with 0745; BOTH C paths kept the pre-0745 block, which
    // returned the rc and aborted boot. Covers the flat AND the tiered emitter,
    // because the tiered site is a separate copy of the same block.
    for (label, node, mut plan) in [
        (
            "flat",
            "param_talker",
            fixture_plan(&[("param_talker_pkg", "param_talker")]),
        ),
        ("tiered", "ctrl", fixture_plan_with_tiers()),
    ] {
        plan.param_services = true;
        plan.nodes[0].params = vec![("publish_period_ms".into(), "250".into())];
        let src = emit_typed(&plan).expect("typed C emit ok");

        assert!(
            src.contains("(void)nros_cpp_register_parameter_services(executor)"),
            "{label}: registration must be non-fatal; got:\n{src}"
        );
        assert!(
            !src.contains("ps_ret"),
            "{label}: no rc check on registration; got:\n{src}"
        );

        // Seeding precedes registration, and happens exactly once per param — the
        // old block re-declared every param AFTER construction (0745 defect 2).
        let seed = format!(
            "nros_cpp_declare_node_param(executor, \"{node}\", \"publish_period_ms\", \"250\")"
        );
        let seed_at = src
            .find(&seed)
            .unwrap_or_else(|| panic!("{label}: no seed; got:\n{src}"));
        let reg_at = src.find("nros_cpp_register_parameter_services").unwrap();
        assert!(seed_at < reg_at, "{label}: params seed BEFORE registration");
        assert_eq!(
            src.matches(&seed).count(),
            1,
            "{label}: one seed per param; got:\n{src}"
        );
    }
}

#[test]
fn typed_emit_param_services_absent_when_disabled() {
    // Guard: non-param plans produce byte-identical output (no param block).
    let plan = fixture_plan(&[("talker_pkg", "talker")]);
    let src = emit_typed(&plan).expect("typed C emit ok");
    assert!(!src.contains("nros_cpp_register_parameter_services"));
    assert!(!src.contains("nros_cpp_declare_node_param"));
}

#[test]
fn typed_emit_lifecycle_active_emits_autostart_block() {
    // Phase 269 W2 — lifecycle = Some("active") → nros_cpp_lifecycle_autostart(executor, 2u)
    // in the post-configure block, AFTER any param block, BEFORE return 0.
    let mut plan = fixture_plan(&[("lifecycle_talker_pkg", "lifecycle_talker")]);
    plan.lifecycle = Some("active".into());
    let src = emit_typed(&plan).expect("typed C lifecycle emit ok");
    // autostart call with code 2 (active = configure + activate)
    assert!(
        src.contains("nros_cpp_lifecycle_autostart(executor, 2u)"),
        "expected nros_cpp_lifecycle_autostart(executor, 2u) in:\n{src}"
    );
    // error-propagation guard
    assert!(src.contains("if (lc_ret != NROS_CPP_RET_OK) return (int32_t)lc_ret"));
    // AFTER configure loop (all node configure calls), BEFORE return 0
    let autostart_at = src.find("nros_cpp_lifecycle_autostart").unwrap();
    let ret_at = src.rfind("return 0;").unwrap();
    assert!(
        autostart_at < ret_at,
        "lifecycle block must precede return 0"
    );
    // configure loop precedes the lifecycle block
    let cfg_at = src
        .find("__nros_c_component_lifecycle_talker_pkg_configure")
        .unwrap();
    assert!(
        cfg_at < autostart_at,
        "lifecycle block must follow configure loop"
    );
}

#[test]
fn typed_emit_lifecycle_configure_emits_code_1() {
    let mut plan = fixture_plan(&[("lc_pkg", "lc")]);
    plan.lifecycle = Some("configure".into());
    let src = emit_typed(&plan).expect("typed C lifecycle configure emit ok");
    assert!(
        src.contains("nros_cpp_lifecycle_autostart(executor, 1u)"),
        "expected autostart_code 1 for 'configure'; src:\n{src}"
    );
}

#[test]
fn typed_emit_lifecycle_none_emits_code_0() {
    let mut plan = fixture_plan(&[("lc_pkg", "lc")]);
    plan.lifecycle = Some("none".into());
    let src = emit_typed(&plan).expect("typed C lifecycle none emit ok");
    assert!(
        src.contains("nros_cpp_lifecycle_autostart(executor, 0u)"),
        "expected autostart_code 0 for 'none'; src:\n{src}"
    );
}

#[test]
fn typed_emit_lifecycle_absent_when_disabled() {
    // Guard: lifecycle = None → byte-identical output (no lifecycle block).
    let plan = fixture_plan(&[("talker_pkg", "talker")]);
    let src = emit_typed(&plan).expect("typed C emit ok");
    assert!(
        !src.contains("nros_cpp_lifecycle_autostart"),
        "lifecycle block must be absent when lifecycle = None"
    );
    assert!(
        !src.contains("nros_cpp_register_lifecycle_services"),
        "lifecycle block must be absent when lifecycle = None"
    );
}

#[test]
fn typed_emit_lifecycle_after_param_block() {
    // Phase 269 W2 — when both param_services and lifecycle are set, the lifecycle
    // block must appear AFTER the param block (same order as the Rust macro: params → lifecycle).
    let mut plan = fixture_plan(&[("talker_pkg", "talker")]);
    plan.param_services = true;
    plan.nodes[0].params = vec![("foo".into(), "bar".into())];
    plan.lifecycle = Some("active".into());
    let src = emit_typed(&plan).expect("typed C combined emit ok");
    let param_at = src.find("nros_cpp_register_parameter_services").unwrap();
    let lc_at = src.find("nros_cpp_lifecycle_autostart").unwrap();
    assert!(
        param_at < lc_at,
        "lifecycle block must follow param-services block"
    );
}

// -------------------------------------------------------------------------
// Phase 269 (W4) — sched-context wiring tests
// -------------------------------------------------------------------------

fn fixture_plan_with_tiers() -> Plan {
    use crate::codegen::entry::PlanNode;
    use nros_orchestration_ir::{ResolvedTier, ResolvedTierTable};
    use std::path::PathBuf;
    // Two-tier plan: high (idx 0) and low (idx 1), each with one C node.
    let high_tier = ResolvedTier {
        name: "high".into(),
        priority: 80,
        stack_bytes: None,
        spin_period_us: Some(10_000),
        preempt_threshold: None,
        time_slice_us: None,
        sched_class: None,
        class: None,
        period_us: None,
        budget_us: None,
        deadline_us: None,
        deadline_policy: None,
        core: None,
        members: vec![("ctrl".into(), "ctrl_grp".into())],
    };
    let low_tier = ResolvedTier {
        name: "low".into(),
        priority: 10,
        stack_bytes: None,
        spin_period_us: Some(100_000),
        preempt_threshold: None,
        time_slice_us: None,
        sched_class: None,
        class: None,
        period_us: None,
        budget_us: None,
        deadline_us: None,
        deadline_policy: None,
        core: None,
        members: vec![("telem".into(), "telem_grp".into())],
    };
    Plan {
        board: "linux".into(),
        nodes: vec![
            PlanNode {
                pkg: "ctrl_pkg".into(),
                exec: "ctrl".into(),
                name: Some("ctrl".into()),
                namespace: None,
                class_name: None,
                class_header: None,
                lang: Some(Lang::C),
                shape: None,
                qos_overrides: Vec::new(),
                params: Vec::new(),
                remaps: Vec::new(),
                callback_groups: vec!["ctrl_grp".into()],
                sched_context: Some(0), // high tier
                group_tiers: std::collections::BTreeMap::new(),
                stack_bytes: None,
            },
            PlanNode {
                pkg: "telem_pkg".into(),
                exec: "telem".into(),
                name: Some("telem".into()),
                namespace: None,
                class_name: None,
                class_header: None,
                lang: Some(Lang::C),
                shape: None,
                qos_overrides: Vec::new(),
                params: Vec::new(),
                remaps: Vec::new(),
                callback_groups: vec!["telem_grp".into()],
                sched_context: Some(1), // low tier
                group_tiers: std::collections::BTreeMap::new(),
                stack_bytes: None,
            },
        ],
        depfile_paths: Vec::new(),
        bringup: "demo_bringup".into(),
        launch_file: PathBuf::from("/tmp/system.launch.xml"),
        lifecycle: None,
        param_services: false,
        safety: None,
        tiers: Default::default(),
        node_overrides: Vec::new(),
        resolved_tiers: Some(ResolvedTierTable {
            tiers: vec![high_tier, low_tier],
        }),
        session: Default::default(),
    }
}

/// issue 1272 -- two nodes that set the SAME parameter name each seed it on
/// their own node, and each node's seeds come right before that node's
/// `nros_cpp_node_create`, so the next node the executor builds is the one
/// the seed names (phase-509 P9b: by name; the shim resolves the index).
#[test]
fn typed_emit_seeds_each_node_on_its_own_index() {
    let mut plan = fixture_plan(&[("a_pkg", "alpha"), ("b_pkg", "beta")]);
    plan.nodes[0].params = vec![("rate".into(), "10".into())];
    plan.nodes[1].params = vec![("rate".into(), "20".into())];
    let src = emit_typed(&plan).expect("typed C emit ok");

    let seed_a = "nros_cpp_declare_node_param(executor, \"alpha\", \"rate\", \"10\")";
    let seed_b = "nros_cpp_declare_node_param(executor, \"beta\", \"rate\", \"20\")";
    let create_a = "nros_cpp_node_create(executor, \"alpha\", \"/\", &__nros_node_0)";
    let create_b = "nros_cpp_node_create(executor, \"beta\", \"/\", &__nros_node_1)";
    let at = |s: &str| {
        src.find(s)
            .unwrap_or_else(|| panic!("missing `{s}`; got:\n{src}"))
    };
    assert!(
        at(seed_a) < at(create_a) && at(create_a) < at(seed_b) && at(seed_b) < at(create_b),
        "each node's seeds must directly precede its own create; got:\n{src}"
    );
}

/// phase-509 P9b -- a tier's seeds NAME their node, never an index: on a
/// runner with one executor per tier the first node of every tier is index 0
/// there, and on a level runner (one executor per core) every tier's setup
/// builds on the same executor, so no build-time index is right for both. The
/// shim resolves the index from the executor it seeds.
#[test]
fn typed_emit_tier_seeds_name_their_node() {
    let mut plan = fixture_plan_with_tiers();
    plan.nodes[0].params = vec![("period".into(), "10".into())];
    plan.nodes[1].params = vec![("period".into(), "100".into())];
    let src = emit_typed(&plan).expect("typed C tier emit ok");

    assert!(
        src.contains("nros_cpp_declare_node_param(executor, \"ctrl\", \"period\", \"10\")"),
        "ctrl's seed names ctrl; got:\n{src}"
    );
    assert!(
        src.contains("nros_cpp_declare_node_param(executor, \"telem\", \"period\", \"100\")"),
        "telem's seed names telem; got:\n{src}"
    );
    assert!(
        !src.contains("nros_cpp_declare_param("),
        "no seed counts its node; got:\n{src}"
    );
}

#[test]
fn typed_emit_tiers_uses_run_tiers_path() {
    // Phase 274.W2 — multi-tier C plan emits per-tier setup functions + run_tiers
    // call instead of the old sched-context wiring.
    let plan = fixture_plan_with_tiers();
    let src = emit_typed(&plan).expect("typed C tier emit ok");

    // Per-tier setup functions present.
    assert!(
        src.contains("static int32_t __nros_entry_setup_tier_0(void* executor)"),
        "expected tier 0 setup fn; got:\n{src}"
    );
    assert!(
        src.contains("static int32_t __nros_entry_setup_tier_1(void* executor)"),
        "expected tier 1 setup fn; got:\n{src}"
    );
    // Each setup fn creates only its tier's nodes (ctrl in tier 0, telem in tier 1).
    assert!(
        src.contains("nros_cpp_node_create(executor, \"ctrl\", \"/\", &__nros_node_0)"),
        "ctrl node in tier-0 setup; src:\n{src}"
    );
    assert!(
        src.contains("nros_cpp_node_create(executor, \"telem\", \"/\", &__nros_node_1)"),
        "telem node in tier-1 setup; src:\n{src}"
    );
    // Tier spec table emitted.
    assert!(
        src.contains("static const nros_native_tier_spec_t __nros_tiers[2]"),
        "expected 2-element tier spec table; src:\n{src}"
    );
    assert!(
        src.contains("\"high\""),
        "high tier name in spec table; src:\n{src}"
    );
    assert!(
        src.contains("\"low\""),
        "low tier name in spec table; src:\n{src}"
    );
    // Priority baked in as LL literals.
    assert!(src.contains("80LL"), "high priority 80LL; src:\n{src}");
    assert!(src.contains("10LL"), "low priority 10LL; src:\n{src}");
    // main calls the levels runner, not run_components (phase-509 P10: one
    // worker thread per level over the core's ONE executor).
    // Issue 1597 — over the entry's own `.bss` executor storage, one block.
    assert!(
        src.contains("nros_board_native_run_levels_in(")
            && src.contains(
                "__nros_tier_executor_storage, sizeof(__nros_tier_executor_storage[0]), \
                 NROS_ENTRY_LEVEL_BACKINGS, NULL, __nros_scheduler_0"
            ),
        "main must call nros_board_native_run_levels_in over the entry's storage; src:\n{src}"
    );
    assert!(
        src.contains("static uint64_t __nros_tier_executor_storage[1]["),
        "a level image owns ONE executor block; src:\n{src}"
    );
    assert!(
        !src.contains("return nros_board_native_run_components"),
        "must NOT call run_components in multi-tier; src:\n{src}"
    );
    // Old sched-context wiring must NOT appear.
    assert!(
        !src.contains("__nros_sc_ids"),
        "no-sched-ctx: sc_ids array must not appear; src:\n{src}"
    );
    assert!(
        !src.contains("nros_cpp_create_sched_context"),
        "no-sched-ctx: create_sched_context must not appear; src:\n{src}"
    );
    assert!(
        !src.contains("nros_cpp_bind_node_name_sched"),
        "no-sched-ctx: bind_node_name_sched must not appear; src:\n{src}"
    );
    assert!(
        !src.contains("nros_cpp_bind_group_sched"),
        "no-sched-ctx: bind_group_sched must not appear; src:\n{src}"
    );
}

#[test]
fn typed_emit_no_tiers_uses_plain_node_create() {
    // Guard: empty resolved_tiers keeps byte-identical plain create (no seed, no sched).
    let plan = fixture_plan(&[("talker_pkg", "talker")]);
    let src = emit_typed(&plan).expect("typed C no-tier emit ok");
    // Must NOT contain sched block artifacts.
    assert!(
        !src.contains("__nros_sc_ids"),
        "no-tier plan must not emit sc_ids array"
    );
    assert!(
        !src.contains("nros_cpp_create_sched_context"),
        "no-tier plan must not emit sched_context_create"
    );
    assert!(
        !src.contains("nros_cpp_bind_node_name_sched"),
        "no-tier plan must not emit bind_node_name_sched"
    );
    assert!(
        !src.contains("nros_cpp_bind_group_sched"),
        "no-tier plan must not emit bind_group_sched"
    );
    assert!(
        !src.contains("nros_cpp_node_create_ex"),
        "no-tier plan must use plain nros_cpp_node_create"
    );
    assert!(
        src.contains("nros_cpp_node_create(executor, \"talker\", \"/\", &__nros_node_0)"),
        "no-tier plan must use plain nros_cpp_node_create"
    );
}

// -----------------------------------------------------------------------
// issue 1604 -- the contract monitor region, on the C road.
//
// phase-462 W1 baked the model's monitor rows into the C++ entry only; the C
// pack received the same rows from the lowering (phase-494) and rendered none,
// so a pure-C image with declared contracts ran unmonitored and said nothing.
// These are `tests_cpp.rs`'s monitor tests, asked of the C pack.
// -----------------------------------------------------------------------

use crate::orchestration::model_ingest::{AgeRow, MonitorRow, render_monitor_rs};

fn emit_typed_monitored(
    plan: &Plan,
    monitors: &[MonitorRow],
    ages: &[AgeRow],
) -> Result<String, String> {
    super::emit_typed_monitored(Lang::C, plan, monitors, ages)
}

fn c_monitor_fixture_rows() -> (Vec<MonitorRow>, Vec<AgeRow>) {
    (
        vec![MonitorRow {
            topic: "/chatter".into(),
            fqn: "/talker/chatter".into(),
            min_rate_hz_milli: 10_000,
            max_latency_ms: 30,
            // phase10-B3 -- a route row: the trigger topic and its budget.
            route_trigger: "/trigger".into(),
            route_max_latency_ms: 25,
            route_deadline_ms: 0,
            reaction: String::new(),
            reaction_id: 0,
            mechanism: 0,
        }],
        vec![AgeRow {
            topic: "/chatter".into(),
            fqn: "/listener/chatter".into(),
            max_age_ms: 150,
            deadline_ms: 0,
        }],
    )
}

/// The C entry bakes the SAME rows the Rust road renders into
/// `system_monitors.rs`, field for field, and installs them on the setup's
/// executor before the first node exists.
#[test]
fn typed_emit_bakes_monitor_table_before_nodes() {
    let plan = fixture_plan(&[("talker_pkg", "talker"), ("listener_pkg", "listener")]);
    let (rows, ages) = c_monitor_fixture_rows();
    let src = emit_typed_monitored(&plan, &rows, &ages).expect("monitored C emit ok");
    let rust = render_monitor_rs(&rows, &ages);

    for r in &rows {
        let trigger = if r.route_trigger.is_empty() {
            "NULL".to_string()
        } else {
            format!("\"{}\"", r.route_trigger)
        };
        let c_row = format!(
            "{{ \"{}\", \"{}\", {}u, {}u, {}, {}u }},",
            r.topic, r.fqn, r.min_rate_hz_milli, r.max_latency_ms, trigger, r.route_max_latency_ms
        );
        let rust_row = format!(
            "topic: {:?}, fqn: {:?}, min_rate_hz_milli: {}u32, max_latency_ms: {}u32, \
             route_trigger: {:?}, route_max_latency_ms: {}u32",
            r.topic,
            r.fqn,
            r.min_rate_hz_milli,
            r.max_latency_ms,
            r.route_trigger,
            r.route_max_latency_ms
        );
        assert!(src.contains(&c_row), "C row `{c_row}` missing; src:\n{src}");
        assert!(rust.contains(&rust_row), "Rust row `{rust_row}` missing");
    }
    for a in &ages {
        let c_row = format!(
            "{{ \"{}\", \"{}\", {}u, {}u }},",
            a.topic, a.fqn, a.max_age_ms, a.deadline_ms
        );
        assert!(
            src.contains(&c_row),
            "C age row `{c_row}` missing; src:\n{src}"
        );
    }
    assert!(
        src.contains("static const nros_cpp_monitor_row_t __nros_mon_rows[1] = {"),
        "{src}"
    );
    assert!(
        src.contains("static const nros_cpp_age_row_t __nros_age_rows[1] = {"),
        "{src}"
    );
    // 8-aligned storage without C11 `_Alignas`, sized from the ABI's own
    // per-row constant.
    assert!(
        src.contains(
            "static uint64_t __nros_mon_storage[(1u * NROS_CPP_MONITOR_ROW_STORAGE + 7u) / 8u];"
        ),
        "{src}"
    );
    assert!(
        src.contains(".n_rows = 1u,") && src.contains(".n_ages = 1u,"),
        "{src}"
    );
    // Installed on the setup's own executor, and checked: a refused install
    // (a table over `NROS_EXECUTOR_MAX_MONITORS`) aborts setup rather than
    // running unmonitored.
    assert!(
        src.contains("nros_cpp_install_monitors(executor, &mtables);"),
        "{src}"
    );
    assert!(
        src.contains("if (mret != NROS_CPP_RET_OK) return (int32_t)mret;"),
        "{src}"
    );
    let install_at = src.find("nros_cpp_install_monitors(").unwrap();
    let create_at = src.find("nros_cpp_node_create(").unwrap();
    assert!(
        install_at < create_at,
        "install must precede node create:\n{src}"
    );
}

/// RFC-0052's zero-cost claim, on the C road: no rows, no table, no call --
/// and the TU is byte-identical to the unmonitored emit (the goldens).
#[test]
fn typed_emit_no_monitor_rows_is_byte_identical() {
    let plan = fixture_plan(&[("talker_pkg", "talker")]);
    let plain = emit_typed(&plan).expect("emit ok");
    let monitored = emit_typed_monitored(&plan, &[], &[]).expect("emit ok");
    assert_eq!(plain, monitored);
    assert!(!plain.contains("nros_cpp_install_monitors"), "{plain}");
    assert!(!plain.contains("nros_cpp_monitor_row_t"), "{plain}");

    // A row for a node this entry does not construct is not this entry's.
    let (rows, ages) = c_monitor_fixture_rows();
    let only_talker = emit_typed_monitored(&plan, &rows, &ages).expect("emit ok");
    assert!(only_talker.contains("\"/talker/chatter\""), "{only_talker}");
    assert!(
        !only_talker.contains("\"/listener/chatter\""),
        "{only_talker}"
    );
    assert!(
        only_talker.contains(".ages = NULL,") && only_talker.contains(".n_ages = 0u,"),
        "{only_talker}"
    );
}

/// phase-503 -- a supervised route row bakes its deadline, reaction id and
/// mechanism after the route budget, names them in a comment, declares the
/// application's reaction hook weak and installs it after the table when the
/// application defines it. An unsupervised row renders as before (the
/// fixture's own row, checked by the test above).
#[test]
fn typed_emit_bakes_supervised_route_and_reaction_hook() {
    let plan = fixture_plan(&[("talker_pkg", "talker")]);
    let (mut rows, ages) = c_monitor_fixture_rows();
    let plain = emit_typed_monitored(&plan, &rows, &ages).expect("emit ok");
    assert!(!plain.contains("nros_app_reaction_hook"), "{plain}");
    assert!(!plain.contains("route-deadline-runtime"), "{plain}");
    rows[0].route_deadline_ms = 25;
    rows[0].reaction = "/talker/brake_now".into();
    rows[0].reaction_id = 1;
    rows[0].mechanism = 1;
    let src = emit_typed_monitored(&plan, &rows, &ages).expect("emit ok");
    assert!(src.contains("\"/trigger\", 25u, 25u, 1u, 1u },"), "{src}");
    assert!(
        src.contains(
            "row 0 /talker/chatter: deadline 25 ms, reaction 1 (/talker/brake_now), mechanism timer"
        ),
        "{src}"
    );
    assert_eq!(src.matches("__attribute__((weak));").count(), 1, "{src}");
    let install_at = src.find("nros_cpp_install_monitors(").expect("install");
    let hook_at = src
        .find("nros_cpp_executor_set_reaction_hook(executor, &nros_app_reaction_hook, NULL)")
        .expect("hook install");
    assert!(install_at < hook_at, "{src}");
    // The Rust road renders the same three fields.
    let rust = render_monitor_rs(&rows, &ages);
    assert!(
        rust.contains("route_deadline_ms: 25u32, reaction_id: 1u16, mechanism: 1u8"),
        "{rust}"
    );
}

/// phase-462 W3b -- a hazard whose guard topic both rows watch is baked as
/// rows, guards and members over THOSE rows (age 0, rate 0), with storage
/// sized by all three counts, and installed after the monitor tables.
#[test]
fn typed_emit_bakes_the_hazard_table_after_the_monitors() {
    let plan = fixture_plan(&[("talker_pkg", "talker"), ("listener_pkg", "listener")]);
    let (rows, ages) = c_monitor_fixture_rows();
    let model = super::hazard_fixture_model();
    let (src, _) = super::emit_typed_monitored_lowered(
        Lang::C,
        &plan,
        &rows,
        &ages,
        Some(&model),
        &[],
        &super::ModeTier::default(),
    )
    .expect("hazard emit ok");
    assert!(
        src.contains("{ 0u, 0u },") && src.contains("{ 1u, 0u },"),
        "members: age row 0 and rate row 0; src:\n{src}"
    );
    assert!(
        src.contains(
            "{ \"d.launch.xml/chatter_lost\", 400u, 0u, __nros_hz_guards_0, 1u, 65535u, 0u, 65535u, 0u },"
        ),
        "hazard row (no safe_state: no sink; no rungs: no route); src:\n{src}"
    );
    assert!(src.contains("static uint64_t __nros_hz_storage[(1u * NROS_CPP_HAZARD_ROW_STORAGE + 1u * NROS_CPP_HAZARD_GUARD_STORAGE + 2u * NROS_CPP_HAZARD_MEMBER_STORAGE + 7u) / 8u];"), "{src}");
    let monitors = src
        .find("nros_cpp_install_monitors(")
        .expect("monitor install");
    let hazards = src
        .find("nros_cpp_install_hazards(")
        .expect("hazard install");
    assert!(hazards > monitors, "{src}");
    // The same rows without the model bake no hazard.
    let plain = emit_typed_monitored(&plan, &rows, &ages).expect("plain emit ok");
    assert!(!plain.contains("nros_cpp_install_hazards"), "{plain}");
}

/// phase-462 W3c -- a hazard reacting through a path that states a
/// `safe_state` carries that path's reaction id, and the entry declares the
/// weak `nros_app_reaction_hook` and installs it so the detection can start
/// the safe state.
#[test]
fn typed_emit_wires_a_hazards_safe_state_reaction_to_the_hook() {
    let plan = fixture_plan(&[("talker_pkg", "talker"), ("listener_pkg", "listener")]);
    let (rows, ages) = c_monitor_fixture_rows();
    let model = super::hazard_fixture_model_reacting(true);
    let (src, _) = super::emit_typed_monitored_lowered(
        Lang::C,
        &plan,
        &rows,
        &ages,
        Some(&model),
        &[],
        &super::ModeTier::default(),
    )
    .expect("hazard emit ok");
    assert!(
        src.contains("__nros_hz_guards_0, 1u, 0u, 1u, 0u, 13u },"),
        "sink rate row 0, reaction id 1, rung route on rate row 0 at 13 ms; src:\n{src}"
    );
    assert!(src.contains("void nros_app_reaction_hook"), "{src}");
    assert!(
        src.contains("reaction 1"),
        "the reaction table comment; src:\n{src}"
    );
    assert!(src.contains("nros_cpp_install_hazards("), "{src}");
    assert!(
        src.contains("nros_cpp_executor_set_reaction_hook("),
        "{src}"
    );
}

/// phase-462 W4b -- a contract mode bakes its numbers for the table's rows,
/// row for row, installs them after the monitor tables, and names its id.
#[test]
fn typed_emit_bakes_a_modes_numbers_after_the_monitors() {
    let plan = fixture_plan(&[("talker_pkg", "talker"), ("listener_pkg", "listener")]);
    let (rows, ages) = c_monitor_fixture_rows();
    let mut relaxed = rows.clone();
    for r in &mut relaxed {
        r.max_latency_ms = 77;
    }
    let mut slow = ages.clone();
    for a in &mut slow {
        a.max_age_ms = 999;
    }
    let modes = vec![("degraded-safe".to_string(), relaxed, slow)];
    let (src, _) = super::emit_typed_monitored_lowered(
        Lang::C,
        &plan,
        &rows,
        &ages,
        None,
        &modes,
        &super::ModeTier::default(),
    )
    .expect("mode emit ok");
    assert!(src.contains("#define NROS_MODE_DEGRADED_SAFE 1u"), "{src}");
    assert!(
        src.contains("static const nros_cpp_mode_row_t __nros_mode_rows_1["),
        "{src}"
    );
    assert!(src.contains(", 77u, "), "the mode's latency; src:\n{src}");
    assert!(src.contains("{ 999u },"), "the mode's age; src:\n{src}");
    assert!(
        src.contains(
            "static uint64_t __nros_mode_storage[((1u + 1u) * (NROS_CPP_MODE_TABLE_STORAGE + 0u * \
             NROS_CPP_MODE_DEADLINE_STORAGE) + 1u * "
        ),
        "{src}"
    );
    let monitors = src
        .find("nros_cpp_install_monitors(")
        .expect("monitor install");
    let modes_at = src.find("nros_cpp_install_modes(").expect("mode install");
    assert!(modes_at > monitors, "{src}");
    // No modes: nothing of this.
    let (plain, _) = super::emit_typed_monitored_lowered(
        Lang::C,
        &plan,
        &rows,
        &ages,
        None,
        &[],
        &super::ModeTier::default(),
    )
    .expect("plain emit ok");
    assert!(
        !plain.contains("NROS_MODE_") && !plain.contains("nros_cpp_install_modes"),
        "{plain}"
    );
}

/// A mode whose numbers equal the base's bakes no rows of its own: the entry
/// hands `nros_cpp_install_modes` NULL / 0, which aliases the base table, and
/// sizes the storage for the modes with rows of their own only. A mode that
/// differs in its rates but not its ages bakes its rows and aliases the ages.
#[test]
fn typed_emit_aliases_a_mode_equal_to_the_base() {
    let plan = fixture_plan(&[("talker_pkg", "talker"), ("listener_pkg", "listener")]);
    let (rows, ages) = c_monitor_fixture_rows();
    let mut relaxed = rows.clone();
    for r in &mut relaxed {
        r.max_latency_ms = 77;
    }
    let modes = vec![
        ("relaxed".to_string(), relaxed, ages.clone()),
        ("same".to_string(), rows.clone(), ages.clone()),
    ];
    let (src, _) = super::emit_typed_monitored_lowered(
        Lang::C,
        &plan,
        &rows,
        &ages,
        None,
        &modes,
        &super::ModeTier::default(),
    )
    .expect("mode emit ok");
    assert!(src.contains("#define NROS_MODE_SAME 2u"), "{src}");
    let same = src.contains("__nros_mode_rows_2[") || src.contains("__nros_mode_ages_2[");
    assert!(
        !same,
        "the mode equal to the base bakes no rows; src:\n{src}"
    );
    assert!(
        src.contains("{ NULL, 0u, NULL, 0u, "),
        "it aliases both base tables; src:\n{src}"
    );
    assert!(
        src.contains("static const nros_cpp_mode_row_t __nros_mode_rows_1["),
        "the differing mode bakes its rows; src:\n{src}"
    );
    assert!(src.contains(", 77u, "), "{src}");
    assert!(
        !src.contains("__nros_mode_ages_1["),
        "and aliases the ages it shares; src:\n{src}"
    );
    let storage = format!(
        "static uint64_t __nros_mode_storage[((2u + 1u) * (NROS_CPP_MODE_TABLE_STORAGE + 0u * NROS_CPP_MODE_DEADLINE_STORAGE) + 1u * {r}u * NROS_CPP_MODE_ROW_STORAGE + 0u * {a}u * NROS_CPP_MODE_AGE_STORAGE + 7u) / 8u];",
        r = rows.len(),
        a = ages.len()
    );
    assert!(src.contains(&storage), "want `{storage}`; src:\n{src}");
}
