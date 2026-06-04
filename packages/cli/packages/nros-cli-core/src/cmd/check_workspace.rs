//! Phase 212.L / O — workspace-walk lints for `nros check --workspace`.
//!
//! Lints land here:
//!
//! * **L.4 — `<pkg>::<Class>` enforcement.** Every `[[component]]` row in a
//!   bringup `system.toml` carries `pkg = "<dir>"` + `class = "<dir>::<Type>"`.
//!   The `class` MUST be prefixed by `<pkg>::` so the codegen path and a human
//!   reader land at the same crate.
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
//! The walk is `nros check --workspace [<dir>]`. Each immediate child of the
//! workspace root is classified as a bringup pkg (has `system.toml`, no
//! `Cargo.toml` / `CMakeLists.txt` / `src/`) or a component pkg (has
//! `Cargo.toml` or `CMakeLists.txt`). Other dirs are skipped.

use std::{fs, path::Path};

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

    let entries = fs::read_dir(workspace_root)
        .wrap_err_with(|| format!("read {}", workspace_root.display()))?;
    let mut dirs: Vec<std::path::PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    // Deterministic order so diagnostics + warnings are reproducible.
    dirs.sort();

    for pkg_dir in dirs {
        let name = match pkg_dir.file_name().and_then(|n| n.to_str()) {
            Some(n) => n.to_string(),
            None => continue,
        };
        // Skip dotted dirs (.git, .cargo, .claude, …) and build output dirs.
        if name.starts_with('.') || name == "target" || name == "build" {
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
                     docs/design/multi-node-workspace-layout.md §4)",
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
                         `nros ws sync`)"
                    ));
                }
            }
            // O.2 + O.6 — peek inside Cargo.toml's `[package.metadata.nros]`
            // for the Entry-pkg / Application-pkg shape lints.
            if has_cargo {
                lint_cargo_metadata_nros(&pkg_dir.join("Cargo.toml"), &name)?;
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
    let mut bad: Vec<String> = Vec::new();
    for c in &parsed.components {
        let prefix = format!("{}::", c.pkg);
        if !c.class.starts_with(&prefix) {
            bad.push(format!(
                "[[component]] name=\"{}\" pkg=\"{}\" class=\"{}\" — class \
                 must start with \"{}\"",
                c.name, c.pkg, c.class, prefix
            ));
        }
    }
    if !bad.is_empty() {
        bail!(
            "bringup pkg {bringup_pkg_name}: system.toml component class \
             mismatch — {}. The `class` field in a `[[component]]` row MUST \
             be `<pkg>::<Type>` so codegen and humans land at the same crate.",
            bad.join("; ")
        );
    }
    Ok(())
}

/// Phase 212.O.2 + O.6 — peek inside a component pkg's `Cargo.toml`
/// `[package.metadata.nros]` table and run the Entry / Application shape
/// lints.
///
/// * **O.2 `entry-deploy-missing`** — `[package.metadata.nros.entry]` MUST
///   carry `deploy = "<board>"` (non-empty string). Phase 212.L.2 / N.7.
/// * **O.6 `application-rtos-deploy-forbidden`** — every entry in
///   `[package.metadata.nros.application].deploy` MUST be `"native"`. Phase
///   212.L.2 / M-F.1 — Application pkgs are native-only orchestration roots.
///
/// Both lints bail with an eyre error whose diagnostic id is embedded in the
/// message body (`entry-deploy-missing` / `application-rtos-deploy-forbidden`).
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

    // ---- O.2 entry-deploy-missing -----------------------------------------
    if let Some(entry) = nros.get("entry") {
        // `deploy` must be present, a string, and non-empty.
        let deploy_ok = entry
            .get("deploy")
            .and_then(|v| v.as_str())
            .is_some_and(|s| !s.trim().is_empty());
        if !deploy_ok {
            bail!(
                "{pkg_name}: [package.metadata.nros.entry] missing or empty \
                 'deploy' field — Entry pkg must name a board target (e.g. \
                 deploy = \"native\" or deploy = \"qemu-mps2-an385-freertos\") \
                 [diagnostic: entry-deploy-missing]"
            );
        }
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
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_root(tag: &str) -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "nros-check-ws-{tag}-{}-{stamp}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
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
    fn nros_check_rejects_class_pkg_mismatch() {
        let root = temp_root("class_mismatch");
        let bringup = root.join("demo_bringup");
        write_bringup_with_components(&bringup, &[("talker_pkg", "wrong::Talker", "talker")]);
        let err = check_workspace(&root).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("class mismatch"), "diag: {msg}");
        assert!(msg.contains("talker_pkg::"), "diag: {msg}");
        assert!(msg.contains("wrong::Talker"), "diag: {msg}");
    }

    #[test]
    fn nros_check_accepts_correct_class_pkg_prefix() {
        let root = temp_root("class_ok");
        let bringup = root.join("demo_bringup");
        write_bringup_with_components(
            &bringup,
            &[
                ("talker_pkg", "talker_pkg::Talker", "talker"),
                ("listener_pkg", "listener_pkg::Listener", "listener"),
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
    // Phase 212.O.2 — `entry-deploy-missing`
    // ------------------------------------------------------------------

    #[test]
    fn nros_check_workspace_rejects_entry_pkg_without_deploy_field() {
        let root = temp_root("o2_entry_no_deploy");
        let pkg = root.join("freertos_entry_pkg");
        fs::create_dir_all(pkg.join("src")).unwrap();
        // Empty [package.metadata.nros.entry] table — no `deploy =` key.
        fs::write(
            pkg.join("Cargo.toml"),
            "[package]\nname=\"freertos_entry_pkg\"\nversion=\"0.1.0\"\n\
             [package.metadata.nros.entry]\n",
        )
        .unwrap();
        fs::write(pkg.join("src/main.rs"), "fn main() {}").unwrap();
        let err = check_workspace(&root).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("entry-deploy-missing"), "diag: {msg}");
        assert!(msg.contains("freertos_entry_pkg"), "diag: {msg}");
        assert!(msg.contains("'deploy'"), "diag: {msg}");
    }

    #[test]
    fn nros_check_workspace_rejects_entry_pkg_with_empty_deploy() {
        let root = temp_root("o2_entry_empty_deploy");
        let pkg = root.join("native_entry_pkg");
        fs::create_dir_all(pkg.join("src")).unwrap();
        fs::write(
            pkg.join("Cargo.toml"),
            "[package]\nname=\"native_entry_pkg\"\nversion=\"0.1.0\"\n\
             [package.metadata.nros.entry]\ndeploy = \"\"\n",
        )
        .unwrap();
        fs::write(pkg.join("src/main.rs"), "fn main() {}").unwrap();
        let err = check_workspace(&root).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("entry-deploy-missing"), "diag: {msg}");
    }

    #[test]
    fn nros_check_workspace_accepts_entry_pkg_with_deploy() {
        let root = temp_root("o2_entry_ok");
        let pkg = root.join("native_entry_pkg");
        fs::create_dir_all(pkg.join("src")).unwrap();
        fs::write(
            pkg.join("Cargo.toml"),
            "[package]\nname=\"native_entry_pkg\"\nversion=\"0.1.0\"\n\
             [package.metadata.nros.entry]\ndeploy = \"native\"\n",
        )
        .unwrap();
        fs::write(pkg.join("src/main.rs"), "fn main() {}").unwrap();
        let report = check_workspace(&root).expect("entry+deploy passes");
        assert_eq!(report.pkgs_visited, 1);
    }

    // ------------------------------------------------------------------
    // Phase 212.O.6 — `application-rtos-deploy-forbidden`
    // ------------------------------------------------------------------

    #[test]
    fn nros_check_workspace_rejects_application_pkg_with_rtos_in_deploy() {
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

    #[test]
    fn nros_check_workspace_accepts_application_pkg_without_deploy_list() {
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
}
