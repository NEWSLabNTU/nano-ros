//! The merged configuration catalog, as the `nros` CLI builds it (RFC-0140
//! D2/D4/D5, phase-581 W1-W3).
//!
//! The loader is `nros_knob_catalog` (re-exported here): the one parser of
//! `nros-knobs.toml`, shared with a package's build script through
//! `nros-knob-build`, which cannot reach this crate. What stays here is the
//! CLI's side: turning the provider scan into the loader's input, reading each
//! package file as a configure stamp input (`configure_stamp::read_external`,
//! so an edit to one re-runs configure), and writing the per-build views
//! atomically under `<build>/nros/`.

use std::path::{Path, PathBuf};

use cargo_nano_ros::provider_scan::{ProviderPackage, ScanResult};
use eyre::{Result, WrapErr};
pub use nros_knob_catalog::{
    CATALOG_ENV, Catalog, DESCRIPTORS, Meta, Origin, PACKAGE_FILE, PackageDir, RetiredEntry, Row,
    RowKind, Shadowed, catalog_path, derive_id, env_header, kconfig_packages, kconfig_rows,
    knobs_dir, parse_own_file, parse_package, registry_cmake,
};

/// The catalog of this run (phase-581 W3): the one `nros configure` built,
/// or loaded by the stage that needs it. The APP rung reads an out-of-tree
/// package's knob from it ([`crate::cmd::leaf_settings::app_rung`]), whose
/// many callers have no workspace in hand.
static CURRENT: std::sync::RwLock<Option<std::sync::Arc<Catalog>>> = std::sync::RwLock::new(None);

/// Make `cat` the catalog of this run.
pub fn set_current(cat: Catalog) {
    *CURRENT
        .write()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(std::sync::Arc::new(cat));
}

/// The catalog of this run, if one was built or loaded.
#[must_use]
pub fn current() -> Option<std::sync::Arc<Catalog>> {
    CURRENT
        .read()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone()
}

/// Make the catalog `nros configure` wrote for `ws_root` the catalog of this
/// run, unless one is set already. A workspace configure never wrote has only
/// the compiled rows, so nothing is set.
pub fn ensure_current(ws_root: &Path) {
    if current().is_some() {
        return;
    }
    if let Ok(text) = std::fs::read_to_string(catalog_path(ws_root))
        && let Ok(cat) = Catalog::parse_toml(&text)
    {
        set_current(cat);
    }
}

/// The owner words an image LINKS (RFC-0140 D6): its RMW's, its board's (the
/// key, `-` read as `_`) and its platform's. A knob whose owner is a package
/// word not among them is refused ([`refuse_unlinked_package_knobs`]).
#[must_use]
pub fn linked_owners(
    rmw: Option<&str>,
    board: Option<&str>,
    platform: Option<&str>,
) -> Vec<String> {
    [rmw, board, platform]
        .into_iter()
        .flatten()
        .map(|w| w.replace('-', "_"))
        .collect()
}

/// RFC-0140 D6 — an image may state a package's knob only if it links that
/// package. `keys` are the image's `[image.<id>.knobs]` keys; a key whose owner
/// word belongs to a package of the catalog that `linked` does not name is
/// refused, naming the owner, its package and what the image links.
///
/// # Errors
///
/// One line per refused key.
pub fn refuse_unlinked_package_knobs<'a>(
    cat: &Catalog,
    image_id: &str,
    keys: impl IntoIterator<Item = &'a String>,
    linked: &[String],
) -> Result<()> {
    let refused: Vec<String> = keys
        .into_iter()
        .filter_map(|key| {
            let row = cat.knob(key)?;
            let owner = row.owner();
            let package = cat.owner_package(owner)?;
            if linked.iter().any(|l| l == owner) {
                return None;
            }
            let (g, n) = key.rsplit_once('.').unwrap_or(("", key));
            Some(format!(
                "`[image.{image_id}.knobs.{g}] {n}` is a knob of package {package} (owner \
                 `{owner}`), which image `{image_id}` does not link: it links {}. State it \
                 only for an image whose rmw, board or platform is `{owner}`.",
                if linked.is_empty() {
                    "no package".to_string()
                } else {
                    linked
                        .iter()
                        .map(|l| format!("`{l}`"))
                        .collect::<Vec<_>>()
                        .join(", ")
                }
            ))
        })
        .collect();
    if refused.is_empty() {
        Ok(())
    } else {
        Err(eyre::eyre!("{}", refused.join("\n")))
    }
}

/// The loader's input for each package the scan found. `scan` names each
/// package's root, when the caller has the whole scan.
#[must_use]
pub fn packages_of(providers: &[ProviderPackage], scan: Option<&ScanResult>) -> Vec<PackageDir> {
    providers
        .iter()
        .map(|p| PackageDir {
            package: p.package.clone(),
            dir: p.dir.clone(),
            root_index: p.root_index,
            root: Some(scan.and_then(|s| s.origin(p.root_index)).map_or_else(
                || format!("root {}", p.root_index),
                |o| format!("{} (root {})", o.label(), p.root_index),
            )),
            provides: p
                .provides
                .iter()
                .map(|pr| (pr.kind.clone(), pr.name.clone()))
                .collect(),
        })
        .collect()
}

/// The board packages under `roots` (the board catalog's extra roots,
/// `$NROS_EXTRA_BOARD_PATH`), as loader input: RFC-0140 D2 finds a board
/// package's file by the scan that finds its descriptor, which for an
/// out-of-tree board is the board catalog's, not the provider scan's. Each root
/// ranks after the provider scan's (`first_index` on), so the same package in
/// both is shadowed, never refused. A directory with no readable `package.xml`
/// is skipped: the board catalog itself refuses that board.
#[must_use]
pub fn board_packages(roots: &[PathBuf], first_index: usize) -> Vec<PackageDir> {
    let mut out = Vec::new();
    for (i, root) in roots.iter().enumerate() {
        let mut dirs = Vec::new();
        if crate::orchestration::board_descriptor::BoardCatalog::collect_board_dirs(
            root, 0, &mut dirs,
        )
        .is_err()
        {
            continue;
        }
        for dir in dirs {
            let Ok(xml) = cargo_nano_ros::package_xml::PackageXml::parse(&dir.join("package.xml"))
            else {
                continue;
            };
            out.push(PackageDir {
                package: xml.name,
                dir,
                root_index: first_index + i,
                root: Some(format!("NROS_EXTRA_BOARD_PATH {}", root.display())),
                provides: xml.provides.into_iter().map(|p| (p.kind, p.name)).collect(),
            });
        }
    }
    out
}

/// [`Catalog::build`] over a provider scan, reading each package file as a
/// configure input.
///
/// # Errors
///
/// A package file that does not parse or breaks RFC-0140 D2's rules C1 to C4.
pub fn build(providers: &[ProviderPackage]) -> Result<Catalog> {
    build_scanned(providers, None)
}

/// [`build`], with the scan's root origins for the catalog's root column.
///
/// # Errors
///
/// As [`build`].
pub fn build_scanned(providers: &[ProviderPackage], scan: Option<&ScanResult>) -> Result<Catalog> {
    build_dirs(&packages_of(providers, scan))
}

/// The provider scan's packages and the board catalog's extra roots'
/// ([`board_packages`]): every package a build in this workspace resolves.
///
/// # Errors
///
/// As [`build`].
pub fn build_workspace(
    providers: &[ProviderPackage],
    scan: Option<&ScanResult>,
) -> Result<Catalog> {
    let mut dirs = packages_of(providers, scan);
    let first = dirs.iter().map(|d| d.root_index + 1).max().unwrap_or(0);
    dirs.extend(board_packages(
        &crate::orchestration::board_descriptor::extra_board_roots(),
        first,
    ));
    build_dirs(&dirs)
}

/// [`Catalog::build`] over loader input, reading each package file as a
/// configure input.
///
/// # Errors
///
/// As [`build`].
pub fn build_dirs(dirs: &[PackageDir]) -> Result<Catalog> {
    let read = |p: &Path| {
        crate::configure_stamp::read_external(p).map_err(|e| std::io::Error::other(e.to_string()))
    };
    Ok(Catalog::build(dirs, &read)?)
}

/// Write [`Catalog::render_toml`] to [`catalog_path`], and the per-build views
/// of the out-of-tree rows beside it (RFC-0140 D5): `registry.cmake`, and an
/// `<owner>_env.h` for each package owner with env rows. Each file is replaced
/// only when its bytes change, so an unchanged catalog leaves no new mtime.
///
/// # Errors
///
/// A file cannot be written.
pub fn write(cat: &Catalog, ws_root: &Path) -> Result<PathBuf> {
    let path = catalog_path(ws_root);
    let dir = knobs_dir(ws_root);
    std::fs::create_dir_all(&dir).wrap_err_with(|| format!("creating {}", dir.display()))?;
    let put = |p: &Path, body: &str| {
        crate::atomic_file::atomic_write(p, body)
            .wrap_err_with(|| format!("writing {}", p.display()))
    };
    put(&path, &cat.render_toml())?;
    put(&dir.join("registry.cmake"), &registry_cmake(cat))?;
    let mut owners: Vec<&str> = cat
        .package_rows()
        .filter(|r| r.kind == RowKind::Env)
        .map(Row::owner)
        .collect();
    owners.sort_unstable();
    owners.dedup();
    for owner in owners {
        put(&dir.join(format!("{owner}_env.h")), &env_header(cat, owner))?;
    }
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(3)
            .expect("repo root")
            .to_path_buf()
    }

    /// RFC-0140 D2 / phase-581 W2: the provider scan of the checkout finds
    /// exactly the in-tree package files the generator rendered, with the
    /// owner word the generator derived, and their rows equal the compiled
    /// ones (so no in-tree package reads as shadowed).
    #[test]
    fn the_scan_finds_the_in_tree_packages_the_generator_rendered() {
        let root = repo_root();
        let roots: Vec<PathBuf> = ["packages/rmw", "packages/platform", "packages/boards"]
            .iter()
            .map(|d| root.join(d))
            .collect();
        let scan = cargo_nano_ros::provider_scan::scan_roots(&roots).unwrap();
        let dirs = packages_of(&scan.providers, Some(&scan));
        let mut found: Vec<(String, String, String)> = dirs
            .iter()
            .filter(|p| p.dir.join(PACKAGE_FILE).is_file())
            .map(|p| {
                let file = p.dir.join(PACKAGE_FILE);
                let text = std::fs::read_to_string(&file).unwrap();
                let (owner, _) = parse_package(p, &file, &text).unwrap();
                let rel = file.strip_prefix(&root).unwrap().display().to_string();
                (p.package.clone(), owner, rel)
            })
            .collect();
        found.sort();
        let mut want: Vec<(String, String, String)> = nros_zephyr_build::IN_TREE_PACKAGES
            .iter()
            .map(|p| (p.package.into(), p.owner.into(), p.file.into()))
            .collect();
        want.sort();
        assert_eq!(found, want);
        assert_eq!(
            want.len(),
            6,
            "zenoh, xrce, cyclonedds, freertos, threadx, zephyr"
        );
        let cat = build(&scan.providers).expect("the in-tree set loads");
        assert!(cat.shadowed().is_empty(), "{:?}", cat.shadowed());
        assert_eq!(cat, Catalog::core());
        // A moved row keeps its spellings and names its package.
        let row = cat
            .rows()
            .iter()
            .find(|r| r.name == "zenoh.max_queryables")
            .unwrap();
        assert!(
            matches!(&row.origin, Origin::Package { package, .. } if package == "nros_rmw_zenoh")
        );
    }

    #[test]
    fn configure_writes_the_catalog_it_builds() {
        let tmp = tempfile::tempdir().unwrap();
        let cat = build(&[]).unwrap();
        let path = write(&cat, tmp.path()).unwrap();
        assert_eq!(path, catalog_path(tmp.path()));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), cat.render_toml());
        assert!(knobs_dir(tmp.path()).join("registry.cmake").is_file());
    }
}
