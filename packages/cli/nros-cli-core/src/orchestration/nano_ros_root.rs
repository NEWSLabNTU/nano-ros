//! Where the nano-ros SDK root is — one ladder, three rungs (RFC-0099 D3,
//! phase-447 A2; RFC-0103 D6 (amended 2026-10-12)).
//!
//! The SDK root is the directory a BUILD reads from: `cmake/` (the modules a
//! workspace includes), `config/` and `packages/` (the board descriptors and
//! the runtime crates a user's project compiles against). The ladder:
//!
//! ```text
//! --nano-ros-path / -DNROS_REPO_DIR  ->  a walk-up from the workspace
//!                                    ->  the binary's own SDK root
//! ```
//!
//! ## The binary's own root, and why nothing is exported any more
//!
//! RFC-0103 D6 (amended 2026-10-12): the `nros` binary knows its own SDK root,
//! so contributors and users follow the same process and nobody exports one.
//! A RELEASED `nros` reads `<prefix>/share/nano-ros` beside it (phase-447 A1);
//! an IN-TREE `nros` (`<checkout>/packages/cli/target/release/nros`) reads the
//! checkout it was built in. Both answers live in ONE function,
//! [`nros_launcher::checkout::own_sdk_root_of`].
//!
//! The `$NROS_REPO_DIR` rung that used to sit between the walk-up and this one
//! is GONE. It was exported by `activate.sh` and inherited by every linked
//! worktree, so it named the parent checkout in a session working in a child
//! (issue 1510 moved it below the walk-up for exactly that reason). An exported
//! variable cannot distinguish intent from inheritance; `--nano-ros-path` can,
//! and the binary's own location needs no export at all.
//!
//! ## Why the own-root rung is LAST
//!
//! The ownership guard ([`crate::stale_guard`], phase-431 W1) requires that an
//! `nros` running inside a checkout be that checkout's own build, because a
//! foreign binary emits with emitters nobody in that tree can see. The own-root
//! rung placed EARLIER would answer a workspace inside checkout B with the root
//! of checkout A (or a store copy) because that is where the binary happened to
//! come from — the same class of silent substitution, one layer down, and it
//! would be invisible because a store copy compiles perfectly well. Placed last
//! it answers only when the workspace sits in no checkout at all: a copy-out
//! project, which is exactly the case that used to need an export.
//!
//! ## One ladder, spelled once
//!
//! Five call sites carried the chain by hand (`cmd::build`,
//! `cmd::board_facts`, `orchestration::planner`, and two in `cmd::ws`). Adding
//! a spelling at one of them is how the sizes-header mirror was fixed five
//! times, so the chain lives here and the sites ask. `nros sdk-root` is the
//! same ladder exposed for cmake and shell — the bridge shape `nros sdk-path`
//! and `nros model-path` already use, for the same reason: the derivation
//! lives once and everyone else asks rather than re-deriving.
//!
//! ## Where the pieces live
//!
//! The own-root RUNG is in [`nros_launcher::checkout`], not here, and that is
//! layering rather than accident: the launcher already owns
//! [`MONOREPO_MARKER`](nros_launcher::checkout::MONOREPO_MARKER) — the tree's
//! one answer to "is this a nano-ros root" — and crates BELOW this one
//! (`cargo-nano-ros`, `rosidl-bindgen`) need the same answer. Putting it in the
//! shared leaf is what stops it being derived in two places. What lives HERE is
//! the ladder: the order, and what to say when no rung answers.
//!
//! A shipped SDK root satisfies the marker because `packages/` is staged whole
//! (`scripts/stage-sdk-root.sh`) — the asset is not a different SHAPE of root,
//! it IS a root, which is what lets every consumer downstream of this module
//! stay unchanged.

use std::path::{Path, PathBuf};

pub use nros_launcher::checkout::{
    SHIPPED_SUBDIR, is_monorepo_root as is_root, shipped_sdk_root as shipped,
    shipped_sdk_root_beside as shipped_beside,
};

/// The ladder's inputs, as data — so [`resolve_from`] is a pure function and
/// the ORDER can be tested without touching `current_exe`.
#[derive(Debug, Default)]
pub struct Rungs<'a> {
    /// `--nano-ros-path` / `-DNROS_REPO_DIR`, whatever the caller was told.
    pub explicit: Option<PathBuf>,
    /// Where the walk-up starts — the workspace root.
    pub workspace: Option<&'a Path>,
    /// `current_exe()`, canonicalized, for the own-root rung.
    pub exe: Option<PathBuf>,
    /// The CLI's compile-time source dir, the own-root rung's fallback for a
    /// binary whose target dir is outside its checkout
    /// ([`nros_launcher::checkout::built_in_dir`]).
    pub built_in: Option<PathBuf>,
}

/// Which rung answered — what `nros sdk-root --explain` names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rung {
    /// `--nano-ros-path` / `-DNROS_REPO_DIR`.
    Explicit,
    /// The checkout enclosing the workspace.
    Workspace,
    /// A released binary's own `<prefix>/share/nano-ros`.
    OwnShipped,
    /// The checkout an in-tree binary was built in.
    OwnCheckout,
}

impl Rung {
    /// The phrase `nros sdk-root --explain` prints after "via".
    #[must_use]
    pub fn describe(self) -> &'static str {
        match self {
            Rung::Explicit => "--nano-ros-path",
            Rung::Workspace => "the nano-ros checkout enclosing the workspace",
            Rung::OwnShipped => "this toolchain's own share/nano-ros",
            Rung::OwnCheckout => "the nano-ros checkout this `nros` was built in",
        }
    }
}

/// Walk the ladder, and say which rung answered. `None` means none did.
#[must_use]
pub fn resolve_explained_from(rungs: Rungs<'_>) -> Option<(PathBuf, Rung)> {
    if let Some(p) = rungs.explicit {
        return Some((p, Rung::Explicit));
    }
    if let Some(p) = rungs
        .workspace
        .and_then(crate::cmd::ws::autodetect_nano_ros_path)
    {
        return Some((p, Rung::Workspace));
    }
    // LAST. See the module header — earlier would answer a workspace in one
    // checkout with the root of whichever tree the binary came from.
    let exe = rungs.exe.as_deref()?;
    if let Some(p) = shipped_beside(exe) {
        return Some((p, Rung::OwnShipped));
    }
    nros_launcher::checkout::own_checkout_of(exe, rungs.built_in.as_deref())
        .map(|p| (p, Rung::OwnCheckout))
}

/// [`resolve_explained_from`] without the attribution.
#[must_use]
pub fn resolve_from(rungs: Rungs<'_>) -> Option<PathBuf> {
    resolve_explained_from(rungs).map(|(p, _)| p)
}

/// The ladder for the running binary, with its rung.
#[must_use]
pub fn resolve_explained(explicit: Option<PathBuf>, workspace: &Path) -> Option<(PathBuf, Rung)> {
    resolve_explained_from(Rungs {
        explicit,
        workspace: Some(workspace),
        exe: nros_launcher::checkout::canonical_current_exe(),
        built_in: Some(nros_launcher::checkout::built_in_dir().to_path_buf()),
    })
}

/// The ladder for the running binary.
#[must_use]
pub fn resolve(explicit: Option<PathBuf>, workspace: &Path) -> Option<PathBuf> {
    resolve_explained(explicit, workspace).map(|(p, _)| p)
}

/// What to tell someone when no rung answered.
///
/// Names every rung, because a reader cannot otherwise learn that the binary
/// is supposed to answer this by itself.
#[must_use]
pub fn not_found_help() -> String {
    "no nano-ros SDK root found, so board ids cannot be resolved. Resolved in \
     order: --nano-ros-path, a walk-up from the workspace, then this `nros` \
     binary's own SDK root (share/nano-ros beside a released `nros`, or the \
     checkout an in-tree `nros` was built in). Pass --nano-ros-path <root> to \
     name one explicitly."
        .to_string()
}

/// Is this nano-ros root a CHECKOUT (writable pin set, gitignored lock) rather
/// than an INSTALLED SDK (the release's read-only pins, a project-owned lock)?
///
/// NOT `CHECKOUT_MARKER`: that file marks any nano-ros ROOT, and an installed
/// SDK root ships it too (`stage-sdk-root.sh`: "the root MARKER"). What marks a
/// root INSTALLED is the absence of a `.git` (a directory, or a linked
/// worktree's file) — the rule `stage-sdk-root.sh` and
/// `sdk_store::recorded_pin` already state. phase-485 W10 found the marker
/// answering "checkout" for a staged SDK.
#[must_use]
pub fn is_checkout(root: &Path) -> bool {
    root.join(".git").exists()
}

#[cfg(test)]
mod tests {
    use super::*;

    const MARKER: &str = nros_launcher::checkout::MONOREPO_MARKER;

    /// A prefix that looks like an installed release: `<p>/bin/nros` beside
    /// `<p>/share/nano-ros/<MARKER>`.
    fn staged_prefix(dir: &Path) -> PathBuf {
        let root = dir.join(SHIPPED_SUBDIR);
        std::fs::create_dir_all(root.join("packages/core/nros-core")).unwrap();
        std::fs::write(root.join(MARKER), "[package]\nname = \"nros-core\"\n").unwrap();
        std::fs::create_dir_all(dir.join("bin")).unwrap();
        let exe = dir.join("bin/nros");
        std::fs::write(&exe, "").unwrap();
        exe
    }

    fn fake_checkout(dir: &Path) -> PathBuf {
        std::fs::create_dir_all(dir.join("packages/core/nros-core")).unwrap();
        std::fs::write(dir.join(MARKER), "").unwrap();
        dir.to_path_buf()
    }

    /// A1 — an `nros` installed from a release answers out of its own asset.
    #[test]
    fn an_installed_toolchain_resolves_its_own_shipped_root() {
        let tmp = tempfile::tempdir().unwrap();
        let exe = staged_prefix(tmp.path());
        let elsewhere = tmp.path().join("some/users/project");
        std::fs::create_dir_all(&elsewhere).unwrap();

        let got = resolve_explained_from(Rungs {
            workspace: Some(&elsewhere),
            exe: Some(exe),
            ..Rungs::default()
        });
        assert_eq!(
            got,
            Some((tmp.path().join(SHIPPED_SUBDIR), Rung::OwnShipped))
        );
    }

    /// RFC-0103 D6 (amended) — an IN-TREE `nros` run from a project outside
    /// every checkout answers with the checkout it was built in. This is the
    /// case `$NROS_REPO_DIR` used to be exported for.
    #[test]
    fn an_in_tree_binary_answers_with_its_own_checkout() {
        let tmp = tempfile::tempdir().unwrap();
        let checkout = fake_checkout(&tmp.path().join("checkout"));
        let exe = checkout.join("packages/cli/target/release/nros");
        std::fs::create_dir_all(exe.parent().unwrap()).unwrap();
        std::fs::write(&exe, "").unwrap();
        let project = tmp.path().join("copy-out/project");
        std::fs::create_dir_all(&project).unwrap();

        let got = resolve_explained_from(Rungs {
            workspace: Some(&project),
            exe: Some(exe),
            ..Rungs::default()
        });
        assert_eq!(got, Some((checkout, Rung::OwnCheckout)));
    }

    /// …and the workspace walk-up still OUTRANKS the binary's own checkout: a
    /// binary built in checkout A, run in a workspace inside checkout B,
    /// answers B. The nested shape (`<main>/.claude/worktrees/<id>`) is the one
    /// a prefix test cannot separate (issue 1391), and the one issue 1510 hit.
    #[test]
    fn the_workspace_checkout_outranks_the_binarys_own() {
        let tmp = tempfile::tempdir().unwrap();
        let parent = fake_checkout(&tmp.path().join("parent"));
        let worktree = fake_checkout(&parent.join(".claude/worktrees/agent-x"));
        let exe = parent.join("packages/cli/target/release/nros");
        std::fs::create_dir_all(exe.parent().unwrap()).unwrap();
        std::fs::write(&exe, "").unwrap();
        let ws = worktree.join("examples/workspaces/mixed");
        std::fs::create_dir_all(&ws).unwrap();

        let got = resolve_explained_from(Rungs {
            workspace: Some(&ws),
            exe: Some(exe),
            built_in: Some(parent.join("packages/cli/nros-launcher")),
            ..Rungs::default()
        });
        assert_eq!(
            got,
            Some((worktree, Rung::Workspace)),
            "the workspace's own checkout must outrank the binary's"
        );
    }

    /// A contributor inside a checkout still resolves to their tree even
    /// though a shipped root is also reachable — the test the LAST placement
    /// exists for. And `--nano-ros-path` outranks everything.
    #[test]
    fn a_checkout_still_wins_over_a_reachable_shipped_root() {
        let tmp = tempfile::tempdir().unwrap();
        let exe = staged_prefix(tmp.path());
        let checkout = fake_checkout(&tmp.path().join("checkout"));
        let ws = checkout.join("examples/workspaces/mixed");
        std::fs::create_dir_all(&ws).unwrap();

        let got = resolve_from(Rungs {
            workspace: Some(&ws),
            exe: Some(exe.clone()),
            ..Rungs::default()
        });
        assert_eq!(
            got,
            Some(checkout.clone()),
            "the walk-up must outrank the store"
        );

        let got = resolve_explained_from(Rungs {
            explicit: Some(checkout.clone()),
            workspace: Some(tmp.path()),
            exe: Some(exe),
            built_in: None,
        });
        assert_eq!(
            got,
            Some((checkout, Rung::Explicit)),
            "--nano-ros-path must outrank everything"
        );
    }

    /// A `cargo build` of the CLI is not an install: `target/release/nros` has
    /// no `share/nano-ros` two levels up, and inventing one would be worse than
    /// answering nothing.
    #[test]
    fn a_binary_with_no_staged_root_answers_nothing() {
        let tmp = tempfile::tempdir().unwrap();
        let exe = tmp.path().join("target/release/nros");
        std::fs::create_dir_all(exe.parent().unwrap()).unwrap();
        std::fs::write(&exe, "").unwrap();
        assert_eq!(shipped_beside(&exe), None);

        let empty = tmp.path().join("empty");
        std::fs::create_dir_all(&empty).unwrap();
        assert_eq!(
            resolve_from(Rungs {
                workspace: Some(&empty),
                exe: Some(exe),
                ..Rungs::default()
            }),
            None
        );
    }

    /// A directory at the right PATH that is not a root is not an answer —
    /// the marker is checked, not the directory's existence. A half-unpacked
    /// asset would otherwise resolve and fail much later, naming nothing.
    #[test]
    fn a_staged_directory_without_the_marker_is_not_a_root() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join(SHIPPED_SUBDIR).join("cmake")).unwrap();
        std::fs::create_dir_all(tmp.path().join("bin")).unwrap();
        let exe = tmp.path().join("bin/nros");
        std::fs::write(&exe, "").unwrap();
        assert_eq!(shipped_beside(&exe), None);
    }

    /// The help text names the explicit override and no longer points at a
    /// variable nothing reads.
    #[test]
    fn not_found_help_names_the_flag_not_the_retired_variable() {
        let h = not_found_help();
        assert!(h.contains("--nano-ros-path"), "{h}");
        assert!(!h.contains("NROS_REPO_DIR"), "{h}");
    }
}
