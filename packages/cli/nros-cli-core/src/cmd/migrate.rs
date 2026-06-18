//! Phase 212.I — `nros migrate workspace` migration tool.
//!
//! Converts a pre-212 workspace (root `nros.toml` + per-package
//! `component_nros.toml` / `nros/components/*.toml` + committed `metadata/*.json`
//! + hand-maintained `package.xml`) to the post-212 shape (workspace-root
//! `Cargo.toml` `[workspace.metadata.nros]`, per-package
//! `Cargo.toml.[package.metadata.nros.node(s)]` +
//! `[package.metadata.ament]`, regenerated `package.xml`, dedicated
//! `<bringup>/system.toml` + `<bringup>/launch/system.launch.xml`).
//!
//! Post-N.12 (Component → Node rename, 2026-06-03): the migrator emits the
//! **new** `node` / `nodes` spelling for `[package.metadata.nros]` entries —
//! this verb's whole purpose is to land a current-shape workspace, so it
//! writes the post-rename terminology even though the legacy input files we
//! read still call the field `component` / `components`. Pre-212
//! `component_nros.toml` inputs keep the legacy `component = "..."` field
//! name (pre-rename); we accept `node = "..."` as a deserialize alias so a
//! partially-edited pre-212 tree still migrates.
//!
//! See `docs/roadmap/phase-212-ux-cargo-native-and-file-consolidation.md` §212.I.

use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use clap::Args as ClapArgs;
use eyre::{Context, Result, bail};
use serde::Deserialize;
use toml_edit::{Array, DocumentMut, Item, Table, value};

use crate::{
    cmd::emit_package_xml,
    orchestration::cargo_metadata_schema::{
        PackageMetadataAment, SystemBridgeEntry, SystemComponentEntry, SystemDomainEntry,
        SystemHeader, SystemToml,
    },
};

#[derive(Debug, ClapArgs)]
pub struct Args {
    /// Workspace root containing the pre-212 `nros.toml` (default: cwd).
    #[arg(default_value = ".")]
    pub dir: PathBuf,

    /// Print the migration plan, do not modify any files.
    #[arg(long)]
    pub dry_run: bool,

    /// Override the auto-derived bringup package name
    /// (`<system_name>_bringup`).
    #[arg(long)]
    pub bringup_name: Option<String>,

    /// Re-run on an already-migrated tree.
    #[arg(long)]
    pub force: bool,
}

pub fn run(args: Args) -> Result<()> {
    let ws = args.dir.canonicalize().unwrap_or(args.dir.clone());

    if !args.force && is_already_migrated(&ws) {
        println!(
            "nros migrate workspace: {} already migrated (workspace.metadata.nros present); \
             pass --force to re-run.",
            ws.display()
        );
        return Ok(());
    }

    let plan = build_plan(&ws, args.bringup_name.as_deref())?;
    print_plan(&plan);

    if args.dry_run {
        println!("nros migrate workspace: --dry-run, no files written.");
        return Ok(());
    }

    apply_plan(&plan)?;
    println!(
        "nros migrate workspace: migrated {} ({} step(s)).",
        ws.display(),
        plan.steps.len()
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// Plan
// ---------------------------------------------------------------------------

#[derive(Debug)]
struct MigrationPlan {
    ws: PathBuf,
    bringup_dir: PathBuf,
    bringup_pkg_name: String,
    system_header: SystemHeader,
    deploy: BTreeMap<String, crate::orchestration::cargo_metadata_schema::DeployTarget>,
    domains: Vec<SystemDomainEntry>,
    bridges: Vec<SystemBridgeEntry>,
    /// Resolved component-package directories (e.g. `<ws>/src/demo_pkg`).
    pkg_dirs: Vec<PathBuf>,
    /// Pre-collected component manifests per pkg dir.
    components_by_pkg: BTreeMap<PathBuf, Vec<PreComponent>>,
    /// Launch file (relative to ws), if `[system].launch` resolved to one.
    launch_src: Option<PathBuf>,
    steps: Vec<Step>,
}

#[derive(Debug, Clone)]
struct PreComponent {
    /// Source file (e.g. `<pkg>/component_nros.toml` or
    /// `<pkg>/nros/components/Talker.toml`).
    src: PathBuf,
    /// Parsed manifest (subset).
    cfg: LegacyComponentConfig,
}

/// Permissive — pre-212 manifests carry various extra fields we don't read.
///
/// Post-N.12 the canonical instance-name field is `node`, but pre-212 files
/// were authored with `component`. We accept either spelling via a serde
/// alias — pre-212 files that have already been hand-edited to the new
/// `node = "..."` field name still parse, while untouched pre-212 files keep
/// working.
#[derive(Debug, Clone, Deserialize)]
struct LegacyComponentConfig {
    package: String,
    #[serde(alias = "node")]
    component: String,
    #[serde(default)]
    linkage: Option<toml::Value>,
    #[serde(default)]
    overrides: Option<LegacyOverrides>,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct LegacyOverrides {
    #[serde(default)]
    default_namespace: Option<String>,
    #[serde(default)]
    parameters: BTreeMap<String, toml::Value>,
    #[serde(default)]
    remaps: Vec<LegacyRemap>,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct LegacyRemap {
    from: String,
    to: String,
}

#[derive(Debug, Clone)]
enum Step {
    WriteSystemToml,
    WriteBringupPackageXml,
    MoveLaunchXml { from: PathBuf, to: PathBuf },
    PatchPackageCargoToml { pkg_dir: PathBuf },
    DeleteComponentManifest { path: PathBuf },
    DeleteMetadataDir { path: PathBuf },
    RegeneratePackageXml { pkg_dir: PathBuf },
    DeleteNrosToml,
    PatchWorkspaceCargoToml,
}

/// Permissive pre-212 `nros.toml` shape — only the fields we lift forward.
#[derive(Debug, Deserialize)]
struct LegacyWorkspaceNrosToml {
    system: LegacySystem,
    #[serde(default)]
    deploy: BTreeMap<String, toml::Value>,
}

#[derive(Debug, Deserialize)]
struct LegacySystem {
    #[serde(default)]
    launch: Option<String>,
    #[serde(default)]
    components: Vec<String>,
    #[serde(default)]
    rmw: Option<String>,
    #[serde(default)]
    domain_id: Option<u32>,
    #[serde(default)]
    locator: Option<String>,
}

fn build_plan(ws: &Path, bringup_override: Option<&str>) -> Result<MigrationPlan> {
    let nros_toml = ws.join("nros.toml");
    if !nros_toml.is_file() {
        bail!(
            "no pre-212 nros.toml at {} — nothing to migrate",
            nros_toml.display()
        );
    }
    let raw =
        fs::read_to_string(&nros_toml).with_context(|| format!("read {}", nros_toml.display()))?;
    let legacy: LegacyWorkspaceNrosToml =
        toml::from_str(&raw).with_context(|| format!("parse {}", nros_toml.display()))?;

    // System name derivation: the spec says default = `<system_name_from_nros_toml>_bringup`.
    // The pre-212 schema has no `[system].name`, so we prefer the first
    // component package name (matches the e2e/composable fixtures' implicit
    // naming) and fall back to the launch-file stem when no components are
    // declared yet.
    let system_name = legacy
        .system
        .components
        .first()
        .cloned()
        .or_else(|| {
            legacy
                .system
                .launch
                .as_deref()
                .and_then(|p| Path::new(p).file_stem().and_then(|n| n.to_str()))
                .map(|s| s.trim_end_matches(".launch").to_string())
        })
        .unwrap_or_else(|| "demo".to_string());

    let bringup_pkg_name = bringup_override
        .map(str::to_string)
        .unwrap_or_else(|| format!("{system_name}_bringup"));
    let bringup_dir = ws.join(&bringup_pkg_name);

    // Walk component packages.
    let mut pkg_dirs = Vec::new();
    let mut components_by_pkg: BTreeMap<PathBuf, Vec<PreComponent>> = BTreeMap::new();
    for pkg_name in &legacy.system.components {
        let pkg_dir = ws.join("src").join(pkg_name);
        if !pkg_dir.is_dir() {
            bail!(
                "component pkg dir not found: {} (referenced in [system].components)",
                pkg_dir.display()
            );
        }
        let comps = discover_components_for_pkg(&pkg_dir)?;
        if comps.is_empty() {
            bail!(
                "no component manifests found under {} (looked for component_nros.toml + \
                 nros/components/*.toml)",
                pkg_dir.display()
            );
        }
        components_by_pkg.insert(pkg_dir.clone(), comps);
        pkg_dirs.push(pkg_dir);
    }

    // Resolve launch source. The pre-212 fixtures keep it in
    // `<first pkg>/launch/system.launch.xml`. We move it into the bringup.
    let launch_src = legacy
        .system
        .launch
        .as_deref()
        .map(|p| ws.join(p))
        .filter(|p| p.is_file());

    // Convert [deploy] tables. We keep keys + parse what we can.
    let deploy = legacy
        .deploy
        .iter()
        .filter_map(|(name, val)| convert_deploy(val).map(|d| (name.clone(), d)))
        .collect();

    let system_header = SystemHeader {
        name: system_name,
        rmw: legacy.system.rmw.unwrap_or_else(|| "zenoh".into()),
        domain_id: legacy.system.domain_id.unwrap_or(0),
        locator: legacy.system.locator,
        default_launch: None,
        default_target: None,
        features: Vec::new(),
    };

    let mut steps = vec![Step::WriteSystemToml, Step::WriteBringupPackageXml];
    if let Some(src) = &launch_src {
        steps.push(Step::MoveLaunchXml {
            from: src.clone(),
            to: bringup_dir.join("launch").join("system.launch.xml"),
        });
    }
    for pkg_dir in &pkg_dirs {
        steps.push(Step::PatchPackageCargoToml {
            pkg_dir: pkg_dir.clone(),
        });
        // Delete component manifests for this pkg.
        for comp in &components_by_pkg[pkg_dir] {
            steps.push(Step::DeleteComponentManifest {
                path: comp.src.clone(),
            });
        }
        let metadata_dir = pkg_dir.join("metadata");
        if metadata_dir.is_dir() {
            steps.push(Step::DeleteMetadataDir { path: metadata_dir });
        }
        steps.push(Step::RegeneratePackageXml {
            pkg_dir: pkg_dir.clone(),
        });
    }
    steps.push(Step::DeleteNrosToml);
    steps.push(Step::PatchWorkspaceCargoToml);

    Ok(MigrationPlan {
        ws: ws.to_path_buf(),
        bringup_dir,
        bringup_pkg_name,
        system_header,
        deploy,
        domains: Vec::new(),
        bridges: Vec::new(),
        pkg_dirs,
        components_by_pkg,
        launch_src,
        steps,
    })
}

fn convert_deploy(
    raw: &toml::Value,
) -> Option<crate::orchestration::cargo_metadata_schema::DeployTarget> {
    let tbl = raw.as_table()?;
    let kind = tbl.get("kind")?.as_str()?.to_string();
    let target = tbl.get("target")?.as_str()?.to_string();
    Some(crate::orchestration::cargo_metadata_schema::DeployTarget {
        kind: Some(kind),
        target: Some(target),
        launch: tbl
            .get("launch")
            .and_then(|v| v.as_str())
            .map(str::to_string),
        board: tbl
            .get("board")
            .and_then(|v| v.as_str())
            .map(str::to_string),
        framework: tbl
            .get("framework")
            .and_then(|v| v.as_str())
            .map(str::to_string),
        // Phase 255 — migrate a per-deploy rmw override if the legacy block had one.
        rmw: tbl.get("rmw").and_then(|v| v.as_str()).map(str::to_string),
        // Phase 256 W3 — migrate per-target build tuning from the legacy block.
        profile: tbl
            .get("profile")
            .and_then(|v| v.as_str())
            .map(str::to_string),
        optimize: tbl
            .get("optimize")
            .and_then(|v| v.as_str())
            .map(str::to_string),
        features: tbl
            .get("features")
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default(),
        // Phase 256 W8 — migrate per-deploy domain/locator overrides.
        domain_id: tbl
            .get("domain_id")
            .and_then(|v| v.as_integer())
            .map(|n| n as u32),
        locator: tbl
            .get("locator")
            .and_then(|v| v.as_str())
            .map(str::to_string),
    })
}

fn discover_components_for_pkg(pkg_dir: &Path) -> Result<Vec<PreComponent>> {
    let mut out = Vec::new();
    // Single-component form.
    let folded = pkg_dir.join("component_nros.toml");
    if folded.is_file() {
        let raw =
            fs::read_to_string(&folded).with_context(|| format!("read {}", folded.display()))?;
        let cfg: LegacyComponentConfig =
            toml::from_str(&raw).with_context(|| format!("parse {}", folded.display()))?;
        out.push(PreComponent { src: folded, cfg });
    }
    // Multi-component form.
    let multi_dir = pkg_dir.join("nros").join("components");
    if multi_dir.is_dir() {
        let mut entries: Vec<_> = fs::read_dir(&multi_dir)?
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("toml"))
            .collect();
        entries.sort();
        for path in entries {
            let raw =
                fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
            let cfg: LegacyComponentConfig =
                toml::from_str(&raw).with_context(|| format!("parse {}", path.display()))?;
            out.push(PreComponent { src: path, cfg });
        }
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Plan rendering
// ---------------------------------------------------------------------------

fn print_plan(plan: &MigrationPlan) {
    let ws = &plan.ws;
    let rel = |p: &Path| -> String { p.strip_prefix(ws).unwrap_or(p).display().to_string() };
    println!("nros migrate workspace plan:");
    for step in &plan.steps {
        match step {
            Step::WriteSystemToml => println!(
                "  read    nros.toml → write {}/system.toml",
                rel(&plan.bringup_dir)
            ),
            Step::WriteBringupPackageXml => println!(
                "  write   {}/package.xml (via nros emit package-xml)",
                rel(&plan.bringup_dir)
            ),
            Step::MoveLaunchXml { from, to } => {
                println!("  move    {} → {}", rel(from), rel(to))
            }
            Step::PatchPackageCargoToml { pkg_dir } => println!(
                "  patch   {}/Cargo.toml [package.metadata.nros.node(s)] + \
                 [package.metadata.ament]",
                rel(pkg_dir)
            ),
            Step::DeleteComponentManifest { path } => {
                println!("  delete  {}", rel(path))
            }
            Step::DeleteMetadataDir { path } => {
                println!("  delete  {}/", rel(path))
            }
            Step::RegeneratePackageXml { pkg_dir } => println!(
                "  write   {}/package.xml (regenerated via nros emit package-xml)",
                rel(pkg_dir)
            ),
            Step::DeleteNrosToml => println!("  delete  nros.toml"),
            Step::PatchWorkspaceCargoToml => println!(
                "  patch   Cargo.toml [workspace] exclude = [\"{}\"], [workspace.metadata.nros] \
                 default_system = \"{}\"",
                plan.bringup_pkg_name, plan.bringup_pkg_name
            ),
        }
    }
}

// ---------------------------------------------------------------------------
// Apply
// ---------------------------------------------------------------------------

fn apply_plan(plan: &MigrationPlan) -> Result<()> {
    // Step 1: build + write system.toml (use SystemToml model).
    fs::create_dir_all(plan.bringup_dir.join("launch"))
        .with_context(|| format!("create {}", plan.bringup_dir.display()))?;
    let system = build_system_toml(plan);
    let toml_str = toml::to_string_pretty(&system).wrap_err("serialize generated system.toml")?;
    let header = "# generated by nros migrate workspace — Phase 212.I\n\n";
    fs::write(
        plan.bringup_dir.join("system.toml"),
        format!("{header}{toml_str}"),
    )?;

    // Step 2: move launch file (if any).
    if let Some(src) = &plan.launch_src {
        let dst = plan.bringup_dir.join("launch").join("system.launch.xml");
        if src != &dst {
            fs::copy(src, &dst)
                .with_context(|| format!("copy {} → {}", src.display(), dst.display()))?;
            fs::remove_file(src).with_context(|| format!("remove old launch {}", src.display()))?;
        }
    }

    // Step 3: per-pkg: patch Cargo.toml, delete component manifests, delete metadata/, regen package.xml.
    for pkg_dir in &plan.pkg_dirs {
        patch_package_cargo_toml(pkg_dir, &plan.components_by_pkg[pkg_dir])?;
        for comp in &plan.components_by_pkg[pkg_dir] {
            if comp.src.is_file() {
                fs::remove_file(&comp.src)
                    .with_context(|| format!("remove {}", comp.src.display()))?;
            }
        }
        // Also remove the now-empty nros/components dir.
        let nros_components = pkg_dir.join("nros").join("components");
        if nros_components.is_dir() {
            let _ = fs::remove_dir(&nros_components);
            let _ = fs::remove_dir(pkg_dir.join("nros"));
        }
        let metadata_dir = pkg_dir.join("metadata");
        if metadata_dir.is_dir() {
            fs::remove_dir_all(&metadata_dir)
                .with_context(|| format!("remove {}", metadata_dir.display()))?;
        }
        // Regenerate package.xml via emit_package_xml.
        let regenerated = emit_package_xml::render_for_pkg(pkg_dir)
            .with_context(|| format!("regenerate {}/package.xml", pkg_dir.display()))?;
        fs::write(pkg_dir.join("package.xml"), regenerated)
            .with_context(|| format!("write {}/package.xml", pkg_dir.display()))?;
    }

    // Step 4: write bringup package.xml AFTER system.toml is on disk.
    let bringup_pkg_xml = emit_package_xml::render_for_pkg(&plan.bringup_dir)
        .with_context(|| format!("render {}/package.xml", plan.bringup_dir.display()))?;
    fs::write(plan.bringup_dir.join("package.xml"), bringup_pkg_xml)?;

    // Step 5: delete root nros.toml.
    let root_nros = plan.ws.join("nros.toml");
    if root_nros.is_file() {
        fs::remove_file(&root_nros).with_context(|| format!("remove {}", root_nros.display()))?;
    }

    // Step 6: patch (or create) workspace-root Cargo.toml.
    patch_workspace_cargo_toml(&plan.ws, &plan.bringup_pkg_name, &plan.pkg_dirs)?;

    Ok(())
}

fn build_system_toml(plan: &MigrationPlan) -> SystemToml {
    // Build component entries from each PreComponent.
    let mut components = Vec::new();
    for pkg_dir in &plan.pkg_dirs {
        for c in &plan.components_by_pkg[pkg_dir] {
            let class = pkg_class_path(&c.cfg);
            components.push(SystemComponentEntry {
                pkg: c.cfg.package.clone(),
                class,
                name: c.cfg.component.clone(),
            });
        }
    }
    SystemToml {
        system: plan.system_header.clone(),
        components,
        deploy: plan.deploy.clone(),
        domains: plan.domains.clone(),
        bridges: plan.bridges.clone(),
        tiers: std::collections::BTreeMap::new(),
        node_overrides: Vec::new(),
        safety: None,
        param_services: None,
        lifecycle: None,
    }
}

/// Derive a Rust-module-path style `class` (`<pkg>::<Component>`) from the
/// legacy linkage block when available, else fall back to `<pkg>::<comp>`.
fn pkg_class_path(cfg: &LegacyComponentConfig) -> String {
    if let Some(linkage) = &cfg.linkage
        && let Some(sym) = linkage
            .as_table()
            .and_then(|t| t.get("exported_symbol"))
            .and_then(|v| v.as_str())
    {
        // `nros_component_<name>` → strip the prefix to get the component name.
        if let Some(stripped) = sym.strip_prefix("nros_component_") {
            return format!("{}::{}", cfg.package, stripped);
        }
    }
    format!("{}::{}", cfg.package, cfg.component)
}

// ---------------------------------------------------------------------------
// Cargo.toml patching
// ---------------------------------------------------------------------------

fn patch_package_cargo_toml(pkg_dir: &Path, comps: &[PreComponent]) -> Result<()> {
    let cargo_toml = pkg_dir.join("Cargo.toml");
    let mut doc: DocumentMut = if cargo_toml.is_file() {
        let raw = fs::read_to_string(&cargo_toml)
            .with_context(|| format!("read {}", cargo_toml.display()))?;
        raw.parse()
            .with_context(|| format!("parse {}", cargo_toml.display()))?
    } else {
        // Create a stub Cargo.toml so the new pkg shape is self-contained.
        let pkg_name = comps
            .first()
            .map(|c| c.cfg.package.clone())
            .unwrap_or_else(|| {
                pkg_dir
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("pkg")
                    .to_string()
            });
        format!("[package]\nname = \"{pkg_name}\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",)
            .parse()
            .expect("stub Cargo.toml parses")
    };

    // Ensure [package] table.
    let package = doc
        .entry("package")
        .or_insert_with(|| Item::Table(Table::new()))
        .as_table_mut()
        .ok_or_else(|| eyre::eyre!("[package] is not a table in {}", cargo_toml.display()))?;
    package
        .entry("metadata")
        .or_insert_with(|| Item::Table(Table::new()));

    // Render the nros section.
    let nros_table = render_nros_metadata_table(comps);
    doc["package"]["metadata"]["nros"] = Item::Table(nros_table);

    // Render the ament section from package.xml (if present).
    let pkg_xml_path = pkg_dir.join("package.xml");
    let ament = if pkg_xml_path.is_file() {
        extract_ament_from_package_xml(&pkg_xml_path)?
    } else {
        PackageMetadataAment::default()
    };
    doc["package"]["metadata"]["ament"] = Item::Table(render_ament_metadata_table(&ament));

    fs::write(&cargo_toml, doc.to_string())
        .with_context(|| format!("write {}", cargo_toml.display()))?;
    Ok(())
}

fn render_nros_metadata_table(comps: &[PreComponent]) -> Table {
    // Post-N.12 (2026-06-03): emit the new `node` / `nodes` spelling. The
    // migrator's job is to land a current-shape workspace, so output uses the
    // post-rename terminology even though the legacy input files we read
    // still call the field `component` / `components`.
    let mut nros_tbl = Table::new();
    if comps.len() == 1 {
        let c = &comps[0];
        nros_tbl["node"] = Item::Table(render_component_table(&c.cfg));
    } else {
        let mut nodes_tbl = Table::new();
        nodes_tbl.set_implicit(true);
        for c in comps {
            let name = c.cfg.component.clone();
            nodes_tbl[&name] = Item::Table(render_component_table(&c.cfg));
        }
        nros_tbl["nodes"] = Item::Table(nodes_tbl);
    }
    nros_tbl
}

fn render_component_table(cfg: &LegacyComponentConfig) -> Table {
    let mut tbl = Table::new();
    if let Some(ov) = &cfg.overrides {
        if let Some(ns) = &ov.default_namespace {
            tbl["default_namespace"] = value(ns);
        }
        if !ov.parameters.is_empty() {
            let mut params_tbl = Table::new();
            for (k, v) in &ov.parameters {
                params_tbl[k] = toml_value_to_item(v);
            }
            tbl["parameters"] = Item::Table(params_tbl);
        }
        if !ov.remaps.is_empty() {
            let mut arr = toml_edit::ArrayOfTables::new();
            for r in &ov.remaps {
                let mut row = Table::new();
                row["from"] = value(&r.from);
                row["to"] = value(&r.to);
                arr.push(row);
            }
            tbl["remaps"] = Item::ArrayOfTables(arr);
        }
    }
    tbl
}

fn toml_value_to_item(v: &toml::Value) -> Item {
    // Round-trip through string: keeps the conversion simple + correct.
    let raw =
        toml::to_string(&BTreeMap::from([(String::from("v"), v.clone())])).unwrap_or_default();
    if let Ok(doc) = raw.parse::<DocumentMut>() {
        if let Some(item) = doc.get("v") {
            return item.clone();
        }
    }
    value(format!("{v}"))
}

fn render_ament_metadata_table(ament: &PackageMetadataAment) -> Table {
    let mut tbl = Table::new();
    if !ament.build_depend.is_empty() {
        let mut a = Array::new();
        for d in &ament.build_depend {
            a.push(d);
        }
        tbl["build_depend"] = value(a);
    }
    if !ament.exec_depend.is_empty() {
        let mut a = Array::new();
        for d in &ament.exec_depend {
            a.push(d);
        }
        tbl["exec_depend"] = value(a);
    }
    if !ament.test_depend.is_empty() {
        let mut a = Array::new();
        for d in &ament.test_depend {
            a.push(d);
        }
        tbl["test_depend"] = value(a);
    }
    if let Some(bt) = &ament.build_type {
        tbl["build_type"] = value(bt);
    }
    tbl
}

/// Lightweight package.xml dep extractor. Reuses the cargo-nano-ros
/// `PackageXml` parser but extracts dep entries from raw text — that parser
/// collapses every dep type into one set, so for the migration we need the
/// per-tag breakdown.
fn extract_ament_from_package_xml(path: &Path) -> Result<PackageMetadataAment> {
    let raw = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    let mut out = PackageMetadataAment::default();
    out.build_depend = extract_tag(&raw, "build_depend");
    out.exec_depend = extract_tag(&raw, "exec_depend");
    out.test_depend = extract_tag(&raw, "test_depend");
    // `<depend>` counts as both build + exec per REP 140.
    for d in extract_tag(&raw, "depend") {
        if !out.build_depend.contains(&d) {
            out.build_depend.push(d.clone());
        }
        if !out.exec_depend.contains(&d) {
            out.exec_depend.push(d);
        }
    }
    // `<export><build_type>…</build_type></export>` → `build_type`.
    let bts = extract_tag(&raw, "build_type");
    if let Some(bt) = bts.into_iter().next() {
        out.build_type = Some(bt);
    }
    Ok(out)
}

fn extract_tag(xml: &str, tag: &str) -> Vec<String> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let mut out = Vec::new();
    let mut cursor = 0;
    while let Some(start) = xml[cursor..].find(&open) {
        let body_start = cursor + start + open.len();
        let Some(end) = xml[body_start..].find(&close) else {
            break;
        };
        let body = xml[body_start..body_start + end].trim().to_string();
        if !body.is_empty() {
            out.push(body);
        }
        cursor = body_start + end + close.len();
    }
    out
}

fn patch_workspace_cargo_toml(ws: &Path, bringup_name: &str, pkg_dirs: &[PathBuf]) -> Result<()> {
    let cargo_toml = ws.join("Cargo.toml");
    let mut doc: DocumentMut = if cargo_toml.is_file() {
        let raw = fs::read_to_string(&cargo_toml)
            .with_context(|| format!("read {}", cargo_toml.display()))?;
        raw.parse()
            .with_context(|| format!("parse {}", cargo_toml.display()))?
    } else {
        "[workspace]\nresolver = \"2\"\n"
            .parse()
            .expect("stub parses")
    };

    let workspace = doc
        .entry("workspace")
        .or_insert_with(|| Item::Table(Table::new()))
        .as_table_mut()
        .ok_or_else(|| eyre::eyre!("[workspace] is not a table in {}", cargo_toml.display()))?;
    if workspace.get("resolver").is_none() {
        workspace["resolver"] = value("2");
    }

    // Update members from pkg_dirs (relative to ws).
    let mut members_arr = workspace
        .get("members")
        .and_then(|i| i.as_array())
        .cloned()
        .unwrap_or_else(Array::new);
    let existing: Vec<String> = members_arr
        .iter()
        .filter_map(|v| v.as_str().map(str::to_string))
        .collect();
    for pkg in pkg_dirs {
        let rel = pkg.strip_prefix(ws).unwrap_or(pkg).display().to_string();
        if !existing.contains(&rel) {
            members_arr.push(rel);
        }
    }
    workspace["members"] = value(members_arr);

    // Update exclude list.
    let mut exclude_arr = workspace
        .get("exclude")
        .and_then(|i| i.as_array())
        .cloned()
        .unwrap_or_else(Array::new);
    if !exclude_arr.iter().any(|v| v.as_str() == Some(bringup_name)) {
        exclude_arr.push(bringup_name);
    }
    workspace["exclude"] = value(exclude_arr);

    // [workspace.metadata.nros].
    workspace
        .entry("metadata")
        .or_insert_with(|| Item::Table(Table::new()));
    let mut nros_tbl = Table::new();
    nros_tbl["default_system"] = value(bringup_name);
    workspace["metadata"]["nros"] = Item::Table(nros_tbl);

    fs::write(&cargo_toml, doc.to_string())
        .with_context(|| format!("write {}", cargo_toml.display()))?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Idempotency probe
// ---------------------------------------------------------------------------

fn is_already_migrated(ws: &Path) -> bool {
    let cargo_toml = ws.join("Cargo.toml");
    if !cargo_toml.is_file() {
        return false;
    }
    let Ok(raw) = fs::read_to_string(&cargo_toml) else {
        return false;
    };
    let Ok(doc) = raw.parse::<DocumentMut>() else {
        return false;
    };
    doc.get("workspace")
        .and_then(|w| w.get("metadata"))
        .and_then(|m| m.get("nros"))
        .is_some()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static SEQ: AtomicU64 = AtomicU64::new(0);

    fn tempdir(tag: &str) -> PathBuf {
        let pid = std::process::id();
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("nros-migrate-{tag}-{pid}-{n}"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn copy_dir_recursive(src: &Path, dst: &Path) -> std::io::Result<()> {
        fs::create_dir_all(dst)?;
        for entry in fs::read_dir(src)? {
            let entry = entry?;
            let ft = entry.file_type()?;
            let from = entry.path();
            let to = dst.join(entry.file_name());
            if ft.is_dir() {
                copy_dir_recursive(&from, &to)?;
            } else {
                fs::copy(&from, &to)?;
            }
        }
        Ok(())
    }

    fn fixture_path(name: &str) -> Option<PathBuf> {
        // Layered fallback: this crate is checked out next to nano-ros (`~/repos/`).
        let candidates = [
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../../nano-ros/packages/testing/nros-tests/fixtures")
                .join(name),
            PathBuf::from("/home/aeon/repos/nano-ros/packages/testing/nros-tests/fixtures")
                .join(name),
        ];
        candidates.into_iter().find(|p| p.is_dir())
    }

    fn clone_fixture(tag: &str, name: &str) -> Option<PathBuf> {
        let src = fixture_path(name)?;
        let dst = tempdir(tag);
        copy_dir_recursive(&src, &dst).unwrap();
        Some(dst)
    }

    fn snapshot_tree(dir: &Path) -> Vec<(String, Vec<u8>)> {
        let mut out = Vec::new();
        walk_collect(dir, dir, &mut out);
        out.sort_by(|a, b| a.0.cmp(&b.0));
        out
    }

    fn walk_collect(root: &Path, dir: &Path, out: &mut Vec<(String, Vec<u8>)>) {
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_dir() {
                walk_collect(root, &p, out);
            } else {
                let rel = p.strip_prefix(root).unwrap().display().to_string();
                let body = fs::read(&p).unwrap_or_default();
                out.push((rel, body));
            }
        }
    }

    fn run_migrate(ws: &Path, dry_run: bool, force: bool) -> Result<()> {
        run(Args {
            dir: ws.to_path_buf(),
            dry_run,
            bringup_name: None,
            force,
        })
    }

    #[test]
    fn migrate_orchestration_e2e_fixture_round_trip() {
        let Some(ws) = clone_fixture("e2e_round", "orchestration_e2e") else {
            eprintln!("[SKIPPED] orchestration_e2e fixture not found");
            return;
        };
        run_migrate(&ws, false, false).expect("migrate ok");

        // Post-212 shape:
        let bringup = ws.join("demo_pkg_bringup");
        assert!(
            bringup.join("system.toml").is_file(),
            "missing {}/system.toml",
            bringup.display()
        );
        assert!(
            bringup.join("package.xml").is_file(),
            "missing {}/package.xml",
            bringup.display()
        );
        assert!(
            bringup.join("launch/system.launch.xml").is_file(),
            "missing bringup launch xml"
        );
        let pkg = ws.join("src/demo_pkg");
        let cargo_toml = fs::read_to_string(pkg.join("Cargo.toml")).unwrap();
        // Post-N.12 (2026-06-03) — migrator emits the new `node` spelling.
        assert!(
            cargo_toml.contains("[package.metadata.nros.node]"),
            "Cargo.toml missing nros.node table:\n{cargo_toml}"
        );
        assert!(
            cargo_toml.contains("[package.metadata.ament]"),
            "Cargo.toml missing ament table:\n{cargo_toml}"
        );
        assert!(
            !pkg.join("component_nros.toml").exists(),
            "component_nros.toml not deleted"
        );
        assert!(!pkg.join("metadata").exists(), "metadata/ not deleted");
        assert!(!ws.join("nros.toml").exists(), "nros.toml not deleted");
        let root_toml = fs::read_to_string(ws.join("Cargo.toml")).unwrap();
        assert!(
            root_toml.contains("default_system = \"demo_pkg_bringup\""),
            "workspace.metadata.nros missing:\n{root_toml}"
        );

        // package.xml carries the generator marker.
        let regen = fs::read_to_string(pkg.join("package.xml")).unwrap();
        assert!(regen.contains("generated by nros emit package-xml"));

        // system.toml round-trips through SystemToml.
        let sys: SystemToml =
            toml::from_str(&fs::read_to_string(bringup.join("system.toml")).unwrap()).unwrap();
        assert_eq!(sys.components.len(), 1);
        assert_eq!(sys.components[0].pkg, "demo_pkg");
        assert_eq!(sys.components[0].name, "talker");
    }

    #[test]
    fn migrate_orchestration_composable_fixture_round_trip() {
        let Some(ws) = clone_fixture("composable_round", "orchestration_composable") else {
            eprintln!("[SKIPPED] orchestration_composable fixture not found");
            return;
        };
        run_migrate(&ws, false, false).expect("migrate ok");

        let bringup = ws.join("demo_container_bringup");
        assert!(bringup.join("system.toml").is_file());
        let pkg = ws.join("src/demo_container");
        let cargo_toml = fs::read_to_string(pkg.join("Cargo.toml")).unwrap();
        // Multi-node → use the table-of-tables shape.
        // Post-N.12 (2026-06-03) — migrator emits the new `nodes` spelling.
        assert!(
            cargo_toml.contains("[package.metadata.nros.nodes.")
                || cargo_toml.contains("metadata.nros.nodes"),
            "Cargo.toml missing nros.nodes table:\n{cargo_toml}"
        );
        assert!(
            !pkg.join("nros/components").exists(),
            "nros/components not deleted"
        );
        assert!(!pkg.join("metadata").exists(), "metadata/ not deleted");

        let sys: SystemToml =
            toml::from_str(&fs::read_to_string(bringup.join("system.toml")).unwrap()).unwrap();
        assert_eq!(sys.components.len(), 2);
        let names: Vec<&str> = sys.components.iter().map(|c| c.name.as_str()).collect();
        assert!(names.contains(&"Talker"));
        assert!(names.contains(&"Listener"));
    }

    #[test]
    fn migrate_idempotent() {
        let Some(ws) = clone_fixture("idem", "orchestration_e2e") else {
            return;
        };
        run_migrate(&ws, false, false).expect("first run");
        let after_first = snapshot_tree(&ws);
        // Second run must be a no-op.
        run_migrate(&ws, false, false).expect("second run is a no-op");
        let after_second = snapshot_tree(&ws);
        assert_eq!(
            after_first.len(),
            after_second.len(),
            "file count must match"
        );
        for ((a_path, a_body), (b_path, b_body)) in after_first.iter().zip(after_second.iter()) {
            assert_eq!(a_path, b_path);
            assert_eq!(a_body, b_body, "{a_path} changed on second run");
        }
    }

    #[test]
    fn migrate_dry_run_writes_no_files() {
        let Some(ws) = clone_fixture("dryrun", "orchestration_e2e") else {
            return;
        };
        let before = snapshot_tree(&ws);
        run_migrate(&ws, true, false).expect("dry-run ok");
        let after = snapshot_tree(&ws);
        assert_eq!(before.len(), after.len(), "dry-run added/removed files");
        for ((p1, b1), (p2, b2)) in before.iter().zip(after.iter()) {
            assert_eq!(p1, p2);
            assert_eq!(b1, b2, "dry-run mutated {p1}");
        }
    }

    #[test]
    fn migrate_force_re_migrates() {
        let Some(ws) = clone_fixture("force", "orchestration_e2e") else {
            return;
        };
        run_migrate(&ws, false, false).expect("first run");
        // Force must succeed without already-migrated bail-out — even when the
        // initial fixture file is now gone, a re-run with --force should
        // surface the "no nros.toml to migrate" error from build_plan, not the
        // "already migrated" early-return. The presence of the eyre error
        // proves --force bypassed the gate.
        let err = run_migrate(&ws, false, true).expect_err("force re-run hits build_plan");
        assert!(
            err.to_string().contains("nros.toml"),
            "diagnostic should reference nros.toml: {err}"
        );
    }

    /// Phase 212.N.12 (Component → Node rename, 2026-06-03) — `migrate.rs`
    /// emits the new `node` / `nodes` spelling at the Cargo.toml layer.
    /// This unit-level check guards the `render_nros_metadata_table` writer
    /// against a future drift back to `component(s)`. We host the rendered
    /// table inside a parent `[package.metadata.nros]` `Item::Table` so the
    /// rendered keys produce inline `[…node]` / `[…nodes.<Name>]` headers
    /// rather than empty implicit tables.
    #[test]
    fn render_nros_metadata_table_emits_node_spelling() {
        use toml_edit::DocumentMut;
        fn legacy_with_override(pkg: &str, comp: &str, ns: &str) -> PreComponent {
            PreComponent {
                src: PathBuf::from("ignored"),
                cfg: LegacyComponentConfig {
                    package: pkg.into(),
                    component: comp.into(),
                    linkage: None,
                    overrides: Some(LegacyOverrides {
                        default_namespace: Some(ns.into()),
                        ..Default::default()
                    }),
                },
            }
        }

        // Single-component → `[…nros.node]`.
        let mut doc: DocumentMut = "[package]\nname = \"x\"\n".parse().unwrap();
        doc["package"]
            .as_table_mut()
            .unwrap()
            .entry("metadata")
            .or_insert_with(|| Item::Table(Table::new()));
        doc["package"]["metadata"]["nros"] =
            Item::Table(render_nros_metadata_table(&[legacy_with_override(
                "demo_pkg", "talker", "/demo",
            )]));
        let rendered = doc.to_string();
        assert!(
            rendered.contains("[package.metadata.nros.node]"),
            "single-component render must produce `[package.metadata.nros.node]`, got:\n{rendered}"
        );
        assert!(
            !rendered.contains("[package.metadata.nros.component]"),
            "single-component render must NOT emit `[package.metadata.nros.component]`, got:\n{rendered}"
        );

        // Multi-component → `[…nros.nodes.<Name>]`.
        let mut doc: DocumentMut = "[package]\nname = \"x\"\n".parse().unwrap();
        doc["package"]
            .as_table_mut()
            .unwrap()
            .entry("metadata")
            .or_insert_with(|| Item::Table(Table::new()));
        doc["package"]["metadata"]["nros"] = Item::Table(render_nros_metadata_table(&[
            legacy_with_override("demo_container", "Talker", "/talker"),
            legacy_with_override("demo_container", "Listener", "/listener"),
        ]));
        let rendered = doc.to_string();
        assert!(
            rendered.contains("[package.metadata.nros.nodes.Talker]"),
            "multi-component render must produce `[…nros.nodes.<Name>]`, got:\n{rendered}"
        );
        assert!(
            !rendered.contains("[package.metadata.nros.components."),
            "multi-component render must NOT emit `[…nros.components.<Name>]`, got:\n{rendered}"
        );
    }

    /// Phase 212.N.12 — `LegacyComponentConfig` parses both the pre-rename
    /// `component = "..."` field name and the post-rename `node = "..."`
    /// alias, so a partially hand-edited pre-212 `component_nros.toml` still
    /// migrates cleanly.
    #[test]
    fn legacy_component_config_accepts_node_alias() {
        let legacy_spelling = r#"
package = "demo_pkg"
component = "talker"
"#;
        let new_spelling = r#"
package = "demo_pkg"
node = "talker"
"#;
        let parsed_old: LegacyComponentConfig =
            toml::from_str(legacy_spelling).expect("parse component-spelled input");
        let parsed_new: LegacyComponentConfig =
            toml::from_str(new_spelling).expect("parse node-spelled input");
        assert_eq!(parsed_old.package, "demo_pkg");
        assert_eq!(parsed_old.component, "talker");
        assert_eq!(parsed_new.package, "demo_pkg");
        assert_eq!(parsed_new.component, "talker");
    }
}
