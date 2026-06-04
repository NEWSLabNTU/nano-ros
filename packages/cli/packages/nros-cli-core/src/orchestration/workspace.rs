//! Workspace and package discovery for host planning.

use cargo_nano_ros::package_xml::PackageXml;
use eyre::{Context, Result};
use serde::Deserialize;
use serde_json::{Value as JsonValue, json};
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
    sync::{LazyLock, Mutex},
};

use super::cargo_metadata_schema::{ComponentMetadata, PackageMetadataNros};
use super::config::ComponentConfig;

/// Permissive envelope for extracting a `[component]` table out of a package's
/// `nros.toml` (Phase 172 W.1 fold) while ignoring sibling tables
/// (`[workspace]` / `[system]` / `[deploy]` / `[node]` / `[[transport]]`).
/// Unknown keys are ignored on purpose — only `[component]` is read here.
#[derive(Debug, Deserialize)]
struct ComponentEnvelope {
    #[serde(default)]
    component: Option<ComponentConfig>,
}

/// Load a component declaration from a manifest path. Handles two forms:
///
/// - **Folded** (Phase 172 W.1): a `[component]` table inside a package's
///   `nros.toml`. Returns `Ok(None)` when that file carries no `[component]`
///   (it is a workspace-root / direct-mode manifest, not a component).
/// - **Legacy**: the standalone whole-file form (`component_nros.toml` or
///   `nros/components/*.toml`), which is deprecated and warns once per file.
pub fn load_component_config(path: &Path) -> Result<Option<ComponentConfig>> {
    let raw = fs::read_to_string(path)
        .with_context(|| format!("failed to read component manifest {}", path.display()))?;
    let is_nros_toml = path.file_name().and_then(|name| name.to_str()) == Some("nros.toml");
    if is_nros_toml {
        let envelope: ComponentEnvelope =
            toml::from_str(&raw).with_context(|| format!("failed to parse {}", path.display()))?;
        Ok(envelope.component)
    } else {
        warn_legacy_component_manifest(path);
        let config: ComponentConfig = toml::from_str(&raw)
            .with_context(|| format!("failed to parse component manifest {}", path.display()))?;
        Ok(Some(config))
    }
}

/// Emit the `component_nros.toml` deprecation notice at most once per file path
/// for the life of the process (Phase 172 W.1 deprecation window).
fn warn_legacy_component_manifest(path: &Path) {
    static WARNED: LazyLock<Mutex<BTreeSet<PathBuf>>> =
        LazyLock::new(|| Mutex::new(BTreeSet::new()));
    if WARNED.lock().unwrap().insert(path.to_path_buf()) {
        eprintln!(
            "warning: `{}` is a deprecated standalone component manifest; fold it into the \
             package's `nros.toml` as a `[component]` table (Phase 172 W.1). The standalone \
             form still works during the deprecation window.",
            path.display()
        );
    }
}

#[derive(Debug, Clone)]
pub struct Workspace {
    pub root: PathBuf,
    pub packages: Vec<Package>,
}

#[derive(Debug, Clone)]
pub struct Package {
    pub name: String,
    pub root: PathBuf,
    pub package_xml: PathBuf,
    pub nros_toml: Option<PathBuf>,
    pub launch_files: Vec<PathBuf>,
    pub manifest_files: Vec<PathBuf>,
    pub metadata_files: Vec<PathBuf>,
    /// Component-declaration candidates that tie a package to a
    /// `nros::component!` export + its source-metadata path (Phase 126.B.7).
    /// In preference order: the package's folded `nros.toml` `[component]`
    /// table (W.1), the legacy standalone `component_nros.toml`, then any
    /// `nros/components/*.toml`. An `nros.toml` without a `[component]` table
    /// is filtered out at parse time (`load_component_config`).
    pub component_config_files: Vec<PathBuf>,
    /// Phase 212.M-F.17 — summaries derived from the package's
    /// `[package.metadata.nros.{component,components,node,nodes}]` tables
    /// in `Cargo.toml`. Populated at discovery time; one entry per
    /// declared component. Empty when the package has no `Cargo.toml`
    /// or no nros component metadata table.
    pub cargo_component_metadata: Vec<CargoComponentSummary>,
}

/// Phase 212.M-F.17 — α-bridge between the in-tree
/// `[package.metadata.nros.component]` / `…components.<Name>` Cargo
/// metadata and the planner's source-metadata pipeline.
///
/// The planner's `find_source_metadata` walk currently keys off the
/// `(package, executable)` pair recorded in a `metadata/*.json` sidecar
/// file. The Phase 212 in-tree fixtures dropped those sidecars in favor
/// of `[package.metadata.nros.component]`, leaving `find_source_metadata`
/// blind. `CargoComponentSummary` carries just enough information to
/// synthesise a minimal `JsonArtifact` for the planner's `(package,
/// executable)` match — full entity / param / remap synthesis is
/// intentionally out of scope (runtime `Component::register(ctx)`
/// carries those in the redesign).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CargoComponentSummary {
    /// Cargo `[package].name` the component belongs to. Used by the
    /// planner's `find_source_metadata` package match.
    pub package: String,
    /// Short component instance name. Derived per Phase 212.M-F.17:
    /// `metadata.name` when present, else the multi-shape table key,
    /// else the class basename (`talker_pkg::Talker` → `Talker`),
    /// else the package name.
    pub component: String,
    /// Executable name. Defaults to the package name; overridden when a
    /// `[[bin]] name = …` row matches the component name.
    pub executable: String,
    /// `metadata.class` when present (`<pkg-dir>::<UserClass>`). Threaded
    /// through to the synthetic JSON so downstream readers that care
    /// about the class can still find it.
    pub class: Option<String>,
    /// `metadata.default_namespace` when present.
    pub default_namespace: Option<String>,
    /// Absolute path to the `Cargo.toml` the summary was derived from.
    /// Recorded so synthetic JSON artifacts can name a real on-disk path
    /// for diagnostics (matches the file-artifact `path` field shape).
    pub manifest_path: PathBuf,
}

impl Workspace {
    pub fn discover(root: &Path) -> Result<Self> {
        let mut packages = Vec::new();
        let root = root.to_path_buf();
        if root.join("package.xml").is_file() {
            packages.push(discover_package(&root)?);
        }
        let src = root.join("src");
        if src.is_dir() {
            for entry in fs::read_dir(&src)? {
                let entry = entry?;
                let path = entry.path();
                if path.join("package.xml").is_file() {
                    packages.push(discover_package(&path)?);
                }
            }
        }
        packages.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(Self { root, packages })
    }

    /// Phase 212.M-F.17 — synthesise `(manifest_path, json_value)` tuples
    /// from every package's `[package.metadata.nros.{component,components,
    /// node,nodes}]` table. The planner appends these to its `JsonArtifact`
    /// list AFTER the sidecar file artifacts so sidecars win the dedup pass
    /// in `schema_components` (back-compat: a package shipping both an
    /// authoritative metadata JSON and a stub component table keeps the
    /// file's richer data on the plan).
    ///
    /// The synthetic JSON carries the minimum keys the planner's
    /// `find_source_metadata` `(package, executable)` walk needs plus
    /// the downstream `schema_components` dedup id (`package` +
    /// `component` + `language`). `class` / `default_namespace` flow
    /// through when present.
    ///
    /// Each tuple's first element is the source `Cargo.toml`; downstream
    /// callers that mint a `JsonArtifact` use it as the artifact `path`
    /// so diagnostics name a real on-disk file.
    pub fn synthetic_metadata_artifacts(&self) -> Vec<(PathBuf, JsonValue)> {
        let mut out = Vec::new();
        for pkg in &self.packages {
            for summary in &pkg.cargo_component_metadata {
                out.push((
                    summary.manifest_path.clone(),
                    summary_to_synthetic_json(summary),
                ));
            }
        }
        out
    }

    pub fn source_metadata_files(&self) -> Vec<PathBuf> {
        unique_paths(
            self.packages
                .iter()
                .flat_map(|pkg| pkg.metadata_files.iter().cloned()),
        )
    }

    pub fn manifest_files(&self) -> Vec<PathBuf> {
        unique_paths(
            self.packages
                .iter()
                .flat_map(|pkg| pkg.manifest_files.iter().cloned()),
        )
    }

    pub fn package_nros_toml(&self, package: &str) -> Option<PathBuf> {
        self.packages
            .iter()
            .find(|pkg| pkg.name == package)
            .and_then(|pkg| pkg.nros_toml.clone())
    }

    /// Iterate every component declaration in the workspace — folded
    /// `nros.toml` `[component]` tables (W.1) and legacy standalone
    /// `component_nros.toml` / `nros/components/*.toml` files — as
    /// `(package_root, manifest_path, parsed_config)` tuples, deduped by
    /// `(package, component)`. Used by the metadata command to detect packages
    /// that
    /// declared themselves nros components but lack the
    /// `nros::component!` export (their `[metadata].source_metadata`
    /// path doesn't exist on disk — see Phase 126.B.7 acceptance
    /// criterion).
    pub fn component_declarations(&self) -> Result<Vec<ComponentDeclaration>> {
        let mut out = Vec::new();
        for pkg in &self.packages {
            // Dedup by `(package, component)` within a package, first-wins. The
            // folded `nros.toml` sorts ahead of a legacy `component_nros.toml`,
            // so when both declare the same component the folded form wins and
            // the legacy file is ignored (it still warns once on read).
            let mut seen = BTreeSet::new();
            for manifest_path in &pkg.component_config_files {
                // A package `nros.toml` is a candidate only if it actually
                // carries a `[component]` table (W.1 fold); skip it otherwise.
                let Some(config) = load_component_config(manifest_path)? else {
                    continue;
                };
                if !seen.insert((config.package.clone(), config.component.clone())) {
                    continue;
                }
                out.push(ComponentDeclaration {
                    package_root: pkg.root.clone(),
                    manifest_path: manifest_path.clone(),
                    config,
                });
            }
        }
        Ok(out)
    }
}

/// Parsed component manifest paired with its on-disk location.
#[derive(Debug, Clone)]
pub struct ComponentDeclaration {
    /// Package root the manifest belongs to. `source_metadata` paths
    /// in the manifest resolve relative to this directory.
    pub package_root: PathBuf,
    /// Absolute path to the manifest the declaration came from — a package's
    /// folded `nros.toml` (W.1) or a legacy standalone `component_nros.toml` /
    /// `nros/components/*.toml`.
    pub manifest_path: PathBuf,
    pub config: ComponentConfig,
}

impl ComponentDeclaration {
    /// Absolute path to the `[metadata].source_metadata` file the
    /// component is expected to emit. Relative paths resolve against
    /// `package_root`.
    pub fn source_metadata_path(&self) -> PathBuf {
        let raw = Path::new(&self.config.metadata.source_metadata);
        if raw.is_absolute() {
            raw.to_path_buf()
        } else {
            self.package_root.join(raw)
        }
    }
}

fn discover_package(root: &Path) -> Result<Package> {
    let package_xml = root.join("package.xml");
    let parsed = PackageXml::parse(&package_xml)
        .wrap_err_with(|| format!("failed to parse {}", package_xml.display()))?;
    // Phase 212.M-F.17 fix: the synth artifact's `package` field must match
    // what `<node pkg="…"/>` in the launch XML references. ROS convention
    // makes `package.xml` `<name>` the canonical pkg key; Cargo.toml
    // `[package].name` is often a *crate* name that diverges (e.g.
    // `talker_pkg` vs `talker_pkg_component`). Drive synthesis from the
    // package.xml name so `find_source_metadata` matches.
    let cargo_component_metadata = discover_cargo_component_metadata(root, &parsed.name)?;
    Ok(Package {
        name: parsed.name,
        root: root.to_path_buf(),
        package_xml,
        nros_toml: root
            .join("nros.toml")
            .is_file()
            .then(|| root.join("nros.toml")),
        launch_files: collect_files(
            root,
            &["launch"],
            &["launch.py", "launch.xml", "launch.yaml", "launch.yml"],
        )?,
        manifest_files: collect_files(
            root,
            &["manifest", "manifests"],
            &["launch.yaml", "launch.yml"],
        )?,
        metadata_files: collect_files(root, &["metadata", "nros", "target/nros"], &["json"])?,
        component_config_files: discover_component_configs(root)?,
        cargo_component_metadata,
    })
}

/// Phase 212.M-F.17 — read `<root>/Cargo.toml` and synthesise one
/// [`CargoComponentSummary`] per `[package.metadata.nros.{component,
/// components.<Name>}]` (and the post-N.12 `node` / `nodes` aliases)
/// entry. Returns an empty vec when:
///
/// * the package has no `Cargo.toml` (every embedded / CMake-only
///   package in the in-tree examples / fixtures), OR
/// * the `Cargo.toml` has no `[package.metadata.nros]` table, OR
/// * the table is present but declares only `application` / `entry` /
///   `deploy` (not a component package).
///
/// Parse errors are propagated so a malformed `Cargo.toml` surfaces at
/// discovery time rather than silently dropping the package.
fn discover_cargo_component_metadata(
    root: &Path,
    pkg_xml_name: &str,
) -> Result<Vec<CargoComponentSummary>> {
    let cargo_toml = root.join("Cargo.toml");
    if !cargo_toml.is_file() {
        return Ok(Vec::new());
    }
    let raw = fs::read_to_string(&cargo_toml)
        .with_context(|| format!("failed to read {}", cargo_toml.display()))?;
    let envelope: CargoManifestEnvelope = toml::from_str(&raw)
        .with_context(|| format!("failed to parse {} for nros metadata", cargo_toml.display()))?;
    let Some(package) = envelope.package else {
        return Ok(Vec::new());
    };
    // M-F.17 fix: `pkg_name` drives the synthetic artifact's `package`
    // field which must match what `<node pkg="…"/>` references — that's
    // the package.xml `<name>`, NOT the Cargo.toml `[package].name`
    // (which is the crate name, often suffixed `_component` / `_pkg`).
    let pkg_name = pkg_xml_name.to_string();
    let _ = package.name; // crate name kept readable for diagnostics if needed
    let Some(metadata) = package.metadata else {
        return Ok(Vec::new());
    };
    let Some(nros) = metadata.nros else {
        return Ok(Vec::new());
    };
    // Mirrors `nros_config::normalise_node_alias`: `node` is the post-N.12
    // canonical spelling, `component` is the deprecated alias. We accept
    // both at discovery time without warning (warnings live in
    // `parse_package_metadata_nros`).
    let single = nros.node.as_ref().or(nros.component.as_ref());
    let multi: Vec<(String, &ComponentMetadata)> = if !nros.nodes.is_empty() {
        nros.nodes.iter().map(|(k, v)| (k.clone(), v)).collect()
    } else {
        nros.components
            .iter()
            .map(|(k, v)| (k.clone(), v))
            .collect()
    };

    let bins: Vec<String> = envelope
        .bin
        .iter()
        .flat_map(|b| b.iter())
        .filter_map(|b| b.name.clone())
        .collect();

    let mut out = Vec::new();
    if let Some(component) = single {
        out.push(synthesise_summary(
            &pkg_name,
            None,
            component,
            &bins,
            &cargo_toml,
        ));
    }
    for (key, component) in multi {
        out.push(synthesise_summary(
            &pkg_name,
            Some(&key),
            component,
            &bins,
            &cargo_toml,
        ));
    }
    Ok(out)
}

/// Build a [`CargoComponentSummary`] for one `[component]` / `[node]`
/// (single) or `[components.<Name>]` / `[nodes.<Name>]` (multi) entry.
///
/// Per the Phase 212.M-F.17 task spec:
///
/// * `metadata.name` wins as the component name when present.
/// * Otherwise the multi-shape key wins (`components.Talker` → `Talker`).
/// * Otherwise the class basename wins (`talker_pkg::Talker` → `Talker`).
/// * Otherwise the package name is used as a last-resort fallback.
///
/// `executable` defaults to the package name; if a `[[bin]] name = …`
/// matches the chosen component name, that bin name wins instead.
fn synthesise_summary(
    pkg_name: &str,
    multi_key: Option<&str>,
    component: &ComponentMetadata,
    bins: &[String],
    manifest_path: &Path,
) -> CargoComponentSummary {
    let component_name = component
        .name
        .clone()
        .or_else(|| multi_key.map(ToString::to_string))
        .or_else(|| {
            component
                .class
                .as_deref()
                .and_then(class_basename)
                .map(ToString::to_string)
        })
        .unwrap_or_else(|| pkg_name.to_string());

    // `[[bin]] name = …` override: when one of the workspace member
    // `[[bin]]` rows happens to share the component name, prefer it
    // (the planner expects `executable` to point at the actual binary
    // a build tree drops on disk).
    // M-F.17 fix: launch XML `<node exec="…"/>` references the component
    // name (e.g. `talker`), not the cargo pkg / crate name (e.g.
    // `talker_pkg_component`). When a `[[bin]] name = component_name`
    // exists, that's the executable — clean Application-pkg case. For a
    // staticlib Component pkg (no `[[bin]]`), the executable IS the
    // component name (it's the symbolic identity the launcher resolves;
    // the actual binary is the Entry pkg that links the component in).
    let executable = if bins.iter().any(|n| n == &component_name) {
        component_name.clone()
    } else if bins.is_empty() {
        component_name.clone()
    } else {
        pkg_name.to_string()
    };

    CargoComponentSummary {
        package: pkg_name.to_string(),
        component: component_name,
        executable,
        class: component.class.clone(),
        default_namespace: component.default_namespace.clone(),
        manifest_path: manifest_path.to_path_buf(),
    }
}

/// `talker_pkg::Talker` → `Some("Talker")`. Returns `None` when the class
/// string carries no `::` separator (a malformed value the lint catches
/// elsewhere).
fn class_basename(class: &str) -> Option<&str> {
    class.rsplit_once("::").map(|(_, tail)| tail)
}

/// Permissive `Cargo.toml` envelope — only the keys M-F.17 cares about
/// are typed; every sibling table (`[dependencies]`, `[lib]`, …) is
/// ignored. Strictness on the nros tables themselves comes from
/// [`PackageMetadataNros`]'s `deny_unknown_fields`.
#[derive(Debug, Deserialize)]
struct CargoManifestEnvelope {
    #[serde(default)]
    package: Option<CargoPackageEnvelope>,
    #[serde(default)]
    bin: Option<Vec<CargoBinEnvelope>>,
}

#[derive(Debug, Deserialize)]
struct CargoPackageEnvelope {
    name: String,
    #[serde(default)]
    metadata: Option<CargoPackageMetadataEnvelope>,
}

#[derive(Debug, Deserialize)]
struct CargoPackageMetadataEnvelope {
    #[serde(default)]
    nros: Option<PackageMetadataNros>,
}

#[derive(Debug, Deserialize)]
struct CargoBinEnvelope {
    #[serde(default)]
    name: Option<String>,
}

/// Build the synthetic JSON object the planner consumes for one
/// summary. Mirrors the keys [`super::planner::schema_components`]
/// + [`super::planner::find_source_metadata`] read:
///
/// * `package` / `component` / `executable` — `(package, executable)`
///   match + `package::component` dedup id.
/// * `language` — every Cargo-resident component is Rust today; the
///   field is required so `schema_components` doesn't fall through to
///   the `"rust"` literal default.
/// * `synthetic` / `synthetic_source` — provenance markers; downstream
///   `nros check` lints distinguish synthesised entries from
///   authoritative metadata.
fn summary_to_synthetic_json(summary: &CargoComponentSummary) -> JsonValue {
    let mut obj = json!({
        "version": 1,
        "package": summary.package,
        "component": summary.component,
        "executable": summary.executable,
        "language": "rust",
        "synthetic": true,
        "synthetic_source": "cargo_metadata",
    });
    let map = obj.as_object_mut().expect("synthetic JSON is an object");
    if let Some(class) = &summary.class {
        map.insert("class".to_string(), JsonValue::String(class.clone()));
    }
    if let Some(namespace) = &summary.default_namespace {
        map.insert(
            "default_namespace".to_string(),
            JsonValue::String(namespace.clone()),
        );
    }
    obj
}

/// Locate component declaration candidates. Preference order (W.1 fold):
/// the package's `nros.toml` (read for a `[component]` table — the canonical
/// folded form), then the deprecated standalone `component_nros.toml` at the
/// package root, then any `nros/components/*.toml`. Whether a candidate is
/// actually a component is decided at parse time (`load_component_config`
/// returns `None` for an `nros.toml` with no `[component]`).
fn discover_component_configs(root: &Path) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    // W.1 fold: a package `nros.toml` may carry a `[component]` table.
    let folded = root.join("nros.toml");
    if folded.is_file() {
        out.push(folded);
    }
    let primary = root.join("component_nros.toml");
    if primary.is_file() {
        out.push(primary);
    }
    // The multi-component glob is order-independent — sort it for determinism,
    // but keep it *after* the root candidates so the folded `nros.toml` and the
    // legacy `component_nros.toml` retain their preference order.
    let components_dir = root.join("nros").join("components");
    if components_dir.is_dir() {
        let mut globbed = Vec::new();
        for entry in fs::read_dir(&components_dir)? {
            let path = entry?.path();
            if path.extension().is_some_and(|ext| ext == "toml") {
                globbed.push(path);
            }
        }
        globbed.sort();
        out.extend(globbed);
    }
    Ok(out)
}

fn collect_files(root: &Path, dirs: &[&str], suffixes: &[&str]) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    for dir in dirs {
        let path = root.join(dir);
        if path.is_dir() {
            collect_matching(&path, suffixes, &mut out)?;
        }
    }
    out.sort();
    Ok(out)
}

fn collect_matching(dir: &Path, suffixes: &[&str], out: &mut Vec<PathBuf>) -> Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_matching(&path, suffixes, out)?;
        } else if suffixes.iter().any(|suffix| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.ends_with(suffix))
        }) {
            out.push(path);
        }
    }
    Ok(())
}

pub fn unique_paths<I>(paths: I) -> Vec<PathBuf>
where
    I: IntoIterator<Item = PathBuf>,
{
    let mut seen = BTreeSet::new();
    paths
        .into_iter()
        .filter(|path| seen.insert(path.clone()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    /// RAII scratch directory under the system temp dir (no `tempfile` dep).
    struct Scratch(PathBuf);
    impl Scratch {
        fn new(tag: &str) -> Self {
            static N: AtomicU32 = AtomicU32::new(0);
            let dir = std::env::temp_dir().join(format!(
                "nros-ws-test-{}-{}-{}",
                tag,
                std::process::id(),
                N.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(&dir).unwrap();
            Scratch(dir)
        }
        fn write(&self, rel: &str, body: &str) {
            let path = self.0.join(rel);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, body).unwrap();
        }
    }
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    const PKG_XML: &str = r#"<?xml version="1.0"?>
<package format="3"><name>demo_pkg</name><version>0.0.0</version>
<description>t</description><maintainer email="a@b.c">a</maintainer><license>MIT</license>
</package>"#;

    const COMPONENT_TABLE: &str = r#"
        [component]
        version = 1
        package = "demo_pkg"
        component = "talker"
        language = "rust"
        [component.linkage]
        crate_name = "demo_pkg"
        executable = "talker"
        [component.metadata]
        source_metadata = "target/nros/metadata/talker.json"
    "#;

    // The same declaration in the legacy standalone (whole-file) shape.
    const LEGACY_WHOLE_FILE: &str = r#"
        version = 1
        package = "demo_pkg"
        component = "talker"
        language = "rust"
        [linkage]
        crate_name = "demo_pkg"
        executable = "talker"
        [metadata]
        source_metadata = "target/nros/metadata/talker.json"
    "#;

    #[test]
    fn folds_component_table_in_package_nros_toml() {
        let s = Scratch::new("fold");
        s.write("src/demo_pkg/package.xml", PKG_XML);
        // A package nros.toml carrying [workspace]-unrelated sibling tables plus
        // the folded [component] — sibling tables must be ignored.
        s.write(
            "src/demo_pkg/nros.toml",
            &format!("[[transport]]\nid = \"t\"\nkind = \"udp\"\n{COMPONENT_TABLE}"),
        );

        let ws = Workspace::discover(&s.0).unwrap();
        let decls = ws.component_declarations().unwrap();
        assert_eq!(decls.len(), 1, "folded [component] is discovered");
        assert_eq!(decls[0].config.package, "demo_pkg");
        assert_eq!(decls[0].config.component, "talker");
        assert!(decls[0].manifest_path.ends_with("nros.toml"));
    }

    #[test]
    fn legacy_component_nros_toml_still_discovered() {
        let s = Scratch::new("legacy");
        s.write("src/demo_pkg/package.xml", PKG_XML);
        s.write("src/demo_pkg/component_nros.toml", LEGACY_WHOLE_FILE);

        let decls = Workspace::discover(&s.0)
            .unwrap()
            .component_declarations()
            .unwrap();
        assert_eq!(decls.len(), 1);
        assert_eq!(decls[0].config.component, "talker");
        assert!(decls[0].manifest_path.ends_with("component_nros.toml"));
    }

    #[test]
    fn folded_nros_toml_wins_over_legacy_for_same_component() {
        let s = Scratch::new("both");
        s.write("src/demo_pkg/package.xml", PKG_XML);
        s.write("src/demo_pkg/nros.toml", COMPONENT_TABLE);
        s.write("src/demo_pkg/component_nros.toml", LEGACY_WHOLE_FILE);

        let decls = Workspace::discover(&s.0)
            .unwrap()
            .component_declarations()
            .unwrap();
        assert_eq!(decls.len(), 1, "duplicate (package, component) deduped");
        assert!(
            decls[0].manifest_path.ends_with("nros.toml")
                && !decls[0].manifest_path.ends_with("component_nros.toml"),
            "folded form wins: {}",
            decls[0].manifest_path.display()
        );
    }

    // -----------------------------------------------------------------
    // Phase 212.M-F.17 — synthetic metadata from Cargo.toml
    // -----------------------------------------------------------------

    /// `[package.metadata.nros.component]` single-shape → one summary.
    /// Component name defaults to class basename when `metadata.name`
    /// is absent; executable defaults to package name.
    #[test]
    fn synthetic_metadata_from_single_component_table() {
        let s = Scratch::new("mf17-single");
        s.write(
            "src/talker_pkg/package.xml",
            PKG_XML.replace("demo_pkg", "talker_pkg").as_str(),
        );
        s.write(
            "src/talker_pkg/Cargo.toml",
            r#"
[package]
name = "talker_pkg"
version = "0.1.0"
edition = "2021"

[package.metadata.nros.component]
class = "talker_pkg::Talker"
default_namespace = "/demo"
"#,
        );

        let ws = Workspace::discover(&s.0).unwrap();
        let pkg = ws
            .packages
            .iter()
            .find(|p| p.name == "talker_pkg")
            .expect("pkg");
        assert_eq!(pkg.cargo_component_metadata.len(), 1, "one summary");
        let summary = &pkg.cargo_component_metadata[0];
        assert_eq!(summary.package, "talker_pkg");
        // `metadata.name` absent → class basename wins.
        assert_eq!(summary.component, "Talker");
        // No `[[bin]]` row → executable is the component name (the
        // symbolic identity the launch XML `<node exec="…"/>` references).
        // M-F.17 fix-up: previously fell back to pkg_name which broke
        // staticlib Component pkgs whose crate name != component name.
        assert_eq!(summary.executable, "Talker");
        assert_eq!(summary.class.as_deref(), Some("talker_pkg::Talker"));
        assert_eq!(summary.default_namespace.as_deref(), Some("/demo"));
        assert!(summary.manifest_path.ends_with("Cargo.toml"));

        let synth = ws.synthetic_metadata_artifacts();
        assert_eq!(synth.len(), 1);
        let (path, value) = &synth[0];
        assert!(path.ends_with("Cargo.toml"));
        assert_eq!(value["version"], 1);
        assert_eq!(value["package"], "talker_pkg");
        assert_eq!(value["component"], "Talker");
        assert_eq!(value["executable"], "Talker");
        assert_eq!(value["language"], "rust");
        assert_eq!(value["class"], "talker_pkg::Talker");
        assert_eq!(value["default_namespace"], "/demo");
        assert_eq!(value["synthetic"], true);
        assert_eq!(value["synthetic_source"], "cargo_metadata");
    }

    /// `metadata.name` wins over class basename when both are present;
    /// a `[[bin]] name = …` row matching the component name overrides
    /// the package-name executable default.
    #[test]
    fn synthetic_metadata_name_and_bin_override() {
        let s = Scratch::new("mf17-name-bin");
        s.write(
            "src/talker_pkg/package.xml",
            PKG_XML.replace("demo_pkg", "talker_pkg").as_str(),
        );
        s.write(
            "src/talker_pkg/Cargo.toml",
            r#"
[package]
name = "talker_pkg"
version = "0.1.0"
edition = "2021"

[[bin]]
name = "talker"
path = "src/bin/talker.rs"

[package.metadata.nros.component]
class = "talker_pkg::Talker"
name = "talker"
"#,
        );

        let ws = Workspace::discover(&s.0).unwrap();
        let pkg = ws
            .packages
            .iter()
            .find(|p| p.name == "talker_pkg")
            .expect("pkg");
        let summary = &pkg.cargo_component_metadata[0];
        // `metadata.name = "talker"` wins.
        assert_eq!(summary.component, "talker");
        // `[[bin]] name = "talker"` matches the component name → wins.
        assert_eq!(summary.executable, "talker");
    }

    /// `[package.metadata.nros.components.<Name>]` multi-shape →
    /// one summary per entry; the table key wins as the component
    /// name when `metadata.name` is absent on the entry.
    #[test]
    fn synthetic_metadata_from_multi_components_table() {
        let s = Scratch::new("mf17-multi");
        s.write(
            "src/multi_pkg/package.xml",
            PKG_XML.replace("demo_pkg", "multi_pkg").as_str(),
        );
        s.write(
            "src/multi_pkg/Cargo.toml",
            r#"
[package]
name = "multi_pkg"
version = "0.1.0"
edition = "2021"

[package.metadata.nros.components.Talker]
class = "multi_pkg::Talker"

[package.metadata.nros.components.Listener]
class = "multi_pkg::Listener"
default_namespace = "/multi"
"#,
        );

        let ws = Workspace::discover(&s.0).unwrap();
        let pkg = ws
            .packages
            .iter()
            .find(|p| p.name == "multi_pkg")
            .expect("pkg");
        assert_eq!(pkg.cargo_component_metadata.len(), 2);
        // BTreeMap iteration order is key-sorted: Listener < Talker.
        let listener = &pkg.cargo_component_metadata[0];
        assert_eq!(listener.component, "Listener");
        assert_eq!(listener.default_namespace.as_deref(), Some("/multi"));
        let talker = &pkg.cargo_component_metadata[1];
        assert_eq!(talker.component, "Talker");
        assert!(talker.default_namespace.is_none());

        let synth = ws.synthetic_metadata_artifacts();
        assert_eq!(synth.len(), 2);
    }

    /// Package with no `Cargo.toml` (e.g. a CMake / Zephyr-only
    /// component) → empty summary list, no error.
    #[test]
    fn synthetic_metadata_no_cargo_toml() {
        let s = Scratch::new("mf17-no-cargo");
        s.write(
            "src/cmake_pkg/package.xml",
            PKG_XML.replace("demo_pkg", "cmake_pkg").as_str(),
        );
        // No Cargo.toml.

        let ws = Workspace::discover(&s.0).unwrap();
        let pkg = ws
            .packages
            .iter()
            .find(|p| p.name == "cmake_pkg")
            .expect("pkg");
        assert!(pkg.cargo_component_metadata.is_empty());
        assert!(ws.synthetic_metadata_artifacts().is_empty());
    }

    /// Cargo.toml present but with no `[package.metadata.nros]` table →
    /// empty summary list (e.g. a regular Rust library that happens to
    /// sit next to a `package.xml`).
    #[test]
    fn synthetic_metadata_no_nros_table() {
        let s = Scratch::new("mf17-no-nros");
        s.write(
            "src/plain_pkg/package.xml",
            PKG_XML.replace("demo_pkg", "plain_pkg").as_str(),
        );
        s.write(
            "src/plain_pkg/Cargo.toml",
            r#"
[package]
name = "plain_pkg"
version = "0.1.0"
edition = "2021"

[dependencies]
"#,
        );

        let ws = Workspace::discover(&s.0).unwrap();
        let pkg = ws
            .packages
            .iter()
            .find(|p| p.name == "plain_pkg")
            .expect("pkg");
        assert!(pkg.cargo_component_metadata.is_empty());
        assert!(ws.synthetic_metadata_artifacts().is_empty());
    }

    /// `node` (post-N.12 canonical key) is treated the same as
    /// `component` (deprecated alias). The discovery path is
    /// warning-free (the warning lives in `nros_config`).
    #[test]
    fn synthetic_metadata_accepts_node_alias() {
        let s = Scratch::new("mf17-node");
        s.write(
            "src/node_pkg/package.xml",
            PKG_XML.replace("demo_pkg", "node_pkg").as_str(),
        );
        s.write(
            "src/node_pkg/Cargo.toml",
            r#"
[package]
name = "node_pkg"
version = "0.1.0"
edition = "2021"

[package.metadata.nros.node]
class = "node_pkg::Node"
"#,
        );

        let ws = Workspace::discover(&s.0).unwrap();
        let pkg = ws
            .packages
            .iter()
            .find(|p| p.name == "node_pkg")
            .expect("pkg");
        assert_eq!(pkg.cargo_component_metadata.len(), 1);
        assert_eq!(pkg.cargo_component_metadata[0].component, "Node");
    }

    #[test]
    fn root_only_nros_toml_is_not_a_component() {
        let s = Scratch::new("rootonly");
        s.write("src/demo_pkg/package.xml", PKG_XML);
        // A workspace-root / direct-mode nros.toml with no [component] table.
        s.write(
            "src/demo_pkg/nros.toml",
            "[workspace]\ndefault = \"x\"\n[node]\nname = \"n\"\n",
        );

        let decls = Workspace::discover(&s.0)
            .unwrap()
            .component_declarations()
            .unwrap();
        assert!(decls.is_empty(), "no [component] table → not a component");
    }
}
