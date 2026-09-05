//! Phase 219.A — Rust Entry-pkg TU emitter.
//!
//! Today the canonical Rust Entry pkg is the `nros::main!()`
//! proc-macro (`packages/core/nros-macros/src/main_macro.rs`); this
//! emitter exists so the CLI verb `nros codegen entry --lang rust …`
//! can produce a standalone `main.rs` body for tooling that wants to
//! pre-bake the macro expansion (e.g. for byte-level diffs against the
//! proc-macro output, or to inspect the launch-XML resolution outside
//! a cargo build).
//!
//! The proc-macro itself remains the canonical compile-time emitter —
//! it keeps direct access to `proc_macro::Span` for diagnostics that
//! a CLI shell-out cannot match. The shared [`crate::codegen::entry`]
//! `Plan` IR makes the two paths converge on a single pkg-index +
//! launch-parse implementation.

use super::{Plan, sanitize_pkg};

/// Emit a Rust `main.rs` body for the given plan.
///
/// Output mirrors the proc-macro's `OwnedSpin` framework branch (which
/// is the only branch the CLI verb dispatches today — RTIC + Embassy
/// emits stay proc-macro-only since they need `proc_macro::Span` for
/// the `custom_tasks` splice). The body installs both a hosted `fn
/// main()` and an embedded `#[unsafe(no_mangle)] extern "C" fn main()`
/// so the same TU works for native + bare-metal targets.
/// The whole TU, as the template sees it.
///
/// Issue 1102 — every field is ALREADY CORRECT: `board_path` came from
/// `nros_orchestration_ir`, and every literal is already escaped. The template
/// places them; it does not compute them.
#[derive(serde::Serialize)]
struct RustEntryView {
    bringup: String,
    launch: String,
    board: String,
    board_path: &'static str,
    /// Raw-string literals, already quoted by `quote_str`.
    depfiles: Vec<String>,
    nodes: Vec<RustNodeView>,
}

/// One launch node's runtime state.
///
/// The three list fields are pre-joined literal text rather than lists,
/// because their ELEMENTS are Rust syntax (`("a", "b")`, `("t", 1, 2, 3)`)
/// assembled from escaped literals. Handing the template a list would make it
/// responsible for composing that syntax, which is the half that must stay in
/// Rust.
#[derive(serde::Serialize)]
struct RustNodeView {
    pkg: String,
    params: String,
    remaps: String,
    qos_overrides: String,
    identity: String,
}

/// Emit a Rust `main.rs` body for the given plan.
///
/// Output mirrors the proc-macro's `OwnedSpin` framework branch (which
/// is the only branch the CLI verb dispatches today — RTIC + Embassy
/// emits stay proc-macro-only since they need `proc_macro::Span` for
/// the `custom_tasks` splice). The body installs both a hosted `fn
/// main()` and an embedded `#[unsafe(no_mangle)] extern "C" fn main()`
/// so the same TU works for native + bare-metal targets.
pub fn emit(plan: &Plan) -> String {
    let view = RustEntryView {
        bringup: plan.bringup.clone(),
        launch: plan.launch_file.display().to_string(),
        board: plan.board.clone(),
        board_path: board_path_for(&plan.board).unwrap_or("::nros_board_linux::LinuxBoard"),
        // include_bytes! tracking — same rebuild-correctness workaround the
        // proc-macro uses. A path that does not exist is skipped, exactly as
        // the proc-macro does.
        depfiles: plan
            .depfile_paths
            .iter()
            .filter(|d| d.exists())
            .map(|d| quote_str(&d.display().to_string()))
            .collect(),
        nodes: plan.nodes.iter().map(node_view).collect(),
    };

    // A render failure is a bug in a template compiled INTO this binary, so it
    // cannot be handled meaningfully at a call site that only has a plan.
    crate::codegen::entry::render::render("rust_entry.rs.jinja", &view)
        .expect("bundled rust entry template must render")
}

/// Bake the per-node runtime state the `nros::main!` proc-macro sets before
/// each `register` call (issue 0302).
///
/// Four features arrived over four phases — params (264 W4a), identity
/// (268 W1), remaps (305 W3 / issue 0255), QoS overrides (issue #52) — and
/// each wired the proc-macro while leaving this emitter behind, so a CLI-baked
/// entry ran every node with default parameters, no remaps, its own hardcoded
/// name and no QoS overrides. From the same plan.
///
/// EVERY field is written unconditionally, including the empty case. That
/// reset discipline is the macro's and it is load-bearing: `runtime` is reused
/// across nodes, so a node with no params must clear the previous node's
/// rather than inherit them.
fn node_view(n: &super::PlanNode) -> RustNodeView {
    let pairs = |items: &[(String, String)]| -> String {
        items
            .iter()
            .map(|(a, b)| format!("({}, {})", lit_str(a), lit_str(b)))
            .collect::<Vec<_>>()
            .join(", ")
    };

    // The plan carries LOWERED codes: `nros_orchestration_ir::qos_override`
    // already rejected anything unusable (issue 0303), so nothing is decoded
    // or silently dropped here.
    let qos_overrides = n
        .qos_overrides
        .iter()
        .map(|o| {
            format!(
                "({}, {}, {}, {})",
                lit_str(&o.topic),
                o.role,
                o.policy,
                o.value
            )
        })
        .collect::<Vec<_>>()
        .join(", ");

    // A namespace without a name is not an identity: the proc-macro keys the
    // override on the name, so `None` here means "keep the node's own".
    let identity = match &n.name {
        Some(name) => format!(
            "::core::option::Option::Some(({}, {}))",
            lit_str(name),
            lit_str(n.namespace.as_deref().unwrap_or(""))
        ),
        None => "::core::option::Option::None".to_string(),
    };

    RustNodeView {
        pkg: sanitize_pkg(&n.pkg),
        params: pairs(&n.params),
        remaps: pairs(&n.remaps),
        qos_overrides,
        identity,
    }
}

/// Board key → Rust ZST path.
///
/// Delegates to [`nros_orchestration_ir::board_path_for`], the single source
/// of truth shared with the `nros::main!()` proc-macro. Any board added to
/// the IR crate is automatically available here with no extra edit.
fn board_path_for(board: &str) -> Option<&'static str> {
    nros_orchestration_ir::board_path_for(board)
}

/// Quote a string into a valid Rust string literal (raw form when
/// possible so backslashes in path components survive on Windows
/// hosts).
/// Quote a value as a PLAIN Rust string literal, escaping as needed.
///
/// The `nros::main!` proc-macro emits these through `LitStr`, i.e. plain
/// quoted form. This emitter exists to be byte-diffable against that output
/// (issue 0302), so it matches rather than using the raw-string form
/// [`quote_str`] uses for paths.
fn lit_str(s: &str) -> String {
    let escaped = s.replace('\\', "\\\\").replace('"', "\\\"");
    format!("\"{escaped}\"")
}

fn quote_str(s: &str) -> String {
    // Pick a raw-string hash count that doesn't collide with the
    // string's own quote sequences. For paths the input is overwhelm-
    // ingly free of `"#` runs, so a single `#` works.
    let mut hashes = 1usize;
    loop {
        let needle = format!("\"{}", "#".repeat(hashes));
        if !s.contains(&needle) {
            break;
        }
        hashes += 1;
    }
    let hs = "#".repeat(hashes);
    format!("r{hs}\"{s}\"{hs}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen::entry::PlanNode;
    use std::path::PathBuf;

    /// Issue 0302 — every per-node field the `nros::main!` proc-macro sets must
    /// be baked here too, INCLUDING the empty case.
    ///
    /// The reset is the point: `runtime` is reused across nodes, so a node with
    /// no params must clear the previous node's rather than inherit them.
    /// Emitting the four assignments only when non-empty would leak state
    /// between nodes and pass a naive "does it contain the value" test.
    #[test]
    fn every_node_gets_the_full_runtime_state_reset() {
        let mut plan = fixture_plan(&[("talker_pkg", "talker"), ("listener_pkg", "listener")]);
        plan.nodes[0].params = vec![("rate".into(), "25".into())];
        plan.nodes[0].remaps = vec![("chatter".into(), "/ns/chatter".into())];
        plan.nodes[0].name = Some("talker".into());
        plan.nodes[0].namespace = Some("/ns".into());
        // node 1 deliberately left bare — it must still be RESET.

        let out = emit(&plan);

        assert!(
            out.contains(r#"runtime.params = &[("rate", "25")];"#),
            "{out}"
        );
        assert!(
            out.contains(r#"runtime.remaps = &[("chatter", "/ns/chatter")];"#),
            "{out}"
        );
        assert!(
            out.contains(
                r#"runtime.node_identity = ::core::option::Option::Some(("talker", "/ns"));"#
            ),
            "{out}"
        );

        // Both nodes reset all four; the bare one gets empties, not omissions.
        assert_eq!(out.matches("runtime.params = &[").count(), 2, "{out}");
        assert_eq!(out.matches("runtime.remaps = &[").count(), 2, "{out}");
        assert_eq!(
            out.matches("runtime.qos_overrides = &[").count(),
            2,
            "{out}"
        );
        assert_eq!(out.matches("runtime.node_identity = ").count(), 2, "{out}");
        assert!(
            out.contains("runtime.params = &[];"),
            "bare node must reset:\n{out}"
        );
        assert!(
            out.contains("runtime.node_identity = ::core::option::Option::None;"),
            "a node with no launch name must clear the previous identity:\n{out}"
        );
    }

    /// The state must be written BEFORE the register call it configures —
    /// after it would configure the next node, or nothing.
    #[test]
    fn state_is_emitted_before_the_register_call() {
        let mut plan = fixture_plan(&[("talker_pkg", "talker")]);
        plan.nodes[0].params = vec![("rate".into(), "25".into())];
        let out = emit(&plan);

        let params_at = out.find("runtime.params").expect("params emitted");
        let register_at = out
            .find("::talker_pkg::register")
            .expect("register emitted");
        assert!(
            params_at < register_at,
            "params must precede the register call:\n{out}"
        );
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
        }
    }

    #[test]
    fn emit_two_node_plan_contains_register_calls() {
        let plan = fixture_plan(&[("talker_pkg", "talker"), ("listener_pkg", "listener")]);
        let src = emit(&plan);
        assert!(src.contains("::talker_pkg::register(runtime)?;"));
        assert!(src.contains("::listener_pkg::register(runtime)?;"));
        assert!(src.contains("LinuxBoard"));
        // Both a hosted main and an embedded main.
        assert!(src.contains("#[cfg(not(target_os = \"none\"))]"));
        assert!(src.contains("#[cfg(target_os = \"none\")]"));
    }

    #[test]
    fn dash_pkg_names_are_sanitised() {
        let plan = fixture_plan(&[("talker-pkg", "talker")]);
        let src = emit(&plan);
        assert!(src.contains("::talker_pkg::register(runtime)?;"));
    }

    #[test]
    fn freertos_board_maps_to_correct_zst() {
        let mut plan = fixture_plan(&[("talker_pkg", "talker")]);
        plan.board = "freertos".into();
        let src = emit(&plan);
        assert!(src.contains("::nros_board_mps2_an385_freertos::Mps2An385"));
    }

    #[test]
    fn quote_str_handles_simple_paths() {
        let q = quote_str("/abs/path.xml");
        assert!(q.starts_with("r#\""));
        assert!(q.ends_with("\"#"));
        assert!(q.contains("/abs/path.xml"));
    }
}
