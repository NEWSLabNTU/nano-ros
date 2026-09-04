//! Phase 212.B.2 — `NrosConfig::from_cargo_metadata` loader.
//!
//! Reads the user-authored Phase 212 surfaces out of a cargo workspace:
//!
//! * `[workspace.metadata.nros]` in the workspace-root `Cargo.toml`
//! * `[package.metadata.nros]` in every workspace-member `Cargo.toml`
//!   (single-shape `node` — canonical post Phase 212.N.12, or
//!   deprecated alias `component` — or multi-shape `components.<Name>`)
//! * `[package.metadata.ament]` in every workspace-member `Cargo.toml`
//! * `<bringup-pkg>/system.toml` for every bringup package the workspace
//!   exposes (a bringup package is a workspace member whose
//!   `package.metadata.nros` is absent and which carries a sibling
//!   `system.toml` next to its `Cargo.toml`).
//!
//! No silent fallback to the old `nros.toml` surface (Phase 172). A
//! workspace whose root carries a `nros.toml` next to its `Cargo.toml`
//! is rejected with a migration pointer (see Phase 212.I).

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use cargo_metadata::MetadataCommand;
use serde::Deserialize;
use thiserror::Error;

use super::cargo_metadata_schema::{
    DeployTarget, DeployTargetMetadata, PackageMetadataAment, PackageMetadataNros,
    SystemComponentEntry, SystemHeader, SystemToml, WorkspaceMetadataNros,
};

/// Errors surfaced by the Phase 212.B loader. Distinct from the catch-all
/// `eyre::Result` so callers can match on `NrosTomlNotSupported` and route to
/// the migration tool (Phase 212.I).
#[derive(Debug, Error)]
pub enum NrosConfigError {
    /// A pre-212 `nros.toml` sits at the workspace root. Clean break — point
    /// the user at the migration path (#186 retired the one-shot
    /// `migrate workspace` verb; it lives in the nros-v0.5.0 tag's CLI).
    #[error(
        "nros.toml at workspace root is no longer supported (Phase 212.B); \
         run `nros migrate workspace .` from the nros-v0.5.0 tag's CLI (the \
         verb is retired on newer trees), or start fresh from \
         `nros new system <bringup>` \
         (see docs/roadmap/phase-212-ux-cargo-native-and-file-consolidation.md)"
    )]
    NrosTomlNotSupported { path: PathBuf },

    /// `cargo metadata` failed (bad manifest, missing cargo, etc.). The
    /// underlying error is preserved.
    #[error("cargo metadata at {path:?}: {source}")]
    CargoMetadata {
        path: PathBuf,
        #[source]
        source: cargo_metadata::Error,
    },

    /// A `[workspace.metadata.nros]` table was present but failed to
    /// deserialize against the strict schema (typo / unknown field).
    #[error("invalid [workspace.metadata.nros] in {manifest:?}: {message}")]
    InvalidWorkspaceMetadata { manifest: PathBuf, message: String },

    /// A per-package `[package.metadata.nros]` failed to deserialize or
    /// failed the mutual-exclusion check between `component` and `components`.
    #[error("invalid [package.metadata.nros] in package `{package}` ({manifest:?}): {message}")]
    InvalidPackageMetadata {
        package: String,
        manifest: PathBuf,
        message: String,
    },

    /// `[package.metadata.ament]` failed to deserialize.
    #[error("invalid [package.metadata.ament] in package `{package}` ({manifest:?}): {message}")]
    InvalidAmentMetadata {
        package: String,
        manifest: PathBuf,
        message: String,
    },

    /// A bringup package's `system.toml` could not be read.
    #[error("read {path:?} for bringup package `{package}`: {source}")]
    BringupSystemTomlIo {
        package: String,
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    /// A bringup package's `system.toml` failed to parse.
    #[error("parse {path:?} for bringup package `{package}`: {source}")]
    BringupSystemTomlParse {
        package: String,
        path: PathBuf,
        // Boxed to keep `NrosConfigError` small (issue 0379 / clippy
        // `result_large_err`): `toml::de::Error` alone is ~144 bytes.
        #[source]
        source: Box<toml::de::Error>,
    },

    /// A bringup system (on-disk or synthesised) declares an RMW backend that
    /// is not one of the supported set (Phase 227.2, RFC-0031).
    #[error("bringup package `{package}`: {source}")]
    InvalidSystemRmw {
        package: String,
        #[source]
        source: crate::orchestration::rmw_resolver::UnknownRmw,
    },

    /// phase-420 W6 — a root `nros.toml` exists but is not readable TOML.
    ///
    /// Its own error rather than falling through to `NrosTomlNotSupported`: a
    /// typo in a search-path file would otherwise be answered with a migration
    /// instruction for a surface the author never used, which is a message
    /// aimed at the wrong problem.
    #[error("parse {path:?}: {source}")]
    NrosTomlParse {
        path: PathBuf,
        // Boxed for the same reason as `BringupSystemTomlParse` — clippy
        // `result_large_err`.
        #[source]
        source: Box<toml::de::Error>,
    },
}

/// Phase 212.B `NrosConfig` — every nros-relevant fact derived from a
/// cargo workspace. Replaces the `nros.toml` reader from Phase 172.
///
/// The data is purely descriptive: callers (`nros plan`, `nros check`,
/// `nros codegen system`, …) consume this to do their work. The loader
/// performs strict schema validation but no cross-component semantic
/// validation — that is the planner's job.
#[derive(Clone, Debug, Default)]
pub struct NrosConfig {
    /// Workspace root directory (where `Cargo.toml` lives).
    pub workspace_root: PathBuf,
    /// `[workspace.metadata.nros]` — absent in some workspaces (treated as
    /// `WorkspaceMetadataNros::default()`).
    pub workspace_metadata: WorkspaceMetadataNros,
    /// Component packages: workspace members carrying
    /// `[package.metadata.nros]`. Keyed by cargo package name.
    pub component_packages: BTreeMap<String, ComponentPackageEntry>,
    /// Bringup packages: workspace members WITHOUT `[package.metadata.nros]`
    /// that carry a sibling `system.toml`. Keyed by cargo package name.
    pub bringup_packages: BTreeMap<String, BringupPackageEntry>,
}

/// A workspace member that exposes one or more nros components via its
/// `[package.metadata.nros]` table.
#[derive(Clone, Debug)]
pub struct ComponentPackageEntry {
    pub name: String,
    pub manifest_path: PathBuf,
    pub nros: PackageMetadataNros,
    pub ament: PackageMetadataAment,
}

/// Phase 212.L.7 — provenance of a [`BringupPackageEntry`]. Helps `nros
/// plan` / `nros codegen-system` distinguish a real `system.toml`-backed
/// bringup from a synthesised one (self-bringup component / application
/// pkg) — the latter has no on-disk `system.toml` and the resolver tracks
/// it via the host package's manifest instead.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BringupSource {
    /// Real bringup pkg with an on-disk `system.toml` (Path A or Path B).
    SystemToml,
    /// Synthesised from a self-bringup component / application pkg's
    /// `[package.metadata.nros]` + `[package.metadata.nros.deploy.*]`
    /// (Phase 212.L.7).
    SelfBringup,
}

/// A workspace member declared as the bringup pkg for a system. Carries a
/// loaded `system.toml` (or a synthesised one for self-bringup pkgs).
#[derive(Clone, Debug)]
pub struct BringupPackageEntry {
    pub name: String,
    pub manifest_path: PathBuf,
    /// `<bringup>/system.toml`. For self-bringup pkgs this points at the
    /// host pkg's `Cargo.toml` (no on-disk system.toml exists); callers
    /// keying on file presence should consult `source` instead.
    pub system_toml_path: PathBuf,
    pub system: SystemToml,
    /// Bringup pkgs may still declare `[package.metadata.ament]` for
    /// `package.xml` regeneration (Phase 212.G).
    pub ament: PackageMetadataAment,
    /// Phase 212.L.7 — provenance (`SystemToml` vs `SelfBringup`).
    pub source: BringupSource,
}

impl NrosConfig {
    /// Load `NrosConfig` from either a Cargo workspace or a C/C++ style
    /// workspace rooted at `workspace_root`.
    ///
    /// Cargo workspaces keep using `cargo metadata`. Non-Cargo workspaces
    /// still support Path-A Bringup packages (`package.xml` + `system.toml`
    /// and no build file) through the recursive package index.
    pub fn from_workspace(workspace_root: &Path) -> Result<Self, NrosConfigError> {
        if workspace_root.join("Cargo.toml").is_file() {
            return Self::from_cargo_metadata(workspace_root);
        }

        let mut bringup_packages = BTreeMap::new();
        discover_path_a_bringups(workspace_root, &mut bringup_packages)?;

        Ok(NrosConfig {
            workspace_root: workspace_root.to_path_buf(),
            workspace_metadata: WorkspaceMetadataNros::default(),
            component_packages: BTreeMap::new(),
            bringup_packages,
        })
    }

    /// Load `NrosConfig` from a cargo workspace rooted at `workspace_root`.
    ///
    /// Steps:
    ///
    /// 1. Reject a `nros.toml` sitting next to the workspace `Cargo.toml`
    ///    with a migration pointer (Phase 212.I).
    /// 2. Shell `cargo metadata --no-deps --format-version 1` via the
    ///    `cargo_metadata` crate.
    /// 3. Parse `metadata.workspace_metadata["nros"]` into
    ///    [`WorkspaceMetadataNros`].
    /// 4. For each workspace member, parse `package.metadata["nros"]` into
    ///    [`PackageMetadataNros`] and `package.metadata["ament"]` into
    ///    [`PackageMetadataAment`].
    /// 5. A member with no `[package.metadata.nros]` AND a sibling
    ///    `system.toml` becomes a bringup pkg (loaded into [`SystemToml`]).
    pub fn from_cargo_metadata(workspace_root: &Path) -> Result<Self, NrosConfigError> {
        // 1 — clean-break rejection of the old root nros.toml, narrowed by
        // phase-420 W6 to the files it was aimed at. See
        // `nros_toml_is_search_path_only`.
        let root_nros_toml = workspace_root.join("nros.toml");
        if root_nros_toml.exists() && !nros_toml_is_search_path_only(&root_nros_toml)? {
            return Err(NrosConfigError::NrosTomlNotSupported {
                path: root_nros_toml,
            });
        }

        let manifest_path = workspace_root.join("Cargo.toml");

        // 2 — run cargo metadata.
        let metadata = MetadataCommand::new()
            .manifest_path(&manifest_path)
            // Run FROM the workspace, not from wherever the caller happens to
            // stand. `manifest_path` tells cargo which manifest to read; it does
            // NOT tell it where to look for `.cargo/config.toml`, which cargo
            // discovers by walking UP from the CURRENT DIRECTORY. Without this,
            // `nros` invoked inside the nano-ros checkout applied THIS repo's
            // cargo config — its `[patch.crates-io]` set included — to a
            // completely unrelated workspace, and the resolved package set came
            // back different. phase-383 W9.c: three codegen tests failed only
            // when the suite was run from the repo root, and passed from /tmp.
            .current_dir(workspace_root)
            .no_deps()
            .exec()
            .map_err(|source| NrosConfigError::CargoMetadata {
                path: manifest_path.clone(),
                source,
            })?;

        // 3 — workspace-level metadata.
        let workspace_metadata =
            parse_workspace_metadata(&metadata.workspace_metadata).map_err(|message| {
                NrosConfigError::InvalidWorkspaceMetadata {
                    manifest: manifest_path.clone(),
                    message,
                }
            })?;

        // 4 — per-member metadata + 5 bringup discovery.
        let mut component_packages: BTreeMap<String, ComponentPackageEntry> = BTreeMap::new();
        let mut bringup_packages: BTreeMap<String, BringupPackageEntry> = BTreeMap::new();

        let member_ids: std::collections::HashSet<&cargo_metadata::PackageId> =
            metadata.workspace_members.iter().collect();

        for package in &metadata.packages {
            if !member_ids.contains(&package.id) {
                continue;
            }

            let pkg_manifest = PathBuf::from(package.manifest_path.as_str());

            let ament = parse_ament_metadata(&package.metadata).map_err(|message| {
                NrosConfigError::InvalidAmentMetadata {
                    package: package.name.clone(),
                    manifest: pkg_manifest.clone(),
                    message,
                }
            })?;

            let nros_opt = parse_package_metadata_nros(&package.metadata).map_err(|message| {
                NrosConfigError::InvalidPackageMetadata {
                    package: package.name.clone(),
                    manifest: pkg_manifest.clone(),
                    message,
                }
            })?;

            match nros_opt {
                Some(nros) => {
                    // Validate single-vs-multi shape exclusion.
                    nros.validate()
                        .map_err(|message| NrosConfigError::InvalidPackageMetadata {
                            package: package.name.clone(),
                            manifest: pkg_manifest.clone(),
                            message,
                        })?;
                    component_packages.insert(
                        package.name.clone(),
                        ComponentPackageEntry {
                            name: package.name.clone(),
                            manifest_path: pkg_manifest,
                            nros,
                            ament,
                        },
                    );
                }
                None => {
                    // Bringup-pkg candidate: look for a sibling `system.toml`.
                    let pkg_dir = pkg_manifest.parent().unwrap_or_else(|| Path::new(""));
                    let system_toml_path = pkg_dir.join("system.toml");
                    if system_toml_path.exists() {
                        let raw = std::fs::read_to_string(&system_toml_path).map_err(|source| {
                            NrosConfigError::BringupSystemTomlIo {
                                package: package.name.clone(),
                                path: system_toml_path.clone(),
                                source,
                            }
                        })?;
                        let system: SystemToml = toml::from_str(&raw).map_err(|source| {
                            NrosConfigError::BringupSystemTomlParse {
                                package: package.name.clone(),
                                path: system_toml_path.clone(),
                                source: Box::new(source),
                            }
                        })?;
                        bringup_packages.insert(
                            package.name.clone(),
                            BringupPackageEntry {
                                name: package.name.clone(),
                                manifest_path: pkg_manifest,
                                system_toml_path,
                                system,
                                ament,
                                source: BringupSource::SystemToml,
                            },
                        );
                    }
                    // Else: a plain workspace member with no nros surface —
                    // ignored (it may be a util/lib crate the bringup pulls in).
                }
            }
        }

        // Phase 212.F.3 — Path A bringup discovery via dirwalk.
        //
        // Bringup pkgs ship `package.xml` + `system.toml` but no
        // `Cargo.toml`; cargo's workspace `exclude` list keeps them out of
        // `metadata.packages`, so the member-loop above never sees them.
        // Walk the workspace root for sibling dirs that match the bringup
        // shape and load each as a `BringupPackageEntry` keyed on its
        // `package.xml` name. Use the recursive package index so colcon-style
        // `src/<pkg>/package.xml` workspaces behave the same as immediate
        // child layouts.
        discover_path_a_bringups(workspace_root, &mut bringup_packages)?;

        // Phase 212.L.7 — self-bringup synthesis.
        //
        // A component / application pkg whose `[package.metadata.nros]`
        // carries `[deploy.<target>]` AND that is not already named as
        // `pkg = "<name>"` by any sibling bringup pkg becomes its own
        // degenerate 1-component bringup. We synthesise a SystemToml
        // from the component metadata + the first deploy block so the
        // planner / codegen path can consume it uniformly.
        let referenced_by_bringup: std::collections::HashSet<String> = bringup_packages
            .values()
            .flat_map(|b| b.system.components.iter().map(|c| c.pkg.clone()))
            .collect();
        let mut synthesised: Vec<(String, BringupPackageEntry)> = Vec::new();
        for comp in component_packages.values() {
            if !comp.nros.is_self_bringup_eligible() {
                continue;
            }
            if referenced_by_bringup.contains(&comp.name) {
                continue; // A real bringup already wires this pkg in.
            }
            if bringup_packages.contains_key(&comp.name) {
                continue; // A bringup pkg already exists under this name.
            }
            let synth = synthesise_self_bringup(comp);
            synthesised.push((comp.name.clone(), synth));
        }
        for (k, v) in synthesised {
            bringup_packages.insert(k, v);
        }

        // Phase 227.2 — validate the declared RMW on every bringup (on-disk,
        // Path-A, and synthesised single-node) so a typo fails early with the
        // known-list rather than as a broken downstream build (RFC-0031).
        for (name, entry) in &bringup_packages {
            crate::orchestration::rmw_resolver::resolve_rmw(&entry.system.system.rmw).map_err(
                |source| NrosConfigError::InvalidSystemRmw {
                    package: name.clone(),
                    source,
                },
            )?;
        }

        Ok(NrosConfig {
            workspace_root: workspace_root.to_path_buf(),
            workspace_metadata,
            component_packages,
            bringup_packages,
        })
    }
}

// ---------------------------------------------------------------------------
// The provider search path — phase-420 W6 (RFC-0087 D6)
// ---------------------------------------------------------------------------

/// `<ws>/nros.toml`, as far as the search path is concerned.
///
/// Deliberately NOT `deny_unknown_fields`: this reader answers one question and
/// must not become a second validator of a file whose ownership sits elsewhere.
/// What the file is ALLOWED to contain is decided by
/// [`nros_toml_is_search_path_only`], in one place.
#[derive(Debug, Default, Deserialize)]
struct NrosTomlSearchPath {
    #[serde(default)]
    workspace: NrosTomlWorkspaceTable,
}

#[derive(Debug, Default, Deserialize)]
struct NrosTomlWorkspaceTable {
    #[serde(default)]
    package_paths: Vec<String>,
}

/// `[workspace] package_paths` from `<ws>/nros.toml`, in authored order.
///
/// Empty when the file is absent, which is the common case — a workspace that
/// configures no extra roots gets the two-root default and never has to own a
/// file to say so.
///
/// **Read directly rather than through `cargo metadata`.** The search path is
/// needed by `nros ws providers`, by `nros sync` and by a cmake configure, on
/// C/C++ workspaces that have no `Cargo.toml` at all; routing it through
/// `NrosConfig` would make "which roots do I scan" cost a `cargo metadata`
/// subprocess and answer nothing for half the workspaces that ask.
pub fn workspace_package_paths(workspace_root: &Path) -> Result<Vec<String>, NrosConfigError> {
    let path = workspace_root.join("nros.toml");
    let Some(parsed) = read_nros_toml_search_path(&path)? else {
        return Ok(Vec::new());
    };
    Ok(parsed.workspace.package_paths)
}

/// Parse the search-path view of a root `nros.toml`. `Ok(None)` ⇒ no such file.
fn read_nros_toml_search_path(path: &Path) -> Result<Option<NrosTomlSearchPath>, NrosConfigError> {
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        // Unreadable is treated as absent on purpose: this is a lookup on the
        // way to a scan, and a permissions problem on an optional file must not
        // take down a build that would otherwise resolve every provider from
        // the default roots.
        Err(_) => return Ok(None),
    };
    let parsed: NrosTomlSearchPath =
        toml::from_str(&text).map_err(|source| NrosConfigError::NrosTomlParse {
            path: path.to_path_buf(),
            source: Box::new(source),
        })?;
    Ok(Some(parsed))
}

/// Does this `nros.toml` carry ONLY the phase-420 W6 search-path surface?
///
/// **Why the Phase 212.I rejection is narrowed rather than lifted.** That
/// rejection exists so a pre-212 `nros.toml` — which carried the whole system
/// definition, `[system]` and `[deploy.*]` and a `[workspace] default` — cannot
/// be silently ignored by a loader that no longer reads it. RFC-0087 D6 then
/// spells the search path `[workspace] package_paths` in that same file, and a
/// blanket rejection would make the RFC's own example unusable in any cargo
/// workspace: the user writes the documented key and every `nros plan`,
/// `nros codegen-system` and `nros config` refuses the workspace, quoting a
/// migration for a surface they never had.
///
/// So the rule is narrowed to exactly the shape it was aimed at: a file whose
/// only top-level table is `[workspace]` and whose only key there is
/// `package_paths` is the new surface and is accepted; anything else — a
/// `[system]`, a `[deploy.*]`, or the legacy `[workspace] default` sitting
/// beside it — is still the old file and still gets the migration pointer. No
/// legacy file can pass, because none of them consists of that key alone.
fn nros_toml_is_search_path_only(path: &Path) -> Result<bool, NrosConfigError> {
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        // Unreadable: fall back to the pre-W6 answer. A file we cannot read is
        // not one we may declare harmless.
        Err(_) => return Ok(false),
    };
    let table: toml::Table =
        toml::from_str(&text).map_err(|source| NrosConfigError::NrosTomlParse {
            path: path.to_path_buf(),
            source: Box::new(source),
        })?;
    // EXACTLY `[workspace] package_paths`, nothing more and nothing less. The
    // "nothing less" half matters: a bare `[workspace]` with no keys declares
    // no search path, so it is a legacy file's remnant rather than the new
    // surface, and accepting it would silence the migration pointer for a file
    // that still needs one.
    let Some(workspace) = table.get("workspace").and_then(|v| v.as_table()) else {
        return Ok(false);
    };
    Ok(table.len() == 1 && workspace.len() == 1 && workspace.contains_key("package_paths"))
}

// ---------------------------------------------------------------------------
// `cargo metadata` JSON helpers
// ---------------------------------------------------------------------------

/// `metadata.workspace_metadata` is a free-form `serde_json::Value`. Pull
/// `nros` out and re-parse via the strict schema. Returns the default when
/// the table is absent.
fn parse_workspace_metadata(value: &serde_json::Value) -> Result<WorkspaceMetadataNros, String> {
    let Some(nros) = value.get("nros") else {
        return Ok(WorkspaceMetadataNros::default());
    };
    WorkspaceMetadataNros::deserialize(nros.clone()).map_err(|e| e.to_string())
}

fn discover_path_a_bringups(
    workspace_root: &Path,
    bringup_packages: &mut BTreeMap<String, BringupPackageEntry>,
) -> Result<(), NrosConfigError> {
    let Ok(index) = crate::pkg_index::build_pkg_index(workspace_root) else {
        return Ok(());
    };

    for (name, path) in index.pkgs() {
        if bringup_packages.contains_key(name) {
            continue;
        }
        let system_toml_path = path.join("system.toml");
        let cargo_toml_path = path.join("Cargo.toml");
        let package_xml_path = path.join("package.xml");
        if cargo_toml_path.exists() {
            continue; // Has Cargo.toml → not Path A.
        }
        if !system_toml_path.exists() || !package_xml_path.exists() {
            continue;
        }
        let raw = std::fs::read_to_string(&system_toml_path).map_err(|source| {
            NrosConfigError::BringupSystemTomlIo {
                package: name.to_string(),
                path: system_toml_path.clone(),
                source,
            }
        })?;
        let system: SystemToml =
            toml::from_str(&raw).map_err(|source| NrosConfigError::BringupSystemTomlParse {
                package: name.to_string(),
                path: system_toml_path.clone(),
                source: Box::new(source),
            })?;
        bringup_packages.insert(
            name.to_string(),
            BringupPackageEntry {
                name: name.to_string(),
                manifest_path: package_xml_path.clone(),
                system_toml_path,
                system,
                ament: Default::default(),
                source: BringupSource::SystemToml,
            },
        );
    }

    Ok(())
}

/// `package.metadata` likewise. Returns `Ok(None)` when the `nros` key is
/// absent.
///
/// Phase 212.N.12 — accept `[package.metadata.nros.node]` as the canonical
/// key (renamed from `.component`). The deprecated `.component` key still
/// parses (with a stderr warning); declaring both is a hard error.
fn parse_package_metadata_nros(
    value: &serde_json::Value,
) -> Result<Option<PackageMetadataNros>, String> {
    let Some(nros) = value.get("nros") else {
        return Ok(None);
    };
    let normalised = normalise_node_alias(nros.clone())?;
    PackageMetadataNros::deserialize(normalised)
        .map(Some)
        .map_err(|e| e.to_string())
}

/// Phase 212.N.12 — accept `node` as the canonical alias for `component`
/// (single-shape) inside `[package.metadata.nros]`. Rules:
///
/// 1. `node` only → rename to `component` (canonical).
/// 2. `component` only → warn to stderr (deprecated), keep as-is.
/// 3. Both `node` and `component` → hard error (ambiguous).
/// 4. Neither → unchanged.
///
/// Multi-shape (`components.<Name>` table-of-tables) and `application`
/// are untouched — the rename in the design doc only renamed the
/// single-shape `component` to `node`; the multi-shape and other tables
/// keep their existing keys.
fn normalise_node_alias(mut nros: serde_json::Value) -> Result<serde_json::Value, String> {
    let Some(obj) = nros.as_object_mut() else {
        return Ok(nros);
    };
    let has_node = obj.contains_key("node");
    let has_component = obj.contains_key("component");
    match (has_node, has_component) {
        (true, true) => Err(
            "`[package.metadata.nros]` declares BOTH `node` (canonical) and \
             `component` (deprecated alias) — pick exactly one (Phase 212.N.12)"
                .to_string(),
        ),
        (true, false) => {
            // Rename `node` → `component` so the existing
            // `PackageMetadataNros` schema (still field-named
            // `component`) deserialises cleanly.
            if let Some(v) = obj.remove("node") {
                obj.insert("component".to_string(), v);
            }
            Ok(nros)
        }
        (false, true) => {
            // Deprecated alias kept working — warn once per parse.
            eprintln!(
                "warning: `[package.metadata.nros.component]` is the pre-N.12 \
                 alias; use `[package.metadata.nros.node]` (Phase 212.N.12)."
            );
            Ok(nros)
        }
        (false, false) => Ok(nros),
    }
}

fn parse_ament_metadata(value: &serde_json::Value) -> Result<PackageMetadataAment, String> {
    let Some(ament) = value.get("ament") else {
        return Ok(PackageMetadataAment::default());
    };
    PackageMetadataAment::deserialize(ament.clone()).map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// Phase 212.L.7 — self-bringup synthesis
// ---------------------------------------------------------------------------

/// Synthesise a [`BringupPackageEntry`] for a self-bringup component /
/// application pkg. The first declared `[deploy.<target>]` block supplies
/// the `[system]` header (rmw / domain_id / locator); every declared deploy
/// block flows into `system.deploy.<target>`; the component(s) flow into
/// `system.components` (single `[component]` → one entry using `class` +
/// `name` fallback; multi `[components.<N>]` → one per entry; application
/// pkgs synthesise an empty component list — the orchestration root is the
/// pkg itself).
fn synthesise_self_bringup(comp: &ComponentPackageEntry) -> BringupPackageEntry {
    let nros = &comp.nros;

    // Pick the first deploy target deterministically (BTreeMap iteration
    // order is key-sorted). The `[system]` header's rmw / domain_id /
    // locator come from this block, defaulting when the deploy table
    // omits them.
    let (first_target_name, first_deploy) = nros
        .deploy
        .iter()
        .next()
        .map(|(k, v)| (k.clone(), v.clone()))
        .unwrap_or_else(|| ("native".to_string(), DeployTargetMetadata::default()));
    let _ = first_target_name; // recorded via the deploy map below.

    let system_header = SystemHeader {
        default_images: Vec::new(),
        name: comp.name.clone(),
        rmw: first_deploy
            .rmw
            .clone()
            .unwrap_or_else(|| "zenoh".to_string()),
        domain_id: first_deploy.domain_id.unwrap_or(0),
        // phase-421 W4 — carry the Cargo-native `[..deploy.<t>].serdes`
        // projection into the synthesized header, the same way `rmw` above
        // comes from the first deploy block. Absent ⇒ `cdr` at resolve time.
        serdes: first_deploy.serdes.clone(),
        // Self-bringup synthesized from component deploy metadata, which carries
        // no edition today → humble default (RFC-0056; phase-304 W2 follow-up:
        // thread a component-declared edition here if one is added).
        ros_edition: None,
        locator: first_deploy.locator.clone(),
        default_launch: None,
        default_target: None,
        features: Vec::new(),
    };

    // Component rows. The `node` spelling (Phase 212.N.12 rename) is
    // accepted as an alias for `component` via `node_or_component()`.
    let mut components: Vec<SystemComponentEntry> = Vec::new();
    if let Some(single) = nros.node_or_component() {
        let class = single
            .class
            .clone()
            .unwrap_or_else(|| format!("{}::Node", comp.name));
        let inst_name = single.name.clone().unwrap_or_else(|| comp.name.clone());
        components.push(SystemComponentEntry {
            pkg: comp.name.clone(),
            class,
            name: inst_name,
            group_tiers: std::collections::BTreeMap::new(),
            params: Default::default(),
            params_files: Vec::new(),
        });
    } else {
        // Phase 212.N.12 in-flight — read the multi-shape via the
        // `nodes_or_components()` accessor so the synthesis works the same
        // whether the manifest uses the legacy `components` spelling or
        // the forward-looking `nodes` spelling.
        for (key, meta) in nros.nodes_or_components() {
            let class = meta
                .class
                .clone()
                .unwrap_or_else(|| format!("{}::{}", comp.name, key));
            let inst_name = meta.name.clone().unwrap_or_else(|| key.clone());
            components.push(SystemComponentEntry {
                pkg: comp.name.clone(),
                class,
                name: inst_name,
                group_tiers: std::collections::BTreeMap::new(),
                params: Default::default(),
                params_files: Vec::new(),
            });
        }
    }
    // Application pkgs: no components on this pkg directly; bringup body
    // stays empty. (Future: discover sibling component pkgs the
    // application allow-lists.)

    // Deploy block — every `[package.metadata.nros.deploy.<target>]`
    // becomes a `[deploy.<target>]` row. The bringup `DeployTarget`
    // schema is shaped around `kind` / `target` / optional `launch` /
    // `board`. Map: `target` ⇒ the target-name key; `kind` ⇒ `"self"`
    // (this is a self-bringup, definitionally); `board` ⇒ verbatim.
    let mut deploy: BTreeMap<String, DeployTarget> = BTreeMap::new();
    for (target_name, dt) in &nros.deploy {
        deploy.insert(
            target_name.clone(),
            DeployTarget {
                // phase-351 W1 — the site block is authored, never synthesised here.
                nros: None,
                kind: Some("self".to_string()),
                target: Some(target_name.clone()),
                launch: None,
                board: dt.board.clone(),
                framework: None,
                // Phase 255 — carry the Cargo-native `[..deploy.<t>].rmw` projection
                // into the synthesized system DeployTarget (RFC-0004 §3.1 ladder).
                rmw: dt.rmw.clone(),
                // Phase 256 W3 — build tuning has no Cargo-native projection yet.
                profile: None,
                optimize: None,
                features: Vec::new(),
                // Phase 256 W8 — carry the Cargo-native deploy domain/locator
                // projection into the synthesized DeployTarget (RFC-0004 §3.1).
                domain_id: dt.domain_id,
                locator: dt.locator.clone(),
                // A self-bringup deploy governs every node in its launch file;
                // explicit placement is the multi-deploy case, which Cargo
                // metadata has no projection for.
                nodes: Vec::new(),
            },
        );
    }

    // Issue 0951 — project the same rows as images. `resolve_image`'s image
    // rung and every `image_for` reader see a synthesised bringup exactly as
    // they see an authored one.
    let image: BTreeMap<String, crate::orchestration::image::ImageBlock> = nros
        .deploy
        .iter()
        .map(|(target_name, dt)| {
            (
                target_name.clone(),
                crate::orchestration::image::ImageBlock {
                    board: dt.board.clone(),
                    rmw: dt.rmw.clone(),
                    // phase-421 W4 — the Cargo-native deploy block is the one
                    // `[deploy.<t>]` nano-ros owns, and it is where a
                    // `serdes = "…"` key can live at all (system.toml's block
                    // is upstream's `deny_unknown_fields` struct). Projected as
                    // an image so `resolved_serdes`'s image rung sees it.
                    serdes: dt.serdes.clone(),
                    ..Default::default()
                },
            )
        })
        .collect();

    let system = SystemToml {
        board_config: Default::default(),
        system: system_header,
        components,
        deploy,
        // Issue 0951 — a synthesised self-bringup declares no machines. It IS
        // one machine by construction (the component package's own host), and
        // placement over a single implicit host is what the empty map already
        // means.
        host: Default::default(),
        // Issue 0951 — the same `[package.metadata.nros.deploy.<t>]` rows, ALSO
        // projected as images.
        //
        // Not "as well as, for tidiness": every consumer migrating from
        // `[deploy.*]` to `[image.*]` would otherwise read an empty map here
        // and silently lose its values for self-bringup packages — a component
        // package with deploy metadata and no sibling bringup. That is the
        // quiet half of the migration, because a self-bringup is synthesised
        // rather than authored, so no file in the tree shows the gap.
        //
        // Only the fields an image OWNS cross over: `board` and `rmw`. `kind`
        // and `target` are placement and stay on the deploy side; nothing here
        // sets profile/features, so they stay absent rather than defaulted.
        image,
        image_defaults: None,
        domains: Vec::new(),
        bridges: Vec::new(),
        models: Vec::new(),
        tiers: std::collections::BTreeMap::new(),
        node_overrides: Vec::new(),
        safety: None,
        param_services: None,
        lifecycle: None,
        // RFC-0078 — no declared bounds; absent is the default.
        wcet: None,
    };

    BringupPackageEntry {
        name: comp.name.clone(),
        manifest_path: comp.manifest_path.clone(),
        // No on-disk system.toml; point at the Cargo manifest as a
        // stand-in so callers needing a real path don't crash.
        system_toml_path: comp.manifest_path.clone(),
        system,
        ament: comp.ament.clone(),
        source: BringupSource::SelfBringup,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// Issue 0455 — scratch naming lives in ONE place; see
    /// `crate::test_support`. The local spellings this replaced differed
    /// from each other, and those differences were the bug.
    fn scratch_dir(test: &str) -> PathBuf {
        crate::test_support::scratch_dir(&format!("nros_config_{test}"))
    }

    /// Write the minimal "1 root + 2 component crates + 1 bringup pkg" cargo
    /// workspace into `dir`.
    fn write_minimal_workspace(dir: &Path) {
        // Workspace root manifest.
        fs::write(
            dir.join("Cargo.toml"),
            r#"
[workspace]
resolver = "2"
members = ["talker_pkg", "listener_pkg", "demo_bringup"]

[workspace.metadata.nros]
default_system = "demo_bringup"
"#,
        )
        .unwrap();

        // talker_pkg — single-component shape.
        fs::create_dir_all(dir.join("talker_pkg/src")).unwrap();
        fs::write(
            dir.join("talker_pkg/Cargo.toml"),
            r#"
[package]
name = "talker_pkg"
version = "0.1.0"
edition = "2021"

[lib]
path = "src/lib.rs"

[package.metadata.nros.component]
default_namespace = "/demo"

[package.metadata.nros.component.parameters]
rate_hz = 10

[[package.metadata.nros.component.remaps]]
from = "chatter"
to = "topic/chatter"

[package.metadata.ament]
build_depend = ["rosidl_default_generators"]
exec_depend = ["std_msgs"]
"#,
        )
        .unwrap();
        fs::write(dir.join("talker_pkg/src/lib.rs"), "").unwrap();

        // listener_pkg — multi-component shape.
        fs::create_dir_all(dir.join("listener_pkg/src")).unwrap();
        fs::write(
            dir.join("listener_pkg/Cargo.toml"),
            r#"
[package]
name = "listener_pkg"
version = "0.1.0"
edition = "2021"

[lib]
path = "src/lib.rs"

[package.metadata.nros.components.Listener]
default_namespace = "/demo"

[package.metadata.nros.components.Echo]
default_namespace = "/demo"
"#,
        )
        .unwrap();
        fs::write(dir.join("listener_pkg/src/lib.rs"), "").unwrap();

        // demo_bringup — no [package.metadata.nros], system.toml sibling.
        fs::create_dir_all(dir.join("demo_bringup/src")).unwrap();
        fs::write(
            dir.join("demo_bringup/Cargo.toml"),
            r#"
[package]
name = "demo_bringup"
version = "0.1.0"
edition = "2021"

[lib]
path = "src/lib.rs"

[package.metadata.ament]
exec_depend = ["talker_pkg", "listener_pkg"]
"#,
        )
        .unwrap();
        fs::write(dir.join("demo_bringup/src/lib.rs"), "").unwrap();
        fs::write(
            dir.join("demo_bringup/system.toml"),
            r#"
[system]
name = "demo"
rmw = "zenoh"
domain_id = 0
locator = "tcp/127.0.0.1:7447"

[[component]]
pkg = "talker_pkg"
class = "talker_pkg::TalkerNode"
name = "talker"

[[component]]
pkg = "listener_pkg"
class = "listener_pkg::ListenerNode"
name = "listener"

[deploy.native]
kind = "self"
target = "x86_64-unknown-linux-gnu"
"#,
        )
        .unwrap();
    }

    /// 212.B.2 — golden fixture w/ root Cargo.toml + 2 component crates +
    /// bringup pkg loads end-to-end.
    #[test]
    fn load_workspace_from_minimal_cargo_metadata() {
        let dir = scratch_dir("load_workspace_from_minimal_cargo_metadata");
        write_minimal_workspace(&dir);

        let cfg = NrosConfig::from_cargo_metadata(&dir).expect("loads minimal workspace");

        assert_eq!(cfg.workspace_root, dir);
        assert_eq!(
            cfg.workspace_metadata.default_system.as_deref(),
            Some("demo_bringup")
        );
        assert!(cfg.workspace_metadata.rmw_override.is_none());

        // Two component packages, one bringup.
        assert_eq!(cfg.component_packages.len(), 2, "talker + listener");
        assert!(cfg.component_packages.contains_key("talker_pkg"));
        assert!(cfg.component_packages.contains_key("listener_pkg"));
        assert_eq!(cfg.bringup_packages.len(), 1);
        assert!(cfg.bringup_packages.contains_key("demo_bringup"));
    }

    /// 212.B.2 — `nros.toml` at workspace root is rejected with a migration
    /// pointer (no silent fallback).
    #[test]
    fn nros_toml_at_root_rejected_with_migration_pointer() {
        let dir = scratch_dir("nros_toml_at_root_rejected_with_migration_pointer");
        write_minimal_workspace(&dir);

        // Drop a pre-212 `nros.toml` next to the workspace root.
        fs::write(dir.join("nros.toml"), "[workspace]\n").unwrap();

        let err = NrosConfig::from_cargo_metadata(&dir).expect_err("must reject root nros.toml");
        match &err {
            NrosConfigError::NrosTomlNotSupported { path } => {
                assert_eq!(path, &dir.join("nros.toml"));
            }
            other => panic!("expected NrosTomlNotSupported, got {other:?}"),
        }
        let msg = err.to_string();
        assert!(
            msg.contains("nros migrate workspace"),
            "diagnostic must mention the migration tool: {msg}"
        );
        assert!(
            msg.contains("Phase 212.B") || msg.contains("phase-212"),
            "diagnostic must point at the phase doc: {msg}"
        );
    }

    // -----------------------------------------------------------------
    // `[workspace] package_paths` — phase-420 W6 (RFC-0087 D6)
    // -----------------------------------------------------------------

    /// The key is read in authored order, and the values are returned VERBATIM.
    /// Resolution (`~`, relative-to-workspace, canonicalization) belongs to
    /// `provider_scan::build_search_path`, which is the one place it happens for
    /// both this source and `NROS_PACKAGE_PATH`.
    #[test]
    fn package_paths_are_read_in_authored_order_and_verbatim() {
        let dir = scratch_dir("package_paths_are_read_in_authored_order_and_verbatim");
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("nros.toml"),
            "[workspace]\npackage_paths = [\"src\", \"~/nros-packages\", \"/opt/nros/packages\"]\n",
        )
        .unwrap();

        let paths = workspace_package_paths(&dir).expect("reads the key");
        assert_eq!(paths, vec!["src", "~/nros-packages", "/opt/nros/packages"]);
    }

    /// No file, or a file without the key, is the common case: a workspace that
    /// configures no extra roots gets the two-root default and never has to own
    /// a file to say so.
    #[test]
    fn a_workspace_with_no_nros_toml_configures_no_extra_roots() {
        let dir = scratch_dir("a_workspace_with_no_nros_toml_configures_no_extra_roots");
        fs::create_dir_all(&dir).unwrap();
        assert_eq!(workspace_package_paths(&dir).unwrap(), Vec::<String>::new());

        fs::write(dir.join("nros.toml"), "[workspace]\npackage_paths = []\n").unwrap();
        assert_eq!(workspace_package_paths(&dir).unwrap(), Vec::<String>::new());
    }

    /// A malformed file gets its OWN error rather than the migration pointer:
    /// a typo in a search path answered with "run `nros migrate workspace`" is
    /// a message aimed at a problem the author does not have.
    #[test]
    fn a_malformed_nros_toml_names_the_parse_error_not_the_migration() {
        let dir = scratch_dir("a_malformed_nros_toml_names_the_parse_error_not_the_migration");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("nros.toml"), "[workspace\npackage_paths = [\n").unwrap();

        let err = workspace_package_paths(&dir).expect_err("must not silently read nothing");
        match &err {
            NrosConfigError::NrosTomlParse { path, .. } => {
                assert_eq!(path, &dir.join("nros.toml"));
            }
            other => panic!("expected NrosTomlParse, got {other:?}"),
        }
    }

    /// phase-420 W6 narrowed the Phase 212.I rejection instead of lifting it.
    ///
    /// RFC-0087 D6 spells the search path `[workspace] package_paths` in
    /// `nros.toml`, and the blanket rejection would have made the RFC's own
    /// example unusable in any cargo workspace — the user writes the documented
    /// key and `nros plan` / `nros codegen-system` / `nros config` all refuse
    /// the workspace, quoting a migration for a surface they never had. So a
    /// file consisting of exactly that key is accepted; everything else is
    /// still the old file and still gets the pointer, which
    /// `nros_toml_at_root_rejected_with_migration_pointer` holds down from the
    /// other side.
    #[test]
    fn a_search_path_only_nros_toml_does_not_trip_the_212_rejection() {
        let dir = scratch_dir("a_search_path_only_nros_toml_does_not_trip_the_212_rejection");
        write_minimal_workspace(&dir);
        fs::write(
            dir.join("nros.toml"),
            "[workspace]\npackage_paths = [\"/opt/nros/packages\"]\n",
        )
        .unwrap();

        let cfg = NrosConfig::from_cargo_metadata(&dir)
            .expect("a search-path-only nros.toml must not refuse the workspace");
        assert!(cfg.component_packages.contains_key("talker_pkg"));
        assert_eq!(
            workspace_package_paths(&dir).unwrap(),
            vec!["/opt/nros/packages"],
        );
    }

    /// And the narrowing is NARROW: a legacy key sitting beside the new one is
    /// still the old file. Every pre-212 `nros.toml` carries `[system]`,
    /// `[deploy.*]` or `[workspace] default`, so none of them can pass.
    #[test]
    fn a_legacy_key_beside_package_paths_is_still_rejected() {
        for legacy in [
            "[workspace]\npackage_paths = [\"x\"]\ndefault = \"native\"\n",
            "[workspace]\npackage_paths = [\"x\"]\n\n[system]\nrmw = \"zenoh\"\n",
            "[workspace]\npackage_paths = [\"x\"]\n\n[deploy.native]\nkind = \"self\"\n",
        ] {
            let dir = scratch_dir("a_legacy_key_beside_package_paths_is_still_rejected");
            write_minimal_workspace(&dir);
            fs::write(dir.join("nros.toml"), legacy).unwrap();

            let err = NrosConfig::from_cargo_metadata(&dir)
                .expect_err("a legacy surface must still get the migration pointer");
            assert!(
                matches!(err, NrosConfigError::NrosTomlNotSupported { .. }),
                "expected NrosTomlNotSupported for {legacy:?}, got {err:?}",
            );
        }
    }

    /// 212.B.2 — single-component-shape `[package.metadata.nros.component]`
    /// parses and round-trips through the loader.
    #[test]
    fn single_component_via_package_metadata_nros_component() {
        let dir = scratch_dir("single_component_via_package_metadata_nros_component");
        write_minimal_workspace(&dir);

        let cfg = NrosConfig::from_cargo_metadata(&dir).expect("loads");

        let talker = cfg
            .component_packages
            .get("talker_pkg")
            .expect("talker present");
        let component = talker
            .nros
            .component
            .as_ref()
            .expect("single-shape component table present");
        assert_eq!(component.default_namespace.as_deref(), Some("/demo"));
        assert_eq!(
            component.parameters.get("rate_hz").map(|v| v.as_integer()),
            Some(Some(10))
        );
        assert_eq!(component.remaps.len(), 1);
        assert_eq!(component.remaps[0].from, "chatter");
        assert!(talker.nros.components.is_empty());

        // The ament side rides through too.
        assert_eq!(talker.ament.build_depend, vec!["rosidl_default_generators"]);
        assert_eq!(talker.ament.exec_depend, vec!["std_msgs"]);
    }

    /// 212.B.2 — multi-component-shape `[package.metadata.nros.components.<N>]`
    /// table-of-tables parses.
    #[test]
    fn multi_component_via_package_metadata_nros_components() {
        let dir = scratch_dir("multi_component_via_package_metadata_nros_components");
        write_minimal_workspace(&dir);

        let cfg = NrosConfig::from_cargo_metadata(&dir).expect("loads");

        let listener = cfg
            .component_packages
            .get("listener_pkg")
            .expect("listener present");
        assert!(listener.nros.component.is_none());
        // BTreeMap-sorted keys.
        let names: Vec<&str> = listener
            .nros
            .components
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(names, ["Echo", "Listener"]);
    }

    // -----------------------------------------------------------------
    // Phase 212.L.7 — self-bringup synthesis
    // -----------------------------------------------------------------

    /// Stage a workspace with a single component pkg that carries
    /// `[deploy.<target>]` AND NO sibling bringup naming it. The loader
    /// must synthesise a `BringupPackageEntry` for the pkg.
    fn write_self_bringup_component_workspace(dir: &Path) {
        fs::write(
            dir.join("Cargo.toml"),
            r#"
[workspace]
resolver = "2"
members = ["alpha_pkg"]
"#,
        )
        .unwrap();
        fs::create_dir_all(dir.join("alpha_pkg/src")).unwrap();
        fs::write(
            dir.join("alpha_pkg/Cargo.toml"),
            r#"
[package]
name = "alpha_pkg"
version = "0.1.0"
edition = "2021"

[lib]
path = "src/lib.rs"

[package.metadata.nros.component]
class = "alpha_pkg::Node"
name = "alpha"
default_namespace = "/demo"

[package.metadata.nros.deploy.native]
board = "native_sim/native/64"
rmw = "zenoh"
domain_id = 7
locator = "tcp/127.0.0.1:7447"
"#,
        )
        .unwrap();
        fs::write(dir.join("alpha_pkg/src/lib.rs"), "").unwrap();
    }

    #[test]
    fn discovers_self_bringup_component_pkg() {
        let dir = scratch_dir("discovers_self_bringup_component_pkg");
        write_self_bringup_component_workspace(&dir);

        let cfg = NrosConfig::from_cargo_metadata(&dir).expect("loads");
        let entry = cfg
            .bringup_packages
            .get("alpha_pkg")
            .expect("self-bringup entry synthesised");
        assert_eq!(entry.source, BringupSource::SelfBringup);
        assert_eq!(entry.system.system.name, "alpha_pkg");
        assert_eq!(entry.system.system.rmw, "zenoh");
        assert_eq!(entry.system.system.domain_id, 7);
        assert_eq!(
            entry.system.system.locator.as_deref(),
            Some("tcp/127.0.0.1:7447")
        );
        assert_eq!(entry.system.components.len(), 1);
        let c = &entry.system.components[0];
        assert_eq!(c.pkg, "alpha_pkg");
        assert_eq!(c.class, "alpha_pkg::Node");
        assert_eq!(c.name, "alpha");
        let native = entry
            .system
            .deploy
            .get("native")
            .expect("native deploy block synthesised");
        assert_eq!(native.kind.as_deref(), Some("self"));
        assert_eq!(native.board.as_deref(), Some("native_sim/native/64"));

        // Issue 0951 — the SAME row, also projected as an image. A synthesised
        // bringup is not authored anywhere, so nothing in the tree would show
        // this gap: a consumer switched to `[image.*]` would just read an empty
        // map and lose the value.
        let img = entry
            .system
            .image_for("native")
            .expect("native image projected");
        assert_eq!(img.board.as_deref(), Some("native_sim/native/64"));
        // Placement stays on the deploy side — an image has no `kind`.
        assert_eq!(
            entry.system.resolve_image(None).as_deref(),
            Some("native"),
            "the sole image resolves the target"
        );
    }

    #[test]
    fn rejects_unknown_rmw_on_synthesised_system() {
        // Phase 227.2 — a typo'd rmw in a deploy block surfaces at load time
        // (via the synthesised single-node system), not as a broken build.
        let dir = scratch_dir("rejects_unknown_rmw_on_synthesised_system");
        fs::write(
            dir.join("Cargo.toml"),
            "[workspace]\nresolver = \"2\"\nmembers = [\"alpha_pkg\"]\n",
        )
        .unwrap();
        fs::create_dir_all(dir.join("alpha_pkg/src")).unwrap();
        fs::write(
            dir.join("alpha_pkg/Cargo.toml"),
            r#"
[package]
name = "alpha_pkg"
version = "0.1.0"
edition = "2021"

[lib]
path = "src/lib.rs"

[package.metadata.nros.component]
class = "alpha_pkg::Node"
name = "alpha"

[package.metadata.nros.deploy.native]
rmw = "dust-dds"
"#,
        )
        .unwrap();
        fs::write(dir.join("alpha_pkg/src/lib.rs"), "").unwrap();

        let err = NrosConfig::from_cargo_metadata(&dir).expect_err("unknown rmw must fail load");
        match err {
            NrosConfigError::InvalidSystemRmw { package, source } => {
                assert_eq!(package, "alpha_pkg");
                assert_eq!(source.declared, "dust-dds");
            }
            other => panic!("expected InvalidSystemRmw, got {other:?}"),
        }
    }

    #[test]
    fn discovers_self_bringup_application_pkg() {
        let dir = scratch_dir("discovers_self_bringup_application_pkg");
        fs::write(
            dir.join("Cargo.toml"),
            r#"
[workspace]
resolver = "2"
members = ["demo_app"]
"#,
        )
        .unwrap();
        fs::create_dir_all(dir.join("demo_app/src")).unwrap();
        fs::write(
            dir.join("demo_app/Cargo.toml"),
            r#"
[package]
name = "demo_app"
version = "0.1.0"
edition = "2021"

[lib]
path = "src/lib.rs"

[package.metadata.nros.application]
name = "demo_app"
deploy = ["native"]

[package.metadata.nros.deploy.native]
board = "native_sim/native/64"
rmw = "zenoh"
domain_id = 0
"#,
        )
        .unwrap();
        fs::write(dir.join("demo_app/src/lib.rs"), "").unwrap();

        let cfg = NrosConfig::from_cargo_metadata(&dir).expect("loads");
        let entry = cfg
            .bringup_packages
            .get("demo_app")
            .expect("self-bringup application entry");
        assert_eq!(entry.source, BringupSource::SelfBringup);
        assert_eq!(entry.system.system.name, "demo_app");
        // Application self-bringup has no in-pkg components.
        assert!(entry.system.components.is_empty());
        assert!(entry.system.deploy.contains_key("native"));
    }

    /// When a real bringup pkg already names a component pkg via
    /// `pkg = "<name>"`, the component's deploy table does NOT cause a
    /// synthesised bringup (no double-counting).
    #[test]
    fn self_bringup_skipped_when_named_by_real_bringup() {
        let dir = scratch_dir("self_bringup_skipped_when_named_by_real_bringup");
        fs::write(
            dir.join("Cargo.toml"),
            r#"
[workspace]
resolver = "2"
members = ["alpha_pkg", "demo_bringup"]

[workspace.metadata.nros]
default_system = "demo_bringup"
"#,
        )
        .unwrap();
        fs::create_dir_all(dir.join("alpha_pkg/src")).unwrap();
        fs::write(
            dir.join("alpha_pkg/Cargo.toml"),
            r#"
[package]
name = "alpha_pkg"
version = "0.1.0"
edition = "2021"

[lib]
path = "src/lib.rs"

[package.metadata.nros.component]
class = "alpha_pkg::Node"
name = "alpha"

[package.metadata.nros.deploy.native]
rmw = "zenoh"
domain_id = 0
"#,
        )
        .unwrap();
        fs::write(dir.join("alpha_pkg/src/lib.rs"), "").unwrap();

        fs::create_dir_all(dir.join("demo_bringup/src")).unwrap();
        fs::write(
            dir.join("demo_bringup/Cargo.toml"),
            r#"
[package]
name = "demo_bringup"
version = "0.1.0"
edition = "2021"

[lib]
path = "src/lib.rs"
"#,
        )
        .unwrap();
        fs::write(dir.join("demo_bringup/src/lib.rs"), "").unwrap();
        fs::write(
            dir.join("demo_bringup/system.toml"),
            r#"
[system]
name = "demo"
rmw = "zenoh"
domain_id = 0

[[component]]
pkg = "alpha_pkg"
class = "alpha_pkg::Node"
name = "alpha"
"#,
        )
        .unwrap();

        let cfg = NrosConfig::from_cargo_metadata(&dir).expect("loads");
        // Only `demo_bringup` is a bringup; alpha_pkg is NOT auto-synthesised.
        assert!(cfg.bringup_packages.contains_key("demo_bringup"));
        assert!(
            !cfg.bringup_packages.contains_key("alpha_pkg"),
            "alpha_pkg should not be self-bringup'd when demo_bringup names it"
        );
        assert_eq!(
            cfg.bringup_packages.get("demo_bringup").unwrap().source,
            BringupSource::SystemToml
        );
    }

    /// 212.B.2 — bringup pkg's `system.toml` is loaded into the entry.
    #[test]
    fn bringup_pkg_loaded_from_system_toml() {
        let dir = scratch_dir("bringup_pkg_loaded_from_system_toml");
        write_minimal_workspace(&dir);

        let cfg = NrosConfig::from_cargo_metadata(&dir).expect("loads");

        let bringup = cfg
            .bringup_packages
            .get("demo_bringup")
            .expect("demo_bringup present");
        assert_eq!(bringup.system.system.name, "demo");
        assert_eq!(bringup.system.system.rmw, "zenoh");
        assert_eq!(bringup.system.system.domain_id, 0);
        assert_eq!(
            bringup.system.system.locator.as_deref(),
            Some("tcp/127.0.0.1:7447")
        );
        assert_eq!(bringup.system.components.len(), 2);
        assert_eq!(bringup.system.components[0].name, "talker");
        assert_eq!(bringup.system.components[1].name, "listener");
        let native = bringup
            .system
            .deploy
            .get("native")
            .expect("native deploy present");
        assert_eq!(native.kind.as_deref(), Some("self"));

        // The bringup pkg's ament block is preserved.
        assert_eq!(
            bringup.ament.exec_depend,
            vec!["talker_pkg", "listener_pkg"]
        );
        // And the system.toml path is recorded (callers regenerating
        // package.xml from system.toml need it).
        assert_eq!(
            bringup.system_toml_path,
            dir.join("demo_bringup/system.toml")
        );
    }

    // -----------------------------------------------------------------
    // Phase 212.N.12 — accept `[package.metadata.nros.node]` as the
    // canonical key (alias for the pre-N.12 `.component` key).
    // -----------------------------------------------------------------

    /// Write a tiny workspace whose single member uses the canonical
    /// `[package.metadata.nros.node]` key shape.
    fn write_node_key_workspace(dir: &Path) {
        fs::write(
            dir.join("Cargo.toml"),
            r#"
[workspace]
resolver = "2"
members = ["talker_pkg"]
"#,
        )
        .unwrap();
        fs::create_dir_all(dir.join("talker_pkg/src")).unwrap();
        fs::write(
            dir.join("talker_pkg/Cargo.toml"),
            r#"
[package]
name = "talker_pkg"
version = "0.1.0"
edition = "2021"

[lib]
path = "src/lib.rs"

[package.metadata.nros.node]
class = "talker_pkg::TalkerNode"
name = "talker"
default_namespace = "/demo"

[package.metadata.nros.node.parameters]
rate_hz = 10
"#,
        )
        .unwrap();
        fs::write(dir.join("talker_pkg/src/lib.rs"), "").unwrap();
    }

    /// Canonical `[package.metadata.nros.node]` key loads through the
    /// loader and surfaces on `ComponentPackageEntry::nros.component`
    /// (renamed-from alias internally).
    #[test]
    fn loads_node_key_as_canonical() {
        let dir = scratch_dir("loads_node_key_as_canonical");
        write_node_key_workspace(&dir);

        let cfg = NrosConfig::from_cargo_metadata(&dir).expect("loads");
        let talker = cfg
            .component_packages
            .get("talker_pkg")
            .expect("talker present");
        let node = talker
            .nros
            .component
            .as_ref()
            .expect("`.node` key landed on the canonical struct field");
        assert_eq!(node.class.as_deref(), Some("talker_pkg::TalkerNode"));
        assert_eq!(node.name.as_deref(), Some("talker"));
        assert_eq!(node.default_namespace.as_deref(), Some("/demo"));
        assert_eq!(
            node.parameters.get("rate_hz").map(|v| v.as_integer()),
            Some(Some(10))
        );
    }

    /// Both keys present → hard error (ambiguous; user must pick one).
    #[test]
    fn rejects_both_node_and_component_keys() {
        let dir = scratch_dir("rejects_both_node_and_component_keys");
        fs::write(
            dir.join("Cargo.toml"),
            r#"
[workspace]
resolver = "2"
members = ["ambig_pkg"]
"#,
        )
        .unwrap();
        fs::create_dir_all(dir.join("ambig_pkg/src")).unwrap();
        fs::write(
            dir.join("ambig_pkg/Cargo.toml"),
            r#"
[package]
name = "ambig_pkg"
version = "0.1.0"
edition = "2021"

[lib]
path = "src/lib.rs"

[package.metadata.nros.node]
class = "ambig_pkg::A"
name = "a"

[package.metadata.nros.component]
class = "ambig_pkg::B"
name = "b"
"#,
        )
        .unwrap();
        fs::write(dir.join("ambig_pkg/src/lib.rs"), "").unwrap();

        let err = NrosConfig::from_cargo_metadata(&dir).expect_err("both keys must be rejected");
        let msg = err.to_string();
        assert!(
            msg.contains("node") && msg.contains("component"),
            "diagnostic mentions both keys: {msg}"
        );
    }

    /// Deprecated `.component` key still parses (back-compat).
    #[test]
    fn loads_deprecated_component_key() {
        let dir = scratch_dir("loads_deprecated_component_key");
        fs::write(
            dir.join("Cargo.toml"),
            r#"
[workspace]
resolver = "2"
members = ["legacy_pkg"]
"#,
        )
        .unwrap();
        fs::create_dir_all(dir.join("legacy_pkg/src")).unwrap();
        fs::write(
            dir.join("legacy_pkg/Cargo.toml"),
            r#"
[package]
name = "legacy_pkg"
version = "0.1.0"
edition = "2021"

[lib]
path = "src/lib.rs"

[package.metadata.nros.component]
class = "legacy_pkg::Node"
name = "legacy"
"#,
        )
        .unwrap();
        fs::write(dir.join("legacy_pkg/src/lib.rs"), "").unwrap();

        let cfg = NrosConfig::from_cargo_metadata(&dir).expect("loads");
        let legacy = cfg
            .component_packages
            .get("legacy_pkg")
            .expect("legacy present");
        let c = legacy
            .nros
            .component
            .as_ref()
            .expect("component table present");
        assert_eq!(c.class.as_deref(), Some("legacy_pkg::Node"));
    }
}
