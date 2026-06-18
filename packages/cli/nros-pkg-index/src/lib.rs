//! Phase 212.N.10 — workspace pkg-index + `$(find <pkg>)` resolver.
//!
//! Language-agnostic build-time mechanism shared by the Rust proc-macro
//! family (`nros::main!`, `nros::launch!`) and the future C++ cmake fn
//! `nros_entry(...)`. Per design-doc §11.4:
//!
//! 1. **Workspace root detection** — walk up from a starting dir
//!    (`CARGO_MANIFEST_DIR` / `CMAKE_SOURCE_DIR`) looking for, in
//!    order:
//!    1. `NROS_WORKSPACE_ROOT` env var (explicit override).
//!    2. `.colcon_workspace` marker.
//!    3. `Cargo.toml` containing `[workspace]`.
//!    4. `.git/` (last-resort fallback).
//! 2. **Pkg-index build** — recurse from workspace root, collect every
//!    `package.xml`. Pkg name = `<name>` element; pkg dir = parent dir.
//! 3. **Cache** — emit `$OUT_DIR/.nros-pkg-index.json` keyed on combined
//!    `package.xml` mtimes (build-script callers only; CLI callers
//!    skip when `$OUT_DIR` is unset).
//!
//! The substitution resolver (`resolve_find_substitution`) feeds the
//! N.11 launch.xml parser's `$(find <pkg>)/path/rest` substitution.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::BufReader,
    path::{Path, PathBuf},
    time::SystemTime,
};

use eyre::{Context, Result, bail, eyre};
use quick_xml::{events::Event, reader::Reader};
use serde::{Deserialize, Serialize};
use walkdir::WalkDir;

/// Workspace-root markers consulted by [`detect_workspace_root`]. The
/// `NROS_WORKSPACE_ROOT` env var short-circuits the walk entirely.
const COLCON_WORKSPACE_MARKER: &str = ".colcon_workspace";
const COLCON_IGNORE_MARKER: &str = "COLCON_IGNORE";
const NROS_IGNORE_MARKER: &str = ".nros-ignore";

/// Directory names skipped during the recursive `package.xml` walk —
/// build / cache artefacts and source-control internals.
const SKIP_DIRS: &[&str] = &[
    "target",
    "build",
    ".git",
    ".cargo",
    "node_modules",
    "__pycache__",
];

/// In-memory pkg-name → pkg-source-dir index.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PkgIndex {
    workspace_root: PathBuf,
    pkgs: BTreeMap<String, PathBuf>,
}

impl PkgIndex {
    /// Workspace root the index was built from.
    pub fn workspace_root(&self) -> &Path {
        &self.workspace_root
    }

    /// Look up a package directory by name. Errors with a diagnostic
    /// listing the known packages when `name` is unknown — keeps the
    /// "did you mean" surface within reach of higher-level callers.
    pub fn resolve_pkg(&self, name: &str) -> Result<&Path> {
        match self.pkgs.get(name) {
            Some(dir) => Ok(dir.as_path()),
            None => {
                let mut known: Vec<&str> = self.pkgs.keys().map(String::as_str).collect();
                known.sort_unstable();
                bail!(
                    "pkg `{name}` not found in workspace `{}`. Known pkgs: [{}]",
                    self.workspace_root.display(),
                    known.join(", "),
                )
            }
        }
    }

    /// Parse a `$(find <pkg>)/optional/rest/of/path` expression and
    /// return the absolute resolved path as a String. Errors when the
    /// expression does not begin with `$(find <pkg>)` or when `<pkg>`
    /// is unknown.
    ///
    /// Whitespace inside `$(find … )` is tolerated (leading + trailing
    /// trimmed). The trailing `/rest` may be absent.
    pub fn resolve_find_substitution(&self, expr: &str) -> Result<String> {
        let trimmed = expr.trim_start();
        let rest = trimmed
            .strip_prefix("$(find")
            .ok_or_else(|| eyre!("expected `$(find <pkg>)` prefix, got: {expr:?}"))?;
        let after_find = rest.strip_prefix(' ').or_else(|| rest.strip_prefix('\t'));
        let after_find = match after_find {
            Some(s) => s,
            // Empty `$(find)` w/o a space between `find` and `<pkg>`.
            None => bail!("missing pkg name after `$(find`: {expr:?}"),
        };
        let close = after_find
            .find(')')
            .ok_or_else(|| eyre!("unterminated `$(find …)` substitution: {expr:?}"))?;
        let pkg_name = after_find[..close].trim();
        if pkg_name.is_empty() {
            bail!("empty pkg name inside `$(find )`: {expr:?}");
        }
        let after_close = &after_find[close + 1..];
        let pkg_dir = self.resolve_pkg(pkg_name)?;
        let joined = if after_close.is_empty() {
            pkg_dir.to_path_buf()
        } else {
            // Strip a leading separator so PathBuf::join doesn't treat the
            // rest as absolute.
            let tail = after_close.trim_start_matches('/');
            if tail.is_empty() {
                pkg_dir.to_path_buf()
            } else {
                pkg_dir.join(tail)
            }
        };
        Ok(joined.to_string_lossy().into_owned())
    }

    /// Iterate (pkg-name, pkg-dir) pairs in sorted-by-name order.
    pub fn pkgs(&self) -> impl Iterator<Item = (&str, &Path)> {
        self.pkgs.iter().map(|(k, v)| (k.as_str(), v.as_path()))
    }
}

/// On-disk shape of `$OUT_DIR/.nros-pkg-index.json`. The mtime stamps
/// invalidate the cache as soon as any contributing `package.xml`
/// changes — keyed on the absolute path so multiple workspaces caching
/// into a shared OUT_DIR don't collide.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
struct CachedIndex {
    workspace_root: PathBuf,
    entries: BTreeMap<String, CachedEntry>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
struct CachedEntry {
    dir: PathBuf,
    /// Modified time as `(secs, nanos)` since the unix epoch. `0` when
    /// the underlying filesystem doesn't surface a modified time.
    mtime: (u64, u32),
}

/// Build the workspace pkg-index by recursive `package.xml` walk.
///
/// Writes the result to `$OUT_DIR/.nros-pkg-index.json` when `$OUT_DIR`
/// is set (build-script context). CLI callers without `$OUT_DIR`
/// rebuild the index every call — fast enough for hundreds of pkgs.
pub fn build_pkg_index(workspace_root: &Path) -> Result<PkgIndex> {
    let workspace_root = workspace_root
        .canonicalize()
        .with_context(|| format!("canonicalize workspace root `{}`", workspace_root.display()))?;

    let mut package_xml_paths: Vec<PathBuf> = Vec::new();
    let walker = WalkDir::new(&workspace_root)
        .follow_links(false)
        .into_iter();
    let filter = |entry: &walkdir::DirEntry| -> bool {
        if entry.depth() == 0 {
            return true;
        }
        let file_name = entry.file_name().to_string_lossy();
        if entry.file_type().is_dir() {
            // Skip well-known build / cache / vcs dirs.
            if SKIP_DIRS.iter().any(|name| file_name == *name) {
                return false;
            }
            if file_name.starts_with("build-") {
                return false;
            }
            // Ament convention: a dir carrying `COLCON_IGNORE` or
            // `.nros-ignore` is opaque to discovery.
            let path = entry.path();
            if path.join(COLCON_IGNORE_MARKER).exists() || path.join(NROS_IGNORE_MARKER).exists() {
                return false;
            }
        }
        true
    };

    for entry in walker.filter_entry(filter) {
        let entry =
            entry.with_context(|| format!("walk workspace root `{}`", workspace_root.display()))?;
        if !entry.file_type().is_file() {
            continue;
        }
        if entry.file_name() == "package.xml" {
            package_xml_paths.push(entry.into_path());
        }
    }
    // Deterministic iteration order so the cache is stable.
    package_xml_paths.sort();

    // Try the OUT_DIR cache first.
    let cache_path = std::env::var_os("OUT_DIR")
        .map(PathBuf::from)
        .map(|d| d.join(".nros-pkg-index.json"));
    if let Some(cache_path) = cache_path.as_ref()
        && let Some(cached) = load_cache_if_fresh(cache_path, &workspace_root, &package_xml_paths)?
    {
        return Ok(cached);
    }

    // Build fresh.
    let mut pkgs: BTreeMap<String, PathBuf> = BTreeMap::new();
    let mut seen_dirs: BTreeSet<PathBuf> = BTreeSet::new();
    for pkg_xml in &package_xml_paths {
        let name = read_package_xml_name(pkg_xml)
            .with_context(|| format!("parse `{}`", pkg_xml.display()))?;
        let dir = pkg_xml
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| workspace_root.clone());
        if !seen_dirs.insert(dir.clone()) {
            // Two `package.xml` in the same dir — shouldn't happen, but
            // be defensive.
            continue;
        }
        if let Some(prev) = pkgs.get(&name) {
            bail!(
                "duplicate pkg name `{name}` in workspace `{}`: `{}` and `{}`",
                workspace_root.display(),
                prev.display(),
                dir.display(),
            );
        }
        pkgs.insert(name, dir);
    }

    let index = PkgIndex {
        workspace_root: workspace_root.clone(),
        pkgs,
    };

    // Persist the cache if we have an OUT_DIR.
    if let Some(cache_path) = cache_path {
        let cached = CachedIndex {
            workspace_root: workspace_root.clone(),
            entries: index
                .pkgs
                .iter()
                .map(|(name, dir)| {
                    let pkg_xml = dir.join("package.xml");
                    let mtime = read_mtime(&pkg_xml).unwrap_or((0, 0));
                    (
                        name.clone(),
                        CachedEntry {
                            dir: dir.clone(),
                            mtime,
                        },
                    )
                })
                .collect(),
        };
        if let Some(parent) = cache_path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let json = serde_json::to_string_pretty(&cached).context("serialise pkg-index cache")?;
        let _ = fs::write(&cache_path, json);
    }

    Ok(index)
}

/// Walk up from `start` looking for a workspace root. Per design-doc §11.4
/// the marker order is:
/// 1. `NROS_WORKSPACE_ROOT` env var.
/// 2. `.colcon_workspace` marker in any ancestor.
/// 3. `Cargo.toml` containing `[workspace]`.
/// 4. `.git/` directory.
///
/// `COLCON_IGNORE` ancestors do NOT terminate the walk (they merely
/// shadow themselves from discovery — handled inside [`build_pkg_index`]).
pub fn detect_workspace_root(start: &Path) -> Result<PathBuf> {
    if let Some(override_) = std::env::var_os("NROS_WORKSPACE_ROOT") {
        let p = PathBuf::from(override_);
        if !p.exists() {
            bail!(
                "NROS_WORKSPACE_ROOT=`{}` does not exist on disk",
                p.display()
            );
        }
        return p
            .canonicalize()
            .with_context(|| format!("canonicalize NROS_WORKSPACE_ROOT=`{}`", p.display()));
    }

    let start = if start.is_absolute() {
        start.to_path_buf()
    } else {
        std::env::current_dir()
            .with_context(|| "read current dir for workspace-root detection")?
            .join(start)
    };
    let start = start.canonicalize().unwrap_or(start);

    // Pass 1: `.colcon_workspace` marker.
    if let Some(root) = walk_ancestors(&start, |dir| dir.join(COLCON_WORKSPACE_MARKER).is_file()) {
        return Ok(root);
    }
    // Pass 2: `Cargo.toml` with `[workspace]`.
    if let Some(root) = walk_ancestors(&start, |dir| is_cargo_workspace_root(dir)) {
        return Ok(root);
    }
    // Pass 3: `.git/` directory.
    if let Some(root) = walk_ancestors(&start, |dir| dir.join(".git").is_dir()) {
        return Ok(root);
    }
    bail!(
        "no workspace root found above `{}` (looked for $NROS_WORKSPACE_ROOT, \
         {COLCON_WORKSPACE_MARKER}, Cargo.toml [workspace], .git/)",
        start.display()
    )
}

fn walk_ancestors(start: &Path, mut pred: impl FnMut(&Path) -> bool) -> Option<PathBuf> {
    let mut current = Some(start);
    while let Some(dir) = current {
        if pred(dir) {
            return Some(dir.to_path_buf());
        }
        current = dir.parent();
    }
    None
}

fn is_cargo_workspace_root(dir: &Path) -> bool {
    let cargo = dir.join("Cargo.toml");
    if !cargo.is_file() {
        return false;
    }
    let Ok(raw) = fs::read_to_string(&cargo) else {
        return false;
    };
    // Cheap textual sniff: `[workspace]` table header. A full TOML parse
    // would be more robust but adds a dep at the walk-up step; the
    // sniff matches the same heuristic `cargo` itself uses for the
    // "is this a workspace root?" prompt.
    raw.lines().any(|line| {
        let l = line.trim_start();
        l == "[workspace]" || l.starts_with("[workspace.") || l.starts_with("[workspace ]")
    })
}

/// Parse the `<name>` element out of a `package.xml`. The element may
/// nest inside `<package>` (ament convention); we accept any depth so
/// long as a top-level `<name>` text node exists.
fn read_package_xml_name(path: &Path) -> Result<String> {
    let file = fs::File::open(path).with_context(|| format!("open `{}`", path.display()))?;
    let mut reader = Reader::from_reader(BufReader::new(file));
    reader.config_mut().trim_text(true);

    let mut buf = Vec::new();
    let mut in_name = false;
    let mut depth = 0_i32;
    let mut name: Option<String> = None;
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                depth += 1;
                // `<name>` lives at depth 2: `<package> <name>...</name> </package>`
                // but be lenient — accept the first `<name>` text node.
                if depth >= 1 && e.local_name().as_ref() == b"name" {
                    in_name = true;
                }
            }
            Ok(Event::End(e)) => {
                depth -= 1;
                if e.local_name().as_ref() == b"name" {
                    in_name = false;
                }
            }
            Ok(Event::Text(e)) => {
                if in_name && name.is_none() {
                    let raw = e.unescape().context("unescape <name> text")?;
                    let trimmed = raw.trim().to_string();
                    if !trimmed.is_empty() {
                        name = Some(trimmed);
                    }
                }
            }
            Ok(Event::Empty(_)) => {}
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(e) => {
                return Err(eyre!(
                    "XML parse error in `{}` at position {}: {e}",
                    path.display(),
                    reader.error_position()
                ));
            }
        }
        buf.clear();
    }
    name.ok_or_else(|| {
        eyre!(
            "no `<name>` element found in `{}` (expected ament package.xml)",
            path.display()
        )
    })
}

fn read_mtime(path: &Path) -> Option<(u64, u32)> {
    let meta = fs::metadata(path).ok()?;
    let mtime = meta.modified().ok()?;
    let dur = mtime.duration_since(SystemTime::UNIX_EPOCH).ok()?;
    Some((dur.as_secs(), dur.subsec_nanos()))
}

fn load_cache_if_fresh(
    cache_path: &Path,
    workspace_root: &Path,
    package_xml_paths: &[PathBuf],
) -> Result<Option<PkgIndex>> {
    let raw = match fs::read_to_string(cache_path) {
        Ok(s) => s,
        Err(_) => return Ok(None),
    };
    let cached: CachedIndex = match serde_json::from_str(&raw) {
        Ok(v) => v,
        Err(_) => return Ok(None),
    };
    if cached.workspace_root != workspace_root {
        return Ok(None);
    }
    if cached.entries.len() != package_xml_paths.len() {
        return Ok(None);
    }
    // Every cached entry's dir must still host a package.xml whose
    // mtime matches.
    for entry in cached.entries.values() {
        let pkg_xml = entry.dir.join("package.xml");
        let Some(mtime) = read_mtime(&pkg_xml) else {
            return Ok(None);
        };
        if mtime != entry.mtime {
            return Ok(None);
        }
    }
    let pkgs = cached
        .entries
        .into_iter()
        .map(|(name, entry)| (name, entry.dir))
        .collect();
    Ok(Some(PkgIndex {
        workspace_root: workspace_root.to_path_buf(),
        pkgs,
    }))
}
