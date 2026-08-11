//! phase-348 W1 — source-time provider discovery: the scan.
//!
//! RFC-0071 D5. A provider announces itself with a `<nano_ros_provides/>` export
//! in its `package.xml`; this walks a list of workspace roots and reports every
//! package that does.
//!
//! **Source-time, not install-time.** colcon's discovery artifact is the ament
//! index, reached by sourcing `setup.sh` — which exists only *after* an install
//! step. nano-ros builds per-target static objects for RTOS targets that
//! generally have no dynamic linking, so there is no install-and-source stage
//! for an index to live in. Discovery therefore reads the source tree.
//!
//! **One concept, no special cases.** The search path is an ordered list of
//! roots, and the nano-ros tree is simply the FIRST entry — `packages/rmw/*` are
//! not builtins reached by a different code path, they are providers found the
//! way a user's are.
//!
//! **The scan reads only `package.xml`.** The descriptor (`nros-rmw.toml`,
//! `nros-board.toml`, …) is read only for the provider actually selected: one
//! cheap parse per package, one detailed parse per build. That is also why
//! [`ProviderPackage`] carries no descriptor fields — a scan that had to
//! understand every provider family would be a second place to teach about
//! them.
//!
//! Policy lives above this module, deliberately: shadowing between roots is
//! phase-348 W5 and ordering is W4, so the scan reports FACTS (what is where,
//! in search-path order) and takes no decision about ambiguity.
//!
//! Not to be confused with [`crate::package_discovery`], which walks
//! `Cargo.toml` to answer "what cargo packages are here". Same trees, different
//! file, different question — they are not two derivations of one fact, and
//! merging them would couple provider discovery to being written in Rust.

use eyre::{Result, WrapErr, bail};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};

use crate::package_xml::{PackageXml, Provision};

/// Directory names never descended into. Build output and vendored trees, both
/// of which contain `package.xml` files that are copies or third-party — and
/// `third-party/` alone is large enough to dominate the walk.
const PRUNED_DIRS: &[&str] = &[
    ".git",
    "target",
    "build",
    "install",
    "log",
    "third-party",
    "generated",
    "node_modules",
];

/// Marker files that exclude a subtree, honoured as colcon and ament spell them
/// plus our own. Buying the convention: a user who already knows `COLCON_IGNORE`
/// should not have to learn a second spelling to get the same effect.
const IGNORE_MARKERS: &[&str] = &["COLCON_IGNORE", "AMENT_IGNORE", "NROS_IGNORE"];

/// A package that announces at least one provision.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderPackage {
    /// `<name>` from package.xml.
    pub package: String,
    /// Directory containing the package.xml.
    pub dir: PathBuf,
    /// Which search-path root it was found under, by index. Retained because
    /// shadowing (W5) is decided by root ORDER, and recomputing which root a
    /// path belongs to from the path alone is ambiguous when one root nests
    /// inside another.
    pub root_index: usize,
    /// The provisions it announces.
    pub provides: Vec<Provision>,
    /// `<depend>` entries. Carried because they come free from the same parse
    /// and W4 derives build ORDER from them — re-reading every package.xml to
    /// get them would be a second walk over the same files.
    pub depends: HashSet<String>,
}

impl ProviderPackage {
    /// Where this provider's descriptor would live, given a kind. Not read by
    /// the scan; this is the handoff to selection time.
    pub fn descriptor_path(&self, kind: &str) -> PathBuf {
        self.dir.join(format!("nros-{kind}.toml"))
    }
}

/// A `package.xml` that could not be read or parsed.
///
/// Kept as data rather than aborting the scan: one malformed package.xml
/// somewhere in a large tree must not make every provider undiscoverable. The
/// caller decides — the CLI prints them, and a gate can make them fatal.
#[derive(Debug, Clone)]
pub struct ScanError {
    pub path: PathBuf,
    pub message: String,
}

/// What a scan found.
#[derive(Debug, Clone, Default)]
pub struct ScanResult {
    /// Providers, in search-path order then by path within a root.
    pub providers: Vec<ProviderPackage>,
    /// Unreadable/malformed package.xml files encountered on the way.
    pub errors: Vec<ScanError>,
    /// Every package.xml READ, provider or not, sorted.
    ///
    /// Two jobs. It is the denominator for "the scan looked at N packages and 4
    /// were providers" — without it an empty result cannot be told apart from a
    /// walk that never ran. And it is the cache-invalidation input set (W3):
    /// cmake watches exactly these files, so editing any package.xml — not only
    /// a provider's — re-configures. A non-provider matters because ADDING a
    /// provision to it is precisely the edit that must be noticed.
    pub inputs: Vec<PathBuf>,
}

impl ScanResult {
    /// How many package.xml files the walk read.
    pub fn packages_seen(&self) -> usize {
        self.inputs.len()
    }
}

/// The default search path: the nano-ros tree first, then the user workspace.
///
/// **Exactly two roots, and both live in the user's repo.** Deliberately
/// rejected: an installed index under `~/.nros`, and any environment variable
/// such as `NROS_RMW_PATH`. Machine state makes a build irreproducible from the
/// checkout and lets CI diverge from a developer's box — the failure would be
/// "works here, not there", which is the expensive kind.
///
/// When the workspace IS inside the nano-ros tree (building the monorepo's own
/// examples, which is the common case in this repo) the two roots coincide and
/// the second is dropped: scanning one tree twice would report every provider
/// as shadowing itself.
pub fn default_search_path(nano_ros_root: Option<&Path>, workspace: &Path) -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Some(r) = nano_ros_root {
        roots.push(r.to_path_buf());
    }
    let ws = workspace.to_path_buf();
    if !roots.iter().any(|r| ws == *r || ws.starts_with(r)) {
        roots.push(ws);
    }
    roots
}

/// Scan an ordered search path. Earlier roots come first in the result.
pub fn scan_roots(roots: &[PathBuf]) -> Result<ScanResult> {
    let mut out = ScanResult::default();
    for (root_index, root) in roots.iter().enumerate() {
        let one = scan_root(root, root_index)
            .wrap_err_with(|| format!("scanning provider root {}", root.display()))?;
        out.providers.extend(one.providers);
        out.errors.extend(one.errors);
        out.inputs.extend(one.inputs);
    }
    Ok(out)
}

/// Scan a single root. A root that does not exist yields nothing rather than an
/// error: the user-workspace entry of the default search path is legitimately
/// absent when someone builds nano-ros on its own.
pub fn scan_root(root: &Path, root_index: usize) -> Result<ScanResult> {
    let mut out = ScanResult::default();
    if !root.is_dir() {
        return Ok(out);
    }

    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        if IGNORE_MARKERS.iter().any(|m| dir.join(m).exists()) {
            continue;
        }

        let manifest = dir.join("package.xml");
        if manifest.is_file() {
            out.inputs.push(manifest.clone());
            match PackageXml::parse(&manifest) {
                Ok(pkg) => {
                    if !pkg.provides.is_empty() {
                        out.providers.push(ProviderPackage {
                            package: pkg.name,
                            dir: dir.clone(),
                            root_index,
                            provides: pkg.provides,
                            depends: pkg.dependencies,
                        });
                    }
                }
                Err(e) => out.errors.push(ScanError {
                    path: manifest,
                    message: format!("{e:#}"),
                }),
            }
            // Do not descend into a package, as colcon does not. A package's
            // own subdirectories are its sources, and anything package-shaped
            // inside one is a fixture or a copy rather than a sibling provider.
            continue;
        }

        let entries = match std::fs::read_dir(&dir) {
            Ok(e) => e,
            // An unreadable directory is worth reporting but not fatal — a
            // permission-denied subtree must not hide every provider above it.
            Err(e) => {
                out.errors.push(ScanError {
                    path: dir.clone(),
                    message: format!("read_dir: {e}"),
                });
                continue;
            }
        };
        for entry in entries.flatten() {
            let path = entry.path();
            // `file_type()` rather than `is_dir()`: symlinks are not followed,
            // so a link pointing at an ancestor cannot make the walk loop.
            let Ok(ft) = entry.file_type() else { continue };
            if !ft.is_dir() {
                continue;
            }
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.starts_with('.') || PRUNED_DIRS.contains(&name.as_ref()) {
                continue;
            }
            stack.push(path);
        }
    }

    // `stack.pop()` makes the walk order depend on read_dir order, which is
    // filesystem-dependent. Sort so the result is reproducible across machines
    // — W5 reports ambiguity by listing paths, and an unstable order would make
    // that message differ between hosts for the same tree.
    out.providers.sort_by(|a, b| a.dir.cmp(&b.dir));
    out.errors.sort_by(|a, b| a.path.cmp(&b.path));
    out.inputs.sort();
    Ok(out)
}

// ===========================================================================
// The index — phase-348 W3
// ===========================================================================

/// Bumped when the on-disk shape changes. A reader that finds a version it does
/// not know REGENERATES rather than guessing: an index is a cache, and a cache
/// that silently misinterprets an old layout is worse than no cache.
pub const INDEX_VERSION: u32 = 1;

/// A scan result written to disk so it is not recomputed per configure.
///
/// Purely a CACHE — every field is rederivable by rescanning, and nothing may
/// depend on the file existing. That is what makes it safe for cmake to read
/// and for a gate to delete.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderIndex {
    pub version: u32,
    /// The search path this was produced from, in order. Recorded because an
    /// index built for a DIFFERENT search path is not stale, it is wrong —
    /// [`ProviderIndex::is_valid_for`] rejects it rather than serving it.
    pub roots: Vec<PathBuf>,
    /// Every package.xml read, sorted. cmake attaches these to
    /// `CMAKE_CONFIGURE_DEPENDS` so editing one re-configures.
    pub inputs: Vec<PathBuf>,
    pub providers: Vec<ProviderPackage>,
}

impl ProviderIndex {
    pub fn from_scan(roots: &[PathBuf], scan: &ScanResult) -> Self {
        Self {
            version: INDEX_VERSION,
            roots: roots.to_vec(),
            inputs: scan.inputs.clone(),
            providers: scan.providers.clone(),
        }
    }

    /// Whether this index answers questions about `roots`.
    pub fn is_valid_for(&self, roots: &[PathBuf]) -> bool {
        self.version == INDEX_VERSION && self.roots == roots
    }

    /// Write atomically — via a temp file in the same directory, then rename.
    ///
    /// A half-written index is a JSON parse error at someone else's configure,
    /// and the two writers here (`nros sync` and a cmake configure) can run
    /// concurrently on one tree. Same reasoning as `check-atomic-sync-writes`
    /// enforces for sync's other outputs.
    pub fn write(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .wrap_err_with(|| format!("creating {}", parent.display()))?;
        }
        let body = serde_json::to_string_pretty(self)? + "\n";
        let tmp = path.with_extension(format!(
            "tmp{}",
            std::process::id() // unique per writer; renamed away immediately
        ));
        std::fs::write(&tmp, body).wrap_err_with(|| format!("writing {}", tmp.display()))?;
        std::fs::rename(&tmp, path)
            .wrap_err_with(|| format!("renaming into place: {}", path.display()))?;
        Ok(())
    }

    pub fn read(path: &Path) -> Result<Self> {
        let body = std::fs::read_to_string(path)
            .wrap_err_with(|| format!("reading provider index {}", path.display()))?;
        let idx: Self = serde_json::from_str(&body)
            .wrap_err_with(|| format!("parsing provider index {}", path.display()))?;
        if idx.version != INDEX_VERSION {
            bail!(
                "provider index {} is version {} but this nros speaks {} — \
                 delete it and re-run `nros sync`",
                path.display(),
                idx.version,
                INDEX_VERSION
            );
        }
        Ok(idx)
    }

    /// One line per provision, `kind\tname\tpackage\troot_index\tdir`.
    ///
    /// The machine-readable seam for cmake, which has no JSON parser. Same role
    /// as `nros ws model-dims`: cmake ASKS rather than re-implementing the read,
    /// so there is never a second parser of this file to drift.
    pub fn to_lines(&self) -> String {
        let mut out = String::new();
        for p in &self.providers {
            for pr in &p.provides {
                out.push_str(&format!(
                    "{}\t{}\t{}\t{}\t{}\n",
                    pr.kind,
                    pr.name,
                    p.package,
                    p.root_index,
                    p.dir.display()
                ));
            }
        }
        out
    }
}

/// How a freshly-scanned tree differs from an index. Empty ⇒ the index is
/// current.
///
/// This exists because watching the recorded `inputs` cannot catch the case
/// that matters most: a package.xml that did not exist when the index was
/// written is in nobody's watch list. That is issue 0196's exact shape — a
/// probe whose inputs never include the thing that breaks it — so the answer is
/// an explicit rescan-and-compare rather than a cleverer file watch.
#[derive(Debug, Default)]
pub struct IndexDiff {
    /// package.xml paths present now, absent from the index.
    pub added_inputs: Vec<PathBuf>,
    /// Recorded in the index, gone from the tree.
    pub removed_inputs: Vec<PathBuf>,
    /// `kind:name -> dir` provisions that appeared, vanished, or moved.
    pub changed_provisions: Vec<String>,
}

impl IndexDiff {
    pub fn is_empty(&self) -> bool {
        self.added_inputs.is_empty()
            && self.removed_inputs.is_empty()
            && self.changed_provisions.is_empty()
    }
}

/// Compare an index against a fresh scan of the same roots.
pub fn diff_index(index: &ProviderIndex, fresh: &ScanResult) -> IndexDiff {
    let old_inputs: HashSet<&PathBuf> = index.inputs.iter().collect();
    let new_inputs: HashSet<&PathBuf> = fresh.inputs.iter().collect();

    let provision_set = |ps: &[ProviderPackage]| -> HashSet<String> {
        ps.iter()
            .flat_map(|p| {
                let dir = p.dir.display().to_string();
                p.provides
                    .iter()
                    .map(move |pr| format!("{}:{} -> {}", pr.kind, pr.name, dir))
            })
            .collect()
    };
    let old_p = provision_set(&index.providers);
    let new_p = provision_set(&fresh.providers);

    let mut changed: Vec<String> = new_p
        .difference(&old_p)
        .map(|s| format!("+ {s}"))
        .chain(old_p.difference(&new_p).map(|s| format!("- {s}")))
        .collect();
    changed.sort();

    let mut added: Vec<PathBuf> = new_inputs
        .difference(&old_inputs)
        .map(|p| (*p).clone())
        .collect();
    let mut removed: Vec<PathBuf> = old_inputs
        .difference(&new_inputs)
        .map(|p| (*p).clone())
        .collect();
    added.sort();
    removed.sort();

    IndexDiff {
        added_inputs: added,
        removed_inputs: removed,
        changed_provisions: changed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn write(path: &Path, body: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, body).unwrap();
    }

    fn provider_xml(name: &str, kind: &str, provides: &str) -> String {
        format!(
            r#"<?xml version="1.0"?>
<package format="3">
  <name>{name}</name>
  <version>0.0.0</version>
  <export>
    <nano_ros_provides kind="{kind}" name="{provides}"/>
  </export>
</package>"#
        )
    }

    fn plain_xml(name: &str) -> String {
        format!(
            r#"<?xml version="1.0"?>
<package format="3">
  <name>{name}</name>
  <depend>std_msgs</depend>
</package>"#
        )
    }

    /// The W1 acceptance criterion, both halves: a package with the export is
    /// listed, one without is not.
    #[test]
    fn scan_lists_providers_and_only_providers() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        write(
            &root.join("src/my_rmw/package.xml"),
            &provider_xml("my_rmw", "rmw", "acme"),
        );
        write(&root.join("src/my_node/package.xml"), &plain_xml("my_node"));

        let r = scan_root(root, 0).unwrap();
        assert_eq!(r.packages_seen(), 2, "both packages were looked at");
        assert_eq!(r.providers.len(), 1);
        assert_eq!(r.providers[0].package, "my_rmw");
        assert_eq!(r.providers[0].provides[0].name, "acme");
        assert!(r.errors.is_empty());
    }

    #[test]
    fn search_path_order_is_preserved_and_root_recorded() {
        let a = tempfile::tempdir().unwrap();
        let b = tempfile::tempdir().unwrap();
        write(
            &a.path().join("packages/rmw/zenoh/package.xml"),
            &provider_xml("nros_rmw_zenoh", "rmw", "zenoh"),
        );
        write(
            &b.path().join("src/patched_zenoh/package.xml"),
            &provider_xml("patched_zenoh", "rmw", "zenoh"),
        );

        let r = scan_roots(&[a.path().to_path_buf(), b.path().to_path_buf()]).unwrap();
        assert_eq!(r.providers.len(), 2, "the scan reports BOTH");
        assert_eq!(r.providers[0].root_index, 0);
        assert_eq!(r.providers[1].root_index, 1);
        // Resolving the collision is W5's job, not the scan's; the scan must
        // not silently drop either one, which is what makes W5 able to warn
        // with both paths.
        assert_eq!(
            r.providers[0].provides[0].name,
            r.providers[1].provides[0].name
        );
    }

    #[test]
    fn ignore_markers_and_pruned_dirs_are_skipped() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        write(
            &root.join("src/ignored/package.xml"),
            &provider_xml("ignored", "rmw", "nope"),
        );
        write(&root.join("src/ignored/COLCON_IGNORE"), "");
        write(
            &root.join("build/stale/package.xml"),
            &provider_xml("stale", "rmw", "stale"),
        );
        write(
            &root.join("third-party/vendored/package.xml"),
            &provider_xml("vendored", "rmw", "vendored"),
        );
        write(
            &root.join("src/real/package.xml"),
            &provider_xml("real", "rmw", "real"),
        );

        let r = scan_root(root, 0).unwrap();
        let names: Vec<_> = r.providers.iter().map(|p| p.package.as_str()).collect();
        assert_eq!(names, vec!["real"]);
        assert_eq!(r.packages_seen(), 1, "the skipped ones were never parsed");
    }

    /// A malformed package.xml is reported without taking the scan down with
    /// it. The failure mode this prevents: one broken file in a large tree
    /// making every provider in it undiscoverable.
    #[test]
    fn malformed_package_xml_is_reported_not_fatal() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        write(&root.join("src/broken/package.xml"), "<package><name>");
        write(
            &root.join("src/good/package.xml"),
            &provider_xml("good", "rmw", "good"),
        );

        let r = scan_root(root, 0).unwrap();
        assert_eq!(r.providers.len(), 1, "the good one is still found");
        assert_eq!(r.errors.len(), 1);
        assert!(r.errors[0].path.ends_with("src/broken/package.xml"));
    }

    #[test]
    fn scan_does_not_descend_into_a_package() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        write(
            &root.join("src/outer/package.xml"),
            &provider_xml("outer", "rmw", "outer"),
        );
        write(
            &root.join("src/outer/test/fixture/package.xml"),
            &provider_xml("fixture", "rmw", "fixture"),
        );

        let r = scan_root(root, 0).unwrap();
        let names: Vec<_> = r.providers.iter().map(|p| p.package.as_str()).collect();
        assert_eq!(names, vec!["outer"], "the nested fixture is not a sibling");
    }

    #[test]
    fn missing_root_is_empty_not_an_error() {
        let tmp = tempfile::tempdir().unwrap();
        let r = scan_root(&tmp.path().join("no-such-workspace"), 1).unwrap();
        assert!(r.providers.is_empty());
        assert_eq!(r.packages_seen(), 0);
    }

    /// `default_search_path` is pure — it compares paths and never touches the
    /// filesystem — so these directories need not exist. They are still built
    /// under a tempdir rather than written as absolute literals, because a
    /// hardcoded home-directory path in a test resolves only on the machine
    /// that wrote it (`check-absolute-paths`, issue 0334 — which reads source
    /// text and so flags such a path in a COMMENT too, deliberately: a doc
    /// example is exactly how the pattern spreads).
    #[test]
    fn search_path_drops_a_workspace_nested_in_the_nano_ros_tree() {
        let tmp = tempfile::tempdir().unwrap();
        let nros = tmp.path().join("nano-ros");
        let nested = nros.join("examples/native");
        let outside = tmp.path().join("my_ws");

        assert_eq!(
            default_search_path(Some(&nros), &nested),
            vec![nros.clone()],
            "a nested workspace would otherwise be scanned twice and every \
             provider would appear to shadow itself"
        );
        assert_eq!(
            default_search_path(Some(&nros), &outside),
            vec![nros.clone(), outside.clone()],
        );
        assert_eq!(
            default_search_path(None, &outside),
            vec![outside],
            "an out-of-tree consumer with no nano-ros source still scans its own \
             workspace",
        );
    }

    // --- index (W3) --------------------------------------------------------

    #[test]
    fn index_round_trips_and_matches_a_fresh_scan() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("ws");
        write(
            &root.join("src/a/package.xml"),
            &provider_xml("a", "rmw", "acme"),
        );
        write(&root.join("src/b/package.xml"), &plain_xml("b"));

        let roots = vec![root.clone()];
        let scan = scan_roots(&roots).unwrap();
        let idx = ProviderIndex::from_scan(&roots, &scan);
        let path = tmp.path().join("build/nros/providers.json");
        idx.write(&path).unwrap();

        let back = ProviderIndex::read(&path).unwrap();
        assert!(back.is_valid_for(&roots));
        assert_eq!(back.providers.len(), 1);
        assert_eq!(back.inputs.len(), 2, "non-providers are inputs too");
        assert!(
            diff_index(&back, &scan).is_empty(),
            "no drift against itself"
        );
    }

    /// The case a file watch cannot cover: a package.xml that did not exist
    /// when the index was written is in nobody's watch list (issue 0196's
    /// shape). Only a rescan-and-compare sees it.
    #[test]
    fn diff_detects_a_provider_added_after_the_index_was_written() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("ws");
        write(
            &root.join("src/a/package.xml"),
            &provider_xml("a", "rmw", "acme"),
        );
        let roots = vec![root.clone()];
        let idx = ProviderIndex::from_scan(&roots, &scan_roots(&roots).unwrap());

        write(
            &root.join("src/late/package.xml"),
            &provider_xml("late", "rmw", "latecomer"),
        );
        let fresh = scan_roots(&roots).unwrap();
        let d = diff_index(&idx, &fresh);

        assert!(!d.is_empty());
        assert_eq!(d.added_inputs.len(), 1);
        assert!(d.added_inputs[0].ends_with("src/late/package.xml"));
        assert!(
            d.changed_provisions
                .iter()
                .any(|c| c.contains("+ rmw:latecomer")),
            "got {:?}",
            d.changed_provisions
        );
    }

    #[test]
    fn diff_detects_a_provision_removed_from_an_existing_package() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("ws");
        let pkg = root.join("src/a/package.xml");
        write(&pkg, &provider_xml("a", "rmw", "acme"));
        let roots = vec![root.clone()];
        let idx = ProviderIndex::from_scan(&roots, &scan_roots(&roots).unwrap());

        // The file still exists and is still an input — only its content moved.
        write(&pkg, &plain_xml("a"));
        let d = diff_index(&idx, &scan_roots(&roots).unwrap());

        assert!(d.added_inputs.is_empty() && d.removed_inputs.is_empty());
        assert!(
            d.changed_provisions
                .iter()
                .any(|c| c.starts_with("- rmw:acme")),
            "got {:?}",
            d.changed_provisions
        );
    }

    /// An index built for a different search path is WRONG, not stale.
    #[test]
    fn index_for_other_roots_is_rejected() {
        let tmp = tempfile::tempdir().unwrap();
        let a = tmp.path().join("a");
        let b = tmp.path().join("b");
        fs::create_dir_all(&a).unwrap();
        let idx = ProviderIndex::from_scan(&[a.clone()], &scan_roots(&[a.clone()]).unwrap());
        assert!(idx.is_valid_for(&[a.clone()]));
        assert!(!idx.is_valid_for(&[a, b]));
    }

    #[test]
    fn unknown_index_version_is_an_error_not_a_guess() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("providers.json");
        fs::write(
            &path,
            r#"{"version":9999,"roots":[],"inputs":[],"providers":[]}"#,
        )
        .unwrap();
        let err = ProviderIndex::read(&path).unwrap_err().to_string();
        assert!(err.contains("version 9999"), "got: {err}");
    }

    #[test]
    fn to_lines_emits_one_row_per_provision() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("ws");
        write(
            &root.join("src/multi/package.xml"),
            &format!(
                r#"<?xml version="1.0"?>
<package format="3">
  <name>multi</name>
  <export>
    <nano_ros_provides kind="rmw" name="one"/>
    <nano_ros_provides kind="board" name="two"/>
  </export>
</package>"#
            ),
        );
        let roots = vec![root];
        let idx = ProviderIndex::from_scan(&roots, &scan_roots(&roots).unwrap());
        let rendered = idx.to_lines();
        let lines: Vec<&str> = rendered.lines().collect();
        assert_eq!(lines.len(), 2);
        assert!(lines[0].starts_with("rmw\tone\tmulti\t0\t"));
        assert!(lines[1].starts_with("board\ttwo\tmulti\t0\t"));
    }

    #[test]
    fn descriptor_path_is_derived_from_kind() {
        let tmp = tempfile::tempdir().unwrap();
        write(
            &tmp.path().join("src/p/package.xml"),
            &provider_xml("p", "rmw", "acme"),
        );
        let r = scan_root(tmp.path(), 0).unwrap();
        assert!(
            r.providers[0]
                .descriptor_path("rmw")
                .ends_with("nros-rmw.toml"),
            "the scan hands selection a path; it does not read it"
        );
    }
}
