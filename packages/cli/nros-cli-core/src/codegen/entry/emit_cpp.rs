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

use super::{Plan, QosOverrideSpec, emit_boot_config_static, sanitize_pkg};

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
        // Phase 240.6 / phase-263 C2b — embedded FreeRTOS (QEMU MPS2-AN385 + lwIP). The
        // board's C `startup.c` spawns the app task + starts the scheduler, brings up the
        // netif, and dispatches to the typed entry's `app_main`, so `FreertosBoard`'s
        // `run_components` runs WITHOUT re-entering the kernel — same machinery as the
        // ThreadX/NuttX adapters.
        "freertos" | "mps2-an385-freertos" => "::nros::board::FreertosBoard",
        // Phase 246 — Azure RTOS ThreadX family (threadx-linux host sim +
        // bare-metal qemu-riscv64). The board's C `startup.c` enters the kernel
        // and dispatches to the typed entry's `app_main` inside the app thread, so
        // the `ThreadxBoard` adapter runs `run_components` WITHOUT re-entering the
        // kernel — same `EntryNodeRuntime` machinery as Native/NuttX/Zephyr.
        "threadx" | "threadx-linux" | "threadx-qemu-riscv64" | "qemu-riscv64-threadx" => {
            "::nros::board::ThreadxBoard"
        }
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

/// phase-263 C2 (issue 0097) — does this board boot via the RTOS `startup.c`
/// (`nros_app_main` + `NROS_APP_MAIN_REGISTER_VOID`) rather than a plain `int main`?
/// Every non-native board does: the board's `startup.c` owns `main` (kernel enter →
/// app thread) and dispatches to the entry's `app_main`, so the LAUNCH entry must NOT
/// define `int main` (it would double-main / never run under the kernel). Native keeps
/// the POSIX `int main`.
pub(crate) fn board_is_embedded(board: &str) -> bool {
    board_cpp_path(board) != "::nros::board::NativeBoard"
}

/// phase-263 C2d — Zephyr is the exception among embedded boards: the Zephyr kernel
/// calls the application's `main()` DIRECTLY (there is no nano-ros `startup.c` owning
/// `main`), so a Zephyr LAUNCH entry emits a plain `int main(void)` driving
/// `ZephyrBoard::run_components` — NOT the `nros_app_main` + `NROS_APP_MAIN_REGISTER_VOID`
/// shape the FreeRTOS/NuttX/ThreadX startup paths require. The entry TU is added to the
/// Zephyr `app` target (`nano_ros_entry` zephyr branch), and the connect locator threads
/// in via the compile-time `CONFIG_NROS_ZENOH_LOCATOR` Kconfig (read through the
/// `NROS_ENTRY_LOCATOR` default in `<nros/main.hpp>`), not a baked `-D`.
pub(crate) fn board_is_zephyr(board: &str) -> bool {
    board_cpp_path(board) == "::nros::board::ZephyrBoard"
}

/// Phase 274.W3 — FreeRTOS embedded boards support `run_tiers` (one RTOS task per tier
/// over one shared session). Unlike the other embedded boards (Zephyr, NuttX, ThreadX)
/// which keep the single-executor sched-context path (W2), `FreertosBoard` has a C
/// `nros_board_freertos_run_tiers` implementation (nros-board-freertos) that mirrors
/// the Rust `run_tiers_entry`. The generated entry emits `nros_app_main` +
/// `NROS_APP_MAIN_REGISTER_VOID`, calling `FreertosBoard::run_tiers` (RFC-0015 §5).
pub(crate) fn board_is_freertos_embedded(board: &str) -> bool {
    board_cpp_path(board) == "::nros::board::FreertosBoard"
}

/// phase-281 W3 (nuttx) — NuttX embedded boards support `run_tiers` (one pthread
/// per tier over one shared session). Like `FreertosBoard`, `NuttxBoard` has a C
/// `nros_board_nuttx_run_tiers` implementation (nros-board-nuttx-qemu-arm) that
/// mirrors the Rust `run_tiers_entry`; NuttX being POSIX, each non-boot tier is a
/// `pthread` (SCHED_FIFO at the tier's raw priority). The generated entry uses
/// the `nros_app_main` + `NROS_APP_MAIN_REGISTER_VOID` shape (the NuttX startup
/// path calls `app_main`, like FreeRTOS — NOT Zephyr's `main(void)`), calling
/// `NuttxBoard::run_tiers` (RFC-0015 §5).
pub(crate) fn board_is_nuttx(board: &str) -> bool {
    board_cpp_path(board) == "::nros::board::NuttxBoard"
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
        if !is_c_node(n) && !is_rust_node(n) && n.class_header.is_none() {
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

    out.push_str("#include <nros/boot_config.h>\n");
    out.push_str("#include <nros/component.hpp>\n");
    out.push_str("#include <nros/main.hpp>\n");
    out.push_str("#include <nros/nros.hpp>\n");
    // phase-263 C2 — embedded boots through the board's startup.c via `app_main`. Zephyr
    // is exempt (the kernel calls `main()` directly — see `board_is_zephyr`).
    if board_is_embedded(&plan.board) && !board_is_zephyr(&plan.board) {
        out.push_str("#include <nros/app_main.h>\n");
    }
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
        // C nodes carry no header; Rust nodes (phase-257) self-create with no C++
        // class — both skip the include (their seams are extern "C" decls below).
        if is_c_node(n) || is_rust_node(n) {
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
    // Phase 257 (W0-B) — forward-declare the uniform install seam for each unique
    // Rust pkg. The Rust node self-creates its node on the shared executor; the entry
    // hands it `::nros::global_handle()` (= `*mut Executor`).
    let mut seen_rust_pkgs: Vec<String> = Vec::new();
    let mut wrote_rust_extern = false;
    for n in &plan.nodes {
        if !is_rust_node(n) {
            continue;
        }
        let pkg = sanitize_pkg(&n.pkg);
        if seen_rust_pkgs.contains(&pkg) {
            continue;
        }
        if !wrote_rust_extern {
            out.push_str(
                "\n// Rust component install seam (nros::node!); the Rust node\n\
                 // self-creates its node on the shared executor handle (phase-257).\n",
            );
            out.push_str("extern \"C\" {\n");
            wrote_rust_extern = true;
        }
        let _ = writeln!(
            out,
            "    int32_t __nros_component_{pkg}_install(const void* node, void* executor, void* self);"
        );
        seen_rust_pkgs.push(pkg);
    }
    if wrote_rust_extern {
        out.push_str("}\n");
    }
    out.push('\n');

    out.push_str("// Static per-node storage (outlives the spin loop; no heap).\n");
    for (i, n) in plan.nodes.iter().enumerate() {
        if is_rust_node(n) {
            // Phase 257 (W0-B) — Rust node self-creates its node + owns its state on
            // the shared executor (D7 Option C); no entry-side `Node`/component object.
            continue;
        }
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

    // Phase 269 (W4) / 272 (W2) — sched-context wiring guard.
    let use_tiers = plan
        .resolved_tiers
        .as_ref()
        .is_some_and(|t| !t.is_single_tier());
    // Phase 274.W2 — multi-tier native → per-tier threads (run_tiers).
    // Phase 274.W3 — FreeRTOS embedded also uses run_tiers (per-RTOS tasks).
    // phase-281 W3a — Zephyr embedded also uses run_tiers (one k_thread per tier
    // over one shared session, via `nros_board_zephyr_run_tiers`).
    // phase-281 W3 (nuttx) — NuttX embedded also uses run_tiers (one pthread per
    // tier over one shared session, via `nros_board_nuttx_run_tiers`). The
    // remaining embedded board (ThreadX) keeps the single-executor sched-context path.
    // Phase 282 follow-up (RFC-0047) — a node whose callback groups map to
    // MORE THAN ONE tier (`group_tiers = { ctrl = "high", telem = "low" }`)
    // cannot be expressed by run_tiers: its per-tier setup fns construct whole
    // NODES, so a group-split node silently landed on whichever tier iterated
    // last and BOTH its timers ran at that tier's cadence. Such plans keep the
    // single-executor sched-context path (`bind_group_sched` seeds each group
    // to its tier's sched context), which expresses the split correctly.
    let has_group_split = plan
        .resolved_tiers
        .as_ref()
        .is_some_and(|t| t.has_group_split_node());
    let use_run_tiers = use_tiers
        && !has_group_split
        && (!board_is_embedded(&plan.board)
            || board_is_freertos_embedded(&plan.board)
            || board_is_zephyr(&plan.board)
            || board_is_nuttx(&plan.board));

    if use_run_tiers {
        // ----------------------------------------------------------------
        // Phase 274.W2 — per-tier setup functions + run_tiers entry point.
        // ----------------------------------------------------------------
        let tiers = plan.resolved_tiers.as_ref().unwrap();

        // node_name → tier_index for per-tier node filtering.
        let node_to_tier: std::collections::HashMap<String, usize> = tiers
            .tiers
            .iter()
            .enumerate()
            .flat_map(|(ti, tier)| {
                tier.members
                    .iter()
                    .map(move |(node_name, _group)| (node_name.clone(), ti))
            })
            .collect();

        // Emit one setup function per tier (only creates THIS tier's nodes).
        for (ti, tier) in tiers.tiers.iter().enumerate() {
            let _ = writeln!(
                out,
                "/* Phase 274.W2 — tier[{ti}] ({name}) setup: creates only this tier's nodes. */",
                name = tier.name
            );
            let _ = writeln!(
                out,
                "static int32_t __nros_entry_setup_tier_{ti}(void* executor) {{"
            );
            out.push_str(
                "    if (executor == nullptr) \
                 return static_cast<int32_t>(::nros::ErrorCode::NotInitialized);\n",
            );

            for (i, n) in plan.nodes.iter().enumerate() {
                let node_name = n.name.as_deref().unwrap_or(&n.exec);
                // Only emit nodes pinned to this tier.
                if node_to_tier.get(node_name).copied() != Some(ti) {
                    continue;
                }
                let name_lit = node_name.replace('\\', "\\\\").replace('"', "\\\"");
                let _ = writeln!(out, "    {{");
                if is_rust_node(n) {
                    // Rust node: install onto the tier's explicit executor handle.
                    let pkg = sanitize_pkg(&n.pkg);
                    let _ = writeln!(
                        out,
                        "        int32_t crc = __nros_component_{pkg}_install(nullptr, executor, nullptr);"
                    );
                    out.push_str("        if (crc != 0) return crc;\n");
                } else if is_rclcpp_node(n) {
                    // rclcpp shape: construct with the tier's explicit executor handle.
                    let cls = n.class_name.as_deref().unwrap();
                    out.push_str("        ::nros::NodeHandle __h(executor);\n");
                    out.push_str(
                        "        if (!__h.valid()) return static_cast<int32_t>(::nros::ErrorCode::NotInitialized);\n",
                    );
                    let _ = writeln!(
                        out,
                        "        __nros_comp_{i} = new (__nros_comp_buf_{i}) ::{cls}(__h);"
                    );
                    let _ = writeln!(out, "        if (!__nros_comp_{i}->ok()) {{");
                    let _ = writeln!(
                        out,
                        "            ::nros::detail::report_component_failure(\"{name_lit}\", __nros_comp_{i}->error_what(), __nros_comp_{i}->error_code());"
                    );
                    let _ = writeln!(out, "            return __nros_comp_{i}->error_code();");
                    out.push_str("        }\n");
                } else {
                    // Configure-shape (C++ or C): create on the tier's executor.
                    let _ = writeln!(
                        out,
                        "        ::nros::Result r = ::nros::create_node_on(__nros_node_{i}, executor, \"{name_lit}\");"
                    );
                    out.push_str("        if (!r.ok()) return static_cast<int32_t>(r.raw());\n");
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
                        out.push_str(
                            "        if (!r.ok()) return static_cast<int32_t>(r.raw());\n",
                        );
                    }
                }
                out.push_str("    }\n");
            }

            // Params + lifecycle go in tier[0] (the boot/owning executor).
            if ti == 0 {
                if plan.param_services {
                    out.push_str(
                        "    /* Phase 269 (W1) — param-services: register + seed launch initials. */\n",
                    );
                    out.push_str("    {\n");
                    out.push_str("        nros_cpp_register_parameter_services(executor);\n");
                    for n in &plan.nodes {
                        for (k, v) in &n.params {
                            let k_esc = k.replace('\\', "\\\\").replace('"', "\\\"");
                            let v_esc = v.replace('\\', "\\\\").replace('"', "\\\"");
                            let _ = writeln!(
                                out,
                                "        nros_cpp_declare_param(executor, \"{k_esc}\", \"{v_esc}\");"
                            );
                        }
                    }
                    out.push_str("    }\n");
                }
                if let Some(autostart) = &plan.lifecycle {
                    let autostart_code: u8 = match autostart.as_str() {
                        "none" => 0,
                        "configure" => 1,
                        _ => 2,
                    };
                    out.push_str(
                        "    /* Phase 269 (W2) — lifecycle-services: register + autostart. */\n",
                    );
                    out.push_str("    {\n");
                    let _ = writeln!(
                        out,
                        "        nros_cpp_lifecycle_autostart(executor, {autostart_code}u);"
                    );
                    out.push_str("    }\n");
                }
            }

            out.push_str("    return 0;\n}\n\n");
        }

        // Emit per-tier groups string arrays.
        // Groups are derived from tier.members (unique callback-group IDs, stable order).
        out.push_str("/* Phase 274.W2 — per-tier groups arrays + tier spec table. */\n");
        let tier_groups_vecs: Vec<Vec<String>> = tiers
            .tiers
            .iter()
            .map(|tier| {
                let mut seen = std::collections::BTreeSet::new();
                tier.members
                    .iter()
                    .filter_map(|(_, g)| {
                        if seen.insert(g.clone()) {
                            Some(g.clone())
                        } else {
                            None
                        }
                    })
                    .collect()
            })
            .collect();
        for (ti, groups) in tier_groups_vecs.iter().enumerate() {
            if !groups.is_empty() {
                let _ = write!(out, "static const char* __nros_tier_{ti}_groups[] = {{");
                for g in groups {
                    let g_lit = g.replace('\\', "\\\\").replace('"', "\\\"");
                    let _ = write!(out, "\"{g_lit}\", ");
                }
                out.push_str("};\n");
            }
        }

        // Emit the NativeTierSpec array (highest-priority-first; resolver produces this order).
        let n_tiers = tiers.tiers.len();
        let _ = writeln!(
            out,
            "static const ::nros::board::NativeTierSpec __nros_tiers[{n_tiers}] = {{"
        );
        for (ti, tier) in tiers.tiers.iter().enumerate() {
            let name_lit = tier.name.replace('\\', "\\\\").replace('"', "\\\"");
            let priority = tier.priority;
            let spin_period_us = tier.spin_period_us.unwrap_or(0);
            // RFC-0052 W2 — stack_bytes now propagates (the pre-W2 literal
            // hardcoded 0, so [tiers.*.freertos].stack_bytes never reached
            // the task-create call); core rides as core+1 (0 = unpinned),
            // preempt_threshold as -1 = unset.
            let stack_bytes = tier.stack_bytes.unwrap_or(0);
            let core_plus1 = tier.core.map(|c| c + 1).unwrap_or(0);
            let preempt = tier.preempt_threshold.unwrap_or(-1);
            let groups = &tier_groups_vecs[ti];
            let (groups_expr, n_groups_val) = if groups.is_empty() {
                ("nullptr".to_string(), 0usize)
            } else {
                (format!("__nros_tier_{ti}_groups"), groups.len())
            };
            let _ = writeln!(
                out,
                "    {{ \"{name_lit}\", {groups_expr}, {n_groups_val}u, \
                 {priority}LL, {stack_bytes}u, {spin_period_us}ull, \
                 &__nros_entry_setup_tier_{ti}, {core_plus1}u, {preempt}LL }},"
            );
        }
        out.push_str("};\n\n");

        // Phase 266 — bake the boot config.
        emit_boot_config_static(&mut out, plan)?;
        out.push('\n');

        // Phase 274.W2/W3 / phase-281 W3a / W3(nuttx) — entry point: Zephyr →
        // plain `int main(void)` (the kernel calls main directly); FreeRTOS +
        // NuttX embedded → nros_app_main + NROS_APP_MAIN_REGISTER_VOID (startup
        // path calls app_main); native → int main(argc, argv).
        let board = board_cpp_path(&plan.board);
        if board_is_zephyr(&plan.board) {
            // phase-281 W3a — Zephyr per-tier embedded entry. The Zephyr kernel
            // calls main(void) directly (no nano-ros startup.c owning main); the
            // connect locator / domain id thread in via NROS_ENTRY_LOCATOR /
            // NROS_ENTRY_DOMAIN_ID (Kconfig-backed) inside ZephyrBoard::run_tiers.
            out.push_str("int main(void) {\n");
            let _ = writeln!(
                out,
                "    return static_cast<int>({board}::run_tiers(\
nros_boot_config_node_name(&NROS_BOOT_CONFIG), __nros_tiers, {n_tiers}u));"
            );
            out.push_str("}\n");
        } else if board_is_freertos_embedded(&plan.board) || board_is_nuttx(&plan.board) {
            // Phase 274.W3 (FreeRTOS) / phase-281 W3 (NuttX) — per-tier embedded
            // entry via the app_main startup shape. `{board}` resolves to
            // FreertosBoard::run_tiers or NuttxBoard::run_tiers accordingly.
            out.push_str("extern \"C\" int nros_app_main(int /*argc*/, char** /*argv*/) {\n");
            let _ = writeln!(
                out,
                "    return {board}::run_tiers(\
nros_boot_config_node_name(&NROS_BOOT_CONFIG), __nros_tiers, {n_tiers}u);"
            );
            out.push_str("}\n\n");
            out.push_str("NROS_APP_MAIN_REGISTER_VOID();\n");
        } else {
            // Native (NativeBoard): int main.
            out.push_str("int main(int /*argc*/, char** /*argv*/) {\n");
            let _ = writeln!(
                out,
                "    return {board}::run_tiers(\
nros_boot_config_node_name(&NROS_BOOT_CONFIG), __nros_tiers, {n_tiers}u);"
            );
            out.push_str("}\n");
        }
    } else {
        // ----------------------------------------------------------------
        // Single-executor path (single-tier OR embedded multi-tier with
        // sched-context scheduling). Byte-identical output for single-tier.
        // ----------------------------------------------------------------
        out.push_str("static int32_t __nros_entry_setup() {\n");

        // Phase 269 (W4) / 272 (W2) — sched-context wiring for embedded multi-tier.
        if use_tiers {
            let tiers = plan.resolved_tiers.as_ref().unwrap();
            let n_tiers = tiers.tiers.len();
            out.push_str(
                "    /* Phase 269 (W4) — sched-context wiring (multi-tier scheduling). */\n",
            );
            let _ = writeln!(out, "    uint8_t __nros_sc_ids[{n_tiers}] = {{0}};");
            out.push_str("    {\n");
            out.push_str("        void* __exec = ::nros::global_handle();\n");
            out.push_str(
                "        if (__exec == nullptr) return static_cast<int32_t>(::nros::ErrorCode::NotInitialized);\n",
            );
            for (ti, tier) in tiers.tiers.iter().enumerate() {
                let os_pri = (tier.priority.clamp(0, 255)) as u8;
                // RFC-0052 / phase-297 W1 — lower the tier's RTOS-agnostic
                // real-time policy (class/budget/period/deadline) onto its
                // sched-context, mirroring `ExecutorNodeRuntime::apply_tier_sched_policy`
                // (nros/src/node_runtime.rs). The single-executor path (ThreadX +
                // group-split plans, see `ResolvedTierTable::has_group_split_node`)
                // reaches the runtime through this C ABI, so before this a
                // `real_time` tier silently ran as `Fifo`. Byte-identical `Fifo`
                // output stays for tiers that declare no RT `class` (the common
                // single-tier case). `time_triggered` is deferred: the C ABI
                // deprecates the TT class (nros-c executor.rs → maps it to `Fifo`)
                // and needs a separate `register_time_triggered_dispatcher` seam
                // the single-executor codegen does not emit. `deadline_action`
                // (the miss policy) has no C sched-context field yet.
                let class = tier.class.as_deref();
                let sporadic = class == Some("real_time")
                    && tier.budget_us.is_some()
                    && tier.period_us.is_some();
                let best_effort = class == Some("best_effort");
                // SC period: the RT replenishment period when Sporadic; otherwise
                // the executor spin cadence (legacy `spin_period_us` semantics).
                let period_us = if sporadic {
                    tier.period_us.unwrap_or(0).min(u32::MAX as u64) as u32
                } else {
                    tier.spin_period_us.unwrap_or(0) as u32
                };
                let (class_val, class_name): (u8, &str) = if sporadic {
                    (2, "Sporadic")
                } else if best_effort {
                    (3, "BestEffort")
                } else {
                    (0, "Fifo")
                };
                out.push_str("        {\n");
                out.push_str("            nros_cpp_sched_context_t __sc = {};\n");
                let _ = writeln!(
                    out,
                    "            __sc.class_ = static_cast<nros_cpp_sched_class_t>({class_val});  /* {class_name} */"
                );
                out.push_str(
                    "            __sc.priority = static_cast<nros_cpp_priority_t>(1);  /* Normal */\n",
                );
                out.push_str(
                    "            __sc.deadline_policy = static_cast<nros_cpp_deadline_policy_t>(0);  /* Released */\n",
                );
                let _ = writeln!(out, "            __sc.period_us = {period_us}u;");
                if sporadic {
                    let budget_us = tier.budget_us.unwrap_or(0).min(u32::MAX as u64) as u32;
                    let _ = writeln!(out, "            __sc.budget_us = {budget_us}u;");
                }
                if let Some(d) = tier.deadline_us {
                    let d = d.min(u32::MAX as u64) as u32;
                    let _ = writeln!(out, "            __sc.deadline_us = {d}u;");
                }
                let _ = writeln!(out, "            __sc.os_pri = {os_pri}u;");
                let _ = writeln!(
                    out,
                    "            nros_cpp_ret_t __scr{ti} = nros_cpp_create_sched_context(__exec, &__sc, &__nros_sc_ids[{ti}]);"
                );
                let _ = writeln!(
                    out,
                    "            if (__scr{ti} != NROS_CPP_RET_OK) return static_cast<int32_t>(__scr{ti});"
                );
                out.push_str("        }\n");
            }
            out.push_str("    }\n");
            out.push_str(
                "    /* Phase 272 (W2) — seed node-name → sched-context table (RFC-0047). */\n",
            );
            out.push_str("    {\n");
            out.push_str("        void* __exec = ::nros::global_handle();\n");
            out.push_str(
                "        if (__exec == nullptr) return static_cast<int32_t>(::nros::ErrorCode::NotInitialized);\n",
            );
            for n in &plan.nodes {
                if let Some(sc_idx) = n.sched_context {
                    let node_name = n.name.as_deref().unwrap_or(&n.exec);
                    let name_lit = node_name.replace('\\', "\\\\").replace('"', "\\\"");
                    let ns_lit = n
                        .namespace
                        .as_deref()
                        .unwrap_or("/")
                        .replace('\\', "\\\\")
                        .replace('"', "\\\"");
                    let _ = writeln!(
                        out,
                        "        nros_cpp_bind_node_name_sched(__exec, \"{name_lit}\", \"{ns_lit}\", __nros_sc_ids[{sc_idx}]);"
                    );
                }
            }
            out.push_str("    }\n");
            out.push_str(
                "    /* Phase 273 (W2) — seed group → sched-context table (RFC-0047). */\n",
            );
            out.push_str("    {\n");
            out.push_str("        void* __exec = ::nros::global_handle();\n");
            out.push_str(
                "        if (__exec == nullptr) return static_cast<int32_t>(::nros::ErrorCode::NotInitialized);\n",
            );
            let node_ns: Vec<(String, String)> = plan
                .nodes
                .iter()
                .map(|n| {
                    let name = n.name.as_deref().unwrap_or(n.exec.as_str()).to_string();
                    let ns = n
                        .namespace
                        .as_deref()
                        .unwrap_or("/")
                        .replace('\\', "\\\\")
                        .replace('"', "\\\"");
                    (name, ns)
                })
                .collect();
            for (ti, tier) in tiers.tiers.iter().enumerate() {
                for (node_name, group) in &tier.members {
                    let name_lit = node_name.replace('\\', "\\\\").replace('"', "\\\"");
                    let group_lit = group.replace('\\', "\\\\").replace('"', "\\\"");
                    let ns_lit = node_ns
                        .iter()
                        .find(|(n, _)| n == node_name)
                        .map(|(_, ns)| ns.as_str())
                        .unwrap_or("/");
                    let _ = writeln!(
                        out,
                        "        nros_cpp_bind_group_sched(__exec, \"{name_lit}\", \"{ns_lit}\", \"{group_lit}\", __nros_sc_ids[{ti}]);"
                    );
                }
            }
            out.push_str("    }\n");
        }

        for (i, n) in plan.nodes.iter().enumerate() {
            let node_name = n.name.as_deref().unwrap_or(&n.exec);
            let name_lit = node_name.replace('\\', "\\\\").replace('"', "\\\"");
            let _ = writeln!(out, "    {{");
            if is_rust_node(n) {
                // Phase 257 (W0-B) — Rust node on global executor.
                let pkg = sanitize_pkg(&n.pkg);
                out.push_str("        void* __exec = ::nros::global_handle();\n");
                out.push_str(
                    "        if (__exec == nullptr) return static_cast<int32_t>(::nros::ErrorCode::NotInitialized);\n",
                );
                let _ = writeln!(
                    out,
                    "        int32_t crc = __nros_component_{pkg}_install(nullptr, __exec, nullptr);"
                );
                out.push_str("        if (crc != 0) return crc;\n");
            } else if is_rclcpp_node(n) {
                // rclcpp shape (RFC-0044): placement-new with global executor handle.
                let cls = n.class_name.as_deref().unwrap();
                out.push_str("        ::nros::NodeHandle __h(::nros::global_handle());\n");
                out.push_str(
                    "        if (!__h.valid()) return static_cast<int32_t>(::nros::ErrorCode::NotInitialized);\n",
                );
                let _ = writeln!(
                    out,
                    "        __nros_comp_{i} = new (__nros_comp_buf_{i}) ::{cls}(__h);"
                );
                let _ = writeln!(out, "        if (!__nros_comp_{i}->ok()) {{");
                let _ = writeln!(
                    out,
                    "            ::nros::detail::report_component_failure(\"{name_lit}\", __nros_comp_{i}->error_what(), __nros_comp_{i}->error_code());"
                );
                let _ = writeln!(out, "            return __nros_comp_{i}->error_code();");
                out.push_str("        }\n");
            } else {
                // Configure-shape (C++ or C) nodes: use global create_node.
                let _ = writeln!(
                    out,
                    "        ::nros::Result r = ::nros::create_node(__nros_node_{i}, \"{name_lit}\");"
                );
                out.push_str("        if (!r.ok()) return static_cast<int32_t>(r.raw());\n");
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
        if plan.param_services {
            out.push_str(
                "    /* Phase 269 (W1) — param-services: register + seed launch initials. */\n",
            );
            out.push_str("    {\n");
            out.push_str("        void* __exec = ::nros::global_handle();\n");
            out.push_str(
                "        if (__exec == nullptr) return static_cast<int32_t>(::nros::ErrorCode::NotInitialized);\n",
            );
            out.push_str("        nros_cpp_register_parameter_services(__exec);\n");
            for n in &plan.nodes {
                for (k, v) in &n.params {
                    let k_esc = k.replace('\\', "\\\\").replace('"', "\\\"");
                    let v_esc = v.replace('\\', "\\\\").replace('"', "\\\"");
                    let _ = writeln!(
                        out,
                        "        nros_cpp_declare_param(__exec, \"{k_esc}\", \"{v_esc}\");"
                    );
                }
            }
            out.push_str("    }\n");
        }
        if let Some(autostart) = &plan.lifecycle {
            let autostart_code: u8 = match autostart.as_str() {
                "none" => 0,
                "configure" => 1,
                _ => 2,
            };
            out.push_str("    /* Phase 269 (W2) — lifecycle-services: register + autostart. */\n");
            out.push_str("    {\n");
            out.push_str("        void* __exec = ::nros::global_handle();\n");
            out.push_str(
                "        if (__exec == nullptr) return static_cast<int32_t>(::nros::ErrorCode::NotInitialized);\n",
            );
            let _ = writeln!(
                out,
                "        nros_cpp_lifecycle_autostart(__exec, {autostart_code}u);"
            );
            out.push_str("    }\n");
        }
        out.push_str("    return 0;\n}\n\n");

        // Phase 266 (W6) — bake the boot config blob.
        emit_boot_config_static(&mut out, plan)?;
        out.push('\n');

        let board = board_cpp_path(&plan.board);
        if board_is_zephyr(&plan.board) {
            // phase-263 C2d — Zephyr: kernel calls main(void) directly.
            out.push_str("int main(void) {\n");
            let _ = writeln!(
                out,
                "    return static_cast<int>({board}::run_components(\
NROS_ENTRY_LOCATOR, nros_boot_config_node_name(&NROS_BOOT_CONFIG), &__nros_entry_setup));"
            );
            out.push_str("}\n");
        } else if board_is_embedded(&plan.board) {
            // phase-263 C2 — embedded: startup.c calls app_main.
            out.push_str("extern \"C\" int nros_app_main(int /*argc*/, char** /*argv*/) {\n");
            let _ = writeln!(
                out,
                "    return {board}::run_components(\
NROS_ENTRY_LOCATOR, nros_boot_config_node_name(&NROS_BOOT_CONFIG), &__nros_entry_setup);"
            );
            out.push_str("}\n\n");
            out.push_str("NROS_APP_MAIN_REGISTER_VOID();\n");
        } else {
            // native (NativeBoard): single-tier or degenerate.
            out.push_str("int main(int /*argc*/, char** /*argv*/) {\n");
            let _ = writeln!(
                out,
                "    return {board}::run_components(\
nros_boot_config_node_name(&NROS_BOOT_CONFIG), &__nros_entry_setup);"
            );
            out.push_str("}\n");
        }
    }

    Ok(out)
}

/// A `lang == "c"` node is built via the C factory/configure seam (no C++ class).
fn is_c_node(n: &super::PlanNode) -> bool {
    n.lang.as_deref() == Some("c")
}

/// Phase 257 (W0-B) — a `lang == "rust"` node is installed via the uniform
/// `__nros_component_<pkg>_install` seam onto the shared executor; it self-creates
/// its node (no entry-created `::nros::Node`, no C++ class, no qos-override — D7
/// Option C).
fn is_rust_node(n: &super::PlanNode) -> bool {
    n.lang.as_deref() == Some("rust")
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
                    params: Vec::new(),
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
                    params: Vec::new(),
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
        // headers included (including boot_config.h for the node-name blob)
        assert!(src.contains("#include <nros/boot_config.h>"));
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
        // routes to the real executor via the named overload (phase 266)
        assert!(src.contains(
            "::nros::board::NativeBoard::run_components(nros_boot_config_node_name(&NROS_BOOT_CONFIG), &__nros_entry_setup)"
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
        assert!(
            src.contains("__nros_comp_0 = new (__nros_comp_buf_0) ::ctrl_pkg::Controller(__h);")
        );
        assert!(src.contains("if (!__nros_comp_0->ok()) {"));
        assert!(src.contains("report_component_failure(\"controller\""));
        // The rclcpp shape does NOT default-construct a Node or call configure.
        assert!(!src.contains("static ::nros::Node __nros_node_0;"));
        assert!(!src.contains("__nros_comp_0.configure"));
        assert!(!src.contains("create_node(__nros_node_0"));
        // still routes to the real executor via the named overload (phase 266)
        assert!(src.contains(
            "::nros::board::NativeBoard::run_components(nros_boot_config_node_name(&NROS_BOOT_CONFIG), &__nros_entry_setup)"
        ));
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
        assert!(
            src.contains("__nros_comp_0 = new (__nros_comp_buf_0) ::ctrl_pkg::Controller(__h);")
        );
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
        // Still routes to the real executor via the named overload (phase 266).
        assert!(src.contains(
            "::nros::board::NativeBoard::run_components(nros_boot_config_node_name(&NROS_BOOT_CONFIG), &__nros_entry_setup)"
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
        // Phase 266: embedded boards use the 3-arg (locator, session_name, setup) overload.
        let mut plan = fixture_plan_typed(&[("t_pkg", "t", "t", "t_pkg::T", "t_pkg/T.hpp")]);
        plan.board = "nuttx".into();
        let src = emit_typed(&plan).expect("typed emit ok");
        assert!(src.contains(
            "::nros::board::NuttxBoard::run_components(NROS_ENTRY_LOCATOR, nros_boot_config_node_name(&NROS_BOOT_CONFIG), &__nros_entry_setup)"
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
            "qemu-riscv64-threadx",
        ] {
            let mut plan = fixture_plan_typed(&[("t_pkg", "t", "t", "t_pkg::T", "t_pkg/T.hpp")]);
            plan.board = key.into();
            let src = emit_typed(&plan).expect("typed emit ok");
            assert!(
                src.contains(
                    "::nros::board::ThreadxBoard::run_components(NROS_ENTRY_LOCATOR, nros_boot_config_node_name(&NROS_BOOT_CONFIG), &__nros_entry_setup)"
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
            "::nros::board::NativeBoard::run_components(nros_boot_config_node_name(&NROS_BOOT_CONFIG), &__nros_entry_setup)"
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
        // Phase 269 W1 — when param_services is true, the post-configure block emits
        // nros_cpp_register_parameter_services + nros_cpp_declare_param per node param.
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
        assert!(src.contains("nros_cpp_declare_param(__exec, \"publish_period_ms\", \"250\")"));
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
            src.contains("::nros::create_node_on(__nros_node_0, executor, \"ctrl\")"),
            "ctrl node must use create_node_on in tier-0 setup; src:\n{src}"
        );
        assert!(
            src.contains("::nros::create_node_on(__nros_node_1, executor, \"telem\")"),
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
            src.contains("::nros::board::NativeBoard::run_tiers("),
            "main must call NativeBoard::run_tiers; src:\n{src}"
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
        // High tier: os_pri=80, period_us=10000.
        assert!(src.contains("__sc.os_pri = 80u;"), "expected os_pri=80");
        assert!(
            src.contains("__sc.period_us = 10000u;"),
            "expected period_us=10000"
        );
        assert!(
            src.contains("nros_cpp_create_sched_context(__exec, &__sc, &__nros_sc_ids[0])"),
            "expected tier 0 sc create"
        );
        // Bind seeds for each tiered node.
        assert!(
            src.contains(
                "nros_cpp_bind_node_name_sched(__exec, \"ctrl\", \"/\", __nros_sc_ids[0])"
            ),
            "ctrl must be seeded; src:\n{src}"
        );
        assert!(
            src.contains(
                "nros_cpp_bind_node_name_sched(__exec, \"telem\", \"/\", __nros_sc_ids[1])"
            ),
            "telem must be seeded; src:\n{src}"
        );
        // run_tiers must NOT be called (embedded boards use single executor).
        assert!(
            !src.contains("NativeBoard::run_tiers"),
            "embedded board must not emit run_tiers; src:\n{src}"
        );
    }

    #[test]
    fn typed_emit_single_executor_lowers_real_time_tier_to_sporadic() {
        // Phase 297 W1 — the single-executor sched-context path (ThreadX +
        // group-split) lowers a `real_time` tier's class/budget/period/deadline
        // onto its sched-context, mirroring `apply_tier_sched_policy`. Before W1
        // it hardcoded `Fifo` + only os_pri/spin-period, so a `real_time` tier
        // silently ran best-effort. `time_triggered` stays deferred (C ABI
        // deprecates the TT class).
        use nros_orchestration_ir::{ResolvedTier, ResolvedTierTable};
        let rt_tier = ResolvedTier {
            name: "control".into(),
            priority: 90,
            stack_bytes: None,
            spin_period_us: Some(5_000),
            preempt_threshold: None,
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
        // class_ lowered to Sporadic (2), not the hardcoded Fifo (0).
        assert!(
            src.contains("__sc.class_ = static_cast<nros_cpp_sched_class_t>(2);  /* Sporadic */"),
            "real_time tier must lower to Sporadic; got:\n{src}"
        );
        // period_us comes from the RT period (20000), NOT the spin cadence (5000).
        assert!(
            src.contains("__sc.period_us = 20000u;"),
            "expected RT period_us=20000; got:\n{src}"
        );
        assert!(
            src.contains("__sc.budget_us = 3000u;"),
            "expected budget_us=3000; got:\n{src}"
        );
        assert!(
            src.contains("__sc.deadline_us = 15000u;"),
            "expected deadline_us=15000; got:\n{src}"
        );
        assert!(
            src.contains("__sc.os_pri = 90u;"),
            "expected os_pri=90; got:\n{src}"
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
        // FreertosBoard::run_tiers called (not NativeBoard).
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
        // ZephyrBoard::run_tiers called (not NativeBoard / FreertosBoard).
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
        // NuttxBoard::run_tiers called (not NativeBoard / FreertosBoard / ZephyrBoard).
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
            src.contains(
                "nros_cpp_bind_node_name_sched(__exec, \"ctrl\", \"/\", __nros_sc_ids[0])"
            ),
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
            src.contains("::nros::create_node(__nros_node_0, \"talker\")"),
            "no-tier plan must use plain create_node"
        );
        assert!(
            !src.contains(".sched("),
            "no-tier plan must not use NodeBuilder sched"
        );
    }
}
