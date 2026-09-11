//! Phase 212.L / O — workspace-walk lints for `nros check --workspace`.
//!
//! Lints land here:
//!
//! * **L.4 (amended by RFC-0057) — qualified-class enforcement.** Every
//!   `[[component]]` row carries an explicit `pkg` (the authority) and a
//!   `class` that must be a namespace-qualified C++ name. The pre-0057
//!   `<pkg>::` prefix requirement is retired — upstream namespaces port
//!   verbatim.
//!
//! * **L.8 — `system.toml` outside bringup is forbidden.** `system.toml` is a
//!   bringup-pkg-only file. A component pkg (carries `Cargo.toml` or
//!   `CMakeLists.txt`) with a stray `system.toml` next to it is rejected.
//!
//! * **L.11 — per-pkg `.cargo/config.toml` with `[patch.crates-io]` is a
//!   warning.** Cargo reads `[patch.crates-io]` from both `Cargo.toml` AND
//!   `.cargo/config.toml`; when both exist the config-file shadows the
//!   manifest. Patches must live in the workspace-root `Cargo.toml` only.
//!
//! * **O.2 `entry-deploy-missing` (hard error).** A component pkg whose
//!   `Cargo.toml` declares `[package.metadata.nros.entry]` MUST also set
//!   `deploy = "<board>"` (per Phase 212.L.2 / N.7). A missing or empty
//!   `deploy` field rejects with the `entry-deploy-missing` diagnostic.
//!
//! * **O.6 `application-rtos-deploy-forbidden` (hard error).** A component
//!   pkg whose `Cargo.toml` declares `[package.metadata.nros.application]`
//!   MUST only name `"native"` in its `deploy = […]` allow-list. Application
//!   pkgs are native-only by definition (Phase 212.L.2 / M-F.1); naming an
//!   RTOS rejects with the `application-rtos-deploy-forbidden` diagnostic.
//!
//! * **Phase 216.D.1 `dispatch-mismatch-<framework>-<strategy>` (hard
//!   error).** Cross-validates the `(framework, DispatchStrategy)` matrix
//!   per the locked 216.D.1 spec. For every Entry pkg in the workspace, the
//!   walk resolves its board (path-dep carrying
//!   `[package.metadata.nros.board] framework = …`) and every Node pkg in
//!   its path-dep closure (carrying `[package.metadata.nros.node]
//!   dispatch = …`). Rejected cells:
//!
//!   | framework | Inline | Deferred | FromIsr |
//!   |-----------|--------|----------|---------|
//!   | OwnedSpin | ok     | ok       | reject  |
//!   | Rtic      | reject | ok       | ok      |
//!   | Embassy   | reject | ok       | reject  |
//!
//!   Missing `[package.metadata.nros.node] dispatch = …` defaults to
//!   Inline (matches the `Node::DISPATCH` trait const default). Missing
//!   board metadata defaults to OwnedSpin (POSIX/RTOS, the
//!   `BoardEntry::run` direct-exec path). Static-only check — does NOT
//!   read the runtime ABI symbol `__nros_node_<pkg>_dispatch_strategy`
//!   the `nros::node!()` macro emits, because that would require linking
//!   the Node crate.
//!
//! The walk is `nros check --workspace [<dir>]`. Each recursively indexed
//! `package.xml` directory is classified as a bringup pkg (has `system.toml`,
//! no `Cargo.toml` / `CMakeLists.txt` / `src/`) or a component pkg (has
//! `Cargo.toml` or `CMakeLists.txt`). Other dirs are skipped.

use std::{collections::BTreeSet, fs, path::Path};

use eyre::{Result, WrapErr, bail};

use crate::orchestration::cargo_metadata_schema::SystemToml;

/// Result of one workspace walk. Hard-error lints bail through `Result`;
/// warnings flow back as a list so the caller can stamp the final
/// "ok (N warning(s))" summary.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct WorkspaceLintReport {
    /// Number of pkg dirs visited (any kind).
    pub pkgs_visited: usize,
    /// Soft warnings collected during the walk (L.11 today).
    pub warnings: Vec<String>,
}

/// Walk `workspace_root` and run the L.4 / L.8 / L.11 lints.
///
/// Hard-error lints (L.4 / L.8) bail with `eyre::Error` carrying a diagnostic
/// that names the offending dir + the rule. Warnings (L.11) accumulate in
/// the returned report.
pub fn check_workspace(workspace_root: &Path) -> Result<WorkspaceLintReport> {
    let mut report = WorkspaceLintReport::default();

    let index = crate::pkg_index::build_pkg_index(workspace_root)
        .wrap_err_with(|| format!("build package index under {}", workspace_root.display()))?;
    let mut dirs: Vec<(String, std::path::PathBuf)> = index
        .pkgs()
        .map(|(name, path)| (name.to_string(), path.to_path_buf()))
        .collect();
    let mut seen: BTreeSet<std::path::PathBuf> =
        dirs.iter().map(|(_, path)| path.clone()).collect();
    let entries = fs::read_dir(workspace_root)
        .wrap_err_with(|| format!("read {}", workspace_root.display()))?;
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() || !seen.insert(path.clone()) {
            continue;
        }
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        dirs.push((name.to_string(), path));
    }
    // Deterministic order so diagnostics + warnings are reproducible.
    dirs.sort_by(|a, b| a.0.cmp(&b.0));

    for (name, pkg_dir) in dirs {
        // Skip dotted dirs (.git, .cargo, .claude, …) and build output dirs.
        // `is_build_output_dir` prefix-matches, so the suffixed variants this
        // exact comparison used to miss (`target-xrce`, `build-zenoh`) go too.
        if name.starts_with('.') || crate::build_output::is_build_output_dir(&name) {
            continue;
        }

        let has_cargo = pkg_dir.join("Cargo.toml").is_file();
        let has_cmake = pkg_dir.join("CMakeLists.txt").is_file();
        let has_system = pkg_dir.join("system.toml").is_file();
        let has_package_xml = pkg_dir.join("package.xml").is_file();
        let has_src = pkg_dir.join("src").is_dir();

        // Phase 212.F.2 — bringup detection is `package.xml + system.toml`
        // presence, regardless of forbidden surfaces. A dir that LOOKS like
        // a bringup but carries Cargo.toml / CMakeLists.txt / src/ must
        // still be classified as a bringup so the pure-declarative lint can
        // surface the offence (rather than mis-classifying it as a
        // component and treating the system.toml as "stray").
        let is_bringup = has_system && has_package_xml;
        // Component shape: has Cargo.toml or CMakeLists.txt AND is NOT a
        // bringup (the bringup classification wins when both are present so
        // the F.2 forbidden-file lint catches the contamination).
        let is_component = (has_cargo || has_cmake) && !is_bringup;

        if !is_component && !is_bringup {
            continue; // Not an nros-managed pkg dir — skip.
        }
        report.pkgs_visited += 1;

        if is_component {
            // L.8 — component pkg with stray `system.toml`.
            if has_system {
                bail!(
                    "pkg {name}: stray system.toml next to {} — `system.toml` \
                     lives ONLY in a Path A bringup pkg (no Cargo.toml / \
                     CMakeLists.txt / src/); move it into a sibling \
                     <system>_bringup/ dir (see \
                     docs/design/0024-multi-node-workspace-layout.md §4)",
                    if has_cargo {
                        "Cargo.toml"
                    } else {
                        "CMakeLists.txt"
                    }
                );
            }
            // L.11 — per-pkg `.cargo/config.toml` shadowing patch.
            let cargo_cfg = pkg_dir.join(".cargo/config.toml");
            if cargo_cfg.is_file() {
                let body = fs::read_to_string(&cargo_cfg).unwrap_or_default();
                if has_patch_crates_io(&body) {
                    report.warnings.push(format!(
                        "pkg {name}: .cargo/config.toml carries \
                         [patch.crates-io] — cargo reads patches from BOTH \
                         Cargo.toml AND .cargo/config.toml and the config \
                         file shadows the manifest; move the block to the \
                         workspace-root Cargo.toml (auto-managed by \
                         `nros sync`)"
                    ));
                }
            }
            // O.2 + O.6 — peek inside Cargo.toml's `[package.metadata.nros]`
            // for the Entry-pkg / Application-pkg shape lints.
            if has_cargo {
                lint_cargo_metadata_nros(&pkg_dir.join("Cargo.toml"), &name)?;
                // Phase 216.D.1 — framework × DispatchStrategy matrix.
                // Runs only on Entry pkgs (presence of
                // `[package.metadata.nros.entry]`); resolves the board's
                // framework via path-dep walk and every Node pkg's
                // dispatch.
                lint_dispatch_matrix(&pkg_dir, &name)?;
            }
        }

        if is_bringup {
            // Phase 212.F.2 — run the pure-declarative surface lint on
            // every bringup pkg discovered by the workspace walk. Catches
            // Cargo.toml / CMakeLists.txt / src/ / include/ / lib/ +
            // nested `add_executable(`. The L.4 `<pkg>::<Class>` lint runs
            // unconditionally below; `<exec_depend>` drift is intentionally
            // NOT in the workspace-walk pass (only on `--bringup <dir>`)
            // because pre-spec fixtures stamp shells without the
            // `<exec_depend>` rows and we don't want to regress them.
            let _ = has_src;
            crate::cmd::bringup::lint_bringup_forbidden_surfaces(&pkg_dir)?;
            // L.4 — class prefix matches pkg.
            lint_class_pkg_prefix(&pkg_dir, &name)?;
        }
    }

    Ok(report)
}

/// L.4 helper. Read `<bringup>/system.toml`, verify each `[[component]]`
/// row's `class` is `<pkg>::<Type>`-shaped. Public so `lint_bringup` can
/// reuse it on the `--bringup <dir>` flow.
pub fn lint_class_pkg_prefix(bringup_dir: &Path, bringup_pkg_name: &str) -> Result<()> {
    let system_toml = bringup_dir.join("system.toml");
    if !system_toml.is_file() {
        return Ok(());
    }
    let raw = fs::read_to_string(&system_toml)
        .wrap_err_with(|| format!("read {}", system_toml.display()))?;
    let parsed: SystemToml =
        toml::from_str(&raw).wrap_err_with(|| format!("parse {}", system_toml.display()))?;
    // RFC-0057 (phase-305): the L.4 prefix rule is retired — `pkg` on the
    // `[[component]]` row is the authority, and `class` may be any
    // namespace-qualified C++ name (verbatim upstream namespaces). The lint
    // now only rejects an UNQUALIFIED class, which cannot name a real type
    // for the entry codegen.
    let mut bad: Vec<String> = Vec::new();
    for c in &parsed.components {
        if !c.class.contains("::") {
            bad.push(format!(
                "[[component]] name=\"{}\" pkg=\"{}\" class=\"{}\" — class \
                 must be a namespace-qualified C++ name (`ns::Type`)",
                c.name, c.pkg, c.class
            ));
        }
    }
    if !bad.is_empty() {
        bail!(
            "bringup pkg {bringup_pkg_name}: system.toml component class \
             invalid — {}.",
            bad.join("; ")
        );
    }
    Ok(())
}

/// Phase 212.O.2 + O.6 — peek inside a component pkg's `Cargo.toml`
/// `[package.metadata.nros]` table and run the Entry / Application shape
/// lints.
///
/// * **O.2 `entry-deploy-retired`** — a manifest must NOT carry
///   `[package.metadata.nros.entry] deploy` or `[package.metadata.nros.deploy.*]`
///   (RFC-0098 D5, phase-445 W5). Until W5 this lint REQUIRED the key
///   (`entry-deploy-missing`, phase 212.L.2 / N.7); the board now lives in a
///   `system.toml` image, and nothing reads the key.
/// * **O.6 `application-rtos-deploy-forbidden`** — every entry in
///   `[package.metadata.nros.application].deploy` MUST be `"native"`. Phase
///   212.L.2 / M-F.1 — Application pkgs are native-only orchestration roots.
///
/// Both lints bail with an eyre error whose diagnostic id is embedded in the
/// message body (`entry-deploy-retired` / `application-rtos-deploy-forbidden`).
/// Diagnostic IDs are part of the stable contract used by `nros check`
/// integration tests + downstream tooling.
fn lint_cargo_metadata_nros(cargo_toml_path: &Path, pkg_name: &str) -> Result<()> {
    let raw = match fs::read_to_string(cargo_toml_path) {
        Ok(s) => s,
        // Unreadable Cargo.toml is not this lint's problem — bail out silently
        // (the rest of cargo / nros plan will surface a cleaner error).
        Err(_) => return Ok(()),
    };
    let value: toml::Value = match toml::from_str(&raw) {
        Ok(v) => v,
        Err(_) => return Ok(()), // malformed manifest — not this lint's job
    };

    let nros = value
        .get("package")
        .and_then(|p| p.get("metadata"))
        .and_then(|m| m.get("nros"));
    let Some(nros) = nros else {
        return Ok(());
    };

    // ---- O.2 entry-deploy-retired (phase-445 W5) -----------------------------
    //
    // This lint used to REQUIRE `[package.metadata.nros.entry] deploy`
    // (`entry-deploy-missing`). RFC-0098 D5 retired the key: an entry's board is
    // its `system.toml` image (a single-package example) or the workspace
    // bringup's image that builds it (`leaf_system::for_entry`), and the reader
    // that understood the key is deleted. So the same shape check now points the
    // other way — the key's PRESENCE is the defect, because nothing reads it.
    // Same key list as the reader's refusal, so the two cannot disagree.
    let retired = nros
        .as_table()
        .map(nros_orchestration_ir::leaf_system::retired_deployment_keys)
        .unwrap_or_default();
    if !retired.is_empty() {
        bail!(
            "{pkg_name}: {} — retired (RFC-0098 D5, phase-445 W5): nothing reads it. \
             State the board as `[image.<id>] board = \"<board>\"` in a `system.toml` \
             beside the package (a single-package example), or in the workspace \
             bringup's image that builds this entry (`entry = \"{pkg_name}\"`), and \
             delete the key. [diagnostic: entry-deploy-retired]",
            retired.join(", ")
        );
    }

    // ---- O.6 application-rtos-deploy-forbidden ----------------------------
    if let Some(app) = nros.get("application") {
        // `deploy` is optional; when present it must be a list of strings,
        // and every entry must be exactly "native".
        if let Some(deploy) = app.get("deploy") {
            let Some(arr) = deploy.as_array() else {
                // Wrong type — leave to the strict schema validator; this
                // lint only flags the RTOS-name policy.
                return Ok(());
            };
            for entry in arr {
                let Some(s) = entry.as_str() else {
                    continue;
                };
                if s != "native" {
                    bail!(
                        "{pkg_name}: Application pkg may not deploy to RTOS \
                         target '{s}' (Applications are native-only; use \
                         Component pkg + Entry pkg for RTOS deployment) \
                         [diagnostic: application-rtos-deploy-forbidden]"
                    );
                }
            }
        }
    }

    Ok(())
}

/// Cheap substring scan for `[patch.crates-io]` in a cargo config body.
/// Avoids a full TOML parse so malformed user configs still flag.
fn has_patch_crates_io(body: &str) -> bool {
    for line in body.lines() {
        let t = line.trim();
        if t.starts_with('#') {
            continue;
        }
        // Accept `[patch.crates-io]` exactly. Be tolerant of trailing comments
        // and whitespace; reject `[patch.crates-io.foo]` (that's a dependency
        // override entry, not the table header — but in practice users hit
        // both with the same shadowing risk, so flag anything starting with
        // the patch.crates-io path).
        if t.starts_with("[patch.crates-io]") || t.starts_with("[patch.crates-io.") {
            return true;
        }
    }
    false
}

// ---------------------------------------------------------------------------
// Phase 216.D.1 — framework × DispatchStrategy matrix
// ---------------------------------------------------------------------------

/// Locked 216.D.1 framework axis. Boards without
/// `[package.metadata.nros.board] framework = …` default to `OwnedSpin`
/// (the POSIX / RTOS `BoardEntry::run` direct-exec path — no framework
/// runtime owns the spin loop).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Framework {
    OwnedSpin,
    Rtic,
    Embassy,
}

impl Framework {
    fn label(self) -> &'static str {
        match self {
            Framework::OwnedSpin => "owned-spin",
            Framework::Rtic => "rtic",
            Framework::Embassy => "embassy",
        }
    }
}

/// Locked 216.D.1 dispatch axis. Mirrors `nros_platform::DispatchStrategy`
/// but lives here so `nros check` stays a static tool (no link against the
/// runtime crate). Node pkgs missing the metadata key default to `Inline`
/// to match the `Node::DISPATCH` trait const default.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DispatchStrategy {
    Inline,
    Deferred,
    FromIsr,
}

impl DispatchStrategy {
    fn label(self) -> &'static str {
        match self {
            DispatchStrategy::Inline => "inline",
            DispatchStrategy::Deferred => "deferred",
            DispatchStrategy::FromIsr => "from-isr",
        }
    }
}

/// Apply the locked 216.D.1 matrix. Returns `Some(suggested_fix)` when the
/// pair is rejected; `None` when the pair is allowed.
fn matrix_reject_reason(fw: Framework, ds: DispatchStrategy) -> Option<&'static str> {
    match (fw, ds) {
        // OwnedSpin (POSIX / RTOS): Inline + Deferred ok; FromIsr rejected
        // (no ISR fast-path on a hosted RTOS / POSIX runtime).
        (Framework::OwnedSpin, DispatchStrategy::FromIsr) => Some(
            "FromIsr is not supported on POSIX / RTOS (owned-spin) boards; \
             change DISPATCH to Inline or Deferred, or deploy on a bare-metal \
             framework board (rtic) that wires the ISR fast-path",
        ),
        // RTIC: Inline rejected (RTIC requires a Deferred hand-off to an
        // `rtic::Mutex`-guarded task — running a callback inline on the
        // spin task breaks the framework's priority model); Deferred ok;
        // FromIsr ok (future — ISR-driven cell).
        (Framework::Rtic, DispatchStrategy::Inline) => Some(
            "RTIC requires Deferred dispatch (callbacks hand off to a \
             framework-owned `#[task]` via an `rtic::Mutex`-guarded SPSC \
             ring); set [package.metadata.nros.node] dispatch = \"deferred\" \
             on the Node pkg",
        ),
        // Embassy: Inline rejected (same reason — Embassy needs a Spawner
        // hand-off); Deferred ok; FromIsr rejected (Embassy's executor
        // runs in thread mode — no ISR fast-path).
        (Framework::Embassy, DispatchStrategy::Inline) => Some(
            "Embassy requires Deferred dispatch (callbacks hand off to a \
             framework-owned `async` task via the `embassy_executor::Spawner` \
             escape); set [package.metadata.nros.node] dispatch = \"deferred\" \
             on the Node pkg",
        ),
        (Framework::Embassy, DispatchStrategy::FromIsr) => Some(
            "FromIsr is not supported on Embassy (the executor runs in \
             thread mode — no ISR fast-path); change DISPATCH to Deferred, \
             or deploy on an RTIC board",
        ),
        // All other (fw, ds) cells are allowed.
        _ => None,
    }
}

/// Parse `[package.metadata.nros.board] framework = "<rtic|embassy>"`.
/// Missing table or unknown framework string → `Framework::OwnedSpin`
/// (the matrix's default cell for POSIX / RTOS boards w/o framework
/// metadata).
fn read_board_framework(board_cargo_toml: &Path) -> Framework {
    let Ok(raw) = fs::read_to_string(board_cargo_toml) else {
        return Framework::OwnedSpin;
    };
    let Ok(value) = toml::from_str::<toml::Value>(&raw) else {
        return Framework::OwnedSpin;
    };
    let fw = value
        .get("package")
        .and_then(|p| p.get("metadata"))
        .and_then(|m| m.get("nros"))
        .and_then(|n| n.get("board"))
        .and_then(|b| b.get("framework"))
        .and_then(|v| v.as_str());
    match fw {
        Some("rtic") => Framework::Rtic,
        Some("embassy") => Framework::Embassy,
        // Issue 0415 — a framework this lint has no arm for is NOT silently
        // owned-spin. `zephyr` and `esp32` are known repo-wide (the proc-macro
        // emits for them) but carry no dispatch-matrix rule here, so they read
        // as owned-spin for THIS lint's purposes; anything outside the shared
        // set is a typo and must be reported rather than absorbed.
        Some(other) if !nros_orchestration_ir::is_known_framework(other) => {
            eprintln!(
                "warning: {}: `[package.metadata.nros.board] framework = \"{other}\"` is not a \
                 known framework ({}). `nros::main!` will refuse to expand for this board. \
                 [diagnostic: unknown-board-framework]",
                board_cargo_toml.display(),
                nros_orchestration_ir::FRAMEWORKS.join(", ")
            );
            Framework::OwnedSpin
        }
        _ => Framework::OwnedSpin,
    }
}

/// Parse a Node pkg's dispatch strategy, `"<inline|deferred|from_isr>"`.
/// Missing key → `Inline` (matches `Node::DISPATCH` default). Returns
/// `Err` on an unknown string so the lint surfaces a typo instead of
/// silently defaulting.
///
/// Two spellings, one answer (issue 1278, phase-445 W3b): a package that
/// states its node in a `system.toml` beside its manifest (RFC-0098 D3/D8)
/// carries the strategy as `[[component]] dispatch`, read through
/// `nros_orchestration_ir::leaf_system` — the one reader; a manifest still on
/// `[package.metadata.nros.node] dispatch` is read as before. A pkg with
/// neither is not a Node pkg.
fn read_node_dispatch_strategy(node_cargo_toml: &Path) -> Result<Option<DispatchStrategy>> {
    let pkg_dir = node_cargo_toml.parent().unwrap_or(Path::new("."));
    let (ds_str, origin, key): (Option<String>, &Path, &str) = if pkg_dir
        .join(nros_orchestration_ir::leaf_system::SYSTEM_TOML)
        .is_file()
    {
        let Some(leaf) =
            nros_orchestration_ir::leaf_system::read(pkg_dir).map_err(|e| eyre::eyre!(e))?
        else {
            return Ok(None);
        };
        if leaf.components.is_empty() {
            return Ok(None);
        }
        // A leaf declares ONE node here; with several, the first stated
        // strategy stands for the package (the manifest could only ever
        // spell one).
        let ds = leaf.components.iter().find_map(|c| c.dispatch.clone());
        (ds, pkg_dir, "system.toml [[component]] dispatch")
    } else {
        let Ok(raw) = fs::read_to_string(node_cargo_toml) else {
            return Ok(None);
        };
        let Ok(value) = toml::from_str::<toml::Value>(&raw) else {
            return Ok(None);
        };
        // Only treat this pkg as a Node pkg if it carries
        // `[package.metadata.nros.node]`.
        let node_tbl = value
            .get("package")
            .and_then(|p| p.get("metadata"))
            .and_then(|m| m.get("nros"))
            .and_then(|n| n.get("node"));
        let Some(node_tbl) = node_tbl else {
            return Ok(None);
        };
        let ds = node_tbl
            .get("dispatch")
            .and_then(|v| v.as_str())
            .map(str::to_string);
        (ds, node_cargo_toml, "[package.metadata.nros.node] dispatch")
    };
    let Some(ds_str) = ds_str else {
        // Node pkg w/o explicit dispatch → mirror the trait const default.
        return Ok(Some(DispatchStrategy::Inline));
    };
    match ds_str.as_str() {
        "inline" => Ok(Some(DispatchStrategy::Inline)),
        "deferred" => Ok(Some(DispatchStrategy::Deferred)),
        // Accept both kebab + snake for the ISR variant; mirror the
        // platform enum's Rust name (`FromIsr`).
        "from_isr" | "from-isr" => Ok(Some(DispatchStrategy::FromIsr)),
        other => bail!(
            "{}: {key} = \"{}\" — unknown \
             dispatch strategy (expected one of: \"inline\", \"deferred\", \
             \"from_isr\")",
            origin.display(),
            other
        ),
    }
}

/// Walk an Entry pkg's `[dependencies]` + `[dev-dependencies]` tables
/// and collect every `path = "<rel>"` dep target. Returned paths are
/// resolved against the Entry pkg's dir + canonicalised. Only path-deps
/// are considered (registry / git deps have no in-tree manifest to read).
fn collect_path_deps(entry_cargo_toml: &Path, entry_dir: &Path) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    let Ok(raw) = fs::read_to_string(entry_cargo_toml) else {
        return out;
    };
    let Ok(value) = toml::from_str::<toml::Value>(&raw) else {
        return out;
    };
    for tbl_name in ["dependencies", "dev-dependencies", "build-dependencies"] {
        let Some(tbl) = value.get(tbl_name).and_then(|v| v.as_table()) else {
            continue;
        };
        for (_dep_name, dep_spec) in tbl {
            let Some(dep_tbl) = dep_spec.as_table() else {
                continue;
            };
            let Some(rel) = dep_tbl.get("path").and_then(|v| v.as_str()) else {
                continue;
            };
            let abs = entry_dir.join(rel);
            // `canonicalize` resolves `..` segments and follows symlinks.
            // Fall back to the joined path if canonicalisation fails (e.g.
            // dep dir doesn't exist — a separate lint will catch that).
            let resolved = fs::canonicalize(&abs).unwrap_or(abs);
            out.push(resolved);
        }
    }
    out
}

/// Phase 216.D.1 — framework × DispatchStrategy matrix entry point.
///
/// Triggered for every component pkg whose `Cargo.toml` carries
/// `[package.metadata.nros.entry]` (= an Entry pkg). Walks the Entry pkg's
/// path-deps, classifies each as `board` (carries `[package.metadata.
/// nros.board]`) or `node` (carries `[package.metadata.nros.node]`), then
/// runs the cross-validation matrix.
///
/// Bail conditions:
/// * Two boards in the path-dep set carry conflicting `framework` values
///   (operator error; the Entry pkg is multi-targeted in a way the matrix
///   can't reason about).
/// * A `(framework, dispatch)` pair lands in a rejected cell — diagnostic
///   ID `dispatch-mismatch-<framework>-<strategy>`.
///
/// Silent on non-Entry pkgs and on Entry pkgs with no Node-pkg path-deps
/// (e.g. a board-bringup Entry that defers all callbacks to the framework
/// runtime without a user Node pkg).
fn lint_dispatch_matrix(entry_dir: &Path, entry_pkg_name: &str) -> Result<()> {
    let cargo_toml = entry_dir.join("Cargo.toml");
    let Ok(raw) = fs::read_to_string(&cargo_toml) else {
        return Ok(());
    };
    let Ok(value) = toml::from_str::<toml::Value>(&raw) else {
        return Ok(());
    };
    // Only run on Entry pkgs.
    let is_entry = value
        .get("package")
        .and_then(|p| p.get("metadata"))
        .and_then(|m| m.get("nros"))
        .and_then(|n| n.get("entry"))
        .is_some();
    if !is_entry {
        return Ok(());
    }

    // Walk path-deps and bucket into boards / nodes.
    let mut framework: Option<Framework> = None;
    let mut framework_source: Option<std::path::PathBuf> = None;
    // (node_pkg_dir, dispatch_strategy).
    let mut nodes: Vec<(std::path::PathBuf, DispatchStrategy)> = Vec::new();

    for dep_dir in collect_path_deps(&cargo_toml, entry_dir) {
        let dep_cargo = dep_dir.join("Cargo.toml");
        if !dep_cargo.is_file() {
            continue;
        }
        let dep_raw = match fs::read_to_string(&dep_cargo) {
            Ok(s) => s,
            Err(_) => continue,
        };
        let dep_value: toml::Value = match toml::from_str(&dep_raw) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let nros_meta = dep_value
            .get("package")
            .and_then(|p| p.get("metadata"))
            .and_then(|m| m.get("nros"));
        let Some(nros_meta) = nros_meta else {
            continue;
        };

        // Board classification.
        if nros_meta.get("board").is_some() {
            let fw = read_board_framework(&dep_cargo);
            if let Some(prev) = framework {
                if prev != fw {
                    bail!(
                        "{entry_pkg_name}: conflicting board frameworks in \
                         path-dep set — {} declared framework='{}', {} declared \
                         framework='{}'. An Entry pkg may depend on at most one \
                         board crate carrying `[package.metadata.nros.board]`. \
                         [diagnostic: dispatch-mismatch-conflicting-boards]",
                        framework_source
                            .as_ref()
                            .map(|p| p.display().to_string())
                            .unwrap_or_else(|| "<unknown>".into()),
                        prev.label(),
                        dep_cargo.display(),
                        fw.label(),
                    );
                }
            } else {
                framework = Some(fw);
                framework_source = Some(dep_cargo.clone());
            }
        }

        // Node classification (a pkg MAY be both a board + a node, but in
        // practice no in-tree crate is — the two tables are mutually
        // exclusive by convention).
        if let Some(ds) = read_node_dispatch_strategy(&dep_cargo)? {
            nodes.push((dep_dir, ds));
        }
    }

    // No board metadata → default to OwnedSpin (POSIX / RTOS / native).
    let framework = framework.unwrap_or(Framework::OwnedSpin);

    // Apply the matrix per (framework, node).
    for (node_dir, ds) in nodes {
        if let Some(fix) = matrix_reject_reason(framework, ds) {
            let node_label = node_dir
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("<unknown>");
            bail!(
                "{entry_pkg_name}: framework × dispatch matrix rejects \
                 ({fw} × {ds}) — Node pkg '{node_label}' declares \
                 dispatch=\"{ds}\" but Entry pkg's board carries \
                 framework=\"{fw}\". Fix: {fix} \
                 [diagnostic: dispatch-mismatch-{fw}-{ds}]",
                fw = framework.label(),
                ds = ds.label(),
                node_label = node_label,
                fix = fix,
            );
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn temp_root(tag: &str) -> PathBuf {
        crate::test_support::scratch_dir(&format!("nros-check-ws-{tag}"))
    }

    fn write_component_pkg(dir: &Path, name: &str) {
        fs::create_dir_all(dir.join("src")).unwrap();
        fs::write(
            dir.join("Cargo.toml"),
            format!("[package]\nname=\"{name}\"\nversion=\"0.1.0\"\n"),
        )
        .unwrap();
        fs::write(dir.join("src/lib.rs"), "// stub\n").unwrap();
    }

    fn write_bringup_with_components(dir: &Path, components: &[(&str, &str, &str)]) {
        fs::create_dir_all(dir).unwrap();
        fs::write(
            dir.join("package.xml"),
            "<?xml version=\"1.0\"?><package format=\"3\">\
             <name>demo_bringup</name><version>0.1.0</version></package>",
        )
        .unwrap();
        let mut s = String::from("[system]\nname = \"demo\"\nrmw = \"zenoh\"\ndomain_id = 0\n");
        for (pkg, class, cname) in components {
            s.push_str(&format!(
                "\n[[component]]\npkg = \"{pkg}\"\nclass = \"{class}\"\nname = \"{cname}\"\n"
            ));
        }
        fs::write(dir.join("system.toml"), s).unwrap();
    }

    #[test]
    fn nros_check_rejects_unqualified_class() {
        // RFC-0057: the prefix rule is retired; what remains fatal is a class
        // that names no namespace at all (the entry codegen can't construct it).
        let root = temp_root("class_unqualified");
        let bringup = root.join("demo_bringup");
        write_bringup_with_components(&bringup, &[("talker_pkg", "Talker", "talker")]);
        let err = check_workspace(&root).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("class invalid"), "diag: {msg}");
        assert!(msg.contains("namespace-qualified"), "diag: {msg}");
    }

    #[test]
    fn nros_check_accepts_nested_upstream_namespaces() {
        // RFC-0057 acceptance: verbatim upstream namespaces (`autoware::x::Y`)
        // pass — pkg on the row is the authority, not the class prefix.
        let root = temp_root("class_ok");
        let bringup = root.join("demo_bringup");
        write_bringup_with_components(
            &bringup,
            &[
                ("talker_pkg", "talker_pkg::Talker", "talker"),
                (
                    "autoware_mrm_emergency_stop_operator",
                    "autoware::mrm_emergency_stop_operator::MrmEmergencyStopOperator",
                    "mrm_emergency_stop_operator",
                ),
            ],
        );
        let report = check_workspace(&root).expect("clean workspace passes");
        assert_eq!(report.pkgs_visited, 1);
        assert!(report.warnings.is_empty());
    }

    #[test]
    fn nros_check_rejects_system_toml_in_component_pkg() {
        let root = temp_root("stray_system");
        let pkg = root.join("talker_pkg");
        write_component_pkg(&pkg, "talker_pkg");
        // Stray system.toml next to Cargo.toml — L.8 reject.
        fs::write(
            pkg.join("system.toml"),
            "[system]\nname=\"x\"\nrmw=\"zenoh\"\ndomain_id=0\n",
        )
        .unwrap();
        let err = check_workspace(&root).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("stray system.toml"), "diag: {msg}");
        assert!(msg.contains("talker_pkg"), "diag: {msg}");
    }

    #[test]
    fn nros_check_accepts_system_toml_in_bringup_pkg() {
        let root = temp_root("bringup_ok");
        let bringup = root.join("demo_bringup");
        write_bringup_with_components(&bringup, &[]);
        // No Cargo.toml, no CMakeLists.txt, no src/ → bringup shape.
        let report = check_workspace(&root).expect("bringup passes");
        assert_eq!(report.pkgs_visited, 1);
        assert!(report.warnings.is_empty());
    }

    #[test]
    fn nros_check_warns_on_per_pkg_cargo_config_patch() {
        let root = temp_root("patch_shadow");
        let pkg = root.join("talker_pkg");
        write_component_pkg(&pkg, "talker_pkg");
        fs::create_dir_all(pkg.join(".cargo")).unwrap();
        fs::write(
            pkg.join(".cargo/config.toml"),
            "[patch.crates-io]\nzenoh = { git = \"https://example.com/x.git\" }\n",
        )
        .unwrap();
        let report = check_workspace(&root).expect("warn-only, not bail");
        assert_eq!(report.warnings.len(), 1, "warnings: {:?}", report.warnings);
        let w = &report.warnings[0];
        assert!(w.contains("talker_pkg"), "warning: {w}");
        assert!(w.contains("[patch.crates-io]"), "warning: {w}");
        assert!(w.contains("shadows"), "warning: {w}");
    }

    #[test]
    fn nros_check_silent_on_cargo_config_without_patch() {
        let root = temp_root("patch_clean");
        let pkg = root.join("talker_pkg");
        write_component_pkg(&pkg, "talker_pkg");
        fs::create_dir_all(pkg.join(".cargo")).unwrap();
        // A plain config.toml without the patch block — no warning.
        fs::write(
            pkg.join(".cargo/config.toml"),
            "[build]\ntarget = \"thumbv7m-none-eabi\"\n",
        )
        .unwrap();
        let report = check_workspace(&root).expect("ok");
        assert!(
            report.warnings.is_empty(),
            "warnings: {:?}",
            report.warnings
        );
    }

    /// Phase 212.F.2 — workspace walk must catch bringup pkgs carrying
    /// forbidden code-bearing files. The bringup classification wins over
    /// the component classification when both `package.xml + system.toml`
    /// and `Cargo.toml` coexist, so the offence surfaces (rather than the
    /// dir being mis-classified as a component with a "stray" system.toml).
    #[test]
    fn nros_check_workspace_rejects_cargo_toml_in_bringup() {
        let root = temp_root("ws_reject_cargo_in_bringup");
        let bringup = root.join("demo_bringup");
        write_bringup_with_components(&bringup, &[]);
        // Sneak a Cargo.toml into the bringup — F.2 must surface this.
        fs::write(
            bringup.join("Cargo.toml"),
            "[package]\nname=\"demo_bringup\"\nversion=\"0.1.0\"\n",
        )
        .unwrap();
        let err = check_workspace(&root).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("Cargo.toml"), "diag: {msg}");
        assert!(msg.contains("pure declarative"), "diag: {msg}");
    }

    #[test]
    fn nros_check_workspace_rejects_src_in_bringup() {
        let root = temp_root("ws_reject_src_in_bringup");
        let bringup = root.join("demo_bringup");
        write_bringup_with_components(&bringup, &[]);
        fs::create_dir_all(bringup.join("src")).unwrap();
        fs::write(bringup.join("src/main.rs"), "fn main() {}").unwrap();
        let err = check_workspace(&root).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("src/"), "diag: {msg}");
    }

    #[test]
    fn nros_check_workspace_rejects_include_dir_in_bringup() {
        let root = temp_root("ws_reject_include_in_bringup");
        let bringup = root.join("demo_bringup");
        write_bringup_with_components(&bringup, &[]);
        fs::create_dir_all(bringup.join("include")).unwrap();
        fs::write(bringup.join("include/demo.h"), "// hdr").unwrap();
        let err = check_workspace(&root).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("include/"), "diag: {msg}");
    }

    // ------------------------------------------------------------------
    // Phase 212.O.2, inverted by phase-445 W5 — `entry-deploy-retired`
    // ------------------------------------------------------------------

    #[test]
    fn nros_check_workspace_accepts_an_entry_table_with_no_deploy() {
        // The board lives in a `system.toml` image now; an entry table with no
        // `deploy` is the correct shape, not a defect.
        let root = temp_root("o2_entry_no_deploy");
        let pkg = root.join("native_entry_pkg");
        fs::create_dir_all(pkg.join("src")).unwrap();
        fs::write(
            pkg.join("Cargo.toml"),
            "[package]\nname=\"native_entry_pkg\"\nversion=\"0.1.0\"\n\
             [package.metadata.nros.entry]\n",
        )
        .unwrap();
        fs::write(pkg.join("src/main.rs"), "fn main() {}").unwrap();
        let report = check_workspace(&root).expect("an entry without deploy passes");
        assert_eq!(report.pkgs_visited, 1);
    }

    #[test]
    fn nros_check_workspace_rejects_a_retired_entry_deploy_key() {
        let root = temp_root("o2_entry_deploy_retired");
        let pkg = root.join("freertos_entry_pkg");
        fs::create_dir_all(pkg.join("src")).unwrap();
        fs::write(
            pkg.join("Cargo.toml"),
            "[package]\nname=\"freertos_entry_pkg\"\nversion=\"0.1.0\"\n\
             [package.metadata.nros.entry]\ndeploy = \"freertos\"\n",
        )
        .unwrap();
        fs::write(pkg.join("src/main.rs"), "fn main() {}").unwrap();
        let msg = check_workspace(&root).unwrap_err().to_string();
        assert!(msg.contains("entry-deploy-retired"), "diag: {msg}");
        assert!(msg.contains("freertos_entry_pkg"), "diag: {msg}");
        assert!(
            msg.contains("system.toml"),
            "names where the board goes: {msg}"
        );
    }

    #[test]
    fn nros_check_workspace_rejects_a_retired_deploy_table() {
        let root = temp_root("o2_deploy_table_retired");
        let pkg = root.join("zephyr_entry_pkg");
        fs::create_dir_all(pkg.join("src")).unwrap();
        fs::write(
            pkg.join("Cargo.toml"),
            "[package]\nname=\"zephyr_entry_pkg\"\nversion=\"0.1.0\"\n\
             [package.metadata.nros.deploy.zephyr]\nrmw = \"zenoh\"\n",
        )
        .unwrap();
        fs::write(pkg.join("src/main.rs"), "fn main() {}").unwrap();
        let msg = check_workspace(&root).unwrap_err().to_string();
        assert!(msg.contains("entry-deploy-retired"), "diag: {msg}");
        assert!(
            msg.contains("[package.metadata.nros.deploy.zephyr]"),
            "diag: {msg}"
        );
    }

    // ------------------------------------------------------------------
    // Phase 212.O.6 — `application-rtos-deploy-forbidden`
    // ------------------------------------------------------------------

    /// Verifies workspace checks reject application packages with an RTOS entry in deploy.
    #[test]
    fn check_workspace_rejects_rtos_in_deploy() {
        let root = temp_root("o6_app_rtos");
        let pkg = root.join("demo_app");
        fs::create_dir_all(pkg.join("src")).unwrap();
        fs::write(
            pkg.join("Cargo.toml"),
            "[package]\nname=\"demo_app\"\nversion=\"0.1.0\"\n\
             [package.metadata.nros.application]\n\
             deploy = [\"native\", \"freertos\"]\n",
        )
        .unwrap();
        fs::write(pkg.join("src/lib.rs"), "// stub\n").unwrap();
        let err = check_workspace(&root).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("application-rtos-deploy-forbidden"),
            "diag: {msg}"
        );
        assert!(msg.contains("demo_app"), "diag: {msg}");
        assert!(msg.contains("freertos"), "diag: {msg}");
    }

    #[test]
    fn nros_check_workspace_accepts_application_pkg_native_only() {
        let root = temp_root("o6_app_native_only");
        let pkg = root.join("demo_app");
        fs::create_dir_all(pkg.join("src")).unwrap();
        fs::write(
            pkg.join("Cargo.toml"),
            "[package]\nname=\"demo_app\"\nversion=\"0.1.0\"\n\
             [package.metadata.nros.application]\n\
             deploy = [\"native\"]\n",
        )
        .unwrap();
        fs::write(pkg.join("src/lib.rs"), "// stub\n").unwrap();
        let report = check_workspace(&root).expect("native-only application ok");
        assert_eq!(report.pkgs_visited, 1);
    }

    /// Verifies workspace checks accept application packages without a deploy list.
    #[test]
    fn check_workspace_accepts_no_deploy_list() {
        // Empty / absent deploy list is allowed by the O.6 lint (the schema
        // tolerates it; the lint only flags RTOS names when present).
        let root = temp_root("o6_app_no_deploy");
        let pkg = root.join("demo_app");
        fs::create_dir_all(pkg.join("src")).unwrap();
        fs::write(
            pkg.join("Cargo.toml"),
            "[package]\nname=\"demo_app\"\nversion=\"0.1.0\"\n\
             [package.metadata.nros.application]\n",
        )
        .unwrap();
        fs::write(pkg.join("src/lib.rs"), "// stub\n").unwrap();
        let report = check_workspace(&root).expect("application w/o deploy ok");
        assert_eq!(report.pkgs_visited, 1);
    }

    #[test]
    fn nros_check_skips_dotted_and_build_dirs() {
        let root = temp_root("skip_dirs");
        // .git with a stray Cargo.toml + system.toml should NOT trigger lint.
        fs::create_dir_all(root.join(".git")).unwrap();
        fs::write(root.join(".git/Cargo.toml"), "[package]\nname=\"x\"\n").unwrap();
        fs::write(root.join(".git/system.toml"), "[system]\nname=\"x\"\n").unwrap();
        fs::create_dir_all(root.join("target")).unwrap();
        fs::write(root.join("target/Cargo.toml"), "[package]\nname=\"y\"\n").unwrap();

        let report = check_workspace(&root).expect("dotted + build dirs skipped");
        assert_eq!(report.pkgs_visited, 0);
    }

    // ------------------------------------------------------------------
    // Phase 216.D.1 — framework × DispatchStrategy matrix
    // ------------------------------------------------------------------

    /// Stamp a board crate at `<root>/<dir>/Cargo.toml` carrying
    /// `[package.metadata.nros.board] framework = "<fw>"`.
    fn write_board_crate(root: &Path, dir: &str, fw: &str) {
        let p = root.join(dir);
        fs::create_dir_all(p.join("src")).unwrap();
        fs::write(p.join("src/lib.rs"), "// stub\n").unwrap();
        fs::write(
            p.join("Cargo.toml"),
            format!(
                "[package]\nname=\"{dir}\"\nversion=\"0.1.0\"\n\
                 [package.metadata.nros.board]\nframework = \"{fw}\"\n"
            ),
        )
        .unwrap();
    }

    /// Stamp a Node pkg at `<root>/<dir>/Cargo.toml` carrying
    /// `[package.metadata.nros.node]` + optional `dispatch = "<ds>"`.
    /// Pass `None` for `dispatch` to omit the key (exercises the default).
    fn write_node_pkg(root: &Path, dir: &str, dispatch: Option<&str>) {
        let p = root.join(dir);
        fs::create_dir_all(p.join("src")).unwrap();
        fs::write(p.join("src/lib.rs"), "// stub\n").unwrap();
        let dispatch_line = dispatch
            .map(|d| format!("dispatch = \"{d}\"\n"))
            .unwrap_or_default();
        fs::write(
            p.join("Cargo.toml"),
            format!(
                "[package]\nname=\"{dir}\"\nversion=\"0.1.0\"\n\
                 [package.metadata.nros.node]\nclass = \"{dir}::Stub\"\n\
                 name = \"stub\"\ndefault_namespace = \"/\"\n{dispatch_line}"
            ),
        )
        .unwrap();
    }

    /// Stamp an Entry pkg at `<root>/<entry_dir>/Cargo.toml` with the listed
    /// path-deps (each entry = (dep_name, rel_path)).
    ///
    /// `_board` names the board the entry is FOR, which reads well at the call
    /// sites and is all it does: the manifest key it used to become retired
    /// (RFC-0098 D5, phase-445 W5), a workspace entry's board is its bringup
    /// image's, and the dispatch lint takes the framework from the board
    /// CRATE's `[package.metadata.nros.board]`, never from a board name.
    fn write_entry_pkg(root: &Path, entry_dir: &str, _board: &str, path_deps: &[(&str, &str)]) {
        let p = root.join(entry_dir);
        fs::create_dir_all(p.join("src")).unwrap();
        fs::write(p.join("src/main.rs"), "fn main() {}").unwrap();
        let mut deps = String::from("[dependencies]\n");
        for (n, rel) in path_deps {
            deps.push_str(&format!("{n} = {{ path = \"{rel}\" }}\n"));
        }
        fs::write(
            p.join("Cargo.toml"),
            format!(
                "[package]\nname=\"{entry_dir}\"\nversion=\"0.1.0\"\n\
                 [package.metadata.nros.entry]\n\
                 {deps}"
            ),
        )
        .unwrap();
    }

    #[test]
    fn inline_node_on_rtic_framework_rejects() {
        let root = temp_root("d1_inline_on_rtic");
        write_board_crate(&root, "board-rtic-stub", "rtic");
        write_node_pkg(&root, "talker-node", Some("inline"));
        write_entry_pkg(
            &root,
            "talker-rtic-entry",
            "rtic-mps2-an385",
            &[
                ("board_rtic_stub", "../board-rtic-stub"),
                ("talker_node", "../talker-node"),
            ],
        );
        let err = check_workspace(&root).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("dispatch-mismatch-rtic-inline"), "diag: {msg}");
        assert!(msg.contains("talker-node"), "diag: {msg}");
        assert!(
            msg.contains("RTIC requires Deferred dispatch"),
            "diag: {msg}"
        );
    }

    #[test]
    fn deferred_node_on_rtic_framework_accepts() {
        let root = temp_root("d1_deferred_on_rtic");
        write_board_crate(&root, "board-rtic-stub", "rtic");
        write_node_pkg(&root, "listener-node", Some("deferred"));
        write_entry_pkg(
            &root,
            "listener-rtic-entry",
            "rtic-mps2-an385",
            &[
                ("board_rtic_stub", "../board-rtic-stub"),
                ("listener_node", "../listener-node"),
            ],
        );
        let report = check_workspace(&root).expect("(rtic, deferred) is an allowed matrix cell");
        // 3 pkg dirs visited: board, node, entry — none are bringup.
        assert_eq!(report.pkgs_visited, 3);
    }

    /// Issue 1278 — a Node pkg that states its node in a `system.toml`
    /// (RFC-0098 D3/D8) carries `dispatch` on its `[[component]]`, and the
    /// matrix reads it exactly as it read the manifest key.
    fn write_node_pkg_system_toml(root: &Path, dir: &str, dispatch: Option<&str>) {
        let p = root.join(dir);
        fs::create_dir_all(p.join("src")).unwrap();
        fs::write(p.join("src/lib.rs"), "// stub\n").unwrap();
        fs::write(
            p.join("Cargo.toml"),
            format!("[package]\nname=\"{dir}\"\nversion=\"0.1.0\"\n"),
        )
        .unwrap();
        let dispatch_line = dispatch
            .map(|d| format!("dispatch = \"{d}\"\n"))
            .unwrap_or_default();
        fs::write(
            p.join("system.toml"),
            format!(
                "[system]\nname = \"{dir}\"\n\n[[component]]\npkg = \"{dir}\"\n\
                 class = \"{dir}::Stub\"\nname = \"stub\"\n{dispatch_line}\n\
                 [image.native]\nboard = \"native\"\n"
            ),
        )
        .unwrap();
    }

    /// The strategy reads the same from both spellings — the matrix consumes
    /// `read_node_dispatch_strategy`'s answer and nothing else, so equal
    /// answers are equal reports. (A single-package leaf is never a path-dep
    /// of another entry, so the workspace walk has no shape that exercises
    /// this end-to-end; a `system.toml` beside a MEMBER's manifest is refused
    /// as stray, which is correct.)
    #[test]
    fn system_toml_dispatch_reads_like_the_manifest_key() {
        let root = temp_root("d1_system_toml");
        for (dir, ds, want) in [
            ("inline", Some("inline"), DispatchStrategy::Inline),
            ("deferred", Some("deferred"), DispatchStrategy::Deferred),
            ("isr", Some("from_isr"), DispatchStrategy::FromIsr),
            ("default", None, DispatchStrategy::Inline),
        ] {
            write_node_pkg_system_toml(&root, &format!("st-{dir}"), ds);
            write_node_pkg(&root, &format!("mf-{dir}"), ds);
            let st = read_node_dispatch_strategy(&root.join(format!("st-{dir}/Cargo.toml")))
                .unwrap()
                .expect("a [[component]] declares a node");
            let mf = read_node_dispatch_strategy(&root.join(format!("mf-{dir}/Cargo.toml")))
                .unwrap()
                .expect("[package.metadata.nros.node] declares a node");
            assert_eq!(st, want, "{dir}: system.toml");
            assert_eq!(mf, want, "{dir}: manifest");
        }
        // A typo is refused naming the file and the key it came from.
        write_node_pkg_system_toml(&root, "st-typo", Some("deffered"));
        let msg = read_node_dispatch_strategy(&root.join("st-typo/Cargo.toml"))
            .unwrap_err()
            .to_string();
        assert!(
            msg.contains("[[component]] dispatch") && msg.contains("deffered"),
            "diag: {msg}"
        );
    }

    #[test]
    fn node_default_dispatch_is_inline() {
        // A Node pkg w/o `dispatch = …` mirrors `Node::DISPATCH`'s trait
        // const default (Inline). Pairing it with an RTIC board MUST
        // therefore land the same `dispatch-mismatch-rtic-inline`
        // diagnostic as an explicit `dispatch = "inline"` Node pkg.
        let root = temp_root("d1_node_default_inline");
        write_board_crate(&root, "board-rtic-stub", "rtic");
        // No `dispatch` key — exercises the default branch in
        // `read_node_dispatch_strategy`.
        write_node_pkg(&root, "default-node", None);
        write_entry_pkg(
            &root,
            "default-rtic-entry",
            "rtic-mps2-an385",
            &[
                ("board_rtic_stub", "../board-rtic-stub"),
                ("default_node", "../default-node"),
            ],
        );
        let err = check_workspace(&root).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("dispatch-mismatch-rtic-inline"),
            "default dispatch must be Inline — diag: {msg}"
        );
    }

    #[test]
    fn inline_node_on_embassy_framework_rejects() {
        let root = temp_root("d1_inline_on_embassy");
        write_board_crate(&root, "board-embassy-stub", "embassy");
        write_node_pkg(&root, "talker-node", Some("inline"));
        write_entry_pkg(
            &root,
            "talker-embassy-entry",
            "my-embassy-board",
            &[
                ("board_embassy_stub", "../board-embassy-stub"),
                ("talker_node", "../talker-node"),
            ],
        );
        let err = check_workspace(&root).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("dispatch-mismatch-embassy-inline"),
            "diag: {msg}"
        );
        assert!(
            msg.contains("Embassy requires Deferred dispatch"),
            "diag: {msg}"
        );
    }

    #[test]
    fn from_isr_on_posix_rejects() {
        // Owned-spin (no board metadata) + FromIsr → rejected.
        let root = temp_root("d1_isr_on_posix");
        write_node_pkg(&root, "isr-node", Some("from_isr"));
        write_entry_pkg(
            &root,
            "native-entry",
            "native",
            &[("isr_node", "../isr-node")],
        );
        let err = check_workspace(&root).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("dispatch-mismatch-owned-spin-from-isr"),
            "diag: {msg}"
        );
        assert!(msg.contains("FromIsr"), "diag: {msg}");
    }

    #[test]
    fn deferred_on_owned_spin_accepts() {
        // (OwnedSpin, Deferred) is an allowed cell — a POSIX / native
        // Entry pkg may pair with a Deferred Node pkg (the runtime serves
        // the signal even though the framework axis is "no framework").
        let root = temp_root("d1_deferred_on_owned_spin");
        write_node_pkg(&root, "deferred-node", Some("deferred"));
        write_entry_pkg(
            &root,
            "native-entry",
            "native",
            &[("deferred_node", "../deferred-node")],
        );
        let report = check_workspace(&root).expect("(owned-spin, deferred) ok");
        assert_eq!(report.pkgs_visited, 2);
    }

    #[test]
    fn unknown_dispatch_string_rejects() {
        // Typo in the dispatch key surfaces a clear error instead of
        // silently defaulting to Inline.
        let root = temp_root("d1_unknown_dispatch");
        write_node_pkg(&root, "typo-node", Some("Inline" /* wrong case */));
        write_entry_pkg(
            &root,
            "native-entry",
            "native",
            &[("typo_node", "../typo-node")],
        );
        let err = check_workspace(&root).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("unknown dispatch strategy"), "diag: {msg}");
    }
}
