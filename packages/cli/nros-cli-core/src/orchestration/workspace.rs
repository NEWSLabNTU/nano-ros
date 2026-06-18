//! Workspace and package discovery for host planning.

use cargo_nano_ros::package_xml::PackageXml;
use eyre::{Context, Result};
use serde::Deserialize;
use serde_json::{Value as JsonValue, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    sync::{LazyLock, Mutex},
};

use super::{
    cargo_metadata_schema::{ComponentMetadata, PackageMetadataNros},
    config::{ComponentConfig, ComponentLinkage, ComponentMetadataConfig, ComponentOverrides},
    source_metadata::ComponentLanguage,
};

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
    /// Phase 254 — the package's `system.toml` (bringup pkg), the typed
    /// capability/topology SSoT (RFC-0004). `None` for non-bringup packages.
    pub system_toml: Option<PathBuf>,
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
    /// Phase 219.L — summaries derived from `nano_ros_node_register(...)`
    /// calls in the package's `CMakeLists.txt`. Populated at discovery
    /// time; one entry per static call. Empty when the package has no
    /// `CMakeLists.txt` or no `nano_ros_node_register` call. Lets
    /// `nros metadata` / `nros plan` discover pure-C/C++ Node pkgs that
    /// carry only `package.xml` + `CMakeLists.txt` (no `nros.toml`, no
    /// `Cargo.toml`) — closes Phase 219 workflow-review Gap 6.
    pub cmake_component_metadata: Vec<CmakeNodeSummary>,
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
/// Phase 219.L — summary derived from a single `nano_ros_node_register(...)`
/// call statically parsed from a package's `CMakeLists.txt`. Carries the
/// minimum information needed for `nros metadata --build` and `nros plan`
/// to dedup + identify C/C++ Node pkgs that don't ship `nros.toml` or
/// `Cargo.toml` component metadata.
///
/// Mirrors [`CargoComponentSummary`] field-for-field where the semantics
/// match (`package` / `component` / `executable` / `class`), so downstream
/// consumers can treat both summary kinds uniformly when the cmake-first
/// path is sufficient.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CmakeNodeSummary {
    /// `package.xml` `<name>` — matches the planner's `(package, executable)`
    /// key shape.
    pub package: String,
    /// `nano_ros_node_register(NAME …)` value. Same as ROS 2 composable
    /// node's "instance name".
    pub component: String,
    /// Defaults to the value of `NAME`. Phase 212.L's `<pkg>_<NAME>_component`
    /// static lib target is what the linked Entry pkg references; the
    /// executable shape here keeps parity with [`CargoComponentSummary`].
    pub executable: String,
    /// `nano_ros_node_register(CLASS <pkg_sym>::<UserClass>)` value.
    pub class: Option<String>,
    /// `nano_ros_node_register(LANGUAGE C|CPP)` value, or a conservative
    /// inference from the class shape for pre-223 callers.
    pub language: ComponentLanguage,
    /// `nano_ros_node_register(DEPLOY <target>[ <target>...])` values.
    pub deploy_targets: Vec<String>,
    /// Absolute path to the `CMakeLists.txt` the summary was derived from.
    pub manifest_path: PathBuf,
}

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
            for summary in &pkg.cmake_component_metadata {
                out.push((
                    summary.manifest_path.clone(),
                    cmake_summary_to_synthetic_json(summary),
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

    /// Phase 254 — the package's `system.toml` path (the bringup pkg's typed
    /// capability/topology SSoT). `None` if the package has none.
    pub fn package_system_toml(&self, package: &str) -> Option<PathBuf> {
        self.packages
            .iter()
            .find(|pkg| pkg.name == package)
            .and_then(|pkg| pkg.system_toml.clone())
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
            // Phase 219.L appends cmake-derived declarations LAST in the same
            // dedup pass, so any `nano_ros_node_register(NAME …)` whose
            // `(package, NAME)` matches an explicit `[component]` table is
            // silently superseded — the explicit metadata wins.
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
            // Phase 219.L — synthesise declarations for `nano_ros_node_register`
            // calls statically parsed from `CMakeLists.txt` (pure-C/C++ Node
            // pkgs that ship no `nros.toml` / `Cargo.toml` component
            // metadata). Closes Phase 219 workflow-review Gap 6.
            for summary in &pkg.cmake_component_metadata {
                if !seen.insert((summary.package.clone(), summary.component.clone())) {
                    continue;
                }
                out.push(ComponentDeclaration {
                    package_root: pkg.root.clone(),
                    manifest_path: summary.manifest_path.clone(),
                    config: cmake_summary_to_component_config(summary),
                });
            }
        }
        Ok(out)
    }
}

/// Phase 219.L — synthesise a [`ComponentConfig`] from a CMake-derived
/// [`CmakeNodeSummary`]. Mirrors the shape `discover_cargo_component_metadata`
/// produces for the Cargo path: minimum keys to identify the component
/// `(package, component, language)` plus an optional class threaded through
/// for downstream consumers.
fn cmake_summary_to_component_config(summary: &CmakeNodeSummary) -> ComponentConfig {
    ComponentConfig {
        version: 1,
        package: summary.package.clone(),
        component: summary.component.clone(),
        language: summary.language.clone(),
        linkage: ComponentLinkage::default(),
        metadata: ComponentMetadataConfig {
            source_metadata: format!("metadata/{}.json", summary.component),
            generated_by: Some("nano_ros_node_register".to_string()),
        },
        overrides: ComponentOverrides::default(),
    }
}

/// Phase 219.L / 223 — Best-effort language inference for CMake Node pkgs.
/// `LANGUAGE` is authoritative when present. Older CMakeLists omitted it, so
/// fall back to the historical class-shape heuristic.
fn infer_cmake_language(language: Option<&str>, class: Option<&str>) -> ComponentLanguage {
    match language.map(|s| s.to_ascii_lowercase()) {
        Some(lang) if lang == "c" => ComponentLanguage::C,
        Some(lang) if lang == "cpp" || lang == "cxx" => ComponentLanguage::Cpp,
        _ => infer_language_from_class(class).unwrap_or(ComponentLanguage::Cpp),
    }
}

fn infer_language_from_class(class: Option<&str>) -> Option<ComponentLanguage> {
    let class = class?;
    if class.contains("::") {
        Some(ComponentLanguage::Cpp)
    } else {
        Some(ComponentLanguage::C)
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
    let cmake_component_metadata = discover_cmake_node_metadata(root, &parsed.name)?;
    Ok(Package {
        name: parsed.name,
        root: root.to_path_buf(),
        package_xml,
        // Phase 254 — the bringup package's `system.toml` (the capability/topology
        // SSoT both codegen paths read).
        system_toml: root
            .join("system.toml")
            .is_file()
            .then(|| root.join("system.toml")),
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
        cmake_component_metadata,
    })
}

/// Phase 219.L — statically parse the package's `CMakeLists.txt` for
/// `nano_ros_node_register(NAME … CLASS … SOURCES … DEPLOY …)` calls and
/// synthesise one [`CmakeNodeSummary`] per call. Returns an empty vec
/// when the package has no `CMakeLists.txt` or no
/// `nano_ros_node_register` calls (e.g. an Entry pkg using
/// `nano_ros_entry(...)`, an interface-only pkg, a Bringup pkg, …).
///
/// **Why static parse, not cmake-configure-first.** Phase 219 review
/// Gap 6 noted two options: (a) extend the walker, (b) require a prior
/// `cmake configure` whose `${CMAKE_BINARY_DIR}/nros-metadata.json` the
/// CLI then reads. (a) wins because:
/// - the user's first `nros metadata` / `nros plan` call needs to work
///   without an explicit configure step ("solo planner mode");
/// - the cmake fn args are single-line + keyword form, so the regex
///   walker is small + bounded.
///
/// Parse policy is conservative: parser failures (malformed argument
/// list, missing required keyword) skip the offending call rather than
/// failing the whole discovery — partial information beats none for
/// `nros metadata --build`. A future hardening pass can promote
/// skipped-call diagnostics to warnings.
fn discover_cmake_node_metadata(root: &Path, package_name: &str) -> Result<Vec<CmakeNodeSummary>> {
    let cmakelists = root.join("CMakeLists.txt");
    if !cmakelists.is_file() {
        return Ok(Vec::new());
    }
    let text = fs::read_to_string(&cmakelists)
        .wrap_err_with(|| format!("read {}", cmakelists.display()))?;
    let stripped = strip_cmake_comments(&text);
    let mut out = Vec::new();
    for call in extract_cmake_calls(&stripped, "nano_ros_node_register") {
        let Some(args) = parse_cmake_kwargs(&call) else {
            continue;
        };
        let Some(name) = args.single("NAME") else {
            continue;
        };
        let class = args.single("CLASS");
        let language_kw = args.single("LANGUAGE").or_else(|| args.single("LANG"));
        let language = infer_cmake_language(language_kw.as_deref(), class.as_deref());
        out.push(CmakeNodeSummary {
            package: package_name.to_string(),
            component: name.clone(),
            executable: name,
            class,
            language,
            deploy_targets: args.multi("DEPLOY"),
            manifest_path: cmakelists.clone(),
        });
    }
    Ok(out)
}

/// Strip `#` line comments from a CMake source while preserving line
/// offsets (each comment span replaced by spaces) — keeps any byte-offset
/// based downstream parser stable.
fn strip_cmake_comments(src: &str) -> String {
    let mut out = String::with_capacity(src.len());
    let mut in_comment = false;
    for ch in src.chars() {
        if in_comment {
            if ch == '\n' {
                in_comment = false;
                out.push(ch);
            } else {
                out.push(' ');
            }
        } else if ch == '#' {
            in_comment = true;
            out.push(' ');
        } else {
            out.push(ch);
        }
    }
    out
}

/// Return every `<fn_name>(...)` call body found in `src`. Handles
/// balanced parentheses inside the body (none of the cmake fns 219.L
/// cares about use nested parens, but the walker stays generic).
fn extract_cmake_calls(src: &str, fn_name: &str) -> Vec<String> {
    let mut out = Vec::new();
    let needle = format!("{fn_name}(");
    let bytes = src.as_bytes();
    let mut i = 0;
    while i + needle.len() <= bytes.len() {
        if !src.is_char_boundary(i) || !src[i..].starts_with(&needle) {
            i += 1;
            continue;
        }
        // Confirm boundary before the call (start of file OR non-identifier).
        // CMake identifiers include `_`, so `my_nano_ros_node_register(`
        // must NOT match the `nano_ros_node_register` needle.
        let prev_is_ident = i > 0 && (bytes[i - 1].is_ascii_alphanumeric() || bytes[i - 1] == b'_');
        if prev_is_ident {
            i += 1;
            continue;
        }
        let body_start = i + needle.len();
        let mut depth = 1usize;
        let mut j = body_start;
        while j < bytes.len() && depth > 0 {
            match bytes[j] {
                b'(' => depth += 1,
                b')' => depth -= 1,
                _ => {}
            }
            if depth == 0 {
                break;
            }
            j += 1;
        }
        if depth == 0 {
            out.push(src[body_start..j].to_string());
            i = j + 1;
        } else {
            // Unterminated call — give up on this match.
            break;
        }
    }
    out
}

/// Minimal CMake keyword-argument parser. Tokenises on whitespace,
/// strips `"`-delimited strings, then collects each known keyword's
/// values until the next keyword. Returns `None` only when the input
/// is empty.
fn parse_cmake_kwargs(body: &str) -> Option<CmakeKwargs> {
    // The valid keyword set for the cmake fns 219.L parses. Conservatively
    // wide to avoid eating later keywords as values.
    const KEYWORDS: &[&str] = &[
        "NAME",
        "CLASS",
        "LANGUAGE",
        "SOURCES",
        "DEPLOY",
        "BOARD",
        "LAUNCH",
        "ARGS",
        "LANG",
        "RMW",
        "DOMAIN_ID",
        "LOCATOR",
        "TARGET",
    ];
    let tokens = tokenize_cmake_body(body);
    if tokens.is_empty() {
        return None;
    }
    let mut map: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut current: Option<String> = None;
    for tok in tokens {
        if KEYWORDS.contains(&tok.as_str()) {
            current = Some(tok.clone());
            map.entry(tok).or_default();
        } else if let Some(key) = current.as_ref() {
            map.entry(key.clone()).or_default().push(tok);
        }
        // Tokens before the first keyword are dropped (CMake fns 219.L
        // cares about don't use positional args).
    }
    Some(CmakeKwargs { map })
}

fn tokenize_cmake_body(body: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut chars = body.chars().peekable();
    while let Some(&ch) = chars.peek() {
        if ch.is_whitespace() {
            chars.next();
            continue;
        }
        if ch == '"' {
            chars.next();
            let mut buf = String::new();
            while let Some(c) = chars.next() {
                if c == '\\' {
                    if let Some(esc) = chars.next() {
                        buf.push(esc);
                    }
                } else if c == '"' {
                    break;
                } else {
                    buf.push(c);
                }
            }
            out.push(buf);
            continue;
        }
        let mut buf = String::new();
        while let Some(&c) = chars.peek() {
            if c.is_whitespace() {
                break;
            }
            buf.push(c);
            chars.next();
        }
        if !buf.is_empty() {
            out.push(buf);
        }
    }
    out
}

#[derive(Debug, Default)]
struct CmakeKwargs {
    map: BTreeMap<String, Vec<String>>,
}

impl CmakeKwargs {
    fn single(&self, key: &str) -> Option<String> {
        self.map.get(key).and_then(|v| v.first().cloned())
    }
    fn multi(&self, key: &str) -> Vec<String> {
        self.map.get(key).cloned().unwrap_or_default()
    }
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

fn cmake_summary_to_synthetic_json(summary: &CmakeNodeSummary) -> JsonValue {
    let language = match &summary.language {
        ComponentLanguage::C => "c",
        ComponentLanguage::Cpp => "cpp",
        ComponentLanguage::Rust => "rust",
    };
    let mut obj = json!({
        "version": 1,
        "package": summary.package,
        "component": summary.component,
        "executable": summary.executable,
        "language": language,
        "synthetic": true,
        "synthetic_source": "cmake_node_register",
    });
    if let Some(class) = &summary.class {
        obj.as_object_mut()
            .expect("synthetic JSON is an object")
            .insert("class".to_string(), JsonValue::String(class.clone()));
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

    // -----------------------------------------------------------------
    // Phase 219.L — `nano_ros_node_register` discovery from CMakeLists.
    // -----------------------------------------------------------------

    #[test]
    fn strip_cmake_comments_preserves_lines() {
        let src = "foo # bar\nbaz\n";
        let out = strip_cmake_comments(src);
        assert_eq!(out, "foo      \nbaz\n");
    }

    #[test]
    fn extract_call_finds_register_body() {
        let src = "project(p)\nnano_ros_node_register(\n    NAME talker\n    CLASS p::T)\n";
        let calls = extract_cmake_calls(src, "nano_ros_node_register");
        assert_eq!(calls.len(), 1);
        assert!(calls[0].contains("NAME talker"));
        assert!(calls[0].contains("CLASS p::T"));
    }

    #[test]
    fn extract_call_skips_identifier_prefix() {
        // `my_nano_ros_node_register` must NOT match.
        let src = "my_nano_ros_node_register(...)\nnano_ros_node_register(NAME real)\n";
        let calls = extract_cmake_calls(src, "nano_ros_node_register");
        assert_eq!(calls.len(), 1);
        assert!(calls[0].contains("NAME real"));
    }

    #[test]
    fn parse_kwargs_extracts_name_class_deploy() {
        let body = r#"
    NAME    talker
    CLASS   talker_pkg::Talker
    SOURCES src/Talker.cpp src/Helper.cpp
    DEPLOY  native freertos
"#;
        let kw = parse_cmake_kwargs(body).unwrap();
        assert_eq!(kw.single("NAME").as_deref(), Some("talker"));
        assert_eq!(kw.single("CLASS").as_deref(), Some("talker_pkg::Talker"));
        assert_eq!(kw.multi("DEPLOY"), vec!["native", "freertos"]);
        assert_eq!(
            kw.multi("SOURCES"),
            vec!["src/Talker.cpp", "src/Helper.cpp"]
        );
    }

    #[test]
    fn parse_kwargs_strips_quoted_strings() {
        let body = r#" NAME "my talker" CLASS "ns::Class" "#;
        let kw = parse_cmake_kwargs(body).unwrap();
        assert_eq!(kw.single("NAME").as_deref(), Some("my talker"));
        assert_eq!(kw.single("CLASS").as_deref(), Some("ns::Class"));
    }

    #[test]
    fn cmake_only_node_pkg_yields_component_declaration() {
        let s = Scratch::new("219l");
        s.write(
            "src/talker_pkg/package.xml",
            "<package format=\"3\"><name>talker_pkg</name><version>0.1.0</version>\
             <description>t</description><maintainer email=\"x@x\">x</maintainer>\
             <license>Apache-2.0</license></package>",
        );
        s.write(
            "src/talker_pkg/CMakeLists.txt",
            r#"project(talker_pkg)
nano_ros_node_register(
    NAME    talker
    CLASS   talker_pkg::Talker
    SOURCES src/Talker.cpp
    DEPLOY  native)
"#,
        );

        let ws = Workspace::discover(&s.0).unwrap();
        assert_eq!(ws.packages.len(), 1);
        assert_eq!(ws.packages[0].cmake_component_metadata.len(), 1);
        let summary = &ws.packages[0].cmake_component_metadata[0];
        assert_eq!(summary.package, "talker_pkg");
        assert_eq!(summary.component, "talker");
        assert_eq!(summary.class.as_deref(), Some("talker_pkg::Talker"));
        assert_eq!(summary.language, ComponentLanguage::Cpp);
        assert_eq!(summary.deploy_targets, vec!["native"]);

        let synth = ws.synthetic_metadata_artifacts();
        assert_eq!(synth.len(), 1);
        assert_eq!(synth[0].1["package"], "talker_pkg");
        assert_eq!(synth[0].1["component"], "talker");
        assert_eq!(synth[0].1["executable"], "talker");
        assert_eq!(synth[0].1["language"], "cpp");
        assert_eq!(synth[0].1["synthetic_source"], "cmake_node_register");

        let decls = ws.component_declarations().unwrap();
        assert_eq!(decls.len(), 1);
        let cfg = &decls[0].config;
        assert_eq!(cfg.package, "talker_pkg");
        assert_eq!(cfg.component, "talker");
        assert_eq!(cfg.language, ComponentLanguage::Cpp);
    }

    /// Verifies CMake declarations are skipped when nros.toml declares the same pair.
    #[test]
    fn cmake_decls_skip_duplicate_pair() {
        // Explicit `nros.toml` `[component]` table wins over cmake parse.
        let s = Scratch::new("219l");
        s.write(
            "src/talker_pkg/package.xml",
            "<package format=\"3\"><name>talker_pkg</name><version>0.1.0</version>\
             <description>t</description><maintainer email=\"x@x\">x</maintainer>\
             <license>Apache-2.0</license></package>",
        );
        s.write(
            "src/talker_pkg/nros.toml",
            r#"[component]
version = 1
package = "talker_pkg"
component = "talker"
language = "cpp"
[component.metadata]
source_metadata = "metadata/talker.json"
"#,
        );
        s.write(
            "src/talker_pkg/CMakeLists.txt",
            r#"nano_ros_node_register(NAME talker CLASS talker_pkg::Talker DEPLOY native)
"#,
        );
        let ws = Workspace::discover(&s.0).unwrap();
        let decls = ws.component_declarations().unwrap();
        assert_eq!(decls.len(), 1, "explicit nros.toml wins");
        assert_eq!(decls[0].manifest_path.file_name().unwrap(), "nros.toml");
    }

    #[test]
    fn cmake_decl_language_default_is_cpp_for_pkg_class_qualname() {
        let s = Scratch::new("219l");
        s.write(
            "src/talker_pkg/package.xml",
            "<package format=\"3\"><name>talker_pkg</name><version>0.1.0</version>\
             <description>t</description><maintainer email=\"x@x\">x</maintainer>\
             <license>Apache-2.0</license></package>",
        );
        s.write(
            "src/talker_pkg/CMakeLists.txt",
            "nano_ros_node_register(NAME t CLASS p::C DEPLOY native)\n",
        );
        let decls = Workspace::discover(&s.0)
            .unwrap()
            .component_declarations()
            .unwrap();
        assert_eq!(decls[0].config.language, ComponentLanguage::Cpp);
    }

    #[test]
    fn cmake_decl_language_c_when_class_has_no_qualname() {
        let s = Scratch::new("219l");
        s.write(
            "src/talker_pkg/package.xml",
            "<package format=\"3\"><name>talker_pkg</name><version>0.1.0</version>\
             <description>t</description><maintainer email=\"x@x\">x</maintainer>\
             <license>Apache-2.0</license></package>",
        );
        s.write(
            "src/talker_pkg/CMakeLists.txt",
            "nano_ros_node_register(NAME t CLASS register_t DEPLOY native)\n",
        );
        let decls = Workspace::discover(&s.0)
            .unwrap()
            .component_declarations()
            .unwrap();
        assert_eq!(decls[0].config.language, ComponentLanguage::C);
    }

    #[test]
    fn cmake_decl_language_c_when_language_keyword_is_c() {
        let s = Scratch::new("223c");
        s.write(
            "src/c_talker_pkg/package.xml",
            "<package format=\"3\"><name>c_talker_pkg</name><version>0.1.0</version>\
             <description>t</description><maintainer email=\"x@x\">x</maintainer>\
             <license>Apache-2.0</license></package>",
        );
        s.write(
            "src/c_talker_pkg/CMakeLists.txt",
            r#"nano_ros_node_register(
    NAME talker
    CLASS c_talker_pkg::Talker
    LANGUAGE C
    SOURCES src/Talker.c
    DEPLOY native)
"#,
        );
        let ws = Workspace::discover(&s.0).unwrap();
        let summary = &ws.packages[0].cmake_component_metadata[0];
        assert_eq!(summary.language, ComponentLanguage::C);

        let synth = ws.synthetic_metadata_artifacts();
        assert_eq!(synth.len(), 1);
        assert_eq!(synth[0].1["package"], "c_talker_pkg");
        assert_eq!(synth[0].1["component"], "talker");
        assert_eq!(synth[0].1["executable"], "talker");
        assert_eq!(synth[0].1["language"], "c");
        assert_eq!(synth[0].1["class"], "c_talker_pkg::Talker");
    }

    #[test]
    fn cmake_decl_skipped_when_name_missing() {
        let s = Scratch::new("219l");
        s.write(
            "src/talker_pkg/package.xml",
            "<package format=\"3\"><name>talker_pkg</name><version>0.1.0</version>\
             <description>t</description><maintainer email=\"x@x\">x</maintainer>\
             <license>Apache-2.0</license></package>",
        );
        s.write(
            "src/talker_pkg/CMakeLists.txt",
            "nano_ros_node_register(CLASS p::T DEPLOY native)\n",
        );
        let ws = Workspace::discover(&s.0).unwrap();
        assert!(ws.packages[0].cmake_component_metadata.is_empty());
    }
}
