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

/// The file whose presence marks a nano-ros source tree — the ONE marker,
/// `nros_build_paths::CHECKOUT_MARKER` (RFC-0103 D6), re-exported under the
/// name this crate's callers use.
pub const MONOREPO_MARKER: &str = nros_build_paths::CHECKOUT_MARKER;

/// Walk up from `start` to find the nano-ros source-tree root — the directory
/// containing [`MONOREPO_MARKER`]. Returns `None` when `start` is not inside
/// such a tree.
///
/// A RELATIVE `start` is made absolute against the current directory first
/// (issue 1791). `Path::parent` climbs a path's TEXT, so `"."` has the parent
/// `""` and then none: the walk never reached a real directory, every relative
/// anchor (`nros build --workspace .`) found no checkout, and
/// `abi_guard::runtime_root` fell through to the ambient `NROS_REPO_DIR` (a
/// rung RFC-0103 D6 has since removed) — in a linked worktree, a different
/// checkout. `std::path::absolute` neither needs
/// the path to exist nor resolves symlinks, so the answer is still the tree the
/// caller named.
#[must_use]
pub fn find_monorepo_root(start: &Path) -> Option<PathBuf> {
    let absolute;
    let start = if start.is_absolute() {
        start
    } else {
        absolute = std::path::absolute(start).ok()?;
        absolute.as_path()
    };
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
/// project's prose spelling because that is what the retired `NROS_REPO_DIR`
/// variable used to name. `scripts/stage-sdk-root.sh` writes it; this constant is what reads it.
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
    shipped_sdk_root_beside(&canonical_current_exe()?)
}

/// The SDK root of the CHECKOUT an `exe` was built in, if any — the in-tree
/// half of [`own_sdk_root_of`].
///
/// Two answers, in order:
///
/// 1. walk up from `exe` itself — `<checkout>/packages/cli/target/release/nros`
///    sits inside the checkout it was built from;
/// 2. walk up from `built_in`, the crate's compile-time `CARGO_MANIFEST_DIR`,
///    for a binary whose cargo target dir is OUTSIDE the checkout
///    (`CARGO_TARGET_DIR=/elsewhere`). Taken only while that path still is a
///    nano-ros root: a released binary carries the CI builder's path, which
///    on a user's host names nothing, and a moved or deleted checkout names
///    nothing either.
///
/// `exe` should already be canonical (the store front is a symlink).
#[must_use]
pub fn own_checkout_of(exe: &Path, built_in: Option<&Path>) -> Option<PathBuf> {
    exe.parent()
        .and_then(find_monorepo_root)
        .or_else(|| built_in.and_then(find_monorepo_root))
}

/// The SDK root an `nros` binary at `exe` reads by itself — RFC-0103 D6
/// (amended 2026-10-12): the binary knows its own root, so nobody exports one.
///
/// * a RELEASED binary (`<prefix>/bin/nros`) answers with the SDK root staged
///   beside it, `<prefix>/share/nano-ros` ([`shipped_sdk_root_beside`]);
/// * an IN-TREE binary answers with the checkout it was built in
///   ([`own_checkout_of`]).
///
/// Pure in its inputs — `built_in` is a parameter rather than read here — so a
/// test can build fake directories and get an answer about THEM, not about the
/// checkout the test binary itself was compiled in.
#[must_use]
pub fn own_sdk_root_of(exe: &Path, built_in: Option<&Path>) -> Option<PathBuf> {
    shipped_sdk_root_beside(exe).or_else(|| own_checkout_of(exe, built_in))
}

/// This crate's source directory at compile time — the `built_in` argument of
/// [`own_sdk_root_of`] for the running binary. Every binary linking this crate
/// was compiled from the checkout holding it, so that checkout is the one this
/// path sits in, for as long as it still exists.
#[must_use]
pub fn built_in_dir() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

/// The running executable, canonicalized, so a `$NROS_HOME/bin/nros` symlink
/// answers with the store prefix it points into.
#[must_use]
pub fn canonical_current_exe() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    Some(exe.canonicalize().unwrap_or(exe))
}

/// [`own_sdk_root_of`] for the running executable.
#[must_use]
pub fn own_sdk_root() -> Option<PathBuf> {
    own_sdk_root_of(&canonical_current_exe()?, Some(built_in_dir()))
}

/// [`own_checkout_of`] for the running executable — the in-tree half alone,
/// for a caller that may WRITE into the answer and so must never be handed an
/// installed toolchain's read-only `share/nano-ros`.
#[must_use]
pub fn own_checkout() -> Option<PathBuf> {
    own_checkout_of(&canonical_current_exe()?, Some(built_in_dir()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fake_root(dir: &Path) -> PathBuf {
        let marker = dir.join(MONOREPO_MARKER);
        std::fs::create_dir_all(marker.parent().unwrap()).unwrap();
        std::fs::write(&marker, "").unwrap();
        dir.to_path_buf()
    }

    fn touch(p: &Path) -> PathBuf {
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, "").unwrap();
        p.to_path_buf()
    }

    /// A released binary reads the SDK root staged beside it.
    #[test]
    fn a_shipped_prefix_answers_with_its_share_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let shipped = fake_root(&tmp.path().join(SHIPPED_SUBDIR));
        let exe = touch(&tmp.path().join("bin/nros"));
        assert_eq!(own_sdk_root_of(&exe, None), Some(shipped));
    }

    /// An in-tree binary reads the checkout its target dir sits in.
    #[test]
    fn an_in_tree_binary_answers_with_its_checkout() {
        let tmp = tempfile::tempdir().unwrap();
        let checkout = fake_root(&tmp.path().join("checkout"));
        let exe = touch(&checkout.join("packages/cli/target/release/nros"));
        assert_eq!(own_sdk_root_of(&exe, None), Some(checkout.clone()));
        assert_eq!(own_checkout_of(&exe, None), Some(checkout));
    }

    /// A target dir OUTSIDE the checkout falls back to where the crate was
    /// compiled — while that still is a root, and not otherwise.
    #[test]
    fn an_out_of_tree_target_dir_falls_back_to_the_built_in_checkout() {
        let tmp = tempfile::tempdir().unwrap();
        let checkout = fake_root(&tmp.path().join("checkout"));
        let built_in = checkout.join("packages/cli/nros-launcher");
        std::fs::create_dir_all(&built_in).unwrap();
        let exe = touch(&tmp.path().join("elsewhere/target/release/nros"));
        assert_eq!(
            own_sdk_root_of(&exe, Some(&built_in)),
            Some(checkout),
            "the compile-time checkout answers when the exe is in none"
        );

        let gone = tmp.path().join("deleted/packages/cli/nros-launcher");
        assert_eq!(
            own_sdk_root_of(&exe, Some(&gone)),
            None,
            "a built-in path that is no longer a root is not an answer"
        );
    }

    /// Neither shipped nor in a checkout: no answer, never an invented one.
    #[test]
    fn a_binary_in_neither_answers_nothing() {
        let tmp = tempfile::tempdir().unwrap();
        let exe = touch(&tmp.path().join("opt/tools/bin/nros"));
        assert_eq!(own_sdk_root_of(&exe, None), None);
        assert_eq!(own_checkout_of(&exe, None), None);
    }

    /// A caller asking for the in-tree half alone is never handed the
    /// shipped root.
    #[test]
    fn own_checkout_ignores_a_shipped_root() {
        let tmp = tempfile::tempdir().unwrap();
        fake_root(&tmp.path().join(SHIPPED_SUBDIR));
        let exe = touch(&tmp.path().join("bin/nros"));
        assert_eq!(own_checkout_of(&exe, None), None);
    }
}
