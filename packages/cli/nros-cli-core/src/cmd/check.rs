//! `nros check` - validate a generated nros-plan.json, a bringup
//! `system.toml` (RFC-0004 §4), or (Phase 212.F) a `<bringup>` pkg directory
//! for pure-declarative shape.

use crate::{
    cmd::{
        bringup::lint_bringup,
        check_workspace::check_workspace,
        emit_package_xml::{DriftStatus, check_drift},
    },
    orchestration::{
        cargo_metadata_schema::SystemToml,
        params::{last_block_source, load_sourced_toml_values},
        planner::check_plan_file,
    },
};
use clap::Args as ClapArgs;
use eyre::{Result, WrapErr};
use std::path::{Path, PathBuf};

/// Phase 256 Wave 7 — the deprecated per-package `nros.toml` overlay blocks the
/// config-SSoT endgame is retiring (RFC-0004 §3.1). A value still sourced from
/// any of these is action-at-a-distance — `nros check` warns + names the file so
/// the migration target is visible. (Capabilities/RMW already moved in
/// phase-254/255; lifecycle/param_persistence in phase-256 W1/W2; build/scheduling/
/// shared_state are the remaining waves — all warn here until removed.)
const LEGACY_OVERLAY_BLOCKS: &[&str] = &[
    "build",
    "lifecycle",
    "param_persistence",
    "param_services",
    "safety",
    "scheduling",
    "shared_state",
];

/// Scan the `nros.toml` sitting next to `system_toml_path` (the bringup overlay)
/// for any still-declared legacy block. Returns one warning per present block,
/// naming the file. Empty when there is no overlay or it carries none.
fn legacy_overlay_warnings(system_toml_path: &Path) -> Result<Vec<String>> {
    let Some(overlay) = system_toml_path
        .parent()
        .map(|dir| dir.join("nros.toml"))
        .filter(|p| p.is_file())
    else {
        return Ok(Vec::new());
    };
    let sourced = load_sourced_toml_values(std::slice::from_ref(&overlay))?;
    let mut warnings = Vec::new();
    for block in LEGACY_OVERLAY_BLOCKS {
        if last_block_source(&sourced, block).is_some() {
            warnings.push(format!(
                "[{block}] is sourced from the deprecated overlay {} — migrate it into the \
                 bringup system.toml (RFC-0004 §3.1; removed after the next release)",
                overlay.display()
            ));
        }
    }
    Ok(warnings)
}

#[derive(Debug, Default, ClapArgs)]
pub struct Args {
    /// Path to nros-plan.json, a bringup `system.toml` (or a directory
    /// containing one), or a `<bringup>` pkg directory when `--bringup` is
    /// set (Phase 212.F).
    #[arg(default_value = "build/nros/nros-plan.json")]
    pub plan: PathBuf,

    /// Phase 212.G.2 — also check a package directory for generated
    /// `package.xml` drift (a generator-marked file edited by hand).
    /// May be passed multiple times.
    #[arg(long = "package-xml-drift")]
    pub package_xml_drift: Vec<PathBuf>,

    /// Phase 212.F — lint the `plan` argument as a `<bringup>` package
    /// directory: reject `Cargo.toml`, `CMakeLists.txt`, `src/`, or any
    /// nested `add_executable(`. The bringup package must be pure
    /// declarative (see docs/design/0024-multi-node-workspace-layout.md §4).
    #[arg(long)]
    pub bringup: bool,

    /// Phase 212.L — walk a workspace root and run L.4 / L.8 / L.11:
    /// `<pkg>::<Class>` enforcement on every `[[component]]` row, stray
    /// `system.toml` next to a component pkg, and per-pkg
    /// `.cargo/config.toml` carrying `[patch.crates-io]` (warn-only). When
    /// the flag is passed with no value the workspace defaults to the
    /// current directory.
    #[arg(long, num_args = 0..=1, default_missing_value = ".", value_name = "DIR")]
    pub workspace: Option<PathBuf>,
}

pub fn run(args: Args) -> Result<()> {
    // Phase 212.L — `--workspace [<dir>]` runs the workspace-walk lint.
    if let Some(ws_root) = args.workspace.as_deref() {
        let report = check_workspace(ws_root)?;
        for w in &report.warnings {
            eprintln!("nros check: warning: {w}");
        }
        eprintln!(
            "nros check: ok (workspace {}, {} pkg(s), {} warning(s))",
            ws_root.display(),
            report.pkgs_visited,
            report.warnings.len()
        );
        return Ok(());
    }

    // Phase 212.F — `--bringup` switches the `plan` argument into a directory
    // path and runs the pure-declarative lint.
    if args.bringup {
        lint_bringup(&args.plan)?;
        for w in &legacy_overlay_warnings(&args.plan.join("system.toml"))? {
            eprintln!("nros check: warning: {w}");
        }
        eprintln!(
            "nros check: ok (bringup pkg {} is pure declarative)",
            args.plan.display()
        );
        return Ok(());
    }

    // Phase 212.F.2 — cwd-bringup auto-detection. When the user runs a bare
    // `nros check` from inside a bringup pkg (default `plan` arg, no
    // `--bringup` flag) AND the cwd carries `package.xml + system.toml`,
    // auto-route into the bringup lint so the manual smoke-test from the
    // F.2 task brief — `cd demo_bringup && nros check` — exits 0 / 1
    // without the user spelling out `--bringup .`.
    let plan_arg_is_default = args.plan == PathBuf::from("build/nros/nros-plan.json");
    if plan_arg_is_default && !args.plan.exists() {
        if let Ok(cwd) = std::env::current_dir() {
            if cwd.join("package.xml").is_file() && cwd.join("system.toml").is_file() {
                lint_bringup(&cwd)?;
                for w in &legacy_overlay_warnings(&cwd.join("system.toml"))? {
                    eprintln!("nros check: warning: {w}");
                }
                eprintln!(
                    "nros check: ok (bringup pkg {} is pure declarative)",
                    cwd.display()
                );
                return Ok(());
            }
        }
    }

    // Phase 212.G.2 — drift sweep over any explicitly named pkg dirs runs
    // first so warnings surface even when the plan check exits early.
    for pkg_dir in &args.package_xml_drift {
        match check_drift(pkg_dir)? {
            DriftStatus::Drift { on_disk_path } => {
                eprintln!(
                    "nros check: warning: {} carries the generated marker but \
                     differs from a fresh `nros emit package-xml` — \
                     re-run the emit to discard local edits",
                    on_disk_path.display()
                );
            }
            DriftStatus::Absent | DriftStatus::Clean | DriftStatus::HandWritten => {}
        }
    }

    // A `system.toml` (or a directory carrying one) is the RFC-0004 §4 system
    // spec; anything else is a generated plan. Parsing as `SystemToml`
    // (strict `deny_unknown_fields`) validates the shape.
    let system_toml = if args.plan.is_dir() {
        let candidate = args.plan.join("system.toml");
        candidate.is_file().then_some(candidate)
    } else if args.plan.extension().is_some_and(|e| e == "toml") {
        Some(args.plan.clone())
    } else {
        None
    };
    if let Some(path) = system_toml {
        let raw =
            std::fs::read_to_string(&path).wrap_err_with(|| format!("read {}", path.display()))?;
        let sys: SystemToml =
            toml::from_str(&raw).wrap_err_with(|| format!("parse {}", path.display()))?;
        // Phase 256 Wave 7 — audit the sibling `nros.toml` for deprecated overlay
        // blocks (the action-at-a-distance guard). Non-fatal: warn + name the file.
        let overlay_warnings = legacy_overlay_warnings(&path)?;
        for w in &overlay_warnings {
            eprintln!("nros check: warning: {w}");
        }
        eprintln!(
            "nros check: ok (system '{}', {} component(s), {} deploy target(s), {} overlay \
             warning(s), {})",
            sys.system.name,
            sys.components.len(),
            sys.deploy.len(),
            overlay_warnings.len(),
            path.display()
        );
        return Ok(());
    }

    let report = check_plan_file(&args.plan)?;
    if report.errors == 0 {
        for message in &report.messages {
            eprintln!("nros check: warning: {message}");
        }
        eprintln!(
            "nros check: ok ({} warning(s), {})",
            report.warnings,
            args.plan.display()
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn scratch(tag: &str) -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("{tag}-{}-{stamp}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    const SYSTEM_TOML: &str = "\
[system]
name = \"demo\"
rmw = \"zenoh\"
domain_id = 0

[deploy.native]
kind = \"self\"
target = \"x86_64-unknown-linux-gnu\"
";

    #[test]
    fn checks_a_system_toml_file() {
        let dir = scratch("nros-check-systoml");
        let path = dir.join("system.toml");
        std::fs::write(&path, SYSTEM_TOML).unwrap();
        run(Args {
            plan: path,
            ..Default::default()
        })
        .expect("valid system.toml passes");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Phase 256 Wave 7 — the overlay audit names every deprecated block a
    /// sibling `nros.toml` still carries; no overlay ⇒ no warnings.
    #[test]
    fn legacy_overlay_audit_names_deprecated_blocks() {
        let dir = scratch("nros-check-legacy-overlay");
        let system_toml = dir.join("system.toml");
        std::fs::write(&system_toml, SYSTEM_TOML).unwrap();

        // No sibling nros.toml → clean.
        assert!(legacy_overlay_warnings(&system_toml).unwrap().is_empty());

        // Sibling overlay with two deprecated blocks → one warning each, named.
        std::fs::write(
            dir.join("nros.toml"),
            "[build]\nprofile=\"release\"\n[lifecycle]\nautostart=\"active\"\n",
        )
        .unwrap();
        let warns = legacy_overlay_warnings(&system_toml).unwrap();
        assert_eq!(warns.len(), 2, "{warns:?}");
        assert!(warns.iter().any(|w| w.contains("[build]")), "{warns:?}");
        assert!(warns.iter().any(|w| w.contains("[lifecycle]")), "{warns:?}");
        assert!(
            warns
                .iter()
                .all(|w| w.contains("nros.toml") && w.contains("RFC-0004")),
            "each warning names the file + the SSoT rule: {warns:?}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn checks_a_bringup_dir_containing_system_toml() {
        let dir = scratch("nros-check-systoml-dir");
        std::fs::write(dir.join("system.toml"), SYSTEM_TOML).unwrap();
        run(Args {
            plan: dir.clone(),
            ..Default::default()
        })
        .expect("dir with system.toml passes");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn rejects_an_invalid_system_toml() {
        let dir = scratch("nros-check-systoml-bad");
        let path = dir.join("system.toml");
        // Unknown field → strict `deny_unknown_fields` rejects.
        std::fs::write(
            &path,
            "[system]\nname=\"d\"\nrmw=\"zenoh\"\ndomain_id=0\nbogus_field=1\n",
        )
        .unwrap();
        assert!(
            run(Args {
                plan: path,
                ..Default::default()
            })
            .is_err(),
            "invalid system.toml must fail the check"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}

// Phase 172 — `[[bridge]]` per-node routing is now emitted by the generator
// (`register_bridges`: a bridge node per endpoint session + the generic-sub →
// generic-pub relay with `bridge_origin` echo suppression), so the former
// "routing not yet emitted" warning is gone. `[[domain]]` routing landed in
// Phase 172.K.5.
