//! `nros new --deploy <name>` deploy-target scaffolder (RFC-0004 home).
//!
//! Materializes a `[deploy.<name>]` target inside the bringup package's
//! `system.toml` — the RFC-0004 §4 single source of truth for deploy targets.
//! (The Phase-172 root `nros.toml` model this used to write is retired; see
//! `docs/issues/0051-deploy-target-ssot-split-root-nros-toml-vs-system-toml.md`.)
//!
//! The edit is surgical (`toml_edit`, preserving the rest of the file). The
//! bringup `system.toml` is located the same way `nros_config` discovers
//! bringup packages: a workspace with exactly one on-disk-`system.toml` bringup
//! uses it implicitly; a workspace with several requires `--bringup <pkg>`.

use std::path::PathBuf;

use eyre::{Result, WrapErr, bail, eyre};
use toml_edit::{DocumentMut, Item, Table, value};

use crate::orchestration::{
    cargo_metadata_schema::SystemToml,
    nros_config::{BringupSource, NrosConfig},
};

pub struct DeployScaffold {
    pub name: String,
    /// Free-form deploy kind written verbatim to `DeployTarget.kind`
    /// (`"self"`, `"qemu"`, `"flash"`, …). `None` ⇒ the runner derives it
    /// from the target-name key.
    pub kind: Option<String>,
    pub target: Option<String>,
    pub board: Option<String>,
    /// `--from-launch <path>`: also set the bringup `[system].default_launch`
    /// (RFC-0004 field) so the system + deploy bootstrap together.
    pub from_launch: Option<String>,
    /// `--from-profile <name>`: base the new target on an existing
    /// `[deploy.<name>]` in the same `system.toml` (forks its fields).
    pub from_profile: Option<String>,
    /// Workspace root to discover the bringup package under.
    pub workspace_root: PathBuf,
    /// `--bringup <pkg>`: pick a specific bringup package when the workspace
    /// exposes more than one.
    pub bringup: Option<String>,
    pub force: bool,
}

pub fn scaffold_deploy(s: &DeployScaffold) -> Result<()> {
    let system_toml = locate_bringup_system_toml(s)?;

    // 1. Surgically edit the bringup system.toml (idempotent unless --force).
    let raw = std::fs::read_to_string(&system_toml)
        .wrap_err_with(|| format!("read {}", system_toml.display()))?;
    let mut doc: DocumentMut = raw
        .parse()
        .wrap_err_with(|| format!("parse {}", system_toml.display()))?;

    if doc.get("deploy").and_then(|d| d.get(&s.name)).is_some() && !s.force {
        bail!(
            "[deploy.{}] already exists in {} — pass --force to overwrite",
            s.name,
            system_toml.display()
        );
    }

    // `--from-launch`: set the RFC-0004 `[system].default_launch` field.
    if let Some(launch) = &s.from_launch {
        doc["system"]["default_launch"] = value(launch.clone());
    }

    // `--from-profile`: fork an existing target; else build a fresh table.
    match &s.from_profile {
        Some(from) => clone_profile(&mut doc, s, from)?,
        None => write_deploy_table(&mut doc, s),
    }

    // Validate the result before writing it back, so a scaffold never leaves an
    // invalid `system.toml` behind.
    let _: SystemToml = toml::from_str(&doc.to_string())
        .wrap_err("scaffolded system.toml failed to parse as a system spec")?;
    std::fs::write(&system_toml, doc.to_string())
        .wrap_err_with(|| format!("write {}", system_toml.display()))?;

    eprintln!(
        "nros new --deploy: added [deploy.{}] to {}",
        s.name,
        system_toml.display()
    );
    eprintln!("  build with the bringup package's platform tool (e.g. `cargo run -p <entry_pkg>`)");
    Ok(())
}

/// Discover the bringup package's `system.toml` to edit, the same way
/// `nros_config` discovers bringup packages. Only bringups with a real on-disk
/// `system.toml` ([`BringupSource::SystemToml`]) are eligible — a synthesised
/// self-bringup has no file to edit.
fn locate_bringup_system_toml(s: &DeployScaffold) -> Result<PathBuf> {
    let cfg = NrosConfig::from_workspace(&s.workspace_root).wrap_err_with(|| {
        format!(
            "discover bringup packages under {}",
            s.workspace_root.display()
        )
    })?;

    let mut candidates: Vec<(String, PathBuf)> = cfg
        .bringup_packages
        .into_iter()
        .filter(|(_, e)| e.source == BringupSource::SystemToml)
        .map(|(name, e)| (name, e.system_toml_path))
        .collect();
    candidates.sort();

    if let Some(want) = &s.bringup {
        return candidates
            .into_iter()
            .find(|(name, _)| name == want)
            .map(|(_, path)| path)
            .ok_or_else(|| {
                eyre!(
                    "--bringup {want}: no bringup package named `{want}` with a \
                     system.toml under {}",
                    s.workspace_root.display()
                )
            });
    }

    match candidates.len() {
        0 => bail!(
            "no bringup package with a system.toml under {} — create one first \
             (`nros new system <name>_bringup --components <...>`); \
             `nros new --deploy` only adds a [deploy.<name>] to an existing system.toml",
            s.workspace_root.display()
        ),
        1 => Ok(candidates.into_iter().next().unwrap().1),
        _ => {
            let names: Vec<&str> = candidates.iter().map(|(n, _)| n.as_str()).collect();
            bail!(
                "workspace under {} exposes multiple bringup packages ({}) — \
                 pass --bringup <pkg> to pick one",
                s.workspace_root.display(),
                names.join(", ")
            )
        }
    }
}

/// Fork an existing `[deploy.<from>]` into `[deploy.<name>]`: clone its table,
/// then apply any explicit `--target` / `--board` overrides.
fn clone_profile(doc: &mut DocumentMut, s: &DeployScaffold, from: &str) -> Result<()> {
    let base = doc
        .get("deploy")
        .and_then(|d| d.get(from))
        .and_then(|i| i.as_table())
        .ok_or_else(|| eyre!("--from-profile: no [deploy.{from}] to fork"))?
        .clone();

    let mut forked = base;
    if let Some(kind) = &s.kind {
        forked["kind"] = value(kind.clone());
    }
    if let Some(target) = &s.target {
        forked["target"] = value(target.clone());
    }
    if let Some(board) = &s.board {
        forked["board"] = value(board.clone());
    }

    insert_deploy(doc, &s.name, forked);
    Ok(())
}

/// Build the `[deploy.<name>]` table programmatically (toml_edit preserves the
/// rest of the file).
fn write_deploy_table(doc: &mut DocumentMut, s: &DeployScaffold) {
    let mut t = Table::new();
    if let Some(kind) = &s.kind {
        t["kind"] = value(kind.clone());
    }
    if let Some(target) = &s.target {
        t["target"] = value(target.clone());
    }
    if let Some(board) = &s.board {
        t["board"] = value(board.clone());
    }
    insert_deploy(doc, &s.name, t);
}

/// Insert `[deploy.<name>]` as a block table under the implicit `[deploy]`
/// super-table (idempotent — removes any existing entry first).
fn insert_deploy(doc: &mut DocumentMut, name: &str, table: Table) {
    let deploy = doc
        .entry("deploy")
        .or_insert_with(|| Item::Table(Table::new()));
    let deploy = deploy.as_table_mut().expect("[deploy] must be a table");
    deploy.set_implicit(true);
    deploy.remove(name); // idempotent / --force
    deploy.insert(name, Item::Table(table));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        path::Path,
        time::{SystemTime, UNIX_EPOCH},
    };

    const SYSTEM_TOML: &str = "\
[system]
name = \"demo\"
rmw = \"zenoh\"
domain_id = 0

[[component]]
pkg = \"talker_pkg\"
class = \"talker_pkg::TalkerNode\"
name = \"talker\"
";

    /// Stage a Path-A bringup workspace (package.xml + system.toml, no
    /// Cargo.toml) so `NrosConfig::from_workspace` discovers it without
    /// invoking cargo.
    fn temp_ws(tag: &str) -> (PathBuf, PathBuf) {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("{tag}-{}-{stamp}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let bringup = root.join("demo_bringup");
        std::fs::create_dir_all(&bringup).unwrap();
        std::fs::write(
            bringup.join("package.xml"),
            "<?xml version=\"1.0\"?>\n<package format=\"3\">\n  <name>demo_bringup</name>\n</package>\n",
        )
        .unwrap();
        std::fs::write(bringup.join("system.toml"), SYSTEM_TOML).unwrap();
        (root, bringup.join("system.toml"))
    }

    fn reload(system_toml: &Path) -> SystemToml {
        let raw = std::fs::read_to_string(system_toml).expect("read");
        toml::from_str(&raw).expect("reparse system.toml")
    }

    fn scaffold(root: &Path, name: &str, kind: Option<&str>) -> DeployScaffold {
        DeployScaffold {
            name: name.into(),
            kind: kind.map(str::to_string),
            target: Some("x86_64-unknown-linux-gnu".into()),
            board: None,
            from_launch: None,
            from_profile: None,
            workspace_root: root.to_path_buf(),
            bringup: None,
            force: false,
        }
    }

    #[test]
    fn writes_deploy_into_bringup_system_toml() {
        let (root, system_toml) = temp_ws("nros-scaffold-sys");
        scaffold_deploy(&scaffold(&root, "native", Some("self"))).expect("scaffold");

        let sys = reload(&system_toml);
        let d = sys.deploy.get("native").expect("[deploy.native] written");
        assert_eq!(d.kind.as_deref(), Some("self"));
        assert_eq!(d.target.as_deref(), Some("x86_64-unknown-linux-gnu"));
        // No root nros.toml is ever created.
        assert!(!root.join("nros.toml").exists());
        // No vendor-dir scaffolding.
        assert!(!root.join("deploy").exists());

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn preserves_existing_system_and_components() {
        let (root, system_toml) = temp_ws("nros-scaffold-preserve");
        scaffold_deploy(&scaffold(&root, "native", Some("self"))).unwrap();
        scaffold_deploy(&scaffold(&root, "qemu", Some("qemu"))).unwrap();

        let sys = reload(&system_toml);
        assert_eq!(sys.system.name, "demo");
        assert_eq!(sys.system.rmw, "zenoh");
        assert_eq!(sys.components.len(), 1);
        assert!(sys.deploy.contains_key("native"));
        assert!(sys.deploy.contains_key("qemu"));

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn rejects_duplicate_without_force() {
        let (root, _) = temp_ws("nros-scaffold-dup");
        scaffold_deploy(&scaffold(&root, "native", Some("self"))).unwrap();
        assert!(scaffold_deploy(&scaffold(&root, "native", Some("self"))).is_err());

        let mut forced = scaffold(&root, "native", Some("self"));
        forced.force = true;
        scaffold_deploy(&forced).expect("force overwrites");

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn from_launch_sets_system_default_launch() {
        let (root, system_toml) = temp_ws("nros-scaffold-launch");
        let mut s = scaffold(&root, "native", Some("self"));
        s.from_launch = Some("demo.launch.xml".into());
        scaffold_deploy(&s).expect("scaffold");

        let sys = reload(&system_toml);
        assert_eq!(
            sys.system.default_launch.as_deref(),
            Some("demo.launch.xml")
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn from_profile_forks_an_existing_target() {
        let (root, system_toml) = temp_ws("nros-scaffold-fork");
        let mut base = scaffold(&root, "qemu", Some("qemu"));
        base.board = Some("mps2_an385".into());
        scaffold_deploy(&base).unwrap();

        let fork = DeployScaffold {
            name: "qemu2".into(),
            kind: None,
            target: None,
            board: None,
            from_launch: None,
            from_profile: Some("qemu".into()),
            workspace_root: root.clone(),
            bringup: None,
            force: false,
        };
        scaffold_deploy(&fork).expect("fork");

        let sys = reload(&system_toml);
        let forked = sys.deploy.get("qemu2").expect("forked target");
        assert_eq!(forked.kind.as_deref(), Some("qemu")); // inherited
        assert_eq!(forked.board.as_deref(), Some("mps2_an385")); // inherited

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn from_profile_errors_on_missing_base() {
        let (root, _) = temp_ws("nros-scaffold-fork-miss");
        let mut s = scaffold(&root, "x", None);
        s.from_profile = Some("ghost".into());
        let err = scaffold_deploy(&s).unwrap_err().to_string();
        assert!(err.contains("no [deploy.ghost]"), "{err}");

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn errors_without_a_bringup_system_toml() {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("nros-scaffold-noboot-{stamp}"));
        std::fs::create_dir_all(&root).unwrap();
        let err = scaffold_deploy(&scaffold(&root, "x", Some("self")))
            .unwrap_err()
            .to_string();
        assert!(err.contains("no bringup package"), "{err}");

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn requires_bringup_selector_when_multiple() {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("nros-scaffold-multi-{stamp}"));
        let _ = std::fs::remove_dir_all(&root);
        for name in ["alpha_bringup", "beta_bringup"] {
            let dir = root.join(name);
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(
                dir.join("package.xml"),
                format!(
                    "<?xml version=\"1.0\"?>\n<package format=\"3\">\n  <name>{name}</name>\n</package>\n"
                ),
            )
            .unwrap();
            std::fs::write(dir.join("system.toml"), SYSTEM_TOML).unwrap();
        }

        // Ambiguous → error naming the candidates.
        let err = scaffold_deploy(&scaffold(&root, "native", Some("self")))
            .unwrap_err()
            .to_string();
        assert!(err.contains("multiple bringup packages"), "{err}");

        // --bringup disambiguates.
        let mut s = scaffold(&root, "native", Some("self"));
        s.bringup = Some("beta_bringup".into());
        scaffold_deploy(&s).expect("explicit --bringup scaffolds");
        let sys = reload(&root.join("beta_bringup/system.toml"));
        assert!(sys.deploy.contains_key("native"));
        // The other bringup is untouched.
        let other = reload(&root.join("alpha_bringup/system.toml"));
        assert!(other.deploy.is_empty());

        let _ = std::fs::remove_dir_all(&root);
    }
}
