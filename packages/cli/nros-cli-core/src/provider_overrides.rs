//! `[overrides]` in `system.toml`: reading the declarations, and reporting
//! every override (RFC-0119 D4, phase-527 W2b).
//!
//! The policy itself — what an undeclared override does, what a stale entry
//! does — is `cargo_nano_ros::provider_overrides`, a pure function of the
//! declared table. This module is the half that knows where the table lives:
//! in the workspace's `system.toml`, parsed by the one strict
//! [`SystemToml`] reader.

use std::{
    collections::BTreeMap,
    fmt::Write as _,
    path::{Path, PathBuf},
};

use cargo_nano_ros::{
    provider_overrides::{OverridePolicy, ReportRow},
    provider_scan::ScanResult,
};
use eyre::{Result, eyre};

use crate::orchestration::cargo_metadata_schema::SystemToml;

/// The `system.toml` files a workspace's `[overrides]` come from: the root's
/// own (a single-package leaf), else every bringup's under `src/*/`.
fn system_tomls(ws: &Path) -> Vec<PathBuf> {
    let direct = ws.join("system.toml");
    if direct.is_file() {
        return vec![direct];
    }
    let mut found: Vec<PathBuf> = std::fs::read_dir(ws.join("src"))
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path().join("system.toml"))
        .filter(|p| p.is_file())
        .collect();
    found.sort();
    found
}

/// Every `[overrides]` entry of the workspace, `name -> path as written`.
///
/// Several bringups may each declare entries; they are one workspace's
/// declarations, so the same name pinned to two different paths is refused
/// rather than resolved by file order.
///
/// # Errors
///
/// A `system.toml` that does not parse, or a name pinned to two paths.
pub fn declared(ws: &Path) -> Result<BTreeMap<String, String>> {
    let mut out: BTreeMap<String, (String, PathBuf)> = BTreeMap::new();
    for path in system_tomls(ws) {
        let raw =
            std::fs::read_to_string(&path).map_err(|e| eyre!("reading {}: {e}", path.display()))?;
        let sys: SystemToml =
            toml::from_str(&raw).map_err(|e| eyre!("parsing {}: {e}", path.display()))?;
        for (name, written) in sys.overrides {
            match out.get(&name) {
                Some((prev, from)) if prev != &written => {
                    return Err(eyre!(
                        "[overrides] {name} is \"{prev}\" in {} and \"{written}\" in {}; one \
                         workspace pins a provider name to one package",
                        from.display(),
                        path.display()
                    ));
                }
                Some(_) => {}
                None => {
                    out.insert(name, (written, path.clone()));
                }
            }
        }
    }
    Ok(out.into_iter().map(|(k, (v, _))| (k, v)).collect())
}

/// The workspace's override policy, with `--allow-overriding` names added.
///
/// # Errors
///
/// As [`declared`].
pub fn policy(ws: &Path, allowed: &[String]) -> Result<OverridePolicy> {
    let ws = ws.canonicalize().unwrap_or_else(|_| ws.to_path_buf());
    Ok(OverridePolicy::new(&ws, &declared(&ws)?, allowed))
}

/// The override report as text, one line per row; empty when there is
/// nothing to say. Shared by `nros dev ws providers` and `nros doctor`, so the
/// two cannot describe one workspace differently.
#[must_use]
pub fn render_report(scan: &ScanResult, policy: &OverridePolicy) -> String {
    let rows = cargo_nano_ros::provider_overrides::report(scan, policy);
    let mut out = String::new();
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
            ReportRow::Unused { name, as_written } => writeln!(
                out,
                "unused   [overrides] {name} = \"{as_written}\" replaces nothing"
            ),
        };
    }
    out
}

/// How many report rows are problems: stale declarations, which a
/// resolution of the name refuses.
#[must_use]
pub fn stale_count(scan: &ScanResult, policy: &OverridePolicy) -> usize {
    cargo_nano_ros::provider_overrides::report(scan, policy)
        .iter()
        .filter(|r| matches!(r, ReportRow::Stale(_)))
        .count()
}
