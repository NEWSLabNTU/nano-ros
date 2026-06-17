//! `nros config show` / `nros config check` — Phase 111.A.6.
//!
//! v1 surface: parse the project's `config.toml`, surface key sections
//! (zenoh, network, wifi, priority, stack) plus the active Cargo
//! features, and merge the `ROS_DOMAIN_ID` environment override.
//!
//! Kconfig (Zephyr) values + the auto-generated `nros_app_config.h`
//! struct land with Phase 112.D — until then `--zephyr` falls back to
//! a "not yet" message.

use crate::orchestration::{
    cargo_metadata_schema::SystemToml, nros_config::NrosConfig, params::load_sourced_toml_values,
};
use clap::{Args as ClapArgs, Subcommand};
use eyre::{Result, WrapErr, eyre};
use std::{
    fs,
    path::{Path, PathBuf},
};

#[derive(Debug, Subcommand)]
pub enum Args {
    /// Print the resolved configuration (config.toml merged with env
    /// overrides + active Cargo features)
    Show(ShowArgs),
    /// Validate config.toml syntactically and warn on missing common
    /// keys (zenoh.locator, zenoh.domain_id, wifi.{ssid,password})
    Check(CheckArgs),
}

#[derive(Debug, ClapArgs)]
pub struct ShowArgs {
    /// Path to config.toml (default: ./config.toml)
    #[arg(long, default_value = "config.toml")]
    pub config: PathBuf,

    /// Phase 256 Wave 6 — new-model view: print the **resolved effective config**
    /// for `<system>` (a bringup pkg) with **per-value provenance** (which file
    /// each value came from). When set, `--config` is ignored. Omit to default to
    /// the workspace's `default_system` (or the sole bringup).
    #[arg(long = "system", num_args = 0..=1, default_missing_value = "")]
    pub system: Option<String>,

    /// Phase 256 Wave 6 — workspace root for `--system` resolution (default: cwd).
    #[arg(long)]
    pub workspace: Option<PathBuf>,
}

#[derive(Debug, ClapArgs)]
pub struct CheckArgs {
    /// Path to config.toml (default: ./config.toml)
    #[arg(long, default_value = "config.toml")]
    pub config: PathBuf,
}

pub fn run(args: Args) -> Result<()> {
    match args {
        Args::Show(args) => show(args),
        Args::Check(args) => check(args),
    }
}

fn show(args: ShowArgs) -> Result<()> {
    // Phase 256 Wave 6 — `--system` switches to the new-model resolved view; the
    // legacy `config.toml` surface (used by the embedded examples) is unchanged
    // when `--system` is absent.
    if let Some(system) = args.system.as_deref() {
        let workspace = match args.workspace {
            Some(w) => w,
            None => std::env::current_dir().wrap_err("resolve cwd")?,
        };
        print!("{}", render_resolved(&workspace, system)?);
        return Ok(());
    }

    let cfg = load(&args.config)?;
    println!("# config.toml ({})", args.config.display());
    println!("{}", toml::to_string_pretty(&cfg)?);

    if let Ok(domain_id) = std::env::var("ROS_DOMAIN_ID") {
        println!("# Environment override: ROS_DOMAIN_ID = {domain_id}");
    }
    Ok(())
}

/// Phase 256 Wave 6 — print the resolved effective config for a bringup system
/// with per-value provenance. The typed `system.toml` is the SSoT both codegen
/// paths read; values resolve from it (provenance `system.toml [section]`) with
/// the built-in default as the floor. Any legacy per-package `nros.toml` overlay
/// is surfaced as DEPRECATED (provenance = the overlay file, via the Wave-0
/// `last_block_source` primitive) so the migration target is visible.
fn render_resolved(workspace: &Path, system: &str) -> Result<String> {
    use std::fmt::Write;
    let cfg = NrosConfig::from_workspace(workspace)
        .wrap_err_with(|| format!("load workspace at {}", workspace.display()))?;

    // Resolve the bringup: explicit `--system <name>`, else `default_system`,
    // else the sole bringup.
    let entry = if !system.is_empty() {
        cfg.bringup_packages
            .get(system)
            .ok_or_else(|| eyre!("no bringup pkg named '{system}' in {}", workspace.display()))?
    } else if let Some(default) = cfg.workspace_metadata.default_system.as_deref() {
        cfg.bringup_packages
            .get(default)
            .ok_or_else(|| eyre!("default_system '{default}' is not a bringup pkg"))?
    } else if cfg.bringup_packages.len() == 1 {
        cfg.bringup_packages.values().next().unwrap()
    } else {
        return Err(eyre!(
            "no `--system <pkg>` given and the workspace has {} bringup pkgs (set \
             [workspace.metadata.nros].default_system or pass --system)",
            cfg.bringup_packages.len()
        ));
    };

    let sys = &entry.system;
    let mut out = String::new();
    let _ = writeln!(
        out,
        "# Resolved config for system '{}' (bringup pkg: {})",
        sys.system.name, entry.name
    );
    let _ = writeln!(out, "# source: {}", entry.system_toml_path.display());
    let _ = writeln!(out);
    let _ = writeln!(out, "[system]");
    line(
        &mut out,
        "rmw",
        &resolved_rmw_display(sys),
        "system.toml [system]",
    );
    line(
        &mut out,
        "domain_id",
        &sys.system.domain_id.to_string(),
        "system.toml [system]",
    );
    if let Some(loc) = &sys.system.locator {
        line(&mut out, "locator", loc, "system.toml [system]");
    }
    let _ = writeln!(out);
    let _ = writeln!(out, "[capabilities]");
    line(
        &mut out,
        "safety",
        &sys.safety
            .as_ref()
            .map(|s| format!("enabled={} crc={}", s.enabled, s.crc))
            .unwrap_or_else(|| "(absent)".to_string()),
        cap_source(sys.safety.is_some()),
    );
    line(
        &mut out,
        "param_services",
        &sys.param_services
            .as_ref()
            .map(|p| format!("enabled={}", p.enabled))
            .unwrap_or_else(|| "(absent)".to_string()),
        cap_source(sys.param_services.is_some()),
    );
    line(
        &mut out,
        "lifecycle",
        &sys.lifecycle
            .as_ref()
            .map(|l| l.autostart.clone())
            .unwrap_or_else(|| "(absent)".to_string()),
        cap_source(sys.lifecycle.is_some()),
    );
    line(
        &mut out,
        "param_persistence",
        &sys.param_persistence
            .as_ref()
            .map(|p| format!("{} @ {}", p.backend, p.path))
            .unwrap_or_else(|| "(absent)".to_string()),
        cap_source(sys.param_persistence.is_some()),
    );

    // Legacy overlay audit — a per-package `nros.toml` sitting next to the bringup
    // `system.toml` is the deprecated action-at-a-distance path (RFC-0004 §3.1).
    // Name which blocks it still carries (Wave-0 `last_block_source`).
    let overlay = entry
        .system_toml_path
        .parent()
        .map(|dir| dir.join("nros.toml"))
        .filter(|p| p.is_file());
    if let Some(overlay_path) = overlay {
        let sourced = load_sourced_toml_values(std::slice::from_ref(&overlay_path))?;
        let blocks = [
            "build",
            "lifecycle",
            "param_persistence",
            "param_services",
            "safety",
            "scheduling",
            "shared_state",
        ];
        let present: Vec<&str> = blocks
            .iter()
            .filter(|b| crate::orchestration::params::last_block_source(&sourced, b).is_some())
            .copied()
            .collect();
        if !present.is_empty() {
            let _ = writeln!(out);
            let _ = writeln!(
                out,
                "# DEPRECATED overlay (phase-256): {} declares [{}]",
                overlay_path.display(),
                present.join("], [")
            );
            let _ = writeln!(
                out,
                "#   migrate these into the bringup system.toml (RFC-0004 §3.1)."
            );
        }
    }
    Ok(out)
}

/// `[system].rmw`, showing the `zenoh` default when the field is empty.
fn resolved_rmw_display(sys: &SystemToml) -> String {
    if sys.system.rmw.is_empty() {
        "zenoh (default)".to_string()
    } else {
        sys.system.rmw.clone()
    }
}

fn cap_source(present: bool) -> &'static str {
    if present { "system.toml" } else { "default" }
}

fn line(out: &mut String, key: &str, value: &str, source: &str) {
    use std::fmt::Write;
    let _ = writeln!(out, "{key:<18} = {value:<28} # {source}");
}

fn check(args: CheckArgs) -> Result<()> {
    let cfg = load(&args.config)?;
    let mut warnings: Vec<String> = Vec::new();

    let zenoh = cfg.get("zenoh").and_then(|v| v.as_table());
    match zenoh {
        Some(t) => {
            if !t.contains_key("locator") {
                warnings.push("zenoh.locator missing".into());
            }
            if !t.contains_key("domain_id") {
                warnings.push("zenoh.domain_id missing (defaults to 0)".into());
            }
        }
        None => warnings.push("[zenoh] section missing".into()),
    }

    if warnings.is_empty() {
        println!("✓ {} OK", args.config.display());
        Ok(())
    } else {
        for w in &warnings {
            eprintln!("warning: {w}");
        }
        Err(eyre!(
            "{} has {} warning(s)",
            args.config.display(),
            warnings.len()
        ))
    }
}

fn load(path: &Path) -> Result<toml::Value> {
    let raw =
        fs::read_to_string(path).wrap_err_with(|| format!("failed to read {}", path.display()))?;
    toml::from_str::<toml::Value>(&raw)
        .wrap_err_with(|| format!("invalid TOML in {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Phase 256 Wave 6 — `render_resolved` prints the resolved SSoT config from
    /// the typed `system.toml` with per-value provenance, and flags a sibling
    /// `nros.toml` legacy overlay (Wave-0 `last_block_source`). Uses a Path-A
    /// bringup (package.xml + system.toml, no Cargo.toml) so no `cargo metadata`
    /// runs in-test.
    #[test]
    fn render_resolved_shows_provenance_and_flags_legacy_overlay() {
        let dir = tempfile::tempdir().unwrap();
        let bringup = dir.path().join("demo_bringup");
        fs::create_dir_all(&bringup).unwrap();
        fs::write(
            bringup.join("package.xml"),
            r#"<package format="3"><name>demo_bringup</name><version>0.1.0</version></package>"#,
        )
        .unwrap();
        fs::write(
            bringup.join("system.toml"),
            "[system]\nname=\"demo\"\nrmw=\"cyclonedds\"\ndomain_id=5\n\
             [safety]\ncrc=true\n[lifecycle]\nautostart=\"active\"\n",
        )
        .unwrap();
        // A legacy overlay still carrying a [build] block — the migration target.
        fs::write(bringup.join("nros.toml"), "[build]\nprofile=\"release\"\n").unwrap();

        let out = render_resolved(dir.path(), "demo_bringup").unwrap();
        assert!(out.contains("system 'demo'"), "{out}");
        assert!(out.contains("rmw") && out.contains("cyclonedds"), "{out}");
        assert!(out.contains("domain_id") && out.contains('5'), "{out}");
        assert!(out.contains("safety") && out.contains("crc=true"), "{out}");
        assert!(out.contains("lifecycle") && out.contains("active"), "{out}");
        assert!(
            out.contains("param_services") && out.contains("(absent)"),
            "{out}"
        );
        // The deprecated overlay is named with the block it carries.
        assert!(
            out.contains("DEPRECATED overlay") && out.contains("[build]"),
            "must flag the legacy nros.toml [build] overlay: {out}"
        );
    }

    /// No `--system` match → a clear error, not a panic.
    #[test]
    fn render_resolved_errors_on_unknown_system() {
        let dir = tempfile::tempdir().unwrap();
        let err = render_resolved(dir.path(), "nope").unwrap_err().to_string();
        assert!(err.contains("nope"), "{err}");
    }
}
