//! Phase 172 WP-A — `nros new --deploy <name>` deploy-target scaffolder.
//!
//! Materializes a `[deploy.<name>]` target: appends the table to the root
//! `nros.toml` (the SSOT — config never leaves it) and, for vendor kinds,
//! drops a `deploy/<name>/` code dir with starter glue the table references via
//! `{self}`. `self` targets need no dir (the startup shim is generated). The
//! per-vendor template *content* (full CMake/Kconfig per RTOS) is WP-C; here
//! the dir gets a minimal, TODO-marked starter so `nros check` passes and the
//! user fills the vendor specifics.

use std::path::{Path, PathBuf};

use eyre::{Result, WrapErr, bail, eyre};
use toml_edit::{Array, DocumentMut, Item, Table, value};

use crate::orchestration::root_config::{DeployKind, WorkspaceConfig};

pub struct DeployScaffold {
    pub name: String,
    pub kind: DeployKind,
    pub target: Option<String>,
    pub board: Option<String>,
    /// `--from-launch <path>`: also set the root `[system].launch` (bootstrap
    /// the system + deploy together).
    pub from_launch: Option<String>,
    /// `--from-profile <name>`: base the new target on an existing
    /// `[deploy.<name>]` (fork its kind/target/vendor/build), under a fresh
    /// `self` dir.
    pub from_profile: Option<String>,
    /// Workspace root holding the root `nros.toml`.
    pub root: PathBuf,
    pub force: bool,
}

pub fn scaffold_deploy(s: &DeployScaffold) -> Result<()> {
    let root_toml = s.root.join("nros.toml");
    if !root_toml.is_file() {
        bail!(
            "no root nros.toml at {} — declare a [system] first (or `nros new` a \
             project); `nros new --deploy` only adds a [deploy.<name>] to it",
            root_toml.display()
        );
    }

    // 1. Append the [deploy.<name>] table (idempotent unless --force).
    let raw = std::fs::read_to_string(&root_toml)
        .wrap_err_with(|| format!("read {}", root_toml.display()))?;
    let mut doc: DocumentMut = raw
        .parse()
        .wrap_err_with(|| format!("parse {}", root_toml.display()))?;
    if doc.get("deploy").and_then(|d| d.get(&s.name)).is_some() && !s.force {
        bail!(
            "[deploy.{}] already exists in {} — pass --force to overwrite",
            s.name,
            root_toml.display()
        );
    }
    // `--from-launch`: bootstrap the root `[system].launch` alongside the deploy.
    if let Some(launch) = &s.from_launch {
        if doc.get("systems").is_some() {
            bail!(
                "--from-launch: this workspace uses [systems.<name>] — set each \
                 system's `launch` manually"
            );
        }
        doc["system"]["launch"] = value(launch.clone());
    }

    let self_rel = format!("deploy/{}", s.name);
    // `--from-profile`: fork an existing target; else build a fresh table. The
    // *effective* kind (which drives the code-dir scaffold) comes from the
    // forked profile when cloning.
    let effective_kind = match &s.from_profile {
        Some(from) => clone_profile(&mut doc, s, from, &self_rel)?,
        None => {
            write_deploy_table(&mut doc, s, &self_rel);
            s.kind
        }
    };

    // Validate the result before writing it back, so a scaffold never leaves an
    // invalid root file.
    let merged: WorkspaceConfig =
        toml::from_str(&doc.to_string()).wrap_err("scaffolded nros.toml failed to parse")?;
    merged
        .validate()
        .wrap_err("scaffolded nros.toml failed validation")?;
    std::fs::write(&root_toml, doc.to_string())
        .wrap_err_with(|| format!("write {}", root_toml.display()))?;

    // Drop the deploy code dir (vendor kinds only; self is generated).
    if effective_kind != DeployKind::Self_ {
        scaffold_dir(s, effective_kind, &self_rel)?;
    }

    eprintln!(
        "nros new --deploy: added [deploy.{}] (kind={}) to {}",
        s.name,
        effective_kind.as_str(),
        root_toml.display()
    );
    if effective_kind != DeployKind::Self_ {
        eprintln!(
            "  scaffolded {}/ — fill the TODO vendor steps, then run the platform build",
            self_rel
        );
    } else {
        eprintln!("  build with the Entry package's platform tool");
    }
    Ok(())
}

/// Fork an existing `[deploy.<from>]` into `[deploy.<name>]`: clone its table,
/// repoint `self` at the new code dir, and return the forked kind.
fn clone_profile(
    doc: &mut DocumentMut,
    s: &DeployScaffold,
    from: &str,
    self_rel: &str,
) -> Result<DeployKind> {
    let base = doc
        .get("deploy")
        .and_then(|d| d.get(from))
        .and_then(|i| i.as_table())
        .ok_or_else(|| eyre!("--from-profile: no [deploy.{from}] to fork"))?
        .clone();
    let kind = base
        .get("kind")
        .and_then(|i| i.as_str())
        .and_then(kind_from_str)
        .unwrap_or(s.kind);

    let mut forked = base;
    // The fork owns its own code dir (build[] steps use {self}, so they carry over).
    if kind != DeployKind::Self_ {
        forked["self"] = value(self_rel);
    }
    let deploy = doc
        .entry("deploy")
        .or_insert_with(|| Item::Table(Table::new()));
    let deploy = deploy.as_table_mut().expect("[deploy] must be a table");
    deploy.set_implicit(true);
    deploy.remove(&s.name);
    deploy.insert(&s.name, Item::Table(forked));
    Ok(kind)
}

fn kind_from_str(s: &str) -> Option<DeployKind> {
    match s {
        "self" => Some(DeployKind::Self_),
        "vendor-lib" => Some(DeployKind::VendorLib),
        "vendor-module" => Some(DeployKind::VendorModule),
        _ => None,
    }
}

/// Build the `[deploy.<name>]` table programmatically (toml_edit preserves the
/// rest of the file).
fn write_deploy_table(doc: &mut DocumentMut, s: &DeployScaffold, self_rel: &str) {
    let name = &s.name;

    // `[deploy]` is an implicit super-table so children render as the block
    // form `[deploy.<name>]`, not a one-line inline table.
    let deploy = doc
        .entry("deploy")
        .or_insert_with(|| Item::Table(Table::new()));
    let deploy = deploy.as_table_mut().expect("[deploy] must be a table");
    deploy.set_implicit(true);
    deploy.remove(name); // idempotent / --force

    let mut t = Table::new();
    t["kind"] = value(s.kind.as_str());
    t["target"] = value(
        s.target
            .clone()
            .unwrap_or_else(|| "TODO: cargo target triple".to_string()),
    );
    if let Some(board) = &s.board {
        t["board"] = value(board.clone());
    }

    match s.kind {
        DeployKind::Self_ => {}
        DeployKind::VendorLib => {
            t["self"] = value(self_rel);
            t["vendor"]["pin"] = value("TODO: vendor SDK version");
            t["vendor"]["dir"]["env"] = value("VENDOR_SDK_DIR");
            t["vendor"]["dir"]["default"] = value("external/vendor-sdk");
            t["build"] = value(arr(&[
                "TODO: link {self}/startup.o {entry_lib} against the vendor lib in {vendor.dir}",
            ]));
        }
        DeployKind::VendorModule => {
            t["self"] = value(self_rel);
            t["build"] = value(arr(&[
                "TODO: invoke the vendor build with EXTERNAL_MODULES_LOCATION={self} (board={board})",
            ]));
        }
    }

    deploy.insert(name, Item::Table(t));
}

fn arr(items: &[&str]) -> Array {
    let mut a = Array::new();
    for it in items {
        a.push(*it);
    }
    a
}

fn scaffold_dir(s: &DeployScaffold, kind: DeployKind, self_rel: &str) -> Result<()> {
    let dir = s.root.join(self_rel);
    std::fs::create_dir_all(&dir).wrap_err_with(|| format!("create {}", dir.display()))?;

    let readme = format!(
        "# deploy/{name} — {kind} deployment glue\n\n\
         Referenced from `[deploy.{name}]` in the root `nros.toml` as `{{self}}`.\n\
         nano-ros generates the wiring entry lib; this dir holds the vendor-side\n\
         glue. Fill the TODO `build`/`package` steps in the root `nros.toml`.\n",
        name = s.name,
        kind = kind.as_str(),
    );
    write_if_absent(&dir.join("README.md"), &readme, s.force)?;

    match kind {
        DeployKind::VendorLib => {
            write_if_absent(&dir.join("startup.rs"), STARTUP_STUB, s.force)?;
        }
        DeployKind::VendorModule => {
            write_if_absent(&dir.join("CMakeLists.txt"), CMAKE_STUB, s.force)?;
            write_if_absent(&dir.join("Kconfig"), KCONFIG_STUB, s.force)?;
        }
        DeployKind::Self_ => {}
    }
    Ok(())
}

fn write_if_absent(path: &Path, contents: &str, force: bool) -> Result<()> {
    if path.exists() && !force {
        return Ok(());
    }
    std::fs::write(path, contents).wrap_err_with(|| format!("write {}", path.display()))
}

const STARTUP_STUB: &str = "\
// vendor-lib startup — runs vendor + transport bring-up, then hands off to the
// generated entry lib via its C ABI. Compile this to {self}/startup.o and link
// it with {entry_lib} in the root nros.toml `build` step.
//
// extern \"C\" {
//     fn nros_<sys>_register_all(exec: *mut core::ffi::c_void);
//     fn nros_<sys>_build_executor() -> *mut core::ffi::c_void;
// }
//
// TODO: vendor/SDK init (clocks, heap, NIC/IVC), then:
//   let exec = nros_<sys>_build_executor();
//   nros_<sys>_register_all(exec);
//   // spin the executor
";

const CMAKE_STUB: &str = "\
# vendor-module CMake glue. The vendor build (west / make / idf.py) includes
# this via EXTERNAL_MODULES_LOCATION={self}. It add_subdirectory()s the
# generated entry-lib source and registers the module entry.
#
# TODO: add_subdirectory(${ENTRY_SRC} nros_entry)   # the generated wiring lib
# TODO: <vendor>_add_module(...) linking the entry lib + the module entry
";

const KCONFIG_STUB: &str = "\
# vendor-module Kconfig — enable the nano-ros module in the vendor menuconfig.
#
# config NROS_MODULE
#     bool \"nano-ros generated module\"
#     default y
";

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_ws(name: &str) -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("{name}-{}-{stamp}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("nros.toml"),
            "[system]\nrmw = \"zenoh\"\ndomain_id = 0\n",
        )
        .unwrap();
        dir
    }

    fn reload(root: &Path) -> WorkspaceConfig {
        WorkspaceConfig::load(&root.join("nros.toml")).expect("reload + validate")
    }

    #[test]
    fn scaffold_vendor_module_appends_table_and_dir() {
        let root = temp_ws("nros-scaffold-vm");
        scaffold_deploy(&DeployScaffold {
            name: "mcu".into(),
            kind: DeployKind::VendorModule,
            target: Some("zephyr".into()),
            board: Some("nucleo_h753zi".into()),
            from_launch: None,
            from_profile: None,
            root: root.clone(),
            force: false,
        })
        .expect("scaffold");

        let cfg = reload(&root);
        let d = &cfg.deploy["mcu"];
        assert_eq!(d.kind, DeployKind::VendorModule);
        assert_eq!(d.board.as_deref(), Some("nucleo_h753zi"));
        assert_eq!(d.self_dir.as_deref(), Some("deploy/mcu"));
        assert!(!d.build.is_empty());
        assert!(root.join("deploy/mcu/CMakeLists.txt").is_file());
        assert!(root.join("deploy/mcu/Kconfig").is_file());

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn scaffold_self_appends_table_no_dir() {
        let root = temp_ws("nros-scaffold-self");
        scaffold_deploy(&DeployScaffold {
            name: "native".into(),
            kind: DeployKind::Self_,
            target: Some("x86_64-unknown-linux-gnu".into()),
            board: None,
            from_launch: None,
            from_profile: None,
            root: root.clone(),
            force: false,
        })
        .expect("scaffold");

        let cfg = reload(&root);
        assert_eq!(cfg.deploy["native"].kind, DeployKind::Self_);
        assert!(!root.join("deploy/native").exists());

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn scaffold_preserves_existing_system_and_other_deploys() {
        let root = temp_ws("nros-scaffold-preserve");
        scaffold_deploy(&DeployScaffold {
            name: "a".into(),
            kind: DeployKind::Self_,
            target: Some("x86_64-unknown-linux-gnu".into()),
            board: None,
            from_launch: None,
            from_profile: None,
            root: root.clone(),
            force: false,
        })
        .unwrap();
        scaffold_deploy(&DeployScaffold {
            name: "b".into(),
            kind: DeployKind::VendorModule,
            target: Some("zephyr".into()),
            board: Some("brd".into()),
            from_launch: None,
            from_profile: None,
            root: root.clone(),
            force: false,
        })
        .unwrap();

        let cfg = reload(&root);
        // both deploys + the original system survive.
        assert!(cfg.deploy.contains_key("a"));
        assert!(cfg.deploy.contains_key("b"));
        assert_eq!(cfg.system.as_ref().unwrap().rmw.as_deref(), Some("zenoh"));

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn scaffold_rejects_duplicate_without_force() {
        let root = temp_ws("nros-scaffold-dup");
        let mk = |force| DeployScaffold {
            name: "x".into(),
            kind: DeployKind::Self_,
            target: Some("t".into()),
            board: None,
            from_launch: None,
            from_profile: None,
            root: root.clone(),
            force,
        };
        scaffold_deploy(&mk(false)).unwrap();
        assert!(scaffold_deploy(&mk(false)).is_err());
        scaffold_deploy(&mk(true)).expect("force overwrites");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn from_launch_sets_system_launch() {
        let root = temp_ws("nros-scaffold-launch");
        scaffold_deploy(&DeployScaffold {
            name: "native".into(),
            kind: DeployKind::Self_,
            target: Some("x86_64-unknown-linux-gnu".into()),
            board: None,
            from_launch: Some("launch/sys.launch.xml".into()),
            from_profile: None,
            root: root.clone(),
            force: false,
        })
        .expect("scaffold");

        let cfg = reload(&root);
        assert_eq!(
            cfg.system.as_ref().unwrap().launch.as_deref(),
            Some("launch/sys.launch.xml")
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn from_profile_forks_an_existing_target() {
        let root = temp_ws("nros-scaffold-fork");
        // seed a vendor-module profile.
        scaffold_deploy(&DeployScaffold {
            name: "mcu".into(),
            kind: DeployKind::VendorModule,
            target: Some("zephyr".into()),
            board: Some("brd".into()),
            from_launch: None,
            from_profile: None,
            root: root.clone(),
            force: false,
        })
        .unwrap();
        // fork it.
        scaffold_deploy(&DeployScaffold {
            name: "mcu2".into(),
            kind: DeployKind::Self_, // ignored — forked kind wins
            target: None,
            board: None,
            from_launch: None,
            from_profile: Some("mcu".into()),
            root: root.clone(),
            force: false,
        })
        .expect("fork");

        let cfg = reload(&root);
        let forked = &cfg.deploy["mcu2"];
        assert_eq!(forked.kind, DeployKind::VendorModule); // inherited
        assert_eq!(forked.board.as_deref(), Some("brd")); // inherited
        assert_eq!(forked.self_dir.as_deref(), Some("deploy/mcu2")); // own dir
        assert!(root.join("deploy/mcu2/CMakeLists.txt").is_file());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn from_profile_errors_on_missing_base() {
        let root = temp_ws("nros-scaffold-fork-miss");
        let err = scaffold_deploy(&DeployScaffold {
            name: "x".into(),
            kind: DeployKind::Self_,
            target: Some("t".into()),
            board: None,
            from_launch: None,
            from_profile: Some("ghost".into()),
            root: root.clone(),
            force: false,
        })
        .unwrap_err()
        .to_string();
        assert!(err.contains("no [deploy.ghost]"), "{err}");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn scaffold_errors_without_root_toml() {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("nros-scaffold-noroot-{stamp}"));
        std::fs::create_dir_all(&root).unwrap();
        let err = scaffold_deploy(&DeployScaffold {
            name: "x".into(),
            kind: DeployKind::Self_,
            target: None,
            board: None,
            from_launch: None,
            from_profile: None,
            root: root.clone(),
            force: false,
        })
        .unwrap_err()
        .to_string();
        assert!(err.contains("no root nros.toml"), "{err}");
        let _ = std::fs::remove_dir_all(&root);
    }
}
