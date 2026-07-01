//! Phase 212.F — `nros new system <name>_bringup --components <list>` scaffolder.
//!
//! Materializes a **Path A** bringup package per the multi-node workspace
//! layout design (`docs/design/0024-multi-node-workspace-layout.md` §4 + §11.3 +
//! `docs/design/0025-workspace-layout-by-case.md` Case 3). A bringup package is
//! **pure declarative** — `package.xml` + `system.toml` + `launch/` (+ optional
//! `config/` + `README.md`) only. No `Cargo.toml`, no `CMakeLists.txt`, no
//! `src/`.
//!
//! When invoked inside an existing cargo workspace, the bringup pkg name is
//! appended to the workspace-root `[workspace] exclude` list (Path A keeps
//! bringup out of `members`).
//!
//! Surface (see `cmd/new.rs` dispatcher):
//!
//! ```text
//! nros new system <name>_bringup --components <pkg1,pkg2,...> [--into <dir>] \
//!     [--workspace-root <dir>] [--no-config] [--no-readme] [--force]
//! ```
//!
//! Each `<pkgN>` becomes one `[[component]]` entry in `system.toml`, one
//! `<exec_depend>` line in `package.xml`, and one `<node>` block in
//! `launch/system.launch.xml`. Component crates themselves are **NOT**
//! scaffolded — that's `nros new --component <name>` (Phase 172 W.3).
//!
//! **J.5 policy.** Generated `package.xml` does NOT include
//! `<buildtool_depend>ament_cmake</buildtool_depend>` — nano-ros planning reads
//! `launch/` directly from the source tree, no install step needed (see
//! `docs/design/0024-multi-node-workspace-layout.md` §11.1).

use std::{
    fs,
    path::{Path, PathBuf},
};

use clap::Args as ClapArgs;
use eyre::{Result, WrapErr, bail};
use toml_edit::{Array, DocumentMut, value};

use crate::orchestration::cargo_metadata_schema::{
    DeployTarget, SystemComponentEntry, SystemHeader, SystemToml,
};

#[derive(Debug, ClapArgs)]
pub struct Args {
    /// Bringup package directory to create (e.g. `demo_bringup`). Conventionally
    /// suffixed `_bringup` per the Phase 212 design doc, but any name is
    /// accepted (`<system>_launch` documented as an alias).
    pub name: PathBuf,

    /// Comma-separated component package names to wire into the bringup
    /// (`pkg1,pkg2,…`). One `[[component]]` entry + one `<exec_depend>` +
    /// one launch `<node>` per name. May be combined with `--component-name`
    /// (repeatable). At least one component (across both forms) is required.
    #[arg(long, value_delimiter = ',')]
    pub components: Vec<String>,

    /// Repeatable single-component flag. Equivalent to one entry in
    /// `--components`; provided for cases where commas in the shell are
    /// awkward.
    #[arg(long = "component-name")]
    pub component_name: Vec<String>,

    /// Parent directory under which `<name>_bringup/` is created. Defaults to
    /// the current working directory. Mutually compatible with
    /// `--workspace-root` (which controls only the `[workspace] exclude`
    /// update target).
    #[arg(long)]
    pub into: Option<PathBuf>,

    /// Workspace root holding the cargo workspace `Cargo.toml` we should
    /// update. Defaults to the parent dir of the bringup. When no workspace
    /// `Cargo.toml` exists we still scaffold the bringup pkg — only the
    /// `[workspace] exclude` update is skipped.
    #[arg(long)]
    pub workspace_root: Option<PathBuf>,

    /// Skip generating the optional `config/` sub-dir (with its `.gitkeep`).
    #[arg(long)]
    pub no_config: bool,

    /// Skip generating the optional `README.md`.
    #[arg(long)]
    pub no_readme: bool,

    /// Overwrite an existing bringup directory.
    #[arg(long)]
    pub force: bool,
}

pub fn run(args: Args) -> Result<()> {
    let cwd = std::env::current_dir()?;
    // Resolve the parent dir: --into takes precedence over the implicit cwd.
    let into = args.into.clone().unwrap_or_else(|| cwd.clone());
    validate_bringup_name(&args.name)?;
    let bringup_dir = if args.name.is_absolute() {
        args.name.clone()
    } else {
        into.join(&args.name)
    };

    let pkg_name = bringup_dir
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| eyre::eyre!("invalid bringup package name"))?
        .to_string();

    // Workspace-root resolution: explicit flag → parent of bringup dir → cwd.
    let workspace_root = args
        .workspace_root
        .clone()
        .or_else(|| bringup_dir.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| cwd.clone());

    // Merge `--components` + `--component-name` lists (preserving order).
    let mut components: Vec<String> = args.components.clone();
    components.extend(args.component_name.clone());

    let _ = scaffold_bringup(&BringupScaffold {
        bringup_dir,
        pkg_name,
        components,
        workspace_root,
        emit_config: !args.no_config,
        emit_readme: !args.no_readme,
        force: args.force,
    })?;
    Ok(())
}

/// Validate the user-supplied bringup pkg name. Reject path traversal (`..`)
/// and absolute / nested paths — the bringup name must be a single directory
/// segment.
pub fn validate_bringup_name(name: &Path) -> Result<()> {
    let display = name.display();
    if name.as_os_str().is_empty() {
        bail!("bringup package name is empty");
    }
    // Single-component name only: a nested path is ambiguous (`--into` is the
    // way to place the bringup somewhere else).
    let components: Vec<_> = name.components().collect();
    if components.len() != 1 {
        bail!(
            "bringup package name {display:?} must be a single path component — \
             use `--into <dir>` to place the bringup elsewhere"
        );
    }
    let segment = match components[0] {
        std::path::Component::Normal(s) => s.to_string_lossy().into_owned(),
        std::path::Component::CurDir
        | std::path::Component::ParentDir
        | std::path::Component::RootDir
        | std::path::Component::Prefix(_) => {
            bail!(
                "bringup package name {display:?} is not a regular dir name — \
                 reserved components (`.`, `..`, `/`) are rejected"
            );
        }
    };
    // Disallow whitespace and obvious path metacharacters even inside the
    // single segment (defence in depth — the OS would reject most of these
    // anyway, but the diagnostic is cleaner here).
    for bad in ['/', '\\', '\0'] {
        if segment.contains(bad) {
            bail!("bringup package name {segment:?} contains forbidden character {bad:?}");
        }
    }
    if segment == "." || segment == ".." || segment.contains("..") {
        bail!(
            "bringup package name {segment:?} contains `..` or is a reserved \
             dir name"
        );
    }
    Ok(())
}

/// Resolved inputs for [`scaffold_bringup`] — public so tests + the
/// dispatcher (`cmd/new.rs`) can build it without going through clap.
pub struct BringupScaffold {
    pub bringup_dir: PathBuf,
    pub pkg_name: String,
    pub components: Vec<String>,
    pub workspace_root: PathBuf,
    pub emit_config: bool,
    pub emit_readme: bool,
    pub force: bool,
}

/// Result of a successful scaffold — paths written, for tests + UX.
#[derive(Debug)]
pub struct ScaffoldedBringup {
    pub bringup_dir: PathBuf,
    pub package_xml: PathBuf,
    pub system_toml: PathBuf,
    pub launch_file: PathBuf,
    pub gitignore: PathBuf,
    /// `Some(path)` when `--no-config` was NOT set; the `config/.gitkeep` path.
    pub config_gitkeep: Option<PathBuf>,
    /// `Some(path)` when `--no-readme` was NOT set; the `README.md` path.
    pub readme: Option<PathBuf>,
    /// `Some(path)` when a workspace `Cargo.toml` existed and got updated.
    pub workspace_cargo_toml: Option<PathBuf>,
}

pub fn scaffold_bringup(s: &BringupScaffold) -> Result<ScaffoldedBringup> {
    if s.components.is_empty() {
        bail!("at least one --components <pkg> (or --component-name <pkg>) is required");
    }
    for c in &s.components {
        if c.trim().is_empty() {
            bail!("empty component name in --components / --component-name list");
        }
    }

    if s.bringup_dir.exists() {
        if !s.force {
            bail!(
                "bringup directory already exists at {} — pass --force to overwrite",
                s.bringup_dir.display()
            );
        }
        fs::remove_dir_all(&s.bringup_dir)
            .wrap_err_with(|| format!("remove existing {} for --force", s.bringup_dir.display()))?;
    }

    fs::create_dir_all(s.bringup_dir.join("launch"))
        .wrap_err_with(|| format!("create {}", s.bringup_dir.display()))?;

    let package_xml = s.bringup_dir.join("package.xml");
    fs::write(&package_xml, render_package_xml(&s.pkg_name, &s.components))
        .wrap_err_with(|| format!("write {}", package_xml.display()))?;

    let system_toml = s.bringup_dir.join("system.toml");
    fs::write(
        &system_toml,
        render_system_toml(&s.pkg_name, &s.components)?,
    )
    .wrap_err_with(|| format!("write {}", system_toml.display()))?;

    let launch_file = s.bringup_dir.join("launch").join("system.launch.xml");
    fs::write(&launch_file, render_launch_xml(&s.pkg_name, &s.components))
        .wrap_err_with(|| format!("write {}", launch_file.display()))?;

    let gitignore = s.bringup_dir.join(".gitignore");
    fs::write(&gitignore, "/target/\n/build/\n")
        .wrap_err_with(|| format!("write {}", gitignore.display()))?;

    let config_gitkeep = if s.emit_config {
        let dir = s.bringup_dir.join("config");
        fs::create_dir_all(&dir).wrap_err_with(|| format!("create {}", dir.display()))?;
        let keep = dir.join(".gitkeep");
        fs::write(&keep, "").wrap_err_with(|| format!("write {}", keep.display()))?;
        Some(keep)
    } else {
        None
    };

    let readme = if s.emit_readme {
        let path = s.bringup_dir.join("README.md");
        fs::write(&path, render_readme(&s.pkg_name, &s.components))
            .wrap_err_with(|| format!("write {}", path.display()))?;
        Some(path)
    } else {
        None
    };

    let workspace_cargo_toml = add_bringup_to_workspace_exclude(&s.workspace_root, &s.pkg_name)?;

    Ok(ScaffoldedBringup {
        bringup_dir: s.bringup_dir.clone(),
        package_xml,
        system_toml,
        launch_file,
        gitignore,
        config_gitkeep,
        readme,
        workspace_cargo_toml,
    })
}

fn render_package_xml(pkg_name: &str, components: &[String]) -> String {
    let mut s = String::new();
    s.push_str("<?xml version=\"1.0\"?>\n");
    s.push_str(
        "<package format=\"3\">\n\
         <!-- generated by nros new system — Phase 212.F -->\n",
    );
    s.push_str(&format!("  <name>{pkg_name}</name>\n"));
    s.push_str("  <version>0.1.0</version>\n");
    s.push_str(&format!(
        "  <description>Bringup package for {pkg_name} (declarative system spec).</description>\n"
    ));
    s.push_str("  <maintainer email=\"nobody@example.invalid\">TODO maintainer</maintainer>\n");
    s.push_str("  <license>Apache-2.0</license>\n");
    // J.5 policy: NO <buildtool_depend>ament_cmake</buildtool_depend>.
    // nano-ros planning reads launch/ directly from source; users wanting ROS 2
    // `ros2 launch <bringup>` add the buildtool_depend manually.
    for c in components {
        s.push_str(&format!("  <exec_depend>{c}</exec_depend>\n"));
    }
    s.push_str("  <export>\n");
    s.push_str("    <build_type>ament_nros</build_type>\n");
    s.push_str("  </export>\n");
    s.push_str("</package>\n");
    s
}

fn render_system_toml(pkg_name: &str, components: &[String]) -> Result<String> {
    // The pkg_name conventionally ends in `_bringup`; the system name strips
    // that suffix (`demo_bringup` → `demo`). When the convention is broken we
    // fall back to the full name.
    let system_name = pkg_name
        .strip_suffix("_bringup")
        .or_else(|| pkg_name.strip_suffix("_launch"))
        .unwrap_or(pkg_name)
        .to_string();

    let entries: Vec<SystemComponentEntry> = components
        .iter()
        .map(|pkg| SystemComponentEntry {
            pkg: pkg.clone(),
            // Placeholder — user fills in real Rust path / C++ class. Documented
            // as TODO in the file header below.
            class: format!("{pkg}::TODO"),
            // Default the node name to the package name; the user typically
            // edits it to drop the `_pkg` suffix.
            name: pkg.clone(),
            group_tiers: std::collections::BTreeMap::new(),
        })
        .collect();

    let mut deploy: std::collections::BTreeMap<String, DeployTarget> =
        std::collections::BTreeMap::new();
    deploy.insert(
        "native".to_string(),
        DeployTarget {
            kind: Some("self".to_string()),
            target: None,
            launch: None,
            board: None,
            framework: None,
            rmw: None,
            profile: None,
            optimize: None,
            features: Vec::new(),
            domain_id: None,
            locator: None,
        },
    );

    let model = SystemToml {
        system: SystemHeader {
            name: system_name,
            rmw: "zenoh".to_string(),
            domain_id: 0,
            locator: None,
            default_launch: Some("system.launch.xml".to_string()),
            default_target: Some("native".to_string()),
            features: Vec::new(),
        },
        components: entries,
        deploy,
        domains: Vec::new(),
        bridges: Vec::new(),
        tiers: std::collections::BTreeMap::new(),
        node_overrides: Vec::new(),
        safety: None,
        param_services: None,
        lifecycle: None,
    };

    let body = toml::to_string_pretty(&model).wrap_err("serialize generated system.toml")?;

    let mut out = String::new();
    out.push_str("# generated by nros new system — Phase 212.F\n");
    out.push_str("# TODO: fill in real component `class` paths (Rust module-path / C++ class).\n");
    out.push_str("# See docs/design/0025-workspace-layout-by-case.md §3 for the full schema.\n");
    out.push_str("\n");
    out.push_str(&body);
    Ok(out)
}

fn render_launch_xml(pkg_name: &str, components: &[String]) -> String {
    let mut s = String::new();
    s.push_str("<?xml version=\"1.0\"?>\n");
    s.push_str(&format!(
        "<!-- generated by nros new system — bringup pkg {pkg_name}, Phase 212.F -->\n"
    ));
    s.push_str("<launch>\n");
    for c in components {
        // node name = pkg name (user edits later); exec = pkg name. Matches
        // the `cmake` Case 4 example in 0025-workspace-layout-by-case.md.
        s.push_str(&format!(
            "  <node pkg=\"{c}\" exec=\"{c}\" name=\"{c}\" />\n"
        ));
    }
    s.push_str("</launch>\n");
    s
}

fn render_readme(pkg_name: &str, components: &[String]) -> String {
    let component_list = components
        .iter()
        .map(|c| format!("- `{c}`"))
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "# {pkg_name}\n\
         \n\
         This is a **bringup package** — a pure-declarative system spec for the\n\
         nano-ros multi-node workspace. It carries:\n\
         \n\
         - `package.xml` — ROS 2 manifest. Hand-edit `<exec_depend>` to match\n\
           `system.toml` `[[component]] pkg`.\n\
         - `system.toml` — components, RMW, deploy targets, bridges. See\n\
           `docs/design/0024-multi-node-workspace-layout.md` §4 and §11.3 for the\n\
           full schema.\n\
         - `launch/*.launch.xml` — ROS 2 launch XML, consumed by `nros plan`\n\
           (or `ros2 launch` after a colcon install). The default is\n\
           `system.launch.xml`; add more launch files for alternate\n\
           topologies (nav2 convention; see design doc §11.3).\n\
         - `config/` — optional `params.yaml`, rviz, etc.\n\
         \n\
         No `Cargo.toml`, no `CMakeLists.txt`, no `src/`. Code goes in sibling\n\
         **Node packages** (one per `[[component]]`).\n\
         \n\
         ## Components\n\
         \n\
         {component_list}\n\
         \n\
         ## Editing\n\
         \n\
         1. Open `system.toml` and replace each `[[component]] class = \"<pkg>::TODO\"`\n\
            with the real Rust module-path / C++ class name.\n\
         2. Open `launch/system.launch.xml` and adjust `<node exec=\"…\">` to\n\
            match each Entry pkg binary name.\n\
         3. Run `nros check` to validate the bringup is well-formed.\n\
         4. Run the Entry package binary, for example `cargo run -p <entry_pkg>`.\n",
    )
}

/// Append `pkg_name` to the workspace-root `Cargo.toml`'s `[workspace] exclude`
/// list (Path A). Returns `None` when no workspace `Cargo.toml` lives at the
/// given root (still allowed — the bringup pkg works fine outside a cargo
/// workspace, e.g. for pure-C++ users).
fn add_bringup_to_workspace_exclude(
    workspace_root: &Path,
    pkg_name: &str,
) -> Result<Option<PathBuf>> {
    let cargo_toml = workspace_root.join("Cargo.toml");
    if !cargo_toml.is_file() {
        return Ok(None);
    }

    let raw = fs::read_to_string(&cargo_toml)
        .wrap_err_with(|| format!("read {}", cargo_toml.display()))?;
    let mut doc: DocumentMut = raw
        .parse()
        .wrap_err_with(|| format!("parse {}", cargo_toml.display()))?;

    // Ensure `[workspace]` table.
    let workspace = doc
        .entry("workspace")
        .or_insert_with(|| toml_edit::Item::Table(toml_edit::Table::new()))
        .as_table_mut()
        .ok_or_else(|| eyre::eyre!("[workspace] is not a table in {}", cargo_toml.display()))?;

    // Read existing exclude list (default empty).
    let mut exclude_array = workspace
        .get("exclude")
        .and_then(|i| i.as_array())
        .cloned()
        .unwrap_or_else(Array::new);

    let already = exclude_array.iter().any(|v| v.as_str() == Some(pkg_name));
    if !already {
        exclude_array.push(pkg_name);
    }
    workspace["exclude"] = value(exclude_array);

    fs::write(&cargo_toml, doc.to_string())
        .wrap_err_with(|| format!("write {}", cargo_toml.display()))?;

    Ok(Some(cargo_toml))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_root(tag: &str) -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "nros-new-system-{tag}-{}-{stamp}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write_workspace_cargo_toml(root: &Path) {
        fs::write(
            root.join("Cargo.toml"),
            "[workspace]\nresolver = \"2\"\nmembers = [\"talker_pkg\", \"listener_pkg\"]\n",
        )
        .unwrap();
    }

    fn default_scaffold(root: &Path) -> BringupScaffold {
        BringupScaffold {
            bringup_dir: root.join("demo_bringup"),
            pkg_name: "demo_bringup".to_string(),
            components: vec!["talker_pkg".to_string(), "listener_pkg".to_string()],
            workspace_root: root.to_path_buf(),
            emit_config: true,
            emit_readme: true,
            force: false,
        }
    }

    // -----------------------------------------------------------------------
    // Spec tests — exact names from Phase 212.F.1 task brief.
    // -----------------------------------------------------------------------

    /// Spec test: invoking with 2 components produces every required file
    /// with the documented content.
    #[test]
    fn nros_new_system_scaffolds_bringup_pkg() {
        let root = temp_root("scaffolds_bringup_pkg");
        write_workspace_cargo_toml(&root);

        let out = scaffold_bringup(&default_scaffold(&root)).expect("scaffold succeeds");

        // 1. Expected tree exists.
        assert!(
            out.package_xml.is_file(),
            "package.xml: {:?}",
            out.package_xml
        );
        assert!(
            out.system_toml.is_file(),
            "system.toml: {:?}",
            out.system_toml
        );
        assert!(
            out.launch_file.is_file(),
            "launch xml: {:?}",
            out.launch_file
        );
        assert!(out.gitignore.is_file(), ".gitignore: {:?}", out.gitignore);
        assert!(out.config_gitkeep.is_some());
        assert!(out.config_gitkeep.as_ref().unwrap().is_file());
        assert!(out.readme.is_some());
        assert!(out.readme.as_ref().unwrap().is_file());

        // 2. NO forbidden files.
        assert!(!out.bringup_dir.join("Cargo.toml").exists());
        assert!(!out.bringup_dir.join("CMakeLists.txt").exists());
        assert!(!out.bringup_dir.join("src").exists());

        // 3. package.xml content.
        let pkg_xml = fs::read_to_string(&out.package_xml).unwrap();
        assert!(pkg_xml.contains("<name>demo_bringup</name>"));
        assert!(pkg_xml.contains("<description>"));
        assert!(pkg_xml.contains("<maintainer "));
        assert!(pkg_xml.contains("<license>"));
        assert!(pkg_xml.contains("<exec_depend>talker_pkg</exec_depend>"));
        assert!(pkg_xml.contains("<exec_depend>listener_pkg</exec_depend>"));

        // 4. system.toml round-trips + carries the 2026-06-03 fields.
        let sys: SystemToml =
            toml::from_str(&fs::read_to_string(&out.system_toml).unwrap()).unwrap();
        assert_eq!(sys.system.name, "demo");
        assert_eq!(
            sys.system.default_launch.as_deref(),
            Some("system.launch.xml")
        );
        assert_eq!(sys.system.default_target.as_deref(), Some("native"));
        assert!(sys.deploy.contains_key("native"));
        assert_eq!(sys.components.len(), 2);
        assert_eq!(sys.components[0].pkg, "talker_pkg");
        assert_eq!(sys.components[1].pkg, "listener_pkg");

        // 5. launch xml has one <node> per component.
        let lx = fs::read_to_string(&out.launch_file).unwrap();
        assert_eq!(lx.matches("<node ").count(), 2);
        assert!(lx.contains("pkg=\"talker_pkg\""));
        assert!(lx.contains("pkg=\"listener_pkg\""));

        // 6. .gitignore is the documented two-liner.
        let gi = fs::read_to_string(&out.gitignore).unwrap();
        assert!(gi.contains("/target/"));
        assert!(gi.contains("/build/"));
    }

    /// Spec test: bringup names containing path traversal / nesting are
    /// rejected with a clean error.
    #[test]
    fn nros_new_system_rejects_invalid_name() {
        // `foo/bar` — multi-component path.
        assert!(validate_bringup_name(Path::new("foo/bar")).is_err());
        // `..` — parent dir traversal.
        assert!(validate_bringup_name(Path::new("..")).is_err());
        // `./demo` — current-dir prefix.
        assert!(validate_bringup_name(Path::new("./demo")).is_err());
        // `/abs/path` — absolute.
        assert!(validate_bringup_name(Path::new("/abs/demo_bringup")).is_err());
        // Empty.
        assert!(validate_bringup_name(Path::new("")).is_err());
        // Clean name passes.
        assert!(validate_bringup_name(Path::new("demo_bringup")).is_ok());
    }

    /// Spec test: the J.5 policy — generated `package.xml` MUST NOT include
    /// `<buildtool_depend>ament_cmake</buildtool_depend>`.
    #[test]
    fn nros_new_system_no_buildtool_depend() {
        let root = temp_root("no_buildtool_depend");
        let out = scaffold_bringup(&default_scaffold(&root)).unwrap();
        let pkg_xml = fs::read_to_string(&out.package_xml).unwrap();
        assert!(
            !pkg_xml.contains("<buildtool_depend>"),
            "package.xml must NOT include <buildtool_depend> per J.5 policy; got:\n{pkg_xml}"
        );
        assert!(
            !pkg_xml.contains("ament_cmake"),
            "package.xml must NOT mention ament_cmake; got:\n{pkg_xml}"
        );
    }

    /// Spec test: `--no-config` omits the `config/` dir entirely.
    #[test]
    fn nros_new_system_skips_config_with_flag() {
        let root = temp_root("skips_config");
        let mut s = default_scaffold(&root);
        s.emit_config = false;
        let out = scaffold_bringup(&s).unwrap();
        assert!(out.config_gitkeep.is_none());
        assert!(
            !out.bringup_dir.join("config").exists(),
            "config/ dir must not exist when --no-config is set"
        );
        // README still emitted by default.
        assert!(out.readme.is_some());
    }

    // -----------------------------------------------------------------------
    // Existing pre-spec tests retained — guard against regressions.
    // -----------------------------------------------------------------------

    #[test]
    fn nros_new_system_adds_to_workspace_exclude() {
        let root = temp_root("workspace_exclude");
        write_workspace_cargo_toml(&root);

        let out = scaffold_bringup(&BringupScaffold {
            components: vec!["talker_pkg".to_string()],
            ..default_scaffold(&root)
        })
        .unwrap();

        let cargo_toml_path = out
            .workspace_cargo_toml
            .as_ref()
            .expect("workspace Cargo.toml exists");
        let after = fs::read_to_string(cargo_toml_path).unwrap();
        let doc: toml::Value = toml::from_str(&after).unwrap();
        let exclude = doc
            .get("workspace")
            .and_then(|w| w.get("exclude"))
            .and_then(|e| e.as_array())
            .expect("[workspace] exclude is an array");
        let names: Vec<&str> = exclude.iter().filter_map(|v| v.as_str()).collect();
        assert!(
            names.contains(&"demo_bringup"),
            "exclude must contain demo_bringup, got: {names:?}"
        );

        // Path A guard: bringup NOT in [workspace] members.
        let members = doc
            .get("workspace")
            .and_then(|w| w.get("members"))
            .and_then(|m| m.as_array())
            .expect("members present");
        let member_names: Vec<&str> = members.iter().filter_map(|v| v.as_str()).collect();
        assert!(
            !member_names.contains(&"demo_bringup"),
            "Path A means bringup must NOT be in [workspace] members"
        );
    }

    #[test]
    fn nros_new_system_without_workspace_cargo_toml_still_works() {
        let root = temp_root("no_workspace");
        // intentionally no Cargo.toml in root
        let out = scaffold_bringup(&BringupScaffold {
            components: vec!["talker_pkg".to_string()],
            ..default_scaffold(&root)
        })
        .unwrap();
        assert!(out.workspace_cargo_toml.is_none());
        assert!(out.package_xml.is_file());
    }

    #[test]
    fn nros_new_system_rejects_existing_dir_without_force() {
        let root = temp_root("no_force");
        let dir = root.join("demo_bringup");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("STAY"), "pre-existing").unwrap();
        let err = scaffold_bringup(&BringupScaffold {
            bringup_dir: dir,
            ..default_scaffold(&root)
        })
        .unwrap_err();
        assert!(
            err.to_string().contains("already exists"),
            "diagnostic: {err}"
        );
    }

    #[test]
    fn nros_new_system_skips_readme_with_flag() {
        let root = temp_root("skips_readme");
        let mut s = default_scaffold(&root);
        s.emit_readme = false;
        let out = scaffold_bringup(&s).unwrap();
        assert!(out.readme.is_none());
        assert!(!out.bringup_dir.join("README.md").exists());
    }
}
