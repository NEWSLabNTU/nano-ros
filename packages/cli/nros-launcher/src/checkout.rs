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
        if is_monorepo_root(dir) {
            return Some(dir.to_path_buf());
        }
        cur = dir.parent();
    }
    None
}

/// Does `dir` itself carry [`MONOREPO_MARKER`]?
///
/// The walk-up's per-step predicate, named because [`shipped_sdk_root_beside`]
/// asks the same question about ONE directory rather than a chain — and a
/// second `join(MARKER).is_file()` written out somewhere else is how a tree
/// ends up with two answers to "is this a nano-ros root".
#[must_use]
pub fn is_monorepo_root(dir: &Path) -> bool {
    dir.join(MONOREPO_MARKER).is_file()
}

/// Where a release stages its SDK root, relative to the install prefix —
/// phase-447 A1, RFC-0099 D2.
///
/// `share/nano-ros`, beside `share/nros`. The two are deliberately different
/// directories: `share/nros/` is about the TOOLCHAIN (its manifest, its
/// `VERSION`, the index it was released with, the installer), while
/// `share/nano-ros/` is the project a build compiles against, and it uses the
/// project's prose spelling because that is what `NANO_ROS_ROOT` has always
/// named. `scripts/stage-sdk-root.sh` writes it; this constant is what reads it.
pub const SHIPPED_SUBDIR: &str = "share/nano-ros";

/// The SDK root shipped inside the release asset an `exe` belongs to, if any.
///
/// A pure function of the path, so its callers' tests need no `current_exe`.
/// `None` for a `cargo build` of the CLI — `target/release/nros` has no
/// `share/nano-ros` two levels up, and inventing one would be worse than
/// answering nothing.
#[must_use]
pub fn shipped_sdk_root_beside(exe: &Path) -> Option<PathBuf> {
    let prefix = exe.parent()?.parent()?; // <prefix>/bin/nros -> <prefix>
    let candidate = prefix.join(SHIPPED_SUBDIR);
    is_monorepo_root(&candidate).then_some(candidate)
}

/// [`shipped_sdk_root_beside`] for the running executable.
///
/// Resolved from the binary rather than from `$NROS_HOME`, exactly as the
/// shipped SDK index is: two versions in the store each answer with their own,
/// which is what makes them independently installable. `$NROS_HOME/bin/nros` is
/// a symlink into the store, and this canonicalizes, so the answer is the
/// prefix the binary really lives in rather than the front's parent.
#[must_use]
pub fn shipped_sdk_root() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let exe = exe.canonicalize().unwrap_or(exe);
    shipped_sdk_root_beside(&exe)
}
