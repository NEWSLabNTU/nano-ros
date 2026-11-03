//! Declared overrides of provider packages (RFC-0119 D4, phase-527 W2b).
//!
//! The search path has three tiers (`provider_scan::build_search_path`): the
//! nano-ros tree is the underlay, the workspace and its `package_paths`
//! entries are SOURCE roots, and `NROS_PACKAGE_PATH` is the overlay. Between
//! tiers a LATER root wins ([`crate::provider_scan::resolve_unique`]), which is
//! colcon's overlay rule; among source roots the same provider twice is
//! refused, which is colcon's "duplicate package names not supported" among
//! base paths. This module applies both, and decides whether a win over a
//! lower tier is ALLOWED, and how loud it is.
//!
//! colcon builds an overriding package and prints a warning, and only when an
//! underlay package depends on the overridden one; a leaf override is silent,
//! and `--allow-overriding <pkg>` silences the warning by name. That is the
//! wrong default for the two kinds that run host code or flash hardware, so:
//!
//! - **`runner` and `command`** (the host kinds): an override nobody declared
//!   is REFUSED when the name is resolved. A plain scan or listing never
//!   refuses, so a stray copy nobody selects never gets in the way.
//! - **every other kind**: an undeclared override WARNS, as colcon does.
//! - **a declaration is PATH-pinned** — `qemu = "src/my_qemu"` under
//!   `nros.toml [workspace.overrides]` (a single-package leaf with no
//!   `nros.toml`: its root `system.toml [overrides]`). Another package that
//!   later claims `qemu` does not inherit the approval.
//! - **the same provider in two SOURCE roots** (the workspace and a
//!   `package_paths` entry, or two entries) is REFUSED for every kind, unless
//!   a declaration pins one of them — the pinned one wins wherever it sits —
//!   or `--allow-overriding <name>` lets the later root win.
//! - **`--allow-overriding <name>`** approves by name for one invocation.
//! - **a declaration that names a path no longer announcing the name** is an
//!   error at resolution, never a silent fall back to the underlay; one that
//!   overrides nothing is a warning.
//!
//! The protocol check (an overriding runner or command speaks the protocol of
//! the one it shadows) reads the descriptors, so its body lives with them in
//! `extension_descriptor`; [`resolve_provider`] calls it.
//!
//! # The one entry point
//!
//! ```text
//! cargo_nano_ros::provider_overrides::resolve_provider(
//!     scan: &ScanResult, kind: &str, name: &str, policy: &OverridePolicy,
//! ) -> Result<PolicyResolution, PolicyResolveError>
//! ```
//!
//! Every caller that resolves a provider by kind and name goes through it, so
//! precedence, `[overrides]`, `--allow-overriding`, the protocol check and the
//! stale/unused rules cannot fork. Build the policy with
//! `nros_cli_core::provider_overrides::policy(workspace, &allow_overriding)`
//! (it reads the declarations from the workspace's `nros.toml`), scan with
//! `provider_scan::scan_search_path(&nros_cli_core::cmd::ws::provider_search_path(ws)?)`
//! (the root origins are what tell a source root from the underlay),
//! print `warnings` once on stderr, and keep `overriding` for `--why`. A
//! runner or command caller that also needs the descriptor and executable
//! uses `extension_descriptor::resolve_runner_with` / `resolve_command_with`,
//! which are this call plus the descriptor read.
//!
//! The policy is pure: it takes the declared table and the flag as data, so
//! the readers of `nros.toml` and `system.toml` stay in the crate that owns
//! those schemas.

use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    path::{Path, PathBuf},
};

use crate::{
    package_xml::HOST_KINDS,
    provider_scan::{
        ProviderPackage, Resolution, ResolveError, RootOrigin, ScanResult, candidates,
        resolve_unique, shadowing,
    },
};

/// Where a workspace's override declarations live.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OverridesFile {
    /// `nros.toml [workspace.overrides]`, beside `package_paths`. Workspace
    /// configuration, so no package can grant itself an override.
    #[default]
    NrosToml,
    /// The root `system.toml [overrides]` of a single-package leaf that has
    /// no `nros.toml`.
    LeafSystemToml,
}

impl OverridesFile {
    /// The file and table, as a user would look for them.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            OverridesFile::NrosToml => "nros.toml [workspace.overrides]",
            OverridesFile::LeafSystemToml => "system.toml [overrides]",
        }
    }

    /// What to paste to declare `name = "shown"`: the file, then the table
    /// header and the entry.
    #[must_use]
    pub fn paste(self, name: &str, shown: &str) -> String {
        match self {
            OverridesFile::NrosToml => {
                format!("nros.toml:   [workspace.overrides] {name} = \"{shown}\"")
            }
            OverridesFile::LeafSystemToml => {
                format!("system.toml:   [overrides] {name} = \"{shown}\"")
            }
        }
    }
}

/// One `[overrides]` entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Declared {
    /// The value as written, workspace-relative. Quoted back in messages,
    /// because the fix for a stale entry is an edit to this string.
    pub as_written: String,
    /// The package directory it pins, absolute (canonical when it exists).
    pub path: PathBuf,
}

/// What a workspace allows to override an underlay provider.
#[derive(Debug, Clone, Default)]
pub struct OverridePolicy {
    /// The workspace root the declarations are relative to. Also the base for
    /// the paths printed in messages.
    pub workspace: PathBuf,
    /// `[overrides]`, by provider name.
    pub declared: BTreeMap<String, Declared>,
    /// `--allow-overriding <name>…`, for this invocation only.
    pub allowed: BTreeSet<String>,
    /// Which file the declarations are (or would be) written in. Messages
    /// quote it, so the fix they print lands in the file that is read.
    pub file: OverridesFile,
    /// The file the declarations were read from, when one was.
    pub declared_in: Option<PathBuf>,
}

impl OverridePolicy {
    /// A policy from the declared table (`name -> workspace-relative path`)
    /// and the names approved on the command line.
    #[must_use]
    pub fn new(workspace: &Path, declared: &BTreeMap<String, String>, allowed: &[String]) -> Self {
        let declared = declared
            .iter()
            .map(|(name, written)| {
                let raw = Path::new(written);
                let abs = if raw.is_absolute() {
                    raw.to_path_buf()
                } else {
                    workspace.join(raw)
                };
                (
                    name.clone(),
                    Declared {
                        as_written: written.clone(),
                        path: canonical(&abs),
                    },
                )
            })
            .collect();
        Self {
            workspace: workspace.to_path_buf(),
            declared,
            allowed: allowed.iter().cloned().collect(),
            file: OverridesFile::default(),
            declared_in: None,
        }
    }

    /// Record where the declarations were read from.
    #[must_use]
    pub fn read_from(mut self, file: OverridesFile, path: Option<PathBuf>) -> Self {
        self.file = file;
        self.declared_in = path;
        self
    }

    /// `path` as a user would write it in `[overrides]`: relative to the
    /// workspace when it is inside it, absolute otherwise.
    #[must_use]
    pub fn shown(&self, path: &Path) -> String {
        let path = canonical(path);
        let ws = canonical(&self.workspace);
        match path.strip_prefix(&ws) {
            Ok(rel) if !rel.as_os_str().is_empty() => rel.display().to_string(),
            _ => path.display().to_string(),
        }
    }
}

fn canonical(p: &Path) -> PathBuf {
    p.canonicalize().unwrap_or_else(|_| p.to_path_buf())
}

fn same_dir(a: &Path, b: &Path) -> bool {
    canonical(a) == canonical(b)
}

/// Does an undeclared override of this kind refuse (rather than warn)?
///
/// The host kinds: a runner launches emulators and flashes hardware, and a
/// command is a verb of `nros` itself, so which package answers must never be
/// decided silently by the search order.
#[must_use]
pub fn refuses_undeclared(kind: &str) -> bool {
    HOST_KINDS.contains(&kind)
}

/// How an override was approved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverrideStatus {
    /// The workspace's override declarations name the winner's path.
    Declared,
    /// `--allow-overriding <name>` on this command line.
    AllowedOnce,
    /// Nobody approved it. Refused for the host kinds, warned otherwise.
    Undeclared,
}

impl OverrideStatus {
    /// How the status is spelled in listings.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            OverrideStatus::Declared => "declared",
            OverrideStatus::AllowedOnce => "allowed by --allow-overriding",
            OverrideStatus::Undeclared => "UNDECLARED",
        }
    }
}

/// One provider that replaces a lower-precedence one.
#[derive(Debug, Clone)]
pub struct Override {
    pub kind: String,
    pub name: String,
    /// The package that wins.
    pub winner: ProviderPackage,
    /// What it hides, nearest first.
    pub shadowed: Vec<ProviderPackage>,
    pub status: OverrideStatus,
}

/// A resolution the policy refuses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OverrideError {
    /// A host-kind provider replaces another and nobody declared it.
    Undeclared {
        kind: String,
        name: String,
        /// The winner, as it would be written in `[overrides]`.
        shown: String,
        /// The package it hides.
        shadowed_package: String,
        shadowed_dir: PathBuf,
        /// Where a declaration would go.
        file: OverridesFile,
    },
    /// A declaration `<name> = "<path>"` names a path that announces no `<name>`.
    Stale {
        name: String,
        as_written: String,
        /// What is at the path instead.
        found: String,
        file: OverridesFile,
    },
    /// Two SOURCE roots of the workspace provide `(kind, name)`, and neither a
    /// declaration nor `--allow-overriding` picks one.
    SourceDuplicate {
        kind: String,
        name: String,
        /// Each provider, as a declaration would name it, with the origin of
        /// its root; highest precedence first.
        candidates: Vec<(String, &'static str)>,
        file: OverridesFile,
    },
    /// An overriding runner or command speaks no protocol version the
    /// package it shadows speaks.
    Protocol(Box<ProtocolConflict>),
}

/// The two sides of [`OverrideError::Protocol`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProtocolConflict {
    pub kind: String,
    pub name: String,
    /// The overriding package, as written in `[overrides]`.
    pub shown: String,
    /// What it speaks.
    pub theirs: Vec<u32>,
    /// The package it replaces.
    pub shadowed_dir: PathBuf,
    /// What that one speaks.
    pub shadowed: Vec<u32>,
}

fn versions(v: &[u32]) -> String {
    if v.is_empty() {
        return "no version".to_string();
    }
    v.iter().map(u32::to_string).collect::<Vec<_>>().join(", ")
}

impl fmt::Display for OverrideError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OverrideError::Undeclared {
                kind,
                name,
                shown,
                shadowed_package,
                shadowed_dir,
                file,
            } => write!(
                f,
                "{shown} announces {kind} `{name}`, which {shadowed_package} ({}) also \
                 provides.\n  To use yours, add to {}\n  or for this run only:   \
                 --allow-overriding {name}",
                shadowed_dir.display(),
                file.paste(name, shown)
            ),
            OverrideError::Stale {
                name,
                as_written,
                found,
                file,
            } => write!(
                f,
                "{} {name} = \"{as_written}\", but {found}. Point the entry at the \
                 package that announces `{name}`, or remove it to use the underlay's.",
                file.label()
            ),
            OverrideError::SourceDuplicate {
                kind,
                name,
                candidates,
                file,
            } => {
                write!(
                    f,
                    "{kind} `{name}` is provided by more than one source tree of this \
                     workspace:"
                )?;
                for (shown, origin) in candidates {
                    write!(f, "\n  {shown}  ({origin})")?;
                }
                write!(
                    f,
                    "\n  A workspace builds one package per name. Pick one in {}\n  \
                     or for this run only (the later root wins):   --allow-overriding {name}",
                    file.paste(name, &candidates[0].0)
                )
            }
            OverrideError::Protocol(c) => write!(
                f,
                "{} overrides {} `{}` but speaks protocol {}, and the package it \
                 replaces ({}) speaks {}. An override must speak the same protocol as the \
                 package it replaces.",
                c.shown,
                c.kind,
                c.name,
                versions(&c.theirs),
                c.shadowed_dir.display(),
                versions(&c.shadowed)
            ),
        }
    }
}

impl std::error::Error for OverrideError {}

/// A by-name lookup under the policy.
#[derive(Debug, Clone)]
pub struct PolicyResolution {
    pub resolution: Resolution,
    /// Set when the winner replaces another provider and the policy allowed it.
    pub overriding: Option<Override>,
    /// To print once, on stderr: an undeclared override of a non-host kind, or
    /// a declaration that overrides nothing.
    pub warnings: Vec<String>,
}

/// [`resolve_provider`] failed.
#[derive(Debug)]
pub enum PolicyResolveError {
    /// No provider, or an ambiguous one.
    Scan(ResolveError),
    /// The policy refuses the winner.
    Override(Box<OverrideError>),
}

impl fmt::Display for PolicyResolveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PolicyResolveError::Scan(e) => write!(f, "{e}"),
            PolicyResolveError::Override(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for PolicyResolveError {}

/// What `[overrides] <name>` means for one kind.
enum DeclFor<'a> {
    /// No entry for the name.
    None,
    /// The entry's path announces `(kind, name)`.
    ThisKind(&'a Declared),
    /// The entry's path announces `name` under another kind only, so the
    /// entry is about that kind and says nothing about this one.
    OtherKind,
}

fn decl_for<'a>(
    scan: &ScanResult,
    kind: &str,
    name: &str,
    policy: &'a OverridePolicy,
) -> Result<DeclFor<'a>, OverrideError> {
    let Some(d) = policy.declared.get(name) else {
        return Ok(DeclFor::None);
    };
    let at = scan.providers.iter().find(|p| same_dir(&p.dir, &d.path));
    let stale = |found: String| OverrideError::Stale {
        name: name.to_string(),
        as_written: d.as_written.clone(),
        found,
        file: policy.file,
    };
    let Some(p) = at else {
        let found = if d.path.join("package.xml").is_file() {
            format!("the package at {} announces no provider", d.path.display())
        } else {
            format!("there is no package at {}", d.path.display())
        };
        return Err(stale(found));
    };
    if p.provides
        .iter()
        .any(|pr| pr.kind == kind && pr.name == name)
    {
        return Ok(DeclFor::ThisKind(d));
    }
    if p.provides.iter().any(|pr| pr.name == name) {
        return Ok(DeclFor::OtherKind);
    }
    let announced: Vec<String> = p
        .provides
        .iter()
        .map(|pr| format!("{} `{}`", pr.kind, pr.name))
        .collect();
    Err(stale(format!(
        "{} ({}) announces {}",
        p.package,
        p.dir.display(),
        announced.join(", ")
    )))
}

/// Classify an already-resolved `(kind, name)` under the policy, without
/// refusing anything: the shared core of [`resolve_provider`] and [`report`].
fn classify(
    scan: &ScanResult,
    kind: &str,
    name: &str,
    resolution: &Resolution,
    policy: &OverridePolicy,
) -> Result<(Option<Override>, Vec<String>), OverrideError> {
    let decl = decl_for(scan, kind, name, policy)?;
    let winner = &resolution.winner;
    let declared_winner = matches!(decl, DeclFor::ThisKind(d) if same_dir(&winner.dir, &d.path));
    let mut warnings = Vec::new();
    if resolution.shadowed.is_empty() {
        if let (true, DeclFor::ThisKind(d)) = (declared_winner, &decl) {
            warnings.push(format!(
                "{} {name} = \"{}\" overrides nothing: no other package provides {kind} \
                 `{name}`. Remove the entry.",
                policy.file.label(),
                d.as_written
            ));
        }
        return Ok((None, warnings));
    }
    let status = if declared_winner {
        OverrideStatus::Declared
    } else if policy.allowed.contains(name) {
        OverrideStatus::AllowedOnce
    } else {
        OverrideStatus::Undeclared
    };
    Ok((
        Some(Override {
            kind: kind.to_string(),
            name: name.to_string(),
            winner: winner.clone(),
            shadowed: resolution.shadowed.clone(),
            status,
        }),
        warnings,
    ))
}

fn is_source(scan: &ScanResult, p: &ProviderPackage) -> bool {
    scan.origin(p.root_index).is_some_and(RootOrigin::is_source)
}

/// The precedence answer for `(kind, name)` with the source-root duplicate
/// rule applied: [`resolve_unique`]'s order, except that two SOURCE roots
/// providing the name must be settled by a declaration (the pinned package
/// wins wherever it sits) or by `--allow-overriding` (the later root wins).
/// The environment overlay still beats every source root.
fn pick(
    scan: &ScanResult,
    kind: &str,
    name: &str,
    policy: &OverridePolicy,
) -> Result<Resolution, PolicyResolveError> {
    let resolution = resolve_unique(scan, kind, name).map_err(PolicyResolveError::Scan)?;
    let hits = candidates(scan, kind, name);
    let source: Vec<&ProviderPackage> = hits
        .iter()
        .copied()
        .filter(|p| is_source(scan, p))
        .collect();
    let roots: BTreeSet<usize> = source.iter().map(|p| p.root_index).collect();
    if roots.len() < 2 {
        return Ok(resolution);
    }
    // A declaration whose path announces nothing is a stale entry, reported
    // as such rather than as the duplicate it failed to settle.
    decl_for(scan, kind, name, policy).map_err(|e| PolicyResolveError::Override(Box::new(e)))?;
    let pinned = policy
        .declared
        .get(name)
        .and_then(|d| source.iter().copied().find(|p| same_dir(&p.dir, &d.path)));
    let chosen = match pinned {
        Some(p) => p,
        None if policy.allowed.contains(name) => source[0],
        None => {
            return Err(PolicyResolveError::Override(Box::new(
                OverrideError::SourceDuplicate {
                    kind: kind.to_string(),
                    name: name.to_string(),
                    candidates: source
                        .iter()
                        .map(|p| {
                            (
                                policy.shown(&p.dir),
                                scan.origin(p.root_index).map_or("", RootOrigin::label),
                            )
                        })
                        .collect(),
                    file: policy.file,
                },
            )));
        }
    };
    if !is_source(scan, &resolution.winner) || same_dir(&resolution.winner.dir, &chosen.dir) {
        return Ok(resolution);
    }
    Ok(Resolution {
        winner: chosen.clone(),
        shadowed: hits
            .iter()
            .filter(|p| !same_dir(&p.dir, &chosen.dir))
            .map(|p| (*p).clone())
            .collect(),
    })
}

/// **The one provider lookup.** Resolve `(kind, name)` on the scan and apply
/// every rule of RFC-0119 D4: precedence (a later root wins between tiers),
/// the source-root duplicate refusal, the declaration (path-pinned),
/// `--allow-overriding`, the
/// protocol check for an overriding runner or command, and the stale and
/// unused rules. Every resolution calls this — the build edge
/// (`nros_cli_core::cmd::build::runner_edges`), `run`/`flash` runner lookup,
/// command dispatch, the rmw dispatch — so the lookup never forks. A listing
/// calls [`report`] instead, and so never refuses.
///
/// Callers print [`PolicyResolution::warnings`] once, on stderr, and carry
/// [`PolicyResolution::overriding`] into `--dry-run --why`.
///
/// # Errors
///
/// [`PolicyResolveError::Scan`] when nothing or something ambiguous answers
/// the name; [`PolicyResolveError::Override`] for a duplicate across source
/// roots nobody settled, an undeclared override of a host kind, a stale
/// declaration, or a protocol mismatch.
pub fn resolve_provider(
    scan: &ScanResult,
    kind: &str,
    name: &str,
    policy: &OverridePolicy,
) -> Result<PolicyResolution, PolicyResolveError> {
    let resolution = pick(scan, kind, name, policy)?;
    let (overriding, mut warnings) = classify(scan, kind, name, &resolution, policy)
        .map_err(|e| PolicyResolveError::Override(Box::new(e)))?;
    if let Some(o) = &overriding
        && o.status == OverrideStatus::Undeclared
    {
        let shown = policy.shown(&o.winner.dir);
        let hidden = &o.shadowed[0];
        if refuses_undeclared(kind) {
            return Err(PolicyResolveError::Override(Box::new(
                OverrideError::Undeclared {
                    kind: kind.to_string(),
                    name: name.to_string(),
                    shown,
                    shadowed_package: hidden.package.clone(),
                    shadowed_dir: hidden.dir.clone(),
                    file: policy.file,
                },
            )));
        }
        warnings.push(format!(
            "{shown} announces {kind} `{name}`, which {} ({}) also provides; yours is used. \
             To say so and silence this, add to {}",
            hidden.package,
            hidden.dir.display(),
            policy.file.paste(name, &shown)
        ));
    }
    if let Some(o) = &overriding
        && refuses_undeclared(kind)
    {
        crate::extension_descriptor::check_override_protocol(o, policy)
            .map_err(|e| PolicyResolveError::Override(Box::new(e)))?;
    }
    Ok(PolicyResolution {
        resolution,
        overriding,
        warnings,
    })
}

/// One line of the override report.
#[derive(Debug, Clone)]
pub enum ReportRow {
    /// A provider that replaces another, with how it was approved.
    Override(Override),
    /// A declaration whose path does not announce its name.
    Stale(OverrideError),
    /// A declaration that replaces nothing.
    Unused { name: String, as_written: String },
    /// Two source roots provide one name and nothing settles it: every
    /// resolution of the name refuses ([`OverrideError::SourceDuplicate`]).
    Duplicate(OverrideError),
}

impl ReportRow {
    /// Is this row a fault a resolution of its name would refuse?
    #[must_use]
    pub fn is_problem(&self) -> bool {
        matches!(self, ReportRow::Stale(_) | ReportRow::Duplicate(_))
    }
}

/// Every override in a scan, and every declaration that is stale or unused:
/// what `nros dev ws providers` and `nros doctor` list. Refuses nothing.
#[must_use]
pub fn report(scan: &ScanResult, policy: &OverridePolicy) -> Vec<ReportRow> {
    let mut rows = Vec::new();
    for s in shadowing(scan).into_iter().filter(|s| !s.same_root_tie) {
        let resolution = match pick(scan, &s.kind, &s.name, policy) {
            Ok(r) => r,
            Err(PolicyResolveError::Override(e))
                if matches!(*e, OverrideError::SourceDuplicate { .. }) =>
            {
                rows.push(ReportRow::Duplicate(*e));
                continue;
            }
            // Stale: reported once below, by name.
            Err(_) => continue,
        };
        match classify(scan, &s.kind, &s.name, &resolution, policy) {
            Ok((Some(o), _)) => rows.push(ReportRow::Override(o)),
            Ok((None, _)) => {}
            // Reported once below, by name, rather than once per kind.
            Err(_) => {}
        }
    }
    for (name, d) in &policy.declared {
        let at = scan.providers.iter().find(|p| same_dir(&p.dir, &d.path));
        let kinds: Vec<&str> = at
            .map(|p| {
                p.provides
                    .iter()
                    .filter(|pr| &pr.name == name)
                    .map(|pr| pr.kind.as_str())
                    .collect()
            })
            .unwrap_or_default();
        if kinds.is_empty() {
            // Nothing there announces the name under any kind: the same
            // refusal a resolution of it would raise.
            if let Err(e) = decl_for(scan, "", name, policy) {
                rows.push(ReportRow::Stale(e));
            }
            continue;
        }
        let used = kinds.iter().any(|k| {
            rows.iter().any(|r| {
                matches!(r, ReportRow::Override(o) if o.kind == *k && &o.name == name
                    && same_dir(&o.winner.dir, &d.path))
            })
        });
        if !used {
            rows.push(ReportRow::Unused {
                name: name.clone(),
                as_written: d.as_written.clone(),
            });
        }
    }
    rows
}
