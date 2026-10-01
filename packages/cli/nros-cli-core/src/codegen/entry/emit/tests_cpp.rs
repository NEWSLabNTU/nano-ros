//! The C++ pack's behavioural tests — `emit_cpp.rs`'s, moved here when
//! phase-474 W4 deleted that emitter. Every body is unchanged: each renders a
//! plan through the C++ pack, now via the generic renderer, and asserts on
//! the TU. The shims below are the old entry points' names.

use super::*;
use crate::codegen::entry::{Lang, Plan, PlanNode};
use nros_entry_lower::BootShape;
use std::path::PathBuf;

/// `emit_cpp::emit_typed`, which is now the generic renderer asked for C++.
fn emit_typed(plan: &Plan) -> Result<String, String> {
    super::emit_typed(Lang::Cpp, plan)
}

/// `emit_cpp::emit_typed_monitored`.
fn emit_typed_monitored(
    plan: &Plan,
    monitors: &[crate::orchestration::model_ingest::MonitorRow],
    ages: &[crate::orchestration::model_ingest::AgeRow],
) -> Result<String, String> {
    super::emit_typed_monitored(Lang::Cpp, plan, monitors, ages)
}

/// `emit_cpp::boot_shape` — the family's one derivation, read directly.
fn boot_shape(board: &str) -> BootShape {
    nros_entry_lower::board_family(board)
        .expect("a known board key")
        .boot_shape()
}

fn fixture_plan(nodes: &[(&str, &str)]) -> Plan {
    Plan {
        board: "native".into(),
        nodes: nodes
            .iter()
            .map(|(pkg, exec)| PlanNode {
                pkg: (*pkg).into(),
                exec: (*exec).into(),
                name: None,
                namespace: None,
                class_name: None,
                class_header: None,
                lang: None,
                shape: None,
                qos_overrides: Vec::new(),
                params: Vec::new(),
                remaps: Vec::new(),
                callback_groups: Vec::new(),
                sched_context: None,
                group_tiers: std::collections::BTreeMap::new(),
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

/// Typed-emit fixture: each tuple is `(pkg, exec, name, class, header)`.
/// Defaults to the `configure(Node&)` shape (240.x); use
/// [`fixture_plan_rclcpp`] for the construct-with-handle shape.
fn fixture_plan_typed(nodes: &[(&str, &str, &str, &str, &str)]) -> Plan {
    Plan {
        board: "native".into(),
        nodes: nodes
            .iter()
            .map(|(pkg, exec, name, class, header)| PlanNode {
                pkg: (*pkg).into(),
                exec: (*exec).into(),
                name: Some((*name).into()),
                namespace: None,
                class_name: Some((*class).into()),
                class_header: Some((*header).into()),
                lang: Some(Lang::Cpp),
                shape: Some("configure".into()),
                qos_overrides: Vec::new(),
                params: Vec::new(),
                remaps: Vec::new(),
                callback_groups: Vec::new(),
                sched_context: None,
                group_tiers: std::collections::BTreeMap::new(),
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

/// Phase 242.4 — rclcpp-shape typed fixture: same tuple as
/// [`fixture_plan_typed`] but `shape == "rclcpp"` (construct-with-handle).
fn fixture_plan_rclcpp(nodes: &[(&str, &str, &str, &str, &str)]) -> Plan {
    let mut plan = fixture_plan_typed(nodes);
    for n in &mut plan.nodes {
        n.shape = Some("rclcpp".into());
    }
    plan
}

/// phase-308 W1 — the probe is the SAME TU an entry would be, minus the
/// board and plus a dump. That is the point: the per-node construction and
/// `configure` calls come from one emitter, so the entity count a probe
/// records cannot drift from what the real entry registers.
#[test]
fn metadata_probe_reuses_the_setup_body_and_swaps_the_tail() {
    let plan = fixture_plan_typed(&[(
        "talker_pkg",
        "talker",
        "talker",
        "talker_pkg::Talker",
        "talker_pkg/Talker.hpp",
    )]);
    let export = ProbeExport {
        package: "talker_pkg".into(),
        component: "talker".into(),
        executable: "talker".into(),
        language: Lang::Cpp,
        out_path: "/ws/src/talker_pkg/metadata/talker.json".into(),
    };
    let src = emit_typed_probe(&plan, &export).expect("probe emit ok");

    // Same setup body as the entry: header, construction, configure.
    assert!(src.contains("#include \"talker_pkg/Talker.hpp\""), "{src}");
    assert!(src.contains("__nros_entry_setup"), "{src}");
    assert!(src.contains(".configure("), "{src}");

    // Probe tail: dump, with the identity the sidecar is stamped with.
    assert!(
        src.contains("nros_cpp_metadata_dump(\"talker_pkg\", \"talker\", \"talker\", \"cpp\""),
        "{src}"
    );
    assert!(
        src.contains("/ws/src/talker_pkg/metadata/talker.json"),
        "{src}"
    );
    // Explicit registration is what pulls the backend object out of the
    // static archive; with no reference the linker omits it entirely.
    assert!(src.contains("nros_rmw_metadata_register()"), "{src}");

    // NOT an entry: no board CALL, no spin, no boot-config blob. Match the
    // call form — the generated file's header comment mentions
    // `Board::run_components` prose, which a bare substring test flags.
    assert!(
        !src.contains("::run_components("),
        "probe must not spin:\n{src}"
    );
    assert!(!src.contains("NROS_BOOT_CONFIG"), "{src}");
    assert!(
        !src.contains("run_tiers"),
        "probe records every tier:\n{src}"
    );
}

#[test]
fn typed_emit_includes_headers_constructs_and_runs_components() {
    let plan = fixture_plan_typed(&[
        (
            "talker_pkg",
            "talker",
            "talker",
            "talker_pkg::Talker",
            "talker_pkg/Talker.hpp",
        ),
        (
            "listener_pkg",
            "listener",
            "listener",
            "listener_pkg::Listener",
            "listener_pkg/Listener.hpp",
        ),
    ]);
    let src = emit_typed(&plan).expect("typed emit ok");
    // headers included (including boot_config.h for the node-name blob)
    assert!(src.contains("#include <nros/boot_config.h>"));
    assert!(src.contains("#include \"talker_pkg/Talker.hpp\""));
    assert!(src.contains("#include \"listener_pkg/Listener.hpp\""));
    assert!(src.contains("#include <nros/component.hpp>"));
    // static component + node storage
    assert!(src.contains("static ::rclcpp::Node __nros_node_0;"));
    assert!(src.contains("static ::talker_pkg::Talker __nros_comp_0;"));
    assert!(src.contains("static ::listener_pkg::Listener __nros_comp_1;"));
    // setup constructs the node + configures the component
    assert!(src.contains("::nros::create_node(__nros_node_0, \"talker\", \"/\")"));
    assert!(src.contains("__nros_comp_0.configure(__nros_node_0)"));
    assert!(src.contains("__nros_comp_1.configure(__nros_node_1)"));
    // routes to the real executor via the named overload (phase 266)
    assert!(src.contains(
        "::nros::board::LinuxBoard::run_components(nros_boot_config_node_name(&NROS_BOOT_CONFIG), nros_boot_config_namespace(&NROS_BOOT_CONFIG), &__nros_entry_setup)"
    ));
    assert!(!src.contains("__nros_component_"));
    assert!(!src.contains("NodeContext"));
    // configure shape: no construct-with-handle artifacts.
    assert!(!src.contains("global_handle()"));
    assert!(!src.contains("__nros_comp_buf_"));
    // multi-node: boot config must be all-unset (no single node name baked)
    assert!(src.contains(".set_flags  = 0,"));
    assert!(!src.contains("NROS_BOOT_SET_NODE_NAME"));
}

// Phase 305 W3 (issue 0255) — launch `<remap>` rules bake as per-pair
// `nros_cpp_declare_remap` calls BEFORE construction/configure (rclcpp
// ctors register entities immediately; configure-shape registers there).
#[test]
fn typed_emit_remaps_declared_before_configure() {
    let mut plan = fixture_plan_typed(&[(
        "talker_pkg",
        "talker",
        "talker",
        "talker_pkg::Talker",
        "talker_pkg/Talker.hpp",
    )]);
    plan.nodes[0].remaps = vec![("chatter".into(), "chatter_remapped".into())];
    let src = emit_typed(&plan).expect("typed emit ok");
    assert!(
        src.contains(
            "nros_cpp_declare_remap(::nros::global_handle(), \"talker\", \"/\", \"chatter\", \"chatter_remapped\")"
        ),
        "expected declare_remap call; src:\n{src}"
    );
    let remap_at = src.find("nros_cpp_declare_remap").unwrap();
    let cfg_at = src.find(".configure(__nros_node_0)").unwrap();
    assert!(remap_at < cfg_at, "remap decl must precede configure");
}

#[test]
fn typed_emit_no_remaps_no_declare_calls() {
    // Guard: remap-free plans produce byte-identical output.
    let plan = fixture_plan_typed(&[(
        "talker_pkg",
        "talker",
        "talker",
        "talker_pkg::Talker",
        "talker_pkg/Talker.hpp",
    )]);
    let src = emit_typed(&plan).expect("typed emit ok");
    assert!(!src.contains("nros_cpp_declare_remap"));
}

/// Phase 211.H (issue #52) — a configure-shape node carrying qos_overrides
/// emits the static `nros_cpp_qos_override_t[]` table + a `set_qos_overrides`
/// call BEFORE `configure`, with the role/policy/value mapped to C-ABI codes.
#[test]
fn typed_emit_bakes_qos_overrides_before_configure() {
    let mut plan = fixture_plan_typed(&[(
        "talker_pkg",
        "talker",
        "talker",
        "talker_pkg::Talker",
        "talker_pkg/Talker.hpp",
    )]);
    // Built through the shared lowering, so the test cannot assert codes
    // the real bake would never produce.
    plan.nodes[0].qos_overrides = nros_orchestration_ir::qos_override::lower_all([
        (
            "qos_overrides./chatter.publisher.reliability",
            "best_effort",
        ),
        (
            "qos_overrides./chatter.subscription.durability",
            "transient_local",
        ),
    ])
    .expect("fixture overrides lower");
    let src = emit_typed(&plan).expect("typed emit ok");

    // Static table with the two overrides, C-ABI codes:
    //   publisher(0)/reliability(0)/best_effort(0); subscription(1)/durability(1)/transient_local(1)
    assert!(src.contains("static const ::nros_cpp_qos_override_t __nros_qos_0[] = {"));
    assert!(src.contains("{ \"/chatter\", 0, 0, 0 }"));
    assert!(src.contains("{ \"/chatter\", 1, 1, 1 }"));
    // Installed on the node, and BEFORE configure.
    assert!(src.contains("__nros_node_0.set_qos_overrides(__nros_qos_0, 2)"));
    let set_at = src.find("set_qos_overrides").unwrap();
    let cfg_at = src.find("__nros_comp_0.configure(__nros_node_0)").unwrap();
    assert!(set_at < cfg_at, "set_qos_overrides must precede configure");
}

/// A node with no qos_overrides emits no table / set call.
#[test]
fn typed_emit_no_qos_overrides_no_table() {
    let plan = fixture_plan_typed(&[(
        "talker_pkg",
        "talker",
        "talker",
        "talker_pkg::Talker",
        "talker_pkg/Talker.hpp",
    )]);
    let src = emit_typed(&plan).expect("typed emit ok");
    assert!(!src.contains("nros_cpp_qos_override_t"));
    assert!(!src.contains("set_qos_overrides"));
}

#[test]
fn typed_emit_rclcpp_shape_constructs_with_handle() {
    // Phase 242.4 (RFC-0044) — an rclcpp-shape component OWNS its node: the
    // entry placement-news it with the executor handle *after* init, then
    // checks ok(); there is no separate `create_node` / `configure`.
    let plan = fixture_plan_rclcpp(&[(
        "ctrl_pkg",
        "controller",
        "controller",
        "ctrl_pkg::Controller",
        "ctrl_pkg/Controller.hpp",
    )]);
    let src = emit_typed(&plan).expect("rclcpp emit ok");
    // construct-with-handle headers + arena slot.
    //
    // phase-427 W4 — this asserted `#include <nros/component_node.hpp>`
    // until the merge deleted that header. The assertion had to MOVE
    // rather than go: it exists to prove the rclcpp-shape branch emits the
    // placement-new header the arena slot needs, and a test that keeps
    // asserting a string the template still emits over a file that no
    // longer exists stays GREEN while every generated entry fails to
    // compile. Both directions are pinned here.
    assert!(src.contains("#include <new> // placement-new into the component arena slot"));
    assert!(src.contains("#include <nros/nros.hpp>"));
    assert!(
        !src.contains("component_node.hpp"),
        "the rclcpp entry must not include the DELETED component_node.hpp"
    );
    assert!(src.contains("#include \"ctrl_pkg/Controller.hpp\""));
    assert!(src.contains(
        "alignas(::ctrl_pkg::Controller) static unsigned char __nros_comp_buf_0[sizeof(::ctrl_pkg::Controller)];"
    ));
    assert!(src.contains("static ::ctrl_pkg::Controller* __nros_comp_0 = nullptr;"));
    // setup: handle → placement-new → ok() check naming the node.
    //
    // Issue 1456 — the handle carries the LAUNCH-DECLARED identity. This
    // fixture's node declares a name (`"controller"`) and no namespace, so
    // the second slot is that name and the third is `nullptr`. The
    // negative direction — a node declaring neither, whose class literal
    // must stand — is `typed_emit_rclcpp_undeclared_identity_is_nullptr`
    // below, and both are in the `cpp_native_shapes` golden.
    assert!(
        src.contains("::nros::NodeHandle __h(::nros::global_handle(), \"controller\", nullptr);")
    );
    assert!(src.contains("__nros_comp_0 = new (__nros_comp_buf_0) ::ctrl_pkg::Controller(__h);"));
    assert!(src.contains("if (!__nros_comp_0->ok()) {"));
    assert!(src.contains("report_component_failure(\"controller\""));
    // The rclcpp shape does NOT default-construct a Node or call configure.
    assert!(!src.contains("static ::rclcpp::Node __nros_node_0;"));
    assert!(!src.contains("__nros_comp_0.configure"));
    assert!(!src.contains("create_node(__nros_node_0"));
    // still routes to the real executor via the named overload (phase 266)
    assert!(src.contains(
        "::nros::board::LinuxBoard::run_components(nros_boot_config_node_name(&NROS_BOOT_CONFIG), nros_boot_config_namespace(&NROS_BOOT_CONFIG), &__nros_entry_setup)"
    ));
}

/// Issue 1456 — the launch identity reaches an `rclcpp`-shape component
/// through its `NodeHandle`, and BOTH directions are load-bearing.
///
/// Measured before the fix, on a real bus (zenoh router, `ros2 node list
/// --no-daemon`), for a plan declaring `name="alpha" namespace="/island"`:
/// the component answered `/rclcpp_class_name` — the literal its class
/// writes — and its relative topics resolved at the ROOT, beside a
/// `configure`-shape node in the SAME image answering `/island/beta`.
///
/// The negative direction matters just as much and is the one a resolved
/// view would get wrong: a node the launch file gives no `name=` must keep
/// its class's literal, so the handle must read `nullptr` there — NOT the
/// `exec` that `n.name`'s resolution falls back to, and never `""`.
#[test]
fn typed_emit_rclcpp_launch_identity_rides_the_handle() {
    let mut plan = fixture_plan_rclcpp(&[(
        "ctrl_pkg",
        "controller",
        "controller",
        "ctrl_pkg::Controller",
        "ctrl_pkg/Controller.hpp",
    )]);
    plan.nodes[0].name = Some("alpha".into());
    plan.nodes[0].namespace = Some("/island".into());
    let src = emit_typed(&plan).expect("rclcpp emit ok");
    assert!(
        src.contains("::nros::NodeHandle __h(::nros::global_handle(), \"alpha\", \"/island\");"),
        "a launch-declared name and namespace must reach the component's handle;\n{src}"
    );
}

#[test]
fn typed_emit_rclcpp_undeclared_identity_is_nullptr() {
    let mut plan = fixture_plan_rclcpp(&[(
        "ctrl_pkg",
        "controller",
        "controller",
        "ctrl_pkg::Controller",
        "ctrl_pkg/Controller.hpp",
    )]);
    plan.nodes[0].name = None;
    plan.nodes[0].namespace = None;
    let src = emit_typed(&plan).expect("rclcpp emit ok");
    assert!(
        src.contains("::nros::NodeHandle __h(::nros::global_handle(), nullptr, nullptr);"),
        "a node the launch file did not name must hand its component NO identity, so \
         the class's own literal stands;\n{src}"
    );
    assert!(
        !src.contains("__h(::nros::global_handle(), \"controller\""),
        "`controller` is the EXEC, not a declared name — passing it would silently \
         outrank the component class's literal for every node in every launch file \
         that omits `name=`"
    );
    // Narrowed to the handle LINE: the boot-config blob legitimately holds
    // `""` for an unset locator/rmw, so a whole-TU search for it proves
    // nothing.
    let handle_line = src
        .lines()
        .find(|l| l.contains("::nros::NodeHandle __h("))
        .expect("the rclcpp arm emits a handle");
    assert!(
        !handle_line.contains("\"\""),
        "an empty string must never reach the handle: it is `unset` at this edge, \
         and a node named `\"\"` is not a node; got: {handle_line}"
    );
}

/// Issue 1456, the other half of the negative direction — a launch file
/// that writes `name=""` / `namespace=""` says "unset", not "a node with
/// the empty name". The resolution is in the EMITTER (`filter`), so it
/// needs its own row: the `None` case above would pass with a `map`.
#[test]
fn typed_emit_rclcpp_empty_launch_identity_is_unset() {
    let mut plan = fixture_plan_rclcpp(&[(
        "ctrl_pkg",
        "controller",
        "controller",
        "ctrl_pkg::Controller",
        "ctrl_pkg/Controller.hpp",
    )]);
    plan.nodes[0].name = Some(String::new());
    plan.nodes[0].namespace = Some(String::new());
    let src = emit_typed(&plan).expect("rclcpp emit ok");
    assert!(
        src.contains("::nros::NodeHandle __h(::nros::global_handle(), nullptr, nullptr);"),
        "an empty declared name or namespace is `unset`, so the handle carries \
         nothing and the component class's literal stands;\n{src}"
    );
}

/// Issue 1456 — the TIERED arm builds its handle from the tier's executor,
/// and it needs the identity for the same reason the single-executor arm
/// does. One `if` in the template selects the executor expression, so a
/// fix applied to one arm and not the other is exactly the shape this
/// pins shut.
#[test]
fn typed_emit_rclcpp_launch_identity_reaches_the_tiered_arm() {
    // The node keeps its declared NAME (`ctrl`) because a tier's members
    // are matched by name; the namespace is what moves.
    let mut plan = fixture_plan_with_tiers();
    plan.board = "freertos".into();
    for n in &mut plan.nodes {
        n.shape = Some("rclcpp".into());
    }
    plan.nodes[0].namespace = Some("/island".into());
    let src = emit_typed(&plan).expect("tiered rclcpp emit ok");
    assert!(
        src.contains("::nros::NodeHandle __h(executor, \"ctrl\", \"/island\");"),
        "the tiered arm must carry the launch identity too;\n{src}"
    );
    assert!(
        src.contains("::nros::NodeHandle __h(executor, \"telem\", nullptr);"),
        "and the sibling that declared no namespace must still get nullptr there;\n{src}"
    );
}

#[test]
fn typed_emit_mixed_rclcpp_and_configure_shapes() {
    // One rclcpp node + one configure node in the same entry: each constructs
    // its own way; the includes carry both seams.
    let mut plan = fixture_plan_typed(&[
        (
            "ctrl_pkg",
            "controller",
            "controller",
            "ctrl_pkg::Controller",
            "ctrl_pkg/Controller.hpp",
        ),
        (
            "legacy_pkg",
            "legacy",
            "legacy",
            "legacy_pkg::Legacy",
            "legacy_pkg/Legacy.hpp",
        ),
    ]);
    plan.nodes[0].shape = Some("rclcpp".into());
    // plan.nodes[1] stays "configure".
    let src = emit_typed(&plan).expect("mixed emit ok");
    // node 0 = rclcpp: arena slot + handle construct, no Node/configure.
    assert!(src.contains("static ::ctrl_pkg::Controller* __nros_comp_0 = nullptr;"));
    assert!(src.contains("__nros_comp_0 = new (__nros_comp_buf_0) ::ctrl_pkg::Controller(__h);"));
    assert!(!src.contains("static ::rclcpp::Node __nros_node_0;"));
    // node 1 = configure: Node + configure, no arena slot.
    assert!(src.contains("static ::rclcpp::Node __nros_node_1;"));
    assert!(src.contains("static ::legacy_pkg::Legacy __nros_comp_1;"));
    assert!(src.contains("__nros_comp_1.configure(__nros_node_1)"));
    assert!(!src.contains("__nros_comp_buf_1"));
    // rclcpp placement-new include present because at least one rclcpp
    // node exists (phase-427 W4 — was `component_node.hpp`, deleted).
    assert!(src.contains("#include <new> // placement-new into the component arena slot"));
    assert!(
        !src.contains("component_node.hpp"),
        "the rclcpp entry must not include the DELETED component_node.hpp"
    );
}

#[test]
fn typed_emit_duplicate_pkg_makes_two_instances_one_include() {
    // Two `<node>` rows of the same pkg → two component objects, one include.
    let plan = fixture_plan_typed(&[
        ("twin_pkg", "a", "a", "twin_pkg::Twin", "twin_pkg/Twin.hpp"),
        ("twin_pkg", "b", "b", "twin_pkg::Twin", "twin_pkg/Twin.hpp"),
    ]);
    let src = emit_typed(&plan).expect("typed emit ok");
    assert_eq!(src.matches("#include \"twin_pkg/Twin.hpp\"").count(), 1);
    assert!(src.contains("static ::twin_pkg::Twin __nros_comp_0;"));
    assert!(src.contains("static ::twin_pkg::Twin __nros_comp_1;"));
    assert!(src.contains("::nros::create_node(__nros_node_0, \"a\", \"/\")"));
    assert!(src.contains("::nros::create_node(__nros_node_1, \"b\", \"/\")"));
}

#[test]
fn typed_emit_c_node_uses_factory_configure_seam() {
    // A `lang == "c"` node routes through the C-ABI factory + configure seam
    // (no C++ class, no header include); the entry hands it `ffi_handle()`.
    let mut plan = fixture_plan_typed(&[(
        "sensor_pkg",
        "sensor",
        "sensor",
        "sensor_pkg::Sensor",
        "sensor_pkg/Sensor.hpp",
    )]);
    plan.nodes[0].lang = Some(Lang::C);
    let src = emit_typed(&plan).expect("typed emit ok");
    // extern "C" factory + configure decls, mangled on pkg.
    assert!(src.contains("void* __nros_c_component_sensor_pkg_create(void);"));
    assert!(src.contains(
        "int32_t __nros_c_component_sensor_pkg_configure(const ::nros_cpp_node_t* node, void* executor, void* self);"
    ));
    // setup uses create() + configure(ffi_handle, executor_handle, self) — not a C++ class.
    assert!(src.contains("void* self = __nros_c_component_sensor_pkg_create();"));
    assert!(src.contains(
        "__nros_c_component_sensor_pkg_configure(__nros_node_0.ffi_handle(), __nros_node_0.executor_handle(), self)"
    ));
    // No C++ class storage / header / .configure for the C node.
    assert!(!src.contains("static ::sensor_pkg::Sensor"));
    assert!(!src.contains("#include \"sensor_pkg/Sensor.hpp\""));
    assert!(!src.contains("__nros_comp_0.configure"));
    // Still routes to the real executor via the named overload (phase 266).
    assert!(src.contains(
        "::nros::board::LinuxBoard::run_components(nros_boot_config_node_name(&NROS_BOOT_CONFIG), nros_boot_config_namespace(&NROS_BOOT_CONFIG), &__nros_entry_setup)"
    ));
}

#[test]
fn typed_emit_mixed_c_and_cpp_nodes() {
    let mut plan = fixture_plan_typed(&[
        (
            "talker_pkg",
            "talker",
            "talker",
            "talker_pkg::Talker",
            "talker_pkg/Talker.hpp",
        ),
        (
            "sensor_pkg",
            "sensor",
            "sensor",
            "sensor_pkg::Sensor",
            "sensor_pkg/Sensor.hpp",
        ),
    ]);
    plan.nodes[1].lang = Some(Lang::C); // sensor is C
    let src = emit_typed(&plan).expect("typed emit ok");
    // C++ node: header + class + .configure.
    assert!(src.contains("#include \"talker_pkg/Talker.hpp\""));
    assert!(src.contains("static ::talker_pkg::Talker __nros_comp_0;"));
    assert!(src.contains("__nros_comp_0.configure(__nros_node_0)"));
    // C node: factory seam, no header/class.
    assert!(src.contains("void* self = __nros_c_component_sensor_pkg_create();"));
    assert!(!src.contains("static ::sensor_pkg::Sensor"));
}

#[test]
fn typed_emit_nuttx_board_uses_nuttxboard_run_components() {
    // Phase 266: embedded boards use the 3-arg (locator, session_name, setup) overload.
    let mut plan = fixture_plan_typed(&[("t_pkg", "t", "t", "t_pkg::T", "t_pkg/T.hpp")]);
    plan.board = "nuttx".into();
    let src = emit_typed(&plan).expect("typed emit ok");
    assert!(src.contains(
        "::nros::board::NuttxBoard::run_components(NROS_ENTRY_LOCATOR, nros_boot_config_node_name(&NROS_BOOT_CONFIG), nros_boot_config_namespace(&NROS_BOOT_CONFIG), &__nros_entry_setup)"
    ));
}

#[test]
fn typed_emit_threadx_board_uses_threadxboard_run_components() {
    // Phase 246 — the ThreadX family keys (host sim + bare-metal riscv64) all
    // route the typed entry to the `ThreadxBoard` adapter's `run_components`.
    // Phase 266: uses the 3-arg (locator, session_name, setup) named overload.
    for key in [
        "threadx",
        "threadx-linux",
        "threadx-qemu-riscv64",
        "rv-virt-threadx",
    ] {
        let mut plan = fixture_plan_typed(&[("t_pkg", "t", "t", "t_pkg::T", "t_pkg/T.hpp")]);
        plan.board = key.into();
        let src = emit_typed(&plan).expect("typed emit ok");
        assert!(
            src.contains(
                "::nros::board::ThreadxBoard::run_components(NROS_ENTRY_LOCATOR, nros_boot_config_node_name(&NROS_BOOT_CONFIG), nros_boot_config_namespace(&NROS_BOOT_CONFIG), &__nros_entry_setup)"
            ),
            "board key {key} must map to ThreadxBoard::run_components with named overload"
        );
    }
}

#[test]
fn typed_emit_native_single_node_bakes_name_in_boot_config() {
    // Phase 266 — single-node native entry: boot config carries the node name.
    let plan = fixture_plan_typed(&[(
        "talker_pkg",
        "talker",
        "talker",
        "talker_pkg::Talker",
        "talker_pkg/Talker.hpp",
    )]);
    let src = emit_typed(&plan).expect("typed emit ok");
    assert!(src.contains("#include <nros/boot_config.h>"));
    assert!(src.contains("NROS_BOOT_SET_NODE_NAME"));
    assert!(src.contains(".node_name  = \"talker\""));
    assert!(src.contains(
        "::nros::board::LinuxBoard::run_components(nros_boot_config_node_name(&NROS_BOOT_CONFIG), nros_boot_config_namespace(&NROS_BOOT_CONFIG), &__nros_entry_setup)"
    ));
}

#[test]
fn typed_emit_errors_when_class_missing() {
    let plan = fixture_plan(&[("talker_pkg", "talker")]); // class_name None
    let err = emit_typed(&plan).unwrap_err();
    assert!(err.contains("missing class_name"), "{err}");
    assert!(err.contains("talker_pkg"), "{err}");
}

#[test]
fn typed_emit_param_services_block_present_when_enabled() {
    // Phase 269 W1 (amended by issue 0745) — param SEEDING now emits per
    // node BEFORE construction (emit_declare_params, the 0255 remap rule:
    // an rclcpp ctor reads declare_parameter initials immediately);
    // param_services gates only the runtime get/set surface.
    let mut plan = fixture_plan_typed(&[(
        "param_talker_pkg",
        "param_talker",
        "param_talker",
        "param_talker_pkg::ParamTalker",
        "param_talker_pkg/ParamTalker.hpp",
    )]);
    plan.param_services = true;
    plan.nodes[0].params = vec![("publish_period_ms".into(), "250".into())];
    let src = emit_typed(&plan).expect("typed cpp emit ok");
    assert!(src.contains("nros_cpp_register_parameter_services(__exec)"));
    assert!(src.contains(
        "nros_cpp_declare_param(::nros::global_handle(), 0, \"publish_period_ms\", \"250\")"
    ));
    // issue 0745 — seeding precedes construction.
    let seed_at = src.find("nros_cpp_declare_param").unwrap();
    let construct_at = src
        .find(".configure(")
        .or_else(|| src.find("new ("))
        .unwrap();
    assert!(
        seed_at < construct_at,
        "param seeding must precede construction"
    );
    // must appear after configure, before return 0
    let reg_at = src.find("nros_cpp_register_parameter_services").unwrap();
    let ret_at = src.rfind("return 0;").unwrap();
    assert!(reg_at < ret_at, "param block must precede return 0");
    // confirms executor handle fetched from global
    assert!(src.contains("::nros::global_handle()"));
}

#[test]
fn typed_emit_param_services_absent_when_disabled() {
    // Guard: non-param plans produce byte-identical output (no param block).
    let plan = fixture_plan_typed(&[(
        "talker_pkg",
        "talker",
        "talker",
        "talker_pkg::Talker",
        "talker_pkg/Talker.hpp",
    )]);
    let src = emit_typed(&plan).expect("typed cpp emit ok");
    assert!(!src.contains("nros_cpp_register_parameter_services"));
    assert!(!src.contains("nros_cpp_declare_param"));
}

#[test]
fn typed_emit_lifecycle_active_emits_autostart_block() {
    // Phase 269 W2 — lifecycle = Some("active") → nros_cpp_lifecycle_autostart(__exec, 2u)
    // in the post-configure block, AFTER any param block, BEFORE return 0.
    let mut plan = fixture_plan_typed(&[(
        "lifecycle_talker_pkg",
        "lifecycle_talker",
        "lifecycle_talker",
        "lifecycle_talker_pkg::LifecycleTalker",
        "lifecycle_talker_pkg/LifecycleTalker.hpp",
    )]);
    plan.lifecycle = Some("active".into());
    let src = emit_typed(&plan).expect("typed cpp lifecycle emit ok");
    // autostart call with code 2 (active = configure + activate)
    assert!(
        src.contains("nros_cpp_lifecycle_autostart(__exec, 2u)"),
        "expected nros_cpp_lifecycle_autostart(__exec, 2u) in:\n{src}"
    );
    // executor handle from global_handle
    assert!(src.contains("::nros::global_handle()"));
    // AFTER configure loop (configure call or C factory), BEFORE return 0
    let autostart_at = src.find("nros_cpp_lifecycle_autostart").unwrap();
    let ret_at = src.rfind("return 0;").unwrap();
    assert!(
        autostart_at < ret_at,
        "lifecycle block must precede return 0"
    );
    // configure call precedes the lifecycle block
    let cfg_at = src.find("__nros_comp_0.configure(__nros_node_0)").unwrap();
    assert!(
        cfg_at < autostart_at,
        "lifecycle block must follow configure call"
    );
}

#[test]
fn typed_emit_lifecycle_configure_emits_code_1() {
    let mut plan = fixture_plan_typed(&[("lc_pkg", "lc", "lc", "lc_pkg::Lc", "lc_pkg/Lc.hpp")]);
    plan.lifecycle = Some("configure".into());
    let src = emit_typed(&plan).expect("typed cpp lifecycle configure emit ok");
    assert!(
        src.contains("nros_cpp_lifecycle_autostart(__exec, 1u)"),
        "expected autostart_code 1 for 'configure'; src:\n{src}"
    );
}

#[test]
fn typed_emit_lifecycle_none_emits_code_0() {
    let mut plan = fixture_plan_typed(&[("lc_pkg", "lc", "lc", "lc_pkg::Lc", "lc_pkg/Lc.hpp")]);
    plan.lifecycle = Some("none".into());
    let src = emit_typed(&plan).expect("typed cpp lifecycle none emit ok");
    assert!(
        src.contains("nros_cpp_lifecycle_autostart(__exec, 0u)"),
        "expected autostart_code 0 for 'none'; src:\n{src}"
    );
}

#[test]
fn typed_emit_lifecycle_absent_when_disabled() {
    // Guard: lifecycle = None → byte-identical output (no lifecycle block).
    let plan = fixture_plan_typed(&[(
        "talker_pkg",
        "talker",
        "talker",
        "talker_pkg::Talker",
        "talker_pkg/Talker.hpp",
    )]);
    let src = emit_typed(&plan).expect("typed cpp emit ok");
    assert!(
        !src.contains("nros_cpp_lifecycle_autostart"),
        "lifecycle block must be absent when lifecycle = None"
    );
}

#[test]
fn typed_emit_lifecycle_after_param_block() {
    // Phase 269 W2 — when both param_services and lifecycle are set, the lifecycle
    // block must appear AFTER the param block (same order as the Rust macro: params → lifecycle).
    let mut plan = fixture_plan_typed(&[(
        "talker_pkg",
        "talker",
        "talker",
        "talker_pkg::Talker",
        "talker_pkg/Talker.hpp",
    )]);
    plan.param_services = true;
    plan.nodes[0].params = vec![("foo".into(), "bar".into())];
    plan.lifecycle = Some("active".into());
    let src = emit_typed(&plan).expect("typed cpp combined emit ok");
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
    use nros_orchestration_ir::{ResolvedTier, ResolvedTierTable};
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
    let mut plan = fixture_plan_typed(&[
        (
            "ctrl_pkg",
            "ctrl",
            "ctrl",
            "ctrl_pkg::Ctrl",
            "ctrl_pkg/Ctrl.hpp",
        ),
        (
            "telem_pkg",
            "telem",
            "telem",
            "telem_pkg::Telem",
            "telem_pkg/Telem.hpp",
        ),
    ]);
    plan.nodes[0].callback_groups = vec!["ctrl_grp".into()];
    plan.nodes[0].sched_context = Some(0);
    plan.nodes[1].callback_groups = vec!["telem_grp".into()];
    plan.nodes[1].sched_context = Some(1);
    plan.resolved_tiers = Some(ResolvedTierTable {
        tiers: vec![high_tier, low_tier],
    });
    plan
}

/// issue 1272 -- two nodes that set the SAME parameter name each seed it on
/// their own node index, and each node's seeds come right before that
/// node's construction, so the index the seed names is the one the
/// executor hands out next.
#[test]
fn typed_emit_seeds_each_node_on_its_own_index() {
    let mut plan = fixture_plan_typed(&[
        ("a_pkg", "alpha", "alpha", "a_pkg::Alpha", "a_pkg/Alpha.hpp"),
        ("b_pkg", "beta", "beta", "b_pkg::Beta", "b_pkg/Beta.hpp"),
    ]);
    plan.nodes[0].params = vec![("rate".into(), "10".into())];
    plan.nodes[1].params = vec![("rate".into(), "20".into())];
    let src = emit_typed(&plan).expect("typed cpp emit ok");

    let seed_a = "nros_cpp_declare_param(::nros::global_handle(), 0, \"rate\", \"10\")";
    let seed_b = "nros_cpp_declare_param(::nros::global_handle(), 1, \"rate\", \"20\")";
    let create_a = "::nros::create_node(__nros_node_0, \"alpha\", \"/\")";
    let create_b = "::nros::create_node(__nros_node_1, \"beta\", \"/\")";
    let at = |s: &str| {
        src.find(s)
            .unwrap_or_else(|| panic!("missing `{s}`; got:\n{src}"))
    };
    assert!(
        at(seed_a) < at(create_a) && at(create_a) < at(seed_b) && at(seed_b) < at(create_b),
        "each node's seeds must directly precede its own construction; got:\n{src}"
    );
}

/// issue 1272 -- each tier setup builds its nodes on the tier's OWN
/// executor, so the first node of every tier is index 0 there.
#[test]
fn typed_emit_tier_seeds_restart_at_zero_per_tier() {
    let mut plan = fixture_plan_with_tiers();
    plan.nodes[0].params = vec![("period".into(), "10".into())];
    plan.nodes[1].params = vec![("period".into(), "100".into())];
    let src = emit_typed(&plan).expect("typed cpp tier emit ok");

    assert!(
        src.contains("nros_cpp_declare_param(executor, 0, \"period\", \"10\")"),
        "ctrl is tier 0's first node; got:\n{src}"
    );
    assert!(
        src.contains("nros_cpp_declare_param(executor, 0, \"period\", \"100\")"),
        "telem is tier 1's first node, index 0 on tier 1's executor; got:\n{src}"
    );
    assert!(
        !src.contains("nros_cpp_declare_param(executor, 1,"),
        "no tier builds a second node here; got:\n{src}"
    );
}

#[test]
fn typed_emit_group_split_node_falls_back_to_sched_context_path() {
    // Phase 282 follow-up (RFC-0047) — ONE node with callback groups on TWO
    // tiers (`group_tiers = { ctrl = "high", telem = "low" }`) cannot use
    // run_tiers: per-tier setup fns construct whole nodes, so the node
    // landed on the last tier and both timers ran at that cadence
    // (regression caught by realtime_subnode_cpp_e2e: ctrl=6 telem=5).
    // Such plans must keep the single-executor sched-context path.
    use nros_orchestration_ir::{ResolvedTier, ResolvedTierTable};
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
        members: vec![("sub_node".into(), "ctrl".into())],
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
        members: vec![("sub_node".into(), "telem".into())],
    };
    let mut plan = fixture_plan_typed(&[(
        "subnode_pkg",
        "sub_node",
        "sub_node",
        "subnode_pkg::SubNode",
        "subnode_pkg/SubNode.hpp",
    )]);
    plan.nodes[0].callback_groups = vec!["ctrl".into(), "telem".into()];
    plan.resolved_tiers = Some(ResolvedTierTable {
        tiers: vec![high_tier, low_tier],
    });
    let src = emit_typed(&plan).expect("typed cpp group-split emit ok");

    // Sched-context path: per-group seeding present, run_tiers absent.
    assert!(
        src.contains("nros_cpp_bind_group_sched"),
        "group-split node must seed bind_group_sched; src:\n{src}"
    );
    assert!(
        src.contains("\"ctrl\"") && src.contains("\"telem\""),
        "both groups must be seeded; src:\n{src}"
    );
    assert!(
        !src.contains("__nros_entry_setup_tier_0"),
        "group-split plan must NOT use the run_tiers path; src:\n{src}"
    );
    assert!(
        !src.contains("run_tiers("),
        "group-split plan must NOT call run_tiers; src:\n{src}"
    );
}

#[test]
fn typed_emit_tiers_native_uses_run_tiers_path() {
    // Phase 274.W2 — native board + multi-tier emits per-tier setup functions +
    // run_tiers call instead of the old sched-context wiring.
    let plan = fixture_plan_with_tiers();
    let src = emit_typed(&plan).expect("typed cpp tier emit ok");

    // Per-tier setup functions emitted.
    assert!(
        src.contains("static int32_t __nros_entry_setup_tier_0(void* executor)"),
        "expected tier-0 setup fn; got:\n{src}"
    );
    assert!(
        src.contains("static int32_t __nros_entry_setup_tier_1(void* executor)"),
        "expected tier-1 setup fn; got:\n{src}"
    );
    // Each setup fn creates only its tier's nodes via create_node_on.
    assert!(
        src.contains("::nros::create_node_on(__nros_node_0, executor, \"ctrl\", \"/\")"),
        "ctrl node must use create_node_on in tier-0 setup; src:\n{src}"
    );
    assert!(
        src.contains("::nros::create_node_on(__nros_node_1, executor, \"telem\", \"/\")"),
        "telem node must use create_node_on in tier-1 setup; src:\n{src}"
    );
    // NativeTierSpec array emitted.
    assert!(
        src.contains("static const ::nros::board::NativeTierSpec __nros_tiers[2]"),
        "expected 2-element NativeTierSpec array; src:\n{src}"
    );
    assert!(
        src.contains("\"high\""),
        "high tier name in spec table; src:\n{src}"
    );
    assert!(
        src.contains("\"low\""),
        "low tier name in spec table; src:\n{src}"
    );
    assert!(src.contains("80LL"), "high priority 80LL; src:\n{src}");
    assert!(src.contains("10LL"), "low priority 10LL; src:\n{src}");
    // main calls run_tiers.
    assert!(
        src.contains("::nros::board::LinuxBoard::run_tiers("),
        "main must call LinuxBoard::run_tiers; src:\n{src}"
    );
    // Old sched-context wiring must NOT appear in the run_tiers path.
    assert!(
        !src.contains("__nros_sc_ids"),
        "run_tiers path must not emit sc_ids; src:\n{src}"
    );
    assert!(
        !src.contains("nros_cpp_create_sched_context"),
        "run_tiers path must not emit create_sched_context; src:\n{src}"
    );
    assert!(
        !src.contains("nros_cpp_bind_node_name_sched"),
        "run_tiers path must not emit bind_node_name_sched; src:\n{src}"
    );
}

#[test]
fn typed_emit_tiers_embedded_uses_sched_context_path() {
    // Phase 272/273 (W2) — a sched-context embedded board (ThreadX) + multi-tier
    // still uses sched-context wiring (bind_node_name_sched + bind_group_sched) because
    // run_tiers is limited to native + FreeRTOS + Zephyr + NuttX (phase-281 W3/W3a).
    // ThreadX keeps board_is_embedded=true && !run_tiers → the single-executor
    // sched-context path.
    use nros_orchestration_ir::{ResolvedTier, ResolvedTierTable};
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
    let mut plan = fixture_plan_typed(&[
        (
            "ctrl_pkg",
            "ctrl",
            "ctrl",
            "ctrl_pkg::Ctrl",
            "ctrl_pkg/Ctrl.hpp",
        ),
        (
            "telem_pkg",
            "telem",
            "telem",
            "telem_pkg::Telem",
            "telem_pkg/Telem.hpp",
        ),
    ]);
    // Sched-context embedded board (ThreadX) → sched-context path (NOT run_tiers).
    plan.board = "threadx".into();
    plan.nodes[0].callback_groups = vec!["ctrl_grp".into()];
    plan.nodes[0].sched_context = Some(0);
    plan.nodes[1].callback_groups = vec!["telem_grp".into()];
    plan.nodes[1].sched_context = Some(1);
    plan.resolved_tiers = Some(ResolvedTierTable {
        tiers: vec![high_tier, low_tier],
    });
    let src = emit_typed(&plan).expect("typed cpp embedded tier emit ok");
    // Sched-context IDs array declared.
    assert!(
        src.contains("uint8_t __nros_sc_ids[2] = {0};"),
        "embedded tier must emit sc_ids array; got:\n{src}"
    );
    // High tier: no RT class → Fifo SC via the common-backend call, carrying
    // only os_pri=80 (nullptr/0 = absent policy). RFC-0052: the codegen
    // forwards RAW tier fields; the lowering lives in the FFI backend.
    assert!(
        src.contains(
            "nros_cpp_create_sched_context_from_policy(__exec, nullptr, 0ull, 0ull, 0ull, nullptr, 80u, &__nros_sc_ids[0])"
        ),
        "expected tier 0 from_policy call (Fifo, os_pri=80); got:\n{src}"
    );
    // Bind seeds for each tiered node.
    assert!(
        src.contains("nros_cpp_bind_node_name_sched(__exec, \"ctrl\", \"/\", __nros_sc_ids[0])"),
        "ctrl must be seeded; src:\n{src}"
    );
    assert!(
        src.contains("nros_cpp_bind_node_name_sched(__exec, \"telem\", \"/\", __nros_sc_ids[1])"),
        "telem must be seeded; src:\n{src}"
    );
    // run_tiers must NOT be called (embedded boards use single executor).
    assert!(
        !src.contains("LinuxBoard::run_tiers"),
        "embedded board must not emit run_tiers; src:\n{src}"
    );
}

#[test]
fn typed_emit_single_executor_forwards_real_time_tier_to_backend() {
    // Phase 297 W1 / RFC-0052 (common backend) — the single-executor
    // sched-context path (ThreadX + group-split) forwards a `real_time`
    // tier's RAW class/budget/period/deadline to
    // `nros_cpp_create_sched_context_from_policy`, whose backend
    // (`SchedContext::from_tier_policy`) does the class→Sporadic lowering —
    // the SAME one the Rust runtime uses. The codegen re-derives nothing.
    use nros_orchestration_ir::{ResolvedTier, ResolvedTierTable};
    let rt_tier = ResolvedTier {
        name: "control".into(),
        priority: 90,
        stack_bytes: None,
        spin_period_us: Some(5_000),
        preempt_threshold: None,
        time_slice_us: None,
        sched_class: None,
        class: Some("real_time".into()),
        period_us: Some(20_000),
        budget_us: Some(3_000),
        deadline_us: Some(15_000),
        deadline_policy: Some("fault".into()),
        core: None,
        members: vec![("ctrl".into(), "ctrl_grp".into())],
    };
    let mut plan = fixture_plan_typed(&[(
        "ctrl_pkg",
        "ctrl",
        "ctrl",
        "ctrl_pkg::Ctrl",
        "ctrl_pkg/Ctrl.hpp",
    )]);
    plan.board = "threadx".into();
    plan.nodes[0].callback_groups = vec!["ctrl_grp".into()];
    plan.nodes[0].sched_context = Some(0);
    plan.resolved_tiers = Some(ResolvedTierTable {
        tiers: vec![rt_tier],
    });
    let src = emit_typed(&plan).expect("typed cpp real_time tier emit ok");
    // RFC-0052 common backend: the codegen forwards the RAW tier fields to
    // `nros_cpp_create_sched_context_from_policy`; the class→Sporadic +
    // budget/period lowering happens in the FFI backend
    // (`SchedContext::from_tier_policy`), unit-tested in nros-node. The
    // codegen must NOT re-derive the mapping (no `__sc.class_ = ...`).
    assert!(
        src.contains(
            "nros_cpp_create_sched_context_from_policy(__exec, \"real_time\", 20000ull, 3000ull, 15000ull, \"fault\", 90u, &__nros_sc_ids[0])"
        ),
        "real_time tier must forward raw fields to the backend; got:\n{src}"
    );
    assert!(
        !src.contains("__sc.class_"),
        "codegen must not re-derive the class mapping (common backend); got:\n{src}"
    );
}

#[test]
fn typed_emit_tiers_freertos_embedded_uses_run_tiers_path() {
    // Phase 274.W3 — FreeRTOS embedded board + multi-tier emits per-tier setup
    // functions + FreertosBoard::run_tiers via nros_app_main +
    // NROS_APP_MAIN_REGISTER_VOID (NOT the sched-context path, NOT int main).
    let mut plan = fixture_plan_with_tiers();
    plan.board = "freertos".into(); // FreertosBoard

    let src = emit_typed(&plan).expect("typed cpp freertos tier emit ok");

    // Per-tier setup functions emitted.
    assert!(
        src.contains("static int32_t __nros_entry_setup_tier_0(void* executor)"),
        "expected tier-0 setup fn; got:\n{src}"
    );
    assert!(
        src.contains("static int32_t __nros_entry_setup_tier_1(void* executor)"),
        "expected tier-1 setup fn; got:\n{src}"
    );
    // NativeTierSpec array emitted.
    assert!(
        src.contains("static const ::nros::board::NativeTierSpec __nros_tiers[2]"),
        "expected 2-element NativeTierSpec array; src:\n{src}"
    );
    // FreertosBoard::run_tiers called (not LinuxBoard).
    assert!(
        src.contains("::nros::board::FreertosBoard::run_tiers("),
        "nros_app_main must call FreertosBoard::run_tiers; src:\n{src}"
    );
    // FreeRTOS embedded entry point: nros_app_main + NROS_APP_MAIN_REGISTER_VOID.
    assert!(
        src.contains("extern \"C\" int nros_app_main("),
        "FreeRTOS run_tiers must emit nros_app_main; src:\n{src}"
    );
    assert!(
        src.contains("NROS_APP_MAIN_REGISTER_VOID()"),
        "FreeRTOS run_tiers must emit NROS_APP_MAIN_REGISTER_VOID; src:\n{src}"
    );
    // NOT int main (that's native).
    assert!(
        !src.contains("int main("),
        "FreeRTOS run_tiers must NOT emit int main; src:\n{src}"
    );
    // Old sched-context wiring must NOT appear.
    assert!(
        !src.contains("__nros_sc_ids"),
        "FreeRTOS run_tiers path must not emit sc_ids; src:\n{src}"
    );
    assert!(
        !src.contains("nros_cpp_create_sched_context"),
        "FreeRTOS run_tiers path must not emit create_sched_context; src:\n{src}"
    );
}

#[test]
fn typed_emit_tiers_zephyr_embedded_uses_run_tiers_path() {
    // phase-281 W3a — Zephyr embedded board + multi-tier emits per-tier setup
    // functions + ZephyrBoard::run_tiers via a plain `int main(void)` (the Zephyr
    // kernel calls main directly — NO nros_app_main, NO sched-context path).
    let mut plan = fixture_plan_with_tiers();
    plan.board = "zephyr".into(); // ZephyrBoard

    let src = emit_typed(&plan).expect("typed cpp zephyr tier emit ok");

    // Per-tier setup functions emitted.
    assert!(
        src.contains("static int32_t __nros_entry_setup_tier_0(void* executor)"),
        "expected tier-0 setup fn; got:\n{src}"
    );
    assert!(
        src.contains("static int32_t __nros_entry_setup_tier_1(void* executor)"),
        "expected tier-1 setup fn; got:\n{src}"
    );
    // NativeTierSpec array emitted.
    assert!(
        src.contains("static const ::nros::board::NativeTierSpec __nros_tiers[2]"),
        "expected 2-element NativeTierSpec array; src:\n{src}"
    );
    // ZephyrBoard::run_tiers called (not LinuxBoard / FreertosBoard).
    assert!(
        src.contains("::nros::board::ZephyrBoard::run_tiers("),
        "main must call ZephyrBoard::run_tiers; src:\n{src}"
    );
    // Zephyr entry point: plain int main(void), kernel calls it directly.
    assert!(
        src.contains("int main(void) {"),
        "Zephyr run_tiers must emit int main(void); src:\n{src}"
    );
    // NOT the FreeRTOS/startup.c app_main shape.
    assert!(
        !src.contains("nros_app_main"),
        "Zephyr run_tiers must NOT emit nros_app_main; src:\n{src}"
    );
    assert!(
        !src.contains("NROS_APP_MAIN_REGISTER_VOID"),
        "Zephyr run_tiers must NOT emit NROS_APP_MAIN_REGISTER_VOID; src:\n{src}"
    );
    // Old sched-context wiring must NOT appear (this is the run_tiers path).
    assert!(
        !src.contains("__nros_sc_ids"),
        "Zephyr run_tiers path must not emit sc_ids; src:\n{src}"
    );
    assert!(
        !src.contains("nros_cpp_create_sched_context"),
        "Zephyr run_tiers path must not emit create_sched_context; src:\n{src}"
    );
    // run_tiers path must not CALL run_components (the string appears once in the
    // file-header doc comment, so assert on the call form specifically).
    assert!(
        !src.contains("ZephyrBoard::run_components"),
        "Zephyr run_tiers path must not call ZephyrBoard::run_components; src:\n{src}"
    );
}

#[test]
fn typed_emit_tiers_nuttx_embedded_uses_run_tiers_path() {
    // phase-281 W3 (nuttx) — NuttX embedded board + multi-tier emits per-tier
    // setup functions + NuttxBoard::run_tiers via nros_app_main +
    // NROS_APP_MAIN_REGISTER_VOID (the NuttX startup path calls app_main, like
    // FreeRTOS — NOT Zephyr's int main(void), NOT the sched-context path).
    let mut plan = fixture_plan_with_tiers();
    plan.board = "nuttx".into(); // NuttxBoard

    let src = emit_typed(&plan).expect("typed cpp nuttx tier emit ok");

    // Per-tier setup functions emitted.
    assert!(
        src.contains("static int32_t __nros_entry_setup_tier_0(void* executor)"),
        "expected tier-0 setup fn; got:\n{src}"
    );
    assert!(
        src.contains("static int32_t __nros_entry_setup_tier_1(void* executor)"),
        "expected tier-1 setup fn; got:\n{src}"
    );
    // NativeTierSpec array emitted.
    assert!(
        src.contains("static const ::nros::board::NativeTierSpec __nros_tiers[2]"),
        "expected 2-element NativeTierSpec array; src:\n{src}"
    );
    // NuttxBoard::run_tiers called (not LinuxBoard / FreertosBoard / ZephyrBoard).
    assert!(
        src.contains("::nros::board::NuttxBoard::run_tiers("),
        "nros_app_main must call NuttxBoard::run_tiers; src:\n{src}"
    );
    // NuttX embedded entry point: nros_app_main + NROS_APP_MAIN_REGISTER_VOID
    // (the app_main startup shape, shared with FreeRTOS).
    assert!(
        src.contains("extern \"C\" int nros_app_main("),
        "NuttX run_tiers must emit nros_app_main; src:\n{src}"
    );
    assert!(
        src.contains("NROS_APP_MAIN_REGISTER_VOID()"),
        "NuttX run_tiers must emit NROS_APP_MAIN_REGISTER_VOID; src:\n{src}"
    );
    // NOT int main (that's native) and NOT the Zephyr int main(void).
    assert!(
        !src.contains("int main("),
        "NuttX run_tiers must NOT emit int main; src:\n{src}"
    );
    // Old sched-context wiring must NOT appear (this is the run_tiers path).
    assert!(
        !src.contains("__nros_sc_ids"),
        "NuttX run_tiers path must not emit sc_ids; src:\n{src}"
    );
    assert!(
        !src.contains("nros_cpp_create_sched_context"),
        "NuttX run_tiers path must not emit create_sched_context; src:\n{src}"
    );
}

#[test]
fn typed_emit_tiers_rclcpp_embedded_node_is_seeded() {
    // Phase 272 (W2) — rclcpp-shape tiered node on an embedded board IS seeded
    // via bind_node_name_sched (the #124 dissolve). Native boards use run_tiers
    // instead; this test covers the embedded (sched-context) path.
    use nros_orchestration_ir::{ResolvedTier, ResolvedTierTable};
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
    let mut plan = fixture_plan_rclcpp(&[(
        "ctrl_pkg",
        "ctrl",
        "ctrl",
        "ctrl_pkg::Ctrl",
        "ctrl_pkg/Ctrl.hpp",
    )]);
    // ThreadX — a sched-context embedded board (phase-281 W3a moved Zephyr and
    // W3(nuttx) moved NuttX onto the run_tiers path, so this seeding proof now
    // uses a board that still schedules via the single-executor sched-context wiring).
    plan.board = "threadx".into();
    plan.nodes[0].callback_groups = vec!["ctrl_grp".into()];
    plan.nodes[0].sched_context = Some(0);
    plan.resolved_tiers = Some(ResolvedTierTable {
        tiers: vec![high_tier],
    });
    let src = emit_typed(&plan).expect("rclcpp embedded tier emit ok");
    // rclcpp-shape node MUST be seeded (the #124 proof, embedded path).
    assert!(
        src.contains("nros_cpp_bind_node_name_sched(__exec, \"ctrl\", \"/\", __nros_sc_ids[0])"),
        "rclcpp-shape tiered node must be seeded via bind_node_name_sched; src:\n{src}"
    );
    // rclcpp construction path unchanged (placement-new with handle).
    assert!(
        src.contains("__nros_comp_0 = new (__nros_comp_buf_0) ::ctrl_pkg::Ctrl(__h);"),
        "rclcpp node still constructs via placement-new"
    );
    // Seed precedes construction.
    let seed_at = src
        .find("nros_cpp_bind_node_name_sched(__exec, \"ctrl\"")
        .unwrap();
    let ctor_at = src.find("new (__nros_comp_buf_0)").unwrap();
    assert!(seed_at < ctor_at, "seed must precede rclcpp construction");
}

#[test]
fn typed_emit_no_tiers_uses_plain_create_node() {
    // Guard: empty resolved_tiers keeps byte-identical plain create (no seed, no sched).
    let plan = fixture_plan_typed(&[(
        "talker_pkg",
        "talker",
        "talker",
        "talker_pkg::Talker",
        "talker_pkg/Talker.hpp",
    )]);
    let src = emit_typed(&plan).expect("typed cpp no-tier emit ok");
    assert!(
        !src.contains("__nros_sc_ids"),
        "no-tier plan must not emit sc_ids"
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
        src.contains("::nros::create_node(__nros_node_0, \"talker\", \"/\")"),
        "no-tier plan must use plain create_node"
    );
    assert!(
        !src.contains(".sched("),
        "no-tier plan must not use NodeBuilder sched"
    );
}

/// Issue 1443 — the C++ pack passes the PLAN's namespace to `create_node`
/// (single executor) and `create_node_on` (a tier's own), instead of
/// relying on either function's `nullptr` default.
///
/// Same three inputs and one answer as the C pack's twin: a real namespace
/// renders itself, `None` and `Some("")` render `"/"`, never `""`. Both
/// packs read `lower::node_namespace`, so they cannot answer differently —
/// which is the cross-language divergence the issue reported.
#[test]
fn typed_emit_creates_each_node_at_its_plan_namespace() {
    for (declared, rendered) in [
        (Some("/island"), "/island"),
        (Some("/a/b"), "/a/b"),
        (None, "/"),
        (Some(""), "/"),
    ] {
        let mut plan = fixture_plan_typed(&[(
            "talker_pkg",
            "talker",
            "talker",
            "talker_pkg::Talker",
            "talker_pkg/Talker.hpp",
        )]);
        plan.nodes[0].namespace = declared.map(str::to_string);
        let src = emit_typed(&plan).expect("typed cpp emit ok");
        assert!(
            src.contains(&format!(
                "::nros::create_node(__nros_node_0, \"talker\", \"{rendered}\")"
            )),
            "namespace {declared:?} must render as {rendered:?}; src:\n{src}"
        );
        assert!(
            !src.contains("::nros::create_node(__nros_node_0, \"talker\", \"\")"),
            "an empty namespace must never reach the C++ edge; src:\n{src}"
        );
    }
}

/// The tiered arm of the same rule — `create_node_on`, one tier each.
#[test]
fn typed_emit_tiered_creates_each_node_at_its_plan_namespace() {
    let mut plan = fixture_plan_with_tiers();
    plan.nodes[0].namespace = Some("/island".into());
    // nodes[1] keeps `None`, so both arms appear in one render.
    let src = emit_typed(&plan).expect("typed cpp tiered emit ok");
    assert!(
        src.contains("::nros::create_node_on(__nros_node_0, executor, \"ctrl\", \"/island\")"),
        "tier-0 node must be created under its plan namespace; src:\n{src}"
    );
    assert!(
        src.contains("::nros::create_node_on(__nros_node_1, executor, \"telem\", \"/\")"),
        "a node the plan gives no namespace stays at the root; src:\n{src}"
    );
}

// ---------------------------------------------------------------
// Issue 1003 — the boot wrapper has ONE derivation
// ---------------------------------------------------------------

/// The wrapper each board family gets. Written as a table because the bug
/// this replaces was a board missing from one of two hand-written branch
/// chains: a table makes an omission visible as a missing row.
#[test]
fn every_board_family_derives_its_boot_shape_once() {
    for (board, want) in [
        ("native", BootShape::Host),
        ("posix", BootShape::Host),
        ("zephyr", BootShape::Kernel),
        ("nuttx", BootShape::App),
        ("freertos", BootShape::App),
        ("threadx", BootShape::App),
        ("threadx-linux", BootShape::App),
    ] {
        assert_eq!(
            boot_shape(board),
            want,
            "board '{board}' derived the wrong boot shape"
        );
    }
}

/// ThreadX's `startup.c` owns `main`, so its entry must be `nros_app_main`
/// — a host `int main` would be a second `main` in an image whose board
/// already defines one.
///
/// ThreadX is the one board the two former branch chains disagreed about,
/// and it is kept out of the per-tier chain by `use_run_tiers`. Pinning it
/// here means the shared derivation is right about it on its own terms,
/// rather than by an argument about a condition elsewhere.
#[test]
fn threadx_is_not_treated_as_a_host_board() {
    assert_ne!(
        boot_shape("threadx"),
        BootShape::Host,
        "ThreadX boots through the board's startup.c, so a host `int main` \
would collide with the one the board defines"
    );
    assert_eq!(boot_shape("threadx"), boot_shape("nuttx"));
}

/// The boards that DO have `run_tiers` still emit it, each in its own
/// wrapper — the consolidation must not have narrowed what works.
#[test]
fn boards_with_run_tiers_still_emit_their_own_wrapper() {
    for (board, wrapper) in [
        ("native", "int main(int /*argc*/, char** /*argv*/) {"),
        ("zephyr", "int main(void) {"),
        (
            "nuttx",
            "extern \"C\" int nros_app_main(int /*argc*/, char** /*argv*/) {",
        ),
        (
            "freertos",
            "extern \"C\" int nros_app_main(int /*argc*/, char** /*argv*/) {",
        ),
    ] {
        let mut plan = fixture_plan_with_tiers();
        plan.board = board.into();
        let src =
            emit_typed(&plan).unwrap_or_else(|e| panic!("{board} multi-tier emit failed: {e}"));
        assert!(
            src.contains("::run_tiers("),
            "{board} must still call run_tiers"
        );
        assert!(
            src.contains(wrapper),
            "{board} must be wrapped in `{wrapper}`"
        );
    }
}

/// Only the host board resolves its locator at runtime, so it is the one
/// that passes none. Both halves of that rule now come from one place.
#[test]
fn only_the_host_entry_omits_the_locator_argument() {
    let mut plan = fixture_plan_typed(&[(
        "talker_pkg",
        "talker",
        "talker",
        "talker_pkg::Talker",
        "talker_pkg/Talker.hpp",
    )]);

    plan.board = "native".into();
    let host = emit_typed(&plan).expect("native emit ok");
    assert!(
        host.contains("run_components(nros_boot_config_node_name("),
        "the host entry passes no locator: {host}"
    );

    plan.board = "nuttx".into();
    let embedded = emit_typed(&plan).expect("nuttx emit ok");
    assert!(
        embedded.contains("run_components(NROS_ENTRY_LOCATOR, nros_boot_config_node_name("),
        "an embedded entry passes NROS_ENTRY_LOCATOR: {embedded}"
    );
}

// -----------------------------------------------------------------------
// phase-462 W1 (RFC-0052) -- the contract monitor table region.
// -----------------------------------------------------------------------

use crate::orchestration::model_ingest::{AgeRow, MonitorRow, render_monitor_rs};

fn monitor_fixture_rows() -> (Vec<MonitorRow>, Vec<AgeRow>) {
    (
        vec![MonitorRow {
            topic: "/chatter".into(),
            fqn: "/talker/chatter".into(),
            min_rate_hz_milli: 10_000,
            max_latency_ms: 30,
        }],
        vec![AgeRow {
            topic: "/chatter".into(),
            fqn: "/listener/chatter".into(),
            max_age_ms: 150,
        }],
    )
}

/// The C++ entry bakes the SAME rows the Rust road renders into
/// `system_monitors.rs`, field for field, and installs them before the
/// first node exists.
#[test]
fn typed_emit_bakes_monitor_table_before_nodes() {
    let plan = fixture_plan_typed(&[
        (
            "talker_pkg",
            "talker",
            "talker",
            "talker_pkg::Talker",
            "talker_pkg/Talker.hpp",
        ),
        (
            "listener_pkg",
            "listener",
            "listener",
            "listener_pkg::Listener",
            "listener_pkg/Listener.hpp",
        ),
    ]);
    let (rows, ages) = monitor_fixture_rows();
    let src = emit_typed_monitored(&plan, &rows, &ages).expect("monitored emit ok");
    let rust = render_monitor_rs(&rows, &ages);

    // Row parity, one row at a time: what the Rust table says, the C++
    // table says, in its own spelling.
    for r in &rows {
        let cpp_row = format!(
            "{{ \"{}\", \"{}\", {}u, {}u }},",
            r.topic, r.fqn, r.min_rate_hz_milli, r.max_latency_ms
        );
        let rust_row = format!(
            "topic: {:?}, fqn: {:?}, min_rate_hz_milli: {}u32, max_latency_ms: {}u32",
            r.topic, r.fqn, r.min_rate_hz_milli, r.max_latency_ms
        );
        assert!(
            src.contains(&cpp_row),
            "C++ row `{cpp_row}` missing; src:\n{src}"
        );
        assert!(
            rust.contains(&rust_row),
            "Rust row `{rust_row}` missing; rs:\n{rust}"
        );
    }
    for a in &ages {
        let cpp_row = format!("{{ \"{}\", \"{}\", {}u }},", a.topic, a.fqn, a.max_age_ms);
        let rust_row = format!(
            "topic: {:?}, fqn: {:?}, max_age_ms: {}u32",
            a.topic, a.fqn, a.max_age_ms
        );
        assert!(
            src.contains(&cpp_row),
            "C++ age row `{cpp_row}` missing; src:\n{src}"
        );
        assert!(
            rust.contains(&rust_row),
            "Rust age row `{rust_row}` missing; rs:\n{rust}"
        );
    }
    assert_eq!(src.matches("__nros_mon_rows[1]").count(), 1, "{src}");
    assert_eq!(src.matches("__nros_age_rows[1]").count(), 1, "{src}");
    assert!(
        src.contains("__nros_mon_storage[1 * NROS_CPP_MONITOR_ROW_STORAGE]"),
        "{src}"
    );
    assert!(
        src.contains(".n_rows = 1u,") && src.contains(".n_ages = 1u,"),
        "{src}"
    );

    // Installed BEFORE entity creation, on the process-global executor.
    let install_at = src
        .find("nros_cpp_install_monitors(")
        .expect("install call");
    let create_at = src.find("::nros::create_node(").expect("create_node");
    assert!(
        install_at < create_at,
        "install must precede create_node; src:\n{src}"
    );
    assert!(
        src.contains("void* __mexec = ::nros::global_handle();"),
        "{src}"
    );
}

/// RFC-0052's zero-cost claim, at the source: no rows, no table, no
/// call -- and the TU is the byte-identical one the unmonitored emitter
/// produces (the goldens are that emitter's).
#[test]
fn typed_emit_no_monitor_rows_is_byte_identical() {
    let plan = fixture_plan_typed(&[(
        "talker_pkg",
        "talker",
        "talker",
        "talker_pkg::Talker",
        "talker_pkg/Talker.hpp",
    )]);
    let plain = emit_typed(&plan).expect("emit ok");
    let monitored = emit_typed_monitored(&plan, &[], &[]).expect("emit ok");
    assert_eq!(plain, monitored);
    assert!(!plain.contains("nros_cpp_install_monitors"), "{plain}");
    assert!(!plain.contains("nros_cpp_monitor_row_t"), "{plain}");
    assert!(!plain.contains("NROS_CPP_MONITOR_ROW_STORAGE"), "{plain}");
}

/// The model's rows cover every node in the system; a row whose node this
/// entry does not construct is not this entry's to watch.
#[test]
fn typed_emit_keeps_only_rows_of_nodes_it_constructs() {
    let plan = fixture_plan_typed(&[(
        "talker_pkg",
        "talker",
        "talker",
        "talker_pkg::Talker",
        "talker_pkg/Talker.hpp",
    )]);
    let (rows, ages) = monitor_fixture_rows();
    // `ages` is `/listener/chatter`; no listener here.
    let src = emit_typed_monitored(&plan, &rows, &ages).expect("emit ok");
    assert!(src.contains("\"/talker/chatter\""), "{src}");
    assert!(!src.contains("\"/listener/chatter\""), "{src}");
    assert!(
        src.contains(".ages = nullptr,") && src.contains(".n_ages = 0u,"),
        "{src}"
    );
    assert!(!src.contains("__nros_age_rows"), "{src}");

    // Nothing of ours at all -> no region.
    let other = vec![MonitorRow {
        topic: "/x".into(),
        fqn: "/elsewhere/x".into(),
        min_rate_hz_milli: 1_000,
        max_latency_ms: 0,
    }];
    let src = emit_typed_monitored(&plan, &other, &[]).expect("emit ok");
    assert_eq!(src, emit_typed(&plan).unwrap());
}

/// A namespaced node keys its rows by its full name.
#[test]
fn typed_emit_monitor_rows_match_namespaced_nodes() {
    let mut plan = fixture_plan_typed(&[("cm_pkg", "pub", "pub", "cm_pkg::Pub", "cm_pkg/Pub.hpp")]);
    plan.nodes[0].namespace = Some("/cm".into());
    let rows = vec![MonitorRow {
        topic: "/cm_header".into(),
        fqn: "/cm/pub/cm_header".into(),
        min_rate_hz_milli: 10_000,
        max_latency_ms: 0,
    }];
    let src = emit_typed_monitored(&plan, &rows, &[]).expect("emit ok");
    assert!(
        src.contains("{ \"/cm_header\", \"/cm/pub/cm_header\", 10000u, 0u },"),
        "{src}"
    );
}

/// run_tiers shape: each tier installs ITS nodes' rows on ITS executor,
/// so a row is checked once. Here `ctrl` is on tier 0 and `telem` on
/// tier 1.
#[test]
fn typed_emit_tiers_slice_monitor_rows_per_tier() {
    let plan = fixture_plan_with_tiers();
    let rows = vec![
        MonitorRow {
            topic: "/cmd".into(),
            fqn: "/ctrl/cmd".into(),
            min_rate_hz_milli: 100_000,
            max_latency_ms: 5,
        },
        MonitorRow {
            topic: "/telemetry".into(),
            fqn: "/telem/telemetry".into(),
            min_rate_hz_milli: 1_000,
            max_latency_ms: 0,
        },
    ];
    let src = emit_typed_monitored(&plan, &rows, &[]).expect("tiered emit ok");
    assert!(
        src.contains("__nros_mon_rows_t0[1]") && src.contains("__nros_mon_rows_t1[1]"),
        "{src}"
    );
    let t0 = src
        .find("static int32_t __nros_entry_setup_tier_0(void* executor)")
        .unwrap();
    let t1 = src
        .find("static int32_t __nros_entry_setup_tier_1(void* executor)")
        .unwrap();
    let setup0 = &src[t0..t1];
    let setup1 = &src[t1..];
    assert!(setup0.contains(".rows = __nros_mon_rows_t0,"), "{setup0}");
    assert!(!setup0.contains("__nros_mon_rows_t1"), "{setup0}");
    assert!(setup1.contains(".rows = __nros_mon_rows_t1,"), "{setup1}");
    assert!(setup1.contains("void* __mexec = executor;"), "{setup1}");
    // Each install precedes that tier's first node.
    let i0 = setup0.find("nros_cpp_install_monitors(").unwrap();
    let c0 = setup0.find("::nros::create_node_on(").unwrap();
    assert!(i0 < c0, "{setup0}");
    // And the whole tier table region is absent when no tier has rows.
    assert!(
        !emit_typed(&plan)
            .unwrap()
            .contains("nros_cpp_install_monitors")
    );
}
