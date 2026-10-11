//! RFC-0104 D3 (phase-485 W2) — a leaf `Cargo.lock` is SEEDED from the
//! registry pin set, never resolved from scratch.
//!
//! The index names the pin set (`[rust] registry_pins`): a registry-only
//! `Cargo.lock` holding one version per semver-compatible range of every
//! registry crate any root in the tree resolves. A leaf with no lock of its own
//! gets a copy of it, and cargo then resolves the leaf against that starting
//! point. Measured in phase-485 M1 and re-checked in W1: cargo KEEPS every
//! seeded version the leaf's manifests accept, PRUNES the rest, and adds the
//! leaf's path crates (generated message crates are `0.0.0`, so they move no
//! registry entry). `nros` decides the starting point; cargo does the
//! resolution (D8) — nothing here parses or edits a resolve.
//!
//! An EXISTING lock is never touched: it is either a previous seed, a lock the
//! user moved on purpose, or a tracked one. Whether it still agrees with the
//! pins is the drift check's question (D9), not this one's.

use std::path::{Path, PathBuf};

use eyre::{Result, WrapErr};

use crate::orchestration::sdk_index::SdkIndex;

/// The index file a nano-ros root carries.
const INDEX_FILE: &str = "nros-sdk-index.toml";

/// What [`seed`] did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Seed {
    /// The leaf already had a `Cargo.lock`; it was left alone.
    Present,
    /// A copy of the pin set was written as the leaf's `Cargo.lock`.
    Seeded { pins: PathBuf },
    /// No pin set to seed from: no nano-ros root, an index that names none, or
    /// a named file that is absent. Cargo resolves fresh, as before RFC-0104.
    NoPinSet,
}

/// The registry pin set a nano-ros root's index names, if it names one that
/// exists. The path is relative to the index's directory.
pub fn pin_file(nano_ros_root: &Path) -> Result<Option<PathBuf>> {
    let index_path = nano_ros_root.join(INDEX_FILE);
    if !index_path.is_file() {
        return Ok(None);
    }
    let index = SdkIndex::load(&index_path)?;
    Ok(index
        .rust
        .registry_pins
        .map(|rel| nano_ros_root.join(rel))
        .filter(|p| p.is_file()))
}

/// Seed `<manifest_dir>/Cargo.lock` from the pin set when the leaf has none.
pub fn seed(manifest_dir: &Path, nano_ros_root: Option<&Path>) -> Result<Seed> {
    let lock = manifest_dir.join("Cargo.lock");
    if lock.exists() {
        return Ok(Seed::Present);
    }
    let Some(root) = nano_ros_root else {
        return Ok(Seed::NoPinSet);
    };
    let Some(pins) = pin_file(root)? else {
        return Ok(Seed::NoPinSet);
    };
    std::fs::copy(&pins, &lock)
        .wrap_err_with(|| format!("seed {} from {}", lock.display(), pins.display()))?;
    Ok(Seed::Seeded { pins })
}

#[cfg(test)]
mod tests {
    use super::*;

    const PINS: &str = "version = 4\n\n[[package]]\nname = \"heapless\"\nversion = \"0.8.0\"\n\
                        source = \"registry+https://github.com/rust-lang/crates.io-index\"\n";

    fn root_with_pins(dir: &Path) {
        std::fs::write(
            dir.join(INDEX_FILE),
            "[rust]\nregistry_pins = \"locks/cargo-registry.lock\"\n",
        )
        .unwrap();
        std::fs::create_dir_all(dir.join("locks")).unwrap();
        std::fs::write(dir.join("locks/cargo-registry.lock"), PINS).unwrap();
    }

    #[test]
    fn a_leaf_with_no_lock_gets_a_copy_of_the_pin_set() {
        let root = tempfile::tempdir().unwrap();
        root_with_pins(root.path());
        let leaf = tempfile::tempdir().unwrap();
        let got = seed(leaf.path(), Some(root.path())).unwrap();
        assert_eq!(
            got,
            Seed::Seeded {
                pins: root.path().join("locks/cargo-registry.lock")
            }
        );
        assert_eq!(
            std::fs::read_to_string(leaf.path().join("Cargo.lock")).unwrap(),
            PINS
        );
    }

    #[test]
    fn an_existing_lock_is_never_reseeded() {
        let root = tempfile::tempdir().unwrap();
        root_with_pins(root.path());
        let leaf = tempfile::tempdir().unwrap();
        std::fs::write(leaf.path().join("Cargo.lock"), "version = 4\n").unwrap();
        assert_eq!(seed(leaf.path(), Some(root.path())).unwrap(), Seed::Present);
        assert_eq!(
            std::fs::read_to_string(leaf.path().join("Cargo.lock")).unwrap(),
            "version = 4\n"
        );
    }

    #[test]
    fn no_pin_set_leaves_the_resolve_to_cargo() {
        let leaf = tempfile::tempdir().unwrap();
        assert_eq!(seed(leaf.path(), None).unwrap(), Seed::NoPinSet);
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join(INDEX_FILE), "").unwrap();
        assert_eq!(
            seed(leaf.path(), Some(root.path())).unwrap(),
            Seed::NoPinSet
        );
        assert!(!leaf.path().join("Cargo.lock").exists());
    }
}
