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

use eyre::{Result, WrapErr};
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
#[derive(Debug, Clone)]
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
    /// Every package.xml seen, provider or not. The denominator for "the scan
    /// looked at N packages and 4 of them were providers" — without it, an
    /// empty result cannot be told apart from a walk that never ran.
    pub packages_seen: usize,
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
        out.packages_seen += one.packages_seen;
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
            out.packages_seen += 1;
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
    Ok(out)
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
        assert_eq!(r.packages_seen, 2, "both packages were looked at");
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
        assert_eq!(r.packages_seen, 1, "the skipped ones were never parsed");
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
        assert_eq!(r.packages_seen, 0);
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
