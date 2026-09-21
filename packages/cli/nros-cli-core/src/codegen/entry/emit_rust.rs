//! Phase 219.A — the CLI's Rust entry renderer, and the proc-macro's mirror.
//!
//! The canonical Rust entry is the `nros::main!()` proc-macro
//! (`packages/core/nros-macros/src/main_macro.rs`), which keeps
//! `proc_macro::Span` for diagnostics a CLI shell-out cannot match. This
//! renderer is its SECOND rendering of the same facts, and phase-432 W2.4
//! finally made that pay: both now render
//! [`nros_entry_lower::LoweredEntry`], and a parity corpus
//! (`packages/cli/nros-entry-lower/testdata/parity/`) is rendered by each and
//! compared. That comparison is the "byte-level diff against the proc-macro
//! output" this file's doc comment promised from 2024 and never had — RFC-0091
//! §7, and the reason the file is still here rather than deleted.
//!
//! **The renderer that stays is not the same thing as the VERB that went.**
//! `nros codegen entry --lang rust` is retired (`cmd/codegen.rs`): nothing
//! invoked it — `nano_ros_entry` passes `--lang` as `c` or `cpp` only, and a
//! Rust entry reaches the proc-macro through `rust_cargo_application()` — and
//! what it produced was a strictly POORER entry than the macro's, missing
//! tiers, lifecycle, param services and executor sizing. A user who found the
//! flag got an entry that silently ignored half their `system.toml`. What
//! survives is the renderer, its goldens and the parity gate.
//!
//! What is rendered here and what is not: every fact arrives computed from
//! `nros-entry-lower` and `nros_orchestration_ir`. What stays in this file is
//! SPELLING — quoting, and the syntax of a Rust tuple — because quoting is a
//! correctness property and a template that got it wrong would fail silently
//! (RFC-0091 §6b).

use nros_entry_lower::{LoweredEntry, LoweredNode};

use super::Plan;

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
    /// Issue 1409 — does an entry crate for this board link `std`?
    ///
    /// The template may name a `std` path only where this is true, exactly as
    /// `nros::main!` may only inside `hosted_std_scaffold_ts`. It is NOT a
    /// `#[cfg]` the emitted code could ask for itself: `#![no_std]` is a
    /// property of the CRATE and is orthogonal to the target OS, so no
    /// `target_os` predicate distinguishes "this crate has `std`" from "this
    /// target has an OS". Issue 1381 measured the difference.
    links_std: bool,
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
/// Output mirrors the proc-macro's `OwnedSpin` framework branch, and that is
/// now ENFORCED rather than merely stated: a board wanting any other shape is
/// refused, naming the shape and pointing at `nros::main!` (issue 1435, see
/// [`refuse_non_owned_spin`]). The four non-owned-spin frameworks stay
/// proc-macro-only for reasons that differ per framework — RTIC and Embassy
/// need `proc_macro::Span` for the `custom_tasks` splice, Zephyr needs the
/// backend register and spin-or-tiers tail that `LoweredEntry` does not carry,
/// ESP32 needs esp-hal's own entry attribute.
///
/// The body installs the same three entry arms the macro does: an
/// `extern "C" fn main` for `target_os = "none"` (a C runtime calls it), a
/// second one for `target_os = "nuttx"` (the family is `no_std` since
/// phase-359 W7, so libstd's `lang_start` is not there to wrap a Rust `main`),
/// and — only for a board whose entry LINKS `std` — a hosted Rust `fn main()`.
/// The third is issue 1409; see [`RustEntryView::links_std`].
///
/// A board key the Rust pack has no ZST for is an error; see
/// [`emit_lowered`].
pub fn emit(plan: &Plan) -> Result<String, String> {
    emit_lowered(&lower(plan))
}

/// Stage 2 — the CLI's `Plan` becomes the facts both Rust producers render.
///
/// This is the half the proc-macro cannot share: it has no `Plan` (RFC-0091
/// §4 — `Plan` is the CLI's own projection of its input, and lives across
/// `cmd/`, `builder/` and `codegen/`). What IS shared is the OUTPUT type, so
/// the two converge here rather than at the renderer.
pub fn lower(plan: &Plan) -> LoweredEntry {
    LoweredEntry {
        bringup: plan.bringup.clone(),
        launch: plan.launch_file.display().to_string(),
        board: plan.board.clone(),
        // include_bytes! tracking — same rebuild-correctness workaround the
        // proc-macro uses. A path that does not exist is skipped, exactly as
        // the proc-macro does: `include_bytes!` on a missing path is a hard
        // compile error, and the pkg-index walk can name a synthesised dir.
        depfiles: plan
            .depfile_paths
            .iter()
            .filter(|d| d.exists())
            .map(|d| d.display().to_string())
            .collect(),
        nodes: plan.nodes.iter().map(lower_node).collect(),
    }
}

/// Stage 3 — render the lowered entry as Rust.
///
/// Public because the parity harness renders the shared corpus through it
/// directly; the corpus is `LoweredEntry` values, which is the point.
///
/// A board key with no Rust board ZST is an ERROR naming the keys the Rust
/// pack knows. That is the message `nros::main!` gives for the same key,
/// from the same `board_path_keys_csv`. It used to fall back to `LinuxBoard`
/// (issue 1285 follow-up), which rendered a host `BoardEntry::run` for any key
/// it did not know. The parity corpus had one such case: `freertos-posix`, a
/// C-only board, rendered as `LinuxBoard`.
///
/// A board key whose FRAMEWORK is not `owned-spin` is the second refusal, and
/// it is issue 1435. See [`refuse_non_owned_spin`].
pub fn emit_lowered(entry: &LoweredEntry) -> Result<String, String> {
    let board_path = board_path_for(&entry.board).ok_or_else(|| {
        format!(
            "the Rust entry pack has no board ZST for `{}`. Known boards: {}.",
            entry.board,
            nros_orchestration_ir::board_path_keys_csv()
        )
    })?;
    refuse_non_owned_spin(&entry.board)?;
    // Issue 1409 — the SAME table `board_path_for` just consulted, so the key
    // is known by construction here and `unwrap_or(true)` is the unreachable
    // arm rather than a policy. The policy for an UNKNOWN board ("assume
    // hosted", `nros_orchestration_ir::board_entry_links_std`'s `None`) lives
    // in the proc-macro, because an out-of-tree board reaches that producer
    // through `NROS_BOARD_FRAMEWORK` and this one refuses it above.
    let links_std = nros_orchestration_ir::board_entry_links_std(&entry.board).unwrap_or(true);
    let view = RustEntryView {
        bringup: entry.bringup.clone(),
        launch: entry.launch.clone(),
        board: entry.board.clone(),
        board_path,
        links_std,
        depfiles: entry.depfiles.iter().map(|d| quote_str(d)).collect(),
        nodes: entry.nodes.iter().map(node_view).collect(),
    };

    // A render failure is a bug in a template compiled INTO this binary, so it
    // cannot be handled meaningfully at a call site that only has a plan.
    Ok(
        crate::codegen::entry::render::render("entry_rust.rs", &view)
            .expect("bundled rust entry template must render"),
    )
}

/// A plan node's per-node runtime bake, as neutral facts (issue 0302).
///
/// Four features arrived over four phases — params (264 W4a), identity
/// (268 W1), remaps (305 W3 / issue 0255), QoS overrides (issue #52) — and
/// each wired the proc-macro while leaving this emitter behind, so a CLI-baked
/// entry ran every node with default parameters, no remaps, its own hardcoded
/// name and no QoS overrides. From the same plan. That is the drift the shared
/// [`LoweredNode`] and the parity corpus exist to make impossible: a fifth
/// feature now cannot reach one producer without the other going red.
fn lower_node(n: &super::PlanNode) -> LoweredNode {
    LoweredNode {
        pkg: n.pkg.clone(),
        params: n.params.clone(),
        remaps: n.remaps.clone(),
        // The plan carries LOWERED codes: `nros_orchestration_ir::qos_override`
        // already rejected anything unusable (issue 0303), so nothing is
        // decoded or silently dropped here.
        qos_overrides: n
            .qos_overrides
            .iter()
            .map(|o| nros_entry_lower::QosOverride {
                topic: o.topic.clone(),
                role: o.role,
                policy: o.policy,
                value: o.value,
            })
            .collect(),
        // A namespace without a name is not an identity: the proc-macro keys
        // the override on the name, so `None` here means "keep the node's own".
        identity: n.name.as_ref().map(|name| {
            nros_entry_lower::NodeIdentity::new(name, n.namespace.as_deref().unwrap_or(""))
        }),
    }
}

/// Spell one lowered node as Rust.
///
/// EVERY field is written unconditionally, including the empty case. That
/// reset discipline is the macro's and it is load-bearing: `runtime` is reused
/// across nodes, so a node with no params must clear the previous node's
/// rather than inherit them.
fn node_view(n: &LoweredNode) -> RustNodeView {
    let pairs = |items: &[(String, String)]| -> String {
        items
            .iter()
            .map(|(a, b)| format!("({}, {})", lit_str(a), lit_str(b)))
            .collect::<Vec<_>>()
            .join(", ")
    };

    // SUFFIXED literals (`1u8`), because `quote!` interpolating a `u8` emits
    // `Literal::u8_suffixed` and the proc-macro therefore always has. The two
    // renderings differed here for as long as both existed — semantically
    // identical, textually not — and nothing compared them, which is precisely
    // what the parity corpus was built to find.
    let qos_overrides = n
        .qos_overrides
        .iter()
        .map(|o| {
            format!(
                "({}, {}u8, {}u8, {}u32)",
                lit_str(&o.topic),
                o.role,
                o.policy,
                o.value
            )
        })
        .collect::<Vec<_>>()
        .join(", ");

    let identity = match n.identity_pair() {
        Some((name, namespace)) => format!(
            "::core::option::Option::Some(({}, {}))",
            lit_str(name),
            lit_str(namespace)
        ),
        None => "::core::option::Option::None".to_string(),
    };

    RustNodeView {
        pkg: n.ident(),
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

/// The framework this template renders, and the only one it can.
///
/// The template's body is `<Board as BoardEntry>::run(closure)` — the
/// proc-macro's `Framework::OwnedSpin` branch and nothing else.
const RENDERED_FRAMEWORK: &str = "owned-spin";

/// Issue 1435 — refuse a board whose entry shape this template does not render.
///
/// [`emit`]'s doc comment has said "RTIC and Embassy stay proc-macro-only"
/// since phase 219.A, and it was prose the code contradicted: every key in
/// `BOARD_PATHS` rendered the same `BoardEntry::run` call, whatever framework
/// the board wants. MEASURED, by compiling the rendering of each key against
/// stub crates carrying the board ZSTs' real impl sets:
///
/// - `zephyr` / `native_sim/native/64` → `ZephyrBoard`, which implements
///   `BoardInit` / `BoardPrint` / `BoardExit` and **no** `BoardEntry`
///   (Zephyr owns `main`; the macro's `Framework::Zephyr` arm emits a
///   `rust_main` staticlib export instead) —
///   `error[E0277]: the trait bound `ZephyrBoard: BoardEntry` is not satisfied`.
/// - `rtic-mps2-an385` / `qemu-rtic-mps2-an385` → `RticMps2An385`, which
///   implements `RticBoardEntry` — a SEPARATE trait, not a subtrait — so the
///   same `E0277`.
/// - `esp32-qemu` / `esp32-c3-baremetal` → `Esp32QemuEntry`, which DOES
///   implement `BoardEntry`, so this one compiles. It still cannot boot:
///   esp-riscv-rt's `_start` jumps to the esp-hal entry registration, so the
///   boot symbol must be `#[::esp_hal::main] fn main() -> !` and the bare
///   `extern "C" fn main` this template emits is never called. Refused with
///   the others because the rule is "this emitter renders ONE framework", not
///   "this emitter renders whatever happens to type-check" — the quieter
///   failure is the worse one to ship.
///
/// The predicate is the SSoT both producers already consult,
/// `nros_orchestration_ir::framework_for_board_key`, so a new non-owned-spin
/// board is refused here the day its framework row is written, with no edit to
/// this file. `None` means "no in-tree opinion", which every caller reads as
/// `owned-spin`; the key is known by construction here (`board_path_for`
/// resolved it one line up), and issue 1435 also closed the one key that was
/// in `BOARD_PATHS` and not in the framework table.
fn refuse_non_owned_spin(board: &str) -> Result<(), String> {
    let framework =
        nros_orchestration_ir::framework_for_board_key(board).unwrap_or(RENDERED_FRAMEWORK);
    if framework == RENDERED_FRAMEWORK {
        return Ok(());
    }
    Err(format!(
        "the Rust entry pack renders the `{RENDERED_FRAMEWORK}` entry shape \
         (`<Board as BoardEntry>::run`), and board `{board}` wants the \
         `{framework}` shape. Use the `nros::main!()` proc-macro, which is the \
         canonical Rust entry emitter and has a branch for it."
    ))
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

        let out = emit(&plan).expect("a board key the Rust pack knows");

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
        let out = emit(&plan).expect("a board key the Rust pack knows");

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
            session: Default::default(),
        }
    }

    #[test]
    fn emit_two_node_plan_contains_register_calls() {
        let plan = fixture_plan(&[("talker_pkg", "talker"), ("listener_pkg", "listener")]);
        let src = emit(&plan).expect("a board key the Rust pack knows");
        assert!(src.contains("::talker_pkg::register(runtime)?;"));
        assert!(src.contains("::listener_pkg::register(runtime)?;"));
        assert!(src.contains("LinuxBoard"));
        // `native` links std, so all three entry arms: hosted, NuttX, none.
        assert!(src.contains("#[cfg(not(any(target_os = \"none\", target_os = \"nuttx\")))]"));
        assert!(src.contains("#[cfg(target_os = \"nuttx\")]"));
        assert!(src.contains("#[cfg(target_os = \"none\")]"));
    }

    /// Issue 1409 — the mirror of `nros-macros`'
    /// `only_a_board_whose_entry_links_std_gets_the_std_scaffold`, one producer
    /// over.
    ///
    /// Asserted per BOARD KEY over the whole table, not on a chosen example:
    /// the property is "no board whose entry is `#![no_std]` gets a `std` path",
    /// and a test naming two keys would go quiet the day a third is added. A
    /// refactor that keeps the flag and renders the block anyway fails this.
    #[test]
    fn only_a_board_whose_entry_links_std_gets_the_hosted_main() {
        let mut checked = 0usize;
        let mut hosted = 0usize;
        for key in nros_orchestration_ir::board_path_keys() {
            // Issue 1435 — the pack now REFUSES a board whose framework is not
            // `owned-spin`, so those keys have no emitted entry to assert a
            // `std` path about. Skipped through the production predicate
            // itself, not a second list of framework names: the day a board
            // changes framework, this follows it.
            if refuse_non_owned_spin(key).is_err() {
                continue;
            }
            let links_std = nros_orchestration_ir::board_entry_links_std(key)
                .expect("a key from the table is in the table");
            let mut plan = fixture_plan(&[("talker_pkg", "talker")]);
            plan.board = key.into();
            let src = emit(&plan).unwrap_or_else(|e| panic!("`{key}`: {e}"));
            checked += 1;

            assert_eq!(
                src.contains("::std::"),
                links_std,
                "`{key}`: links_std = {links_std}, but the emitted entry {} a \
                 `std` path. `builder::entry` writes `#![no_std]` on top of a \
                 board-run / zephyr-staticlib entry TU, and no `target_os` cfg \
                 saves a `std` path there:\n{src}",
                if links_std { "omits" } else { "names" }
            );
            // The hosted `fn main()` is the ONLY thing that moves. Both
            // C-ABI arms are unconditional, exactly as the proc-macro emits
            // them — a board with no main at all is the hole this test also
            // has to see.
            assert!(
                src.contains("#[cfg(target_os = \"nuttx\")]")
                    && src.contains("#[cfg(target_os = \"none\")]"),
                "`{key}`: an entry with no embedded `main` arm:\n{src}"
            );
            if links_std {
                hosted += 1;
                assert!(
                    src.contains("fn main() {\n    if let ::core::result::Result::Err(e)"),
                    "`{key}`: links_std, but no hosted `fn main()`:\n{src}"
                );
            }
        }
        // Both arms must be exercised, or this passes having compared one.
        assert!(checked > 2, "only {checked} board key(s) rendered");
        assert!(
            hosted > 0,
            "no board key links std — the true arm is untested"
        );
        assert!(
            hosted < checked,
            "every board key links std — the false arm, which is issue 1409, \
             is untested"
        );
    }

    #[test]
    fn dash_pkg_names_are_sanitised() {
        let plan = fixture_plan(&[("talker-pkg", "talker")]);
        let src = emit(&plan).expect("a board key the Rust pack knows");
        assert!(src.contains("::talker_pkg::register(runtime)?;"));
    }

    #[test]
    fn freertos_board_maps_to_correct_zst() {
        let mut plan = fixture_plan(&[("talker_pkg", "talker")]);
        plan.board = "freertos".into();
        let src = emit(&plan).expect("a board key the Rust pack knows");
        assert!(src.contains("::nros_board_mps2_an385_freertos::Mps2An385"));
    }

    /// Issue 1285 follow-up — a key with no Rust board ZST is refused, naming
    /// the keys the Rust pack knows. It used to render `LinuxBoard`. The keys
    /// here are a C-only board (`freertos-posix`, which the parity corpus used
    /// to name), a key the family table knows but the Rust pack does not
    /// (`s32z270`), and one it has never heard of.
    #[test]
    fn a_board_with_no_rust_zst_is_refused_not_rendered_as_linux() {
        for board in ["freertos-posix", "s32z270", "zigos"] {
            let mut plan = fixture_plan(&[("talker_pkg", "talker")]);
            plan.board = board.into();
            let err = emit(&plan).expect_err(board);
            assert!(err.contains(&format!("`{board}`")), "{err}");
            for known in ["native", "mps2-an385-freertos", "zephyr"] {
                assert!(
                    err.contains(known),
                    "`{board}`: message omits `{known}`: {err}"
                );
            }
        }
    }

    /// Issue 1435 — every board key either renders the `owned-spin` shape, or
    /// is refused.
    ///
    /// Asserted per KEY over the whole table, not on a chosen example, and
    /// keyed on the framework SSoT rather than on a list of board names: a
    /// list is what `nros-macros`'
    /// `in_tree_board_keys_resolve_to_an_emit_shape` already is, it names ten
    /// of twenty-one keys, and the key that was wrong
    /// (`native_sim/native/64`) is not among them.
    ///
    /// The rendering side is checked too, not just the boolean: a refactor
    /// that keeps the refusal and renders something other than
    /// `BoardEntry::run` for an accepted key fails here.
    #[test]
    fn a_board_wanting_another_framework_is_refused_not_rendered_as_owned_spin() {
        let mut rendered = 0usize;
        let mut refused: Vec<&str> = Vec::new();
        for key in nros_orchestration_ir::board_path_keys() {
            let framework =
                nros_orchestration_ir::framework_for_board_key(key).unwrap_or("owned-spin");
            let mut plan = fixture_plan(&[("talker_pkg", "talker")]);
            plan.board = key.into();
            match emit(&plan) {
                Ok(src) => {
                    assert_eq!(
                        framework, "owned-spin",
                        "`{key}` wants the `{framework}` entry shape and was \
                         RENDERED anyway. This template emits \
                         `<Board as BoardEntry>::run`, which is `owned-spin`; \
                         for any other framework the board ZST either does not \
                         implement `BoardEntry` (a compile error minutes later) \
                         or does and is never called (issue 1435):\n{src}"
                    );
                    // …and it really is that shape.
                    let zst = nros_orchestration_ir::board_path_for(key).expect("a known key");
                    assert!(
                        src.contains(&format!("<{zst} as ")) && src.contains("BoardEntry>::run("),
                        "`{key}`: accepted, but did not render the \
                         `BoardEntry::run` shape the refusal is defined \
                         against:\n{src}"
                    );
                    rendered += 1;
                }
                Err(e) => {
                    assert_ne!(
                        framework, "owned-spin",
                        "`{key}` wants the shape this template renders and was \
                         refused: {e}"
                    );
                    assert!(
                        e.contains(&format!("`{key}`")) && e.contains(&format!("`{framework}`")),
                        "`{key}`: the refusal must name the board AND the shape \
                         it wanted, or the reader cannot tell it from the \
                         unknown-board refusal: {e}"
                    );
                    assert!(
                        e.contains("nros::main!"),
                        "`{key}`: the refusal must point at the producer that \
                         CAN emit this board: {e}"
                    );
                    refused.push(key);
                }
            }
        }
        // Both arms must be exercised, or this passes having compared one.
        assert!(rendered >= 10, "only {rendered} board key(s) rendered");
        // In `BOARD_PATHS` order. Pinned rather than counted: this set is the
        // whole behaviour change of issue 1435, and a key leaving it silently
        // is how a rendering nobody can compile comes back.
        assert_eq!(
            refused,
            vec![
                "esp32-qemu",
                "esp32-c3-baremetal",
                "zephyr",
                "native_sim/native/64",
                "rtic-mps2-an385",
                "qemu-rtic-mps2-an385",
            ],
            "the refused set moved — read issue 1435 before editing this list"
        );
    }

    /// The ESP32 half of the same refusal, spelled out because it is the one
    /// the reader will want to argue with: `Esp32QemuEntry` DOES implement
    /// `BoardEntry`, so this rendering compiled. It could not boot —
    /// esp-riscv-rt's `_start` reaches the esp-hal entry registration, not a
    /// bare `extern "C" fn main` — and a rendering that compiles and does not
    /// run is the worse of the two failures to ship.
    #[test]
    fn esp32_is_refused_although_its_zst_does_implement_board_entry() {
        for key in ["esp32-qemu", "esp32-c3-baremetal"] {
            let mut plan = fixture_plan(&[("talker_pkg", "talker")]);
            plan.board = key.into();
            let err = emit(&plan).expect_err(key);
            assert!(err.contains("`esp32`"), "{err}");
        }
    }

    #[test]
    fn quote_str_handles_simple_paths() {
        let q = quote_str("/abs/path.xml");
        assert!(q.starts_with("r#\""));
        assert!(q.ends_with("\"#"));
        assert!(q.contains("/abs/path.xml"));
    }
}
