//! Override declarations: reading them, and reporting every override
//! (RFC-0119 D4, phase-527 W2b/W2c).
//!
//! The policy itself — what an undeclared override does, what a stale entry
//! does, what two source roots providing one name do — is
//! `cargo_nano_ros::provider_overrides`, a pure function of the declared
//! table. This module is the half that knows where the table lives:
//!
//! - `nros.toml [workspace.overrides]`, beside `package_paths` — workspace
//!   configuration, the primary home;
//! - the ROOT `system.toml [overrides]`, only when the workspace has no
//!   `nros.toml` (a single-package leaf).
//!
//! A `src/*/system.toml` is never read for overrides: that is a package's
//! file, and a vendored package must not grant itself an override. One that
//! carries `[overrides]` is refused, naming the file and the move.

use std::{
    collections::BTreeMap,
    fmt::Write as _,
    path::{Path, PathBuf},
};

use cargo_nano_ros::{
    provider_overrides::{OverridePolicy, OverridesFile, ReportRow},
    provider_scan::ScanResult,
};
use eyre::{Result, eyre};

use crate::orchestration::{cargo_metadata_schema::SystemToml, nros_config::workspace_overrides};

/// A workspace's override declarations and where they came from.
#[derive(Debug, Clone, Default)]
pub struct Declarations {
    /// `name -> path as written`.
    pub entries: BTreeMap<String, String>,
    /// The file the declarations live in (or would be written in).
    pub file: OverridesFile,
    /// The file read, when there was one.
    pub path: Option<PathBuf>,
    /// Notes to print once on stderr: a declaration read from the legacy
    /// leaf fallback says where it belongs now.
    pub notes: Vec<String>,
}

/// Does this TOML file carry a top-level `[overrides]` table? Read loosely:
/// the question is only where a declaration sits, and validating the rest of
/// the file is its own reader's job.
fn has_overrides_table(path: &Path) -> Result<bool> {
    let raw =
        std::fs::read_to_string(path).map_err(|e| eyre!("reading {}: {e}", path.display()))?;
    let table: toml::Table =
        toml::from_str(&raw).map_err(|e| eyre!("parsing {}: {e}", path.display()))?;
    Ok(table.contains_key("overrides"))
}

/// The workspace's override declarations.
///
/// # Errors
///
/// A file that does not parse; `[overrides]` in a `src/*/system.toml`; or a
/// workspace with a `nros.toml` whose root `system.toml` also carries
/// `[overrides]`.
pub fn declared(ws: &Path) -> Result<Declarations> {
    let mut bringups: Vec<PathBuf> = std::fs::read_dir(ws.join("src"))
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path().join("system.toml"))
        .filter(|p| p.is_file())
        .collect();
    bringups.sort();
    for path in &bringups {
        if has_overrides_table(path)? {
            return Err(eyre!(
                "{} declares [overrides], but overrides are workspace configuration: a \
                 package cannot grant itself one. Move the entries to {} under \
                 [workspace.overrides].",
                path.display(),
                ws.join("nros.toml").display()
            ));
        }
    }

    let nros_toml = ws.join("nros.toml");
    let root_sys = ws.join("system.toml");
    match workspace_overrides(ws).map_err(|e| eyre!("{e}"))? {
        Some(entries) => {
            if root_sys.is_file() && has_overrides_table(&root_sys)? {
                return Err(eyre!(
                    "overrides are declared in both {} [workspace.overrides] and {} \
                     [overrides]. Keep the nros.toml table and remove the system.toml one: a \
                     workspace with a nros.toml reads its overrides from there.",
                    nros_toml.display(),
                    root_sys.display()
                ));
            }
            Ok(Declarations {
                entries,
                file: OverridesFile::NrosToml,
                path: Some(nros_toml),
                notes: Vec::new(),
            })
        }
        None => leaf_system_toml_fallback(ws),
    }
}

/// The single-package leaf fallback: a workspace with no `nros.toml` reads
/// its root `system.toml [overrides]`. The ONLY place that reads overrides
/// from a `system.toml`, kept in one function so it can be retired in one
/// edit. A non-empty table comes back with a note naming the new home.
fn leaf_system_toml_fallback(ws: &Path) -> Result<Declarations> {
    let root_sys = ws.join("system.toml");
    if !root_sys.is_file() {
        return Ok(Declarations::default());
    }
    let raw = std::fs::read_to_string(&root_sys)
        .map_err(|e| eyre!("reading {}: {e}", root_sys.display()))?;
    let sys: SystemToml =
        toml::from_str(&raw).map_err(|e| eyre!("parsing {}: {e}", root_sys.display()))?;
    let notes = if sys.overrides.is_empty() {
        Vec::new()
    } else {
        vec![format!(
            "{} declares [overrides]; move this to nros.toml [workspace.overrides] (create \
             a nros.toml at the workspace root and delete the system.toml table).",
            root_sys.display()
        )]
    };
    Ok(Declarations {
        entries: sys.overrides,
        file: OverridesFile::LeafSystemToml,
        path: Some(root_sys),
        notes,
    })
}

/// The workspace's override policy, with `--allow-overriding` names added.
/// Prints [`Declarations::notes`] on stderr.
///
/// # Errors
///
/// As [`declared`].
pub fn policy(ws: &Path, allowed: &[String]) -> Result<OverridePolicy> {
    let ws = ws.canonicalize().unwrap_or_else(|_| ws.to_path_buf());
    let d = declared(&ws)?;
    for n in &d.notes {
        eprintln!("note: {n}");
    }
    Ok(OverridePolicy::new(&ws, &d.entries, allowed).read_from(d.file, d.path))
}

/// The override report as text, one line per row; empty when there is
/// nothing to say. Shared by `nros dev ws providers` and `nros doctor`, so the
/// two cannot describe one workspace differently. The first line says where
/// the declarations were read from.
#[must_use]
pub fn render_report(scan: &ScanResult, policy: &OverridePolicy) -> String {
    let rows = cargo_nano_ros::provider_overrides::report(scan, policy);
    let mut out = String::new();
    if rows.is_empty() {
        return out;
    }
    let _ = match &policy.declared_in {
        Some(p) if !policy.declared.is_empty() => {
            writeln!(
                out,
                "overrides read from {} ({})",
                p.display(),
                policy.file.label()
            )
        }
        _ => writeln!(
            out,
            "no overrides declared (declare them in {})",
            policy.file.label()
        ),
    };
    for row in rows {
        let _ = match row {
            ReportRow::Override(o) => writeln!(
                out,
                "override {kind}:{name}  {winner}  replaces {hidden}  ({status})",
                kind = o.kind,
                name = o.name,
                winner = policy.shown(&o.winner.dir),
                hidden = o.shadowed[0].dir.display(),
                status = o.status.label(),
            ),
            ReportRow::Stale(e) => writeln!(out, "STALE    {e}"),
            ReportRow::Duplicate(e) => writeln!(out, "DUPLICATE {e}"),
            ReportRow::Unused { name, as_written } => writeln!(
                out,
                "unused   {} {name} = \"{as_written}\" replaces nothing",
                policy.file.label()
            ),
        };
    }
    out
}

/// How many report rows are problems: stale declarations and unsettled
/// source-root duplicates, which a resolution of the name refuses.
#[must_use]
pub fn problem_count(scan: &ScanResult, policy: &OverridePolicy) -> usize {
    cargo_nano_ros::provider_overrides::report(scan, policy)
        .iter()
        .filter(|r| r.is_problem())
        .count()
}
