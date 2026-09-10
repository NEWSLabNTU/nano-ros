//! Is a path inside a nano-ros checkout?
//!
//! MOVED from `nros_cli_core::abi_guard` (which re-exports it, so there is one
//! spelling). It has to live in the launcher crate because the launcher's very
//! first question is this one: RFC-0095 D0/D5 give the contributor audience a
//! different answer from the user audience, and RFC-0097 states it for the
//! launcher directly — *"inside a checkout the launcher is not involved: the
//! ownership guard requires that clone's own build and dispatch declines at its
//! own seam."*

use std::path::{Path, PathBuf};

/// The file whose presence marks a nano-ros source tree.
pub const MONOREPO_MARKER: &str = "packages/core/nros-core/Cargo.toml";

/// Walk up from `start` to find the nano-ros source-tree root — the directory
/// containing [`MONOREPO_MARKER`]. Returns `None` when `start` is not inside
/// such a tree.
#[must_use]
pub fn find_monorepo_root(start: &Path) -> Option<PathBuf> {
    let mut cur: Option<&Path> = if start.is_file() {
        start.parent()
    } else {
        Some(start)
    };
    while let Some(dir) = cur {
        if dir.join(MONOREPO_MARKER).is_file() {
            return Some(dir.to_path_buf());
        }
        cur = dir.parent();
    }
    None
}
