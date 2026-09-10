//! Where the nano-ros SDK root is — one ladder, four rungs (RFC-0099 D3,
//! phase-447 A2).
//!
//! The SDK root is the directory a BUILD reads from: `cmake/` (the modules a
//! workspace includes), `config/` and `packages/` (the board descriptors and
//! the runtime crates a user's project compiles against). Until this module
//! there was no such concept — every site resolved "a nano-ros CHECKOUT", three
//! ways, in three separately-written `or_else` chains:
//!
//! ```text
//! --nano-ros-path / -DNANO_ROS_ROOT   ->  $NROS_REPO_DIR  ->  a walk-up
//! ```
//!
//! All three are a checkout, so the released `nros` could resolve none of them
//! and `nros build` bailed with "no nano-ros checkout found". That is RFC-0099
//! D1: the journey `install.sh` -> `nros setup` -> `nros new` -> `nros build`
//! dead-ends one step from the end, and it breaks nobody today only because
//! every contributor has `$NROS_REPO_DIR` from `activate.sh`.
//!
//! ## The fourth rung, and why it is LAST
//!
//! A release now carries its own SDK root at `<prefix>/share/nano-ros/`
//! (phase-447 A1), so a toolchain can answer the question out of itself.
//!
//! It is consulted **last**, and that is a decision rather than an ordering
//! detail. The ownership guard ([`crate::stale_guard`], phase-431 W1) requires
//! that an `nros` running inside a checkout be that checkout's own build,
//! because a foreign binary emits with emitters nobody in that tree can see. A
//! store rung placed EARLIER would answer with a store copy of the very tree a
//! contributor is editing — the same class of silent substitution, one layer
//! down, and it would be invisible because a store copy compiles perfectly
//! well. Placed last it changes no existing behaviour: it is reached only when
//! the three checkout rungs found nothing, which today is the case that has no
//! answer at all.
//!
//! ## One ladder, spelled once
//!
//! Five call sites carried the three-rung chain by hand (`cmd::build`,
//! `cmd::board_facts`, `orchestration::planner`, and two in `cmd::ws`). Adding
//! a fourth spelling at one of them is how the sizes-header mirror was fixed
//! five times, so the chain moved here and the sites ask. `nros sdk-root` is
//! the same ladder exposed for cmake and shell — the bridge shape
//! `nros sdk-path` and `nros model-path` already use, for the same reason: the
//! derivation lives once and everyone else asks rather than re-deriving.
//!
//! ## Where the pieces live
//!
//! The RUNG itself is in [`nros_launcher::checkout`], not here, and that is
//! layering rather than accident: the launcher already owns
//! [`MONOREPO_MARKER`](nros_launcher::checkout::MONOREPO_MARKER) — the tree's
//! one answer to "is this a nano-ros root" — and `cargo-nano-ros` needs the
//! same rung for the bundled `.msg` share dirs while sitting BELOW this crate.
//! Putting it in the shared leaf is what stops `<prefix>/share/nano-ros` being
//! derived in two places. What lives HERE is the ladder: the order, and what to
//! say when no rung answers.
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

/// The four rungs, as data — so [`resolve_from`] is a pure function and the
/// ORDER can be tested without touching the environment or `current_exe`.
#[derive(Debug, Default)]
pub struct Rungs<'a> {
    /// `--nano-ros-path` / `-DNANO_ROS_ROOT`, whatever the caller was told.
    pub explicit: Option<PathBuf>,
    /// `$NROS_REPO_DIR`, as `activate.sh` exports it.
    pub repo_dir: Option<PathBuf>,
    /// Where the walk-up starts — the workspace root.
    pub workspace: Option<&'a Path>,
    /// `current_exe()`, for the shipped rung.
    pub exe: Option<PathBuf>,
}

/// Walk the ladder. `None` means no rung answered.
#[must_use]
pub fn resolve_from(rungs: Rungs<'_>) -> Option<PathBuf> {
    rungs
        .explicit
        .or(rungs.repo_dir)
        .or_else(|| {
            rungs
                .workspace
                .and_then(crate::cmd::ws::autodetect_nano_ros_path)
        })
        // LAST. See the module header — earlier would redirect a contributor to
        // a store copy of the tree they are editing.
        .or_else(|| rungs.exe.as_deref().and_then(shipped_beside))
}

/// The ladder with the process environment read for it, once, here.
///
/// The shape every call site used to spell by hand, plus the rung a released
/// toolchain can actually reach.
#[must_use]
pub fn resolve(explicit: Option<PathBuf>, workspace: &Path) -> Option<PathBuf> {
    resolve_from(Rungs {
        explicit,
        repo_dir: std::env::var_os("NROS_REPO_DIR").map(PathBuf::from),
        workspace: Some(workspace),
        exe: std::env::current_exe()
            .ok()
            .map(|e| e.canonicalize().unwrap_or(e)),
    })
}

/// What to tell someone when no rung answered.
///
/// Names all four, because before A2 it named two and the reader had no way to
/// learn that an installed toolchain is supposed to answer this by itself.
#[must_use]
pub fn not_found_help() -> String {
    "no nano-ros SDK root found, so board ids cannot be resolved. Resolved in \
     order: --nano-ros-path, $NROS_REPO_DIR, a walk-up from the workspace, then \
     this toolchain's own share/nano-ros (present in a released `nros`, absent \
     from a `cargo build` of the CLI). Inside a checkout, `source ./activate.sh`."
        .to_string()
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

    /// A1 — an `nros` installed from a release answers out of its own asset.
    /// This is the arm that has no answer at all before phase-447.
    #[test]
    fn an_installed_toolchain_resolves_its_own_shipped_root() {
        let tmp = tempfile::tempdir().unwrap();
        let exe = staged_prefix(tmp.path());
        let elsewhere = tmp.path().join("some/users/project");
        std::fs::create_dir_all(&elsewhere).unwrap();

        let got = resolve_from(Rungs {
            workspace: Some(&elsewhere),
            exe: Some(exe),
            ..Rungs::default()
        });
        assert_eq!(got, Some(tmp.path().join(SHIPPED_SUBDIR)));
    }

    /// The OTHER arm of the guard: a contributor inside a checkout still
    /// resolves to their tree even though a shipped root is also reachable.
    ///
    /// This is the test the LAST placement exists for. With the rung moved
    /// first it goes red; with the rung absent entirely it stays green, which
    /// is why the test above is its pair rather than its substitute.
    #[test]
    fn a_checkout_still_wins_over_a_reachable_shipped_root() {
        let tmp = tempfile::tempdir().unwrap();
        let exe = staged_prefix(tmp.path());

        // A checkout, and a workspace nested inside it.
        let checkout = tmp.path().join("checkout");
        std::fs::create_dir_all(checkout.join("packages/core/nros-core")).unwrap();
        std::fs::write(checkout.join(MARKER), "").unwrap();
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

        // …and so must the two rungs above it.
        let got = resolve_from(Rungs {
            repo_dir: Some(checkout.clone()),
            workspace: Some(tmp.path()),
            exe: Some(exe.clone()),
            ..Rungs::default()
        });
        assert_eq!(
            got,
            Some(checkout.clone()),
            "$NROS_REPO_DIR must outrank the store"
        );

        let got = resolve_from(Rungs {
            explicit: Some(checkout.clone()),
            repo_dir: Some(tmp.path().to_path_buf()),
            workspace: Some(tmp.path()),
            exe: Some(exe),
            ..Rungs::default()
        });
        assert_eq!(
            got,
            Some(checkout),
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
}
