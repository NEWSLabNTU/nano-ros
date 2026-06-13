//! Phase 219.B — C++ Entry-pkg TU emitter.
//!
//! Maps a [`Plan`] (see `super::mod`) onto the canonical generated
//! `main.cpp` shape from `docs/roadmap/archived/phase-219-cpp-entry-pkg.md` §3.3.
//!
//! The TU pulls in `<nros/main.hpp>` (the Phase 219.E header that
//! defines the `NROS_MAIN` declarative marker), declares one
//! `extern "C" int32_t __nros_component_<pkg>_register(...)` per
//! launch-XML node, then invokes them in launch order from inside a
//! lambda passed to `nros::board::NativeBoard::run(...)`.
//!
//! Today the Native-board entry boils down to:
//!
//! - `nros::init()` (no-arg, reads `$NROS_LOCATOR` / `$ROS_DOMAIN_ID`).
//! - Call each Node-pkg register fn in turn — they describe their
//!   `<node>` / `<entity>` set against the supplied `NodeContext`.
//! - `nros::spin()` until `nros::ok()` flips false.
//! - `nros::shutdown()`.
//!
//! The thin `nros::board::<Board>::run(lambda)` adapter shipped by
//! `packages/core/nros-cpp/include/nros/main.hpp` owns the
//! init/spin/shutdown ritual so the generated TU stays one declarative
//! lambda. Phase 235.B added the embedded `ZephyrBoard` sibling: a
//! non-`native` board key (e.g. `"zephyr"`, derived by `nano_ros_entry`
//! from the Phase 215 `NROS_BOARD_RUNNER`) emits
//! `nros::board::ZephyrBoard::run(...)`, which owns the Zephyr + Cyclone
//! `init → network-wait → register → spin → shutdown` lifecycle.

use std::fmt::Write;

use super::{Plan, QosOverrideSpec, sanitize_pkg};

/// Phase 211.H (issue #52) — map a decomposed [`QosOverrideSpec`] to the C-ABI
/// `(role, policy, value)` scalar codes the `nros_cpp_qos_override_t` struct
/// uses. Returns `None` for an unrecognised role/policy (skipped — never baked
/// as a silent wrong override).
fn qos_override_codes(o: &QosOverrideSpec) -> Option<(u8, u8, u32)> {
    let role = match o.role.as_str() {
        "publisher" => 0u8,
        "subscription" => 1u8,
        _ => return None,
    };
    let v = o.value.trim();
    let (policy, value) = match o.policy.as_str() {
        "reliability" => (0u8, if v == "best_effort" { 0 } else { 1 }),
        "durability" => (1u8, if v == "transient_local" { 1 } else { 0 }),
        "history" => (2u8, if v == "keep_all" { 1 } else { 0 }),
        "depth" => (3u8, v.parse::<u32>().ok()?),
        _ => return None,
    };
    Some((role, policy, value))
}

/// Emit a `static const nros_cpp_qos_override_t __nros_qos_<i>[] = {…};` + the
/// `__nros_node_<i>.set_qos_overrides(…)` call for node `i`. No-op when the node
/// has no (recognised) overrides.
fn emit_qos_overrides(out: &mut String, i: usize, overrides: &[QosOverrideSpec]) {
    let coded: Vec<(&QosOverrideSpec, (u8, u8, u32))> = overrides
        .iter()
        .filter_map(|o| qos_override_codes(o).map(|c| (o, c)))
        .collect();
    if coded.is_empty() {
        return;
    }
    let _ = writeln!(
        out,
        "        static const ::nros_cpp_qos_override_t __nros_qos_{i}[] = {{"
    );
    for (o, (role, policy, value)) in &coded {
        let topic = o.topic.replace('\\', "\\\\").replace('"', "\\\"");
        let _ = writeln!(
            out,
            "            {{ \"{topic}\", {role}, {policy}, {value} }},"
        );
    }
    out.push_str("        };\n");
    let _ = writeln!(
        out,
        "        __nros_node_{i}.set_qos_overrides(__nros_qos_{i}, {});",
        coded.len()
    );
}

/// Emit the generated C++ TU as a `String`.
///
/// The output is deterministic for a given plan — every test rests on
/// that invariant. Caller writes it verbatim to the path the cmake fn
/// derives (typically `${CMAKE_BINARY_DIR}/nros_main_generated.cpp`).
pub fn emit(plan: &Plan) -> String {
    let mut out = String::new();
    write_header(&mut out, plan);

    // Stable platform-neutral includes. The `<nros/main.hpp>` header
    // provides the Board adapter shim + the empty `NROS_MAIN(...)`
    // declarative macro the user's source TU may carry as an IDE hint.
    out.push_str("#include <nros/main.hpp>\n");
    out.push_str("#include <nros/nros.hpp>\n");
    out.push_str("#include <nros/node_pkg.hpp>\n\n");

    // One `extern "C" int32_t __nros_component_<pkg>_register(...)`
    // declaration per node. The body lives in the linked Node-pkg
    // static lib (`<pkg>_<exec>_component`), wired up by
    // `nano_ros_node_register()` + the cmake fn's auto-link from
    // 219.J.
    out.push_str("// Phase 212.M.5.a.1 mangled register fns — one per Node pkg the\n");
    out.push_str("// launch XML pulled in. The Entry pkg's CMakeLists.txt\n");
    out.push_str("// (`nano_ros_entry(LAUNCH …)`) auto-links the matching\n");
    out.push_str("// `<pkg>_<exec>_component` static libs (Phase 219.J).\n");
    out.push_str("extern \"C\" {\n");
    // Dedup by pkg name: multiple `<node>` rows from the same pkg get a
    // single declaration. Order is first-seen, matching the call order
    // below.
    let mut seen_pkgs: Vec<String> = Vec::new();
    for n in &plan.nodes {
        let sym = format!("__nros_component_{}_register", sanitize_pkg(&n.pkg));
        if !seen_pkgs.contains(&sym) {
            let _ = writeln!(out, "    int32_t {sym}(::nros::NodeContext* context);");
            seen_pkgs.push(sym);
        }
    }
    out.push_str("}\n\n");

    // The boot stub. `nros::board::<Board>::run(lambda)` owns
    // init/spin/shutdown; the lambda calls each Node-pkg register fn
    // in launch order, bailing on the first non-zero return.
    let board_type = board_cpp_path(&plan.board);
    out.push_str("int main(int /*argc*/, char** /*argv*/) {\n");
    let _ = writeln!(
        out,
        "    return {board_type}::run([](::nros::NodeContext* context) -> int32_t {{"
    );
    // Emit C++14-compatible two-statement check (init-statement in
    // `if` is a C++17 feature). The Node-pkg side compiles with
    // `set(CMAKE_CXX_STANDARD 14)` per Phase 212.L.9 (see
    // `examples/templates/multi-node-workspace-cpp/src/*_pkg/CMakeLists.txt`)
    // so the generated TU must stay portable to the same floor.
    for n in &plan.nodes {
        let sym = format!("__nros_component_{}_register", sanitize_pkg(&n.pkg));
        let _ = writeln!(out, "        {{");
        let _ = writeln!(out, "            int32_t rc = {sym}(context);");
        out.push_str("            if (rc != 0) return rc;\n");
        out.push_str("        }\n");
    }
    out.push_str("        return 0;\n");
    out.push_str("    });\n");
    out.push_str("}\n");

    out
}

fn write_header(out: &mut String, plan: &Plan) {
    let _ = writeln!(
        out,
        "// Generated by `nros codegen entry --lang cpp`\n\
         //   bringup = {bringup}\n\
         //   launch  = {launch}\n\
         //   board   = {board}\n\
         //\n\
         // DO NOT EDIT — re-run the build to regenerate. The cmake fn\n\
         // `nano_ros_entry(LAUNCH \"…\")` drives this codegen via\n\
         // `nros codegen entry --lang cpp` at configure time.",
        bringup = plan.bringup,
        launch = plan.launch_file.display(),
        board = plan.board,
    );
    out.push('\n');
}

/// Board key → C++ Board adapter path.
///
/// Two adapters ship today (Phase 235): `NativeBoard` (host/POSIX) and
/// `ZephyrBoard` (embedded Zephyr — RFC-0032 §8a). Per the §8a decision
/// there is ONE metadata-driven `ZephyrBoard` rather than per-board C++
/// types: everything board-specific (the Zephyr `BOARD` id, DTS overlay,
/// default RMW, `west` runner) is supplied by the Phase 215
/// `nano_ros_use_board(<name>)` cmake import at build time, so the C++
/// adapter has nothing left to specialize. The `nano_ros_entry` cmake fn
/// derives the `"zephyr"` board key from `NROS_BOARD_RUNNER` (set by the
/// Phase 215 import) when the Entry pkg's DEPLOY target is embedded.
///
/// An explicit C++ path-like key (`"::nros::board::…"`) passes through
/// verbatim so callers can name a board adapter the emitter doesn't yet
/// know.
fn board_cpp_path(board: &str) -> &str {
    match board {
        "native" | "posix" => "::nros::board::NativeBoard",
        // Embedded Zephyr family — every Phase 215 Zephyr board (FVP,
        // qemu-zephyr, …) compiles with `__ZEPHYR__` and shares the one
        // metadata-driven `ZephyrBoard` adapter.
        "zephyr" | "fvp-aemv8r-smp" | "armfvp" => "::nros::board::ZephyrBoard",
        // Phase 238 — embedded NuttX (qemu-arm-virt etc.). Network is up at
        // kernel boot; shares the EntryNodeRuntime via the `NuttxBoard`
        // lifecycle adapter.
        "nuttx" | "nuttx-qemu-arm" | "nuttx-qemu-riscv" => "::nros::board::NuttxBoard",
        // An explicit, already-qualified C++ board path passes through.
        b if b.starts_with("::nros::board::") => b,
        // Unknown / future board keys fall back to NativeBoard with the
        // assumption the cmake-side configure will have already errored
        // on the DEPLOY check (`nano_ros_entry` requires a BOARD for
        // non-`native` DEPLOY). Keeping the default as NativeBoard lets
        // unit tests cover the unhappy path without teaching the emitter
        // every embedded board prematurely.
        _ => "::nros::board::NativeBoard",
    }
}

/// Phase 240.2 (RFC-0043) — **typed** entry emitter. Routes each launch node to
/// the REAL executor via its component object, instead of the legacy type-erased
/// `__nros_component_<pkg>_register` call into the synthesizing
/// `EntryNodeRuntime`. Per node: `#include` the component header, declare static
/// component + node storage (outlives the spin loop — the executor holds
/// `&component` as the dispatch context; no heap), construct the node + call
/// `component.configure(node)` (binds the real callbacks). `main` hands the setup
/// fn to `Board::run_components` (init → setup → `spin_once` loop → shutdown).
///
/// Each node is routed by its `lang` (Phase 240.4): a **C++** node needs
/// `class_name` + `class_header` (construct the class, call `configure(node)`);
/// a **C** node (`lang == "c"`) needs only its pkg — it is built via the C-ABI
/// factory + configure seam `__nros_c_component_<pkg>_{create,configure}`
/// (`NROS_C_COMPONENT`), to which the entry hands the node's `ffi_handle()`.
/// Returns an error naming the offending pkg on a missing requirement.
pub fn emit_typed(plan: &Plan) -> Result<String, String> {
    for n in &plan.nodes {
        if n.class_name.is_none() {
            return Err(format!(
                "typed entry emit: node pkg `{}` exec `{}` is missing class_name (cmake metadata)",
                n.pkg, n.exec
            ));
        }
        if !is_c_node(n) && n.class_header.is_none() {
            return Err(format!(
                "typed entry emit: C++ node pkg `{}` exec `{}` is missing class_header \
                 — the typed Entry needs the component's class header (cmake metadata)",
                n.pkg, n.exec
            ));
        }
    }

    let mut out = String::new();
    let _ = writeln!(
        out,
        "// Generated by `nros codegen entry --lang cpp` (typed — RFC-0043)\n\
         //   bringup = {bringup}\n\
         //   launch  = {launch}\n\
         //   board   = {board}\n\
         //\n\
         // DO NOT EDIT — regenerated at configure time. Routes each launch node\n\
         // to the real executor via its component object (no synthesizing\n\
         // interpreter); `Board::run_components` owns init/spin/shutdown.",
        bringup = plan.bringup,
        launch = plan.launch_file.display(),
        board = plan.board,
    );
    out.push('\n');

    out.push_str("#include <nros/component.hpp>\n");
    out.push_str("#include <nros/main.hpp>\n");
    out.push_str("#include <nros/nros.hpp>\n");
    // Phase 242.4 (RFC-0044) — `nros::ComponentNode` / `NodeHandle` /
    // `detail::report_component_failure` for any rclcpp-shape (construct-with-
    // handle) node. Pulled in only when one is present.
    if plan.nodes.iter().any(is_rclcpp_node) {
        out.push_str("#include <new> // placement-new into the component arena slot\n");
        out.push_str("#include <nros/component_node.hpp>\n");
    }
    out.push('\n');

    // One `#include` per unique C++ component header (first-seen order). C nodes
    // carry no header — their factory/configure are extern "C" decls below.
    let mut seen_headers: Vec<&str> = Vec::new();
    for n in &plan.nodes {
        if is_c_node(n) {
            continue;
        }
        let h = n.class_header.as_deref().unwrap();
        if !seen_headers.contains(&h) {
            let _ = writeln!(out, "#include \"{h}\"");
            seen_headers.push(h);
        }
    }

    // Forward-declare the C-ABI factory + configure for each unique C pkg.
    let mut seen_c_pkgs: Vec<String> = Vec::new();
    let mut wrote_extern = false;
    for n in &plan.nodes {
        if !is_c_node(n) {
            continue;
        }
        let pkg = sanitize_pkg(&n.pkg);
        if seen_c_pkgs.contains(&pkg) {
            continue;
        }
        if !wrote_extern {
            out.push_str(
                "\n// C component factory + configure seam (NROS_C_COMPONENT); the\n\
                 // node's `ffi_handle()` is handed to the C `configure` as an opaque\n\
                 // `nros_cpp_node_t*` — the C side registers real callbacks on it.\n",
            );
            out.push_str("extern \"C\" {\n");
            wrote_extern = true;
        }
        let _ = writeln!(out, "    void* __nros_c_component_{pkg}_create(void);");
        let _ = writeln!(
            out,
            "    int32_t __nros_c_component_{pkg}_configure(const ::nros_cpp_node_t* node, void* executor, void* self);"
        );
        seen_c_pkgs.push(pkg);
    }
    if wrote_extern {
        out.push_str("}\n");
    }
    out.push('\n');

    // Static per-node storage — one node per launch `<node>` row. Shape-branched
    // (Phase 242.4):
    //  - configure (240.x) C++ node: a static `Node` + a static component object
    //    (default-constructed before init, then `configure(node)` in setup).
    //  - C node: only the static `Node` (its state lives in its own TU — the
    //    factory returns `&static_instance`).
    //  - rclcpp (RFC-0044) C++ node: NO separate `Node` — the component OWNS its
    //    node, constructed from the executor handle. An aligned arena slot holds
    //    the component; it is placement-new'd in setup *after* `nros::init`.
    out.push_str("// Static per-node storage (outlives the spin loop; no heap).\n");
    for (i, n) in plan.nodes.iter().enumerate() {
        if is_rclcpp_node(n) {
            let cls = n.class_name.as_deref().unwrap();
            let _ = writeln!(
                out,
                "alignas(::{cls}) static unsigned char __nros_comp_buf_{i}[sizeof(::{cls})];"
            );
            let _ = writeln!(out, "static ::{cls}* __nros_comp_{i} = nullptr;");
        } else {
            let _ = writeln!(out, "static ::nros::Node __nros_node_{i};");
            if !is_c_node(n) {
                let cls = n.class_name.as_deref().unwrap();
                let _ = writeln!(out, "static ::{cls} __nros_comp_{i};");
            }
        }
    }
    out.push('\n');

    // setup (post-`nros::init`): construct each node + wire its component's real
    // callbacks. Shape-branched (Phase 242.4).
    out.push_str("static int32_t __nros_entry_setup() {\n");
    for (i, n) in plan.nodes.iter().enumerate() {
        let node_name = n.name.as_deref().unwrap_or(&n.exec);
        let name_lit = node_name.replace('\\', "\\\\").replace('"', "\\\"");
        let _ = writeln!(out, "    {{");
        if is_rclcpp_node(n) {
            // rclcpp shape (RFC-0044): placement-new the component with the live
            // executor node handle — the ctor creates the node + entities. The
            // component owns its node, so no separate `create_node`/`configure`.
            let cls = n.class_name.as_deref().unwrap();
            out.push_str(
                "        ::nros::NodeHandle __h(::nros::global_handle());\n",
            );
            out.push_str(
                "        if (!__h.valid()) return static_cast<int32_t>(::nros::ErrorCode::NotInitialized);\n",
            );
            let _ = writeln!(
                out,
                "        __nros_comp_{i} = new (__nros_comp_buf_{i}) ::{cls}(__h);"
            );
            // Q2: check ok() post-construct, halt naming the failing node.
            let _ = writeln!(out, "        if (!__nros_comp_{i}->ok()) {{");
            let _ = writeln!(
                out,
                "            ::nros::detail::report_component_failure(\"{name_lit}\", __nros_comp_{i}->error_what(), __nros_comp_{i}->error_code());"
            );
            let _ = writeln!(
                out,
                "            return __nros_comp_{i}->error_code();"
            );
            out.push_str("        }\n");
        } else {
            let _ = writeln!(
                out,
                "        ::nros::Result r = ::nros::create_node(__nros_node_{i}, \"{name_lit}\");"
            );
            out.push_str("        if (!r.ok()) return static_cast<int32_t>(r.raw());\n");
            // Phase 211.H (issue #52) — install the plan's per-topic QoS
            // overrides on the node BEFORE `configure`, so the entities the
            // component creates fold them in (mirrors Rust's
            // `NodeHandle::set_qos_overrides`). Configure-shape nodes only — an
            // rclcpp-shape component creates its node + entities in its ctor,
            // before this seam, so it can't be reached here.
            emit_qos_overrides(&mut out, i, &n.qos_overrides);
            if is_c_node(n) {
                let pkg = sanitize_pkg(&n.pkg);
                let _ = writeln!(
                    out,
                    "        void* self = __nros_c_component_{pkg}_create();"
                );
                let _ = writeln!(
                    out,
                    "        int32_t crc = __nros_c_component_{pkg}_configure(__nros_node_{i}.ffi_handle(), __nros_node_{i}.executor_handle(), self);"
                );
                out.push_str("        if (crc != 0) return crc;\n");
            } else {
                let _ = writeln!(
                    out,
                    "        r = __nros_comp_{i}.configure(__nros_node_{i});"
                );
                out.push_str("        if (!r.ok()) return static_cast<int32_t>(r.raw());\n");
            }
        }
        out.push_str("    }\n");
    }
    out.push_str("    return 0;\n}\n\n");

    let board = board_cpp_path(&plan.board);
    out.push_str("int main(int /*argc*/, char** /*argv*/) {\n");
    let _ = writeln!(
        out,
        "    return {board}::run_components(&__nros_entry_setup);"
    );
    out.push_str("}\n");

    Ok(out)
}

/// A `lang == "c"` node is built via the C factory/configure seam (no C++ class).
fn is_c_node(n: &super::PlanNode) -> bool {
    n.lang.as_deref() == Some("c")
}

/// Phase 242.4 (RFC-0044) — an rclcpp-shape (IS-A-node, construct-with-handle)
/// C++ component: `shape == "rclcpp"` AND not a C node. Everything else (incl.
/// `shape == None` / `"configure"`) keeps the 240.x `configure(Node&)` path.
fn is_rclcpp_node(n: &super::PlanNode) -> bool {
    !is_c_node(n) && n.shape.as_deref() == Some("rclcpp")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen::entry::PlanNode;
    use std::path::PathBuf;

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
                    host: None,
                    qos_overrides: Vec::new(),
                })
                .collect(),
            depfile_paths: Vec::new(),
            bringup: "demo_bringup".into(),
            launch_file: PathBuf::from("/tmp/system.launch.xml"),
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
                    lang: Some("cpp".into()),
                    shape: Some("configure".into()),
                    host: None,
                    qos_overrides: Vec::new(),
                })
                .collect(),
            depfile_paths: Vec::new(),
            bringup: "demo_bringup".into(),
            launch_file: PathBuf::from("/tmp/system.launch.xml"),
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
        // headers included
        assert!(src.contains("#include \"talker_pkg/Talker.hpp\""));
        assert!(src.contains("#include \"listener_pkg/Listener.hpp\""));
        assert!(src.contains("#include <nros/component.hpp>"));
        // static component + node storage
        assert!(src.contains("static ::nros::Node __nros_node_0;"));
        assert!(src.contains("static ::talker_pkg::Talker __nros_comp_0;"));
        assert!(src.contains("static ::listener_pkg::Listener __nros_comp_1;"));
        // setup constructs the node + configures the component
        assert!(src.contains("::nros::create_node(__nros_node_0, \"talker\")"));
        assert!(src.contains("__nros_comp_0.configure(__nros_node_0)"));
        assert!(src.contains("__nros_comp_1.configure(__nros_node_1)"));
        // routes to the real executor (no EntryNodeRuntime / register symbol)
        assert!(src.contains("::nros::board::NativeBoard::run_components(&__nros_entry_setup)"));
        assert!(!src.contains("__nros_component_"));
        assert!(!src.contains("NodeContext"));
        // configure shape: no construct-with-handle artifacts.
        assert!(!src.contains("global_handle()"));
        assert!(!src.contains("__nros_comp_buf_"));
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
        plan.nodes[0].qos_overrides = vec![
            QosOverrideSpec {
                topic: "/chatter".into(),
                role: "publisher".into(),
                policy: "reliability".into(),
                value: "best_effort".into(),
            },
            QosOverrideSpec {
                topic: "/chatter".into(),
                role: "subscription".into(),
                policy: "durability".into(),
                value: "transient_local".into(),
            },
        ];
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
        // construct-with-handle headers + arena slot
        assert!(src.contains("#include <nros/component_node.hpp>"));
        assert!(src.contains("#include \"ctrl_pkg/Controller.hpp\""));
        assert!(src.contains(
            "alignas(::ctrl_pkg::Controller) static unsigned char __nros_comp_buf_0[sizeof(::ctrl_pkg::Controller)];"
        ));
        assert!(src.contains("static ::ctrl_pkg::Controller* __nros_comp_0 = nullptr;"));
        // setup: handle → placement-new → ok() check naming the node
        assert!(src.contains("::nros::NodeHandle __h(::nros::global_handle());"));
        assert!(src.contains("__nros_comp_0 = new (__nros_comp_buf_0) ::ctrl_pkg::Controller(__h);"));
        assert!(src.contains("if (!__nros_comp_0->ok()) {"));
        assert!(src.contains("report_component_failure(\"controller\""));
        // The rclcpp shape does NOT default-construct a Node or call configure.
        assert!(!src.contains("static ::nros::Node __nros_node_0;"));
        assert!(!src.contains("__nros_comp_0.configure"));
        assert!(!src.contains("create_node(__nros_node_0"));
        // still routes to the real executor
        assert!(src.contains("::nros::board::NativeBoard::run_components(&__nros_entry_setup)"));
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
        assert!(!src.contains("static ::nros::Node __nros_node_0;"));
        // node 1 = configure: Node + configure, no arena slot.
        assert!(src.contains("static ::nros::Node __nros_node_1;"));
        assert!(src.contains("static ::legacy_pkg::Legacy __nros_comp_1;"));
        assert!(src.contains("__nros_comp_1.configure(__nros_node_1)"));
        assert!(!src.contains("__nros_comp_buf_1"));
        // rclcpp include present because at least one rclcpp node exists.
        assert!(src.contains("#include <nros/component_node.hpp>"));
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
        assert!(src.contains("::nros::create_node(__nros_node_0, \"a\")"));
        assert!(src.contains("::nros::create_node(__nros_node_1, \"b\")"));
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
        plan.nodes[0].lang = Some("c".into());
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
        // Still routes to the real executor.
        assert!(src.contains("::nros::board::NativeBoard::run_components(&__nros_entry_setup)"));
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
        plan.nodes[1].lang = Some("c".into()); // sensor is C
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
        let mut plan = fixture_plan_typed(&[("t_pkg", "t", "t", "t_pkg::T", "t_pkg/T.hpp")]);
        plan.board = "nuttx".into();
        let src = emit_typed(&plan).expect("typed emit ok");
        assert!(src.contains("::nros::board::NuttxBoard::run_components(&__nros_entry_setup)"));
    }

    #[test]
    fn typed_emit_errors_when_class_missing() {
        let plan = fixture_plan(&[("talker_pkg", "talker")]); // class_name None
        let err = emit_typed(&plan).unwrap_err();
        assert!(err.contains("missing class_name"), "{err}");
        assert!(err.contains("talker_pkg"), "{err}");
    }

    #[test]
    fn one_node_plan_emits_single_extern_c_decl_and_one_call() {
        let plan = fixture_plan(&[("talker_pkg", "talker")]);
        let src = emit(&plan);
        // extern "C" block contains the register decl.
        assert!(src.contains("int32_t __nros_component_talker_pkg_register(::nros::NodeContext*"));
        // main() calls it.
        assert!(src.contains("__nros_component_talker_pkg_register(context)"));
        // NativeBoard::run wraps the body.
        assert!(src.contains("::nros::board::NativeBoard::run(["));
    }

    #[test]
    fn two_node_plan_preserves_launch_order() {
        let plan = fixture_plan(&[("talker_pkg", "talker"), ("listener_pkg", "listener")]);
        let src = emit(&plan);
        let pos_talker = src
            .find("__nros_component_talker_pkg_register(context)")
            .unwrap();
        let pos_listener = src
            .find("__nros_component_listener_pkg_register(context)")
            .unwrap();
        assert!(
            pos_talker < pos_listener,
            "talker register call must precede listener (launch order)"
        );
    }

    #[test]
    fn duplicate_pkg_decl_is_deduped_but_calls_repeated() {
        // Two `<node>` rows from the same pkg → one extern "C" decl
        // but two calls (each launch entry is its own register
        // invocation; the per-pkg register fn is idempotent on the
        // Node-pkg side).
        let plan = fixture_plan(&[("twin_pkg", "a"), ("twin_pkg", "b")]);
        let src = emit(&plan);
        let decl = "int32_t __nros_component_twin_pkg_register(::nros::NodeContext*";
        let decl_count = src.matches(decl).count();
        assert_eq!(decl_count, 1, "extern \"C\" decl deduped per pkg");
        let call_count = src
            .matches("__nros_component_twin_pkg_register(context)")
            .count();
        assert_eq!(call_count, 2, "one call per launch-XML <node> entry");
    }

    #[test]
    fn dashed_pkg_names_are_sanitised_in_symbols() {
        let plan = fixture_plan(&[("dashy-pkg", "talker")]);
        let src = emit(&plan);
        assert!(src.contains("__nros_component_dashy_pkg_register"));
        assert!(!src.contains("dashy-pkg_register"));
    }

    #[test]
    fn generated_tu_has_main_and_required_includes() {
        let plan = fixture_plan(&[("talker_pkg", "talker")]);
        let src = emit(&plan);
        assert!(src.contains("#include <nros/main.hpp>"));
        assert!(src.contains("#include <nros/nros.hpp>"));
        assert!(src.contains("#include <nros/node_pkg.hpp>"));
        assert!(src.contains("int main(int /*argc*/, char** /*argv*/)"));
    }

    #[test]
    fn header_banner_mentions_bringup_launch_board() {
        let plan = fixture_plan(&[("talker_pkg", "talker")]);
        let src = emit(&plan);
        assert!(src.contains("bringup = demo_bringup"));
        assert!(src.contains("launch  = /tmp/system.launch.xml"));
        assert!(src.contains("board   = native"));
    }

    #[test]
    fn zephyr_board_key_emits_embedded_adapter() {
        // Phase 235.B — a `"zephyr"` board key (derived by nano_ros_entry
        // from the Phase 215 NROS_BOARD_RUNNER) selects the embedded
        // ZephyrBoard adapter instead of NativeBoard.
        let mut plan = fixture_plan(&[("talker_pkg", "talker")]);
        plan.board = "zephyr".into();
        let src = emit(&plan);
        assert!(src.contains("::nros::board::ZephyrBoard::run(["));
        assert!(!src.contains("NativeBoard::run("));
        assert!(src.contains("board   = zephyr"));
    }

    #[test]
    fn fvp_board_key_maps_to_zephyr_adapter() {
        // The concrete Phase 215 board name + its west runner both route
        // to the one metadata-driven ZephyrBoard (RFC-0032 §8a).
        for key in ["fvp-aemv8r-smp", "armfvp"] {
            let mut plan = fixture_plan(&[("talker_pkg", "talker")]);
            plan.board = key.into();
            let src = emit(&plan);
            assert!(
                src.contains("::nros::board::ZephyrBoard::run(["),
                "board key {key} must map to ZephyrBoard"
            );
        }
    }

    #[test]
    fn nuttx_board_key_emits_nuttx_adapter() {
        // Phase 238 — a `"nuttx"` board key selects the embedded NuttxBoard
        // adapter (network up at kernel boot), not NativeBoard.
        for key in ["nuttx", "nuttx-qemu-arm", "nuttx-qemu-riscv"] {
            let mut plan = fixture_plan(&[("talker_pkg", "talker")]);
            plan.board = key.into();
            let src = emit(&plan);
            assert!(
                src.contains("::nros::board::NuttxBoard::run(["),
                "board key {key} must map to NuttxBoard"
            );
            assert!(!src.contains("NativeBoard::run("));
        }
    }

    #[test]
    fn explicit_cpp_board_path_passes_through() {
        let mut plan = fixture_plan(&[("talker_pkg", "talker")]);
        plan.board = "::nros::board::ZephyrBoard".into();
        let src = emit(&plan);
        assert!(src.contains("::nros::board::ZephyrBoard::run(["));
    }
}
