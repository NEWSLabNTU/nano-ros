//! RFC-0120 D4 (phase-530 W2) -- where a standalone leaf's entity counts
//! come from. ONE rule, asked by every reader of a leaf's entities: the
//! descriptor's composer, the leaf reader, the Kconfig rendering, `nros build`'s
//! leaf road, and (through `nros dev image entity-census source`) the cmake
//! and Zephyr hooks, which used to grep `system.toml` for one word.
//!
//! * a component that states `entities = [...]` -> **declared** (it wins, and
//!   needs no census);
//! * `entities = "census"` -> the **census**, as since issue 1556;
//! * `entities = "defaults"` -> the crate **defaults**, written down;
//! * nothing stated, on a C/C++ leaf (a `CMakeLists.txt` and no `Cargo.toml`)
//!   with at least one `[[component]]`, for a CROSS image -> the **census, by
//!   default**. A leaf is sized tight or says it is loose; nothing is silently
//!   loose.
//!
//! What stays [`EntitySource::Unstated`]: a Rust leaf (the metadata probe
//! counts it, RFC-0120 D4's other half), a leaf with no `[[component]]` (a
//! census records the program a component names), and a HOST image -- the
//! census producer's own platform, sized at the hosted defaults as before,
//! the same line `build::census_take_is_ours` draws for a workspace.

use std::path::Path;

use nros_orchestration_ir::leaf_system::LeafSystem;

/// Where a leaf's entity counts come from. See the module docs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntitySource {
    /// Some component states `entities = [...]`.
    Declared,
    /// The leaf's census. `explicit` when `entities = "census"` is written,
    /// `false` when it is the C/C++ default.
    Census { explicit: bool },
    /// `entities = "defaults"`.
    Defaults,
    /// Nothing stated and no default applies: a Rust leaf, a leaf with no
    /// component, or a host image.
    Unstated,
}

impl EntitySource {
    /// The word `entity-census source` prints, which the cmake hooks match.
    pub fn word(self) -> &'static str {
        match self {
            EntitySource::Declared => "declared",
            EntitySource::Census { .. } => "census",
            EntitySource::Defaults => "defaults",
            EntitySource::Unstated => "unstated",
        }
    }

    pub fn is_census(self) -> bool {
        matches!(self, EntitySource::Census { .. })
    }
}

/// Is `dir` a C/C++ leaf: a `CMakeLists.txt` and no `Cargo.toml`? A Rust
/// Zephyr leaf carries both, and the probe counts it.
pub fn is_cmake_leaf(dir: &Path) -> bool {
    dir.join("CMakeLists.txt").is_file() && !dir.join("Cargo.toml").is_file()
}

/// [`entity_source`] with the host question answered by the caller:
/// `is_host(board)` is `Some(true)` for a board whose platform is the host's,
/// `Some(false)` for a cross board, `None` when the board resolves to nothing
/// (counted as cross: an unknown board is no reason to be loose).
pub fn entity_source_with(
    dir: &Path,
    system: &LeafSystem,
    is_host: &dyn Fn(&str) -> Option<bool>,
) -> EntitySource {
    if system.entities_from_census() {
        return EntitySource::Census { explicit: true };
    }
    if system.components.iter().any(|c| c.entities.is_some()) {
        return EntitySource::Declared;
    }
    if system.entities_defaults() {
        return EntitySource::Defaults;
    }
    if system.components.is_empty() || !is_cmake_leaf(dir) {
        return EntitySource::Unstated;
    }
    let host = system.board.as_deref().and_then(is_host).unwrap_or(false);
    if host {
        EntitySource::Unstated
    } else {
        EntitySource::Census { explicit: false }
    }
}

/// [`entity_source`] for the leaf at `dir`, the host question answered from
/// the board catalog of the nano-ros checkout `dir` resolves.
pub fn entity_source(dir: &Path, system: &LeafSystem) -> EntitySource {
    use crate::orchestration::board_descriptor::PlatformKind;
    // Only a leaf the default could reach pays for the catalog.
    if system.states_entities() || system.components.is_empty() || !is_cmake_leaf(dir) {
        return entity_source_with(dir, system, &|_| None);
    }
    let catalog = crate::orchestration::nano_ros_root::resolve(None, dir)
        .and_then(|root| crate::cmd::leaf_system::load_catalog(&root, dir));
    let is_host = |board: &str| -> Option<bool> {
        let catalog = catalog.as_ref()?;
        crate::cmd::board_facts::resolve_board(catalog, board)
            .map(|d| d.platform == PlatformKind::Posix)
    };
    entity_source_with(dir, system, &is_host)
}

/// The leaf at `dir`, read and classified. `Ok(None)` when `dir` declares no
/// deployment.
pub fn entity_source_of(dir: &Path, image: Option<&str>) -> eyre::Result<Option<EntitySource>> {
    let system = match image {
        Some(id) => nros_orchestration_ir::leaf_system::read_image(dir, id),
        None => nros_orchestration_ir::leaf_system::read(dir),
    }
    .map_err(|e| eyre::eyre!(e))?;
    Ok(system.map(|s| entity_source(dir, &s)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn leaf(files: &[(&str, &str)]) -> tempfile::TempDir {
        let d = tempfile::tempdir().unwrap();
        for (name, body) in files {
            std::fs::write(d.path().join(name), body).unwrap();
        }
        d
    }

    const HEAD: &str = "[system]\nname = \"t\"\n[image.a]\nboard = \"zephyr\"\n";

    fn classify(files: &[(&str, &str)], host: Option<bool>) -> EntitySource {
        let d = leaf(files);
        let s = nros_orchestration_ir::leaf_system::read(d.path())
            .unwrap()
            .unwrap();
        entity_source_with(d.path(), &s, &|_| host)
    }

    fn system(component: &str) -> String {
        format!("{HEAD}[[component]]\nname = \"n\"\n{component}")
    }

    #[test]
    fn a_cpp_cross_leaf_that_states_nothing_is_counted_by_its_census() {
        let s = system("");
        assert_eq!(
            classify(&[("CMakeLists.txt", ""), ("system.toml", &s)], Some(false)),
            EntitySource::Census { explicit: false }
        );
        // An unknown board is no reason to be loose.
        assert_eq!(
            classify(&[("CMakeLists.txt", ""), ("system.toml", &s)], None),
            EntitySource::Census { explicit: false }
        );
    }

    #[test]
    fn what_a_leaf_writes_wins_over_the_default() {
        let cm = ("CMakeLists.txt", "");
        let declared = system("entities = [\"timer\"]\n");
        assert_eq!(
            classify(&[cm, ("system.toml", &declared)], Some(false)),
            EntitySource::Declared
        );
        let census = system("entities = \"census\"\n");
        // Written, so it holds even for a host board, as it did before.
        assert_eq!(
            classify(&[cm, ("system.toml", &census)], Some(true)),
            EntitySource::Census { explicit: true }
        );
        let defaults = system("entities = \"defaults\"\n");
        assert_eq!(
            classify(&[cm, ("system.toml", &defaults)], Some(false)),
            EntitySource::Defaults
        );
    }

    #[test]
    fn rust_host_and_component_less_leaves_keep_no_default() {
        let s = system("");
        // A Rust leaf: the probe counts it.
        assert_eq!(
            classify(
                &[
                    ("CMakeLists.txt", ""),
                    ("Cargo.toml", "[package]\nname = \"t\"\n"),
                    ("system.toml", &s)
                ],
                Some(false)
            ),
            EntitySource::Unstated
        );
        // A host image: the census producer's own platform.
        assert_eq!(
            classify(&[("CMakeLists.txt", ""), ("system.toml", &s)], Some(true)),
            EntitySource::Unstated
        );
        // No component: no program for a census to be of.
        assert_eq!(
            classify(
                &[("CMakeLists.txt", ""), ("system.toml", HEAD)],
                Some(false)
            ),
            EntitySource::Unstated
        );
    }
}
