//! RFC-0071 D14 (phase-545 W1) — where a CLI site gets its [`RmwIndex`].
//!
//! Every site that resolves an RMW name takes an [`RmwIndex`] as a parameter
//! and asks it; none reads the table `cargo-nano-ros/build.rs` bakes. This
//! module is the one place an index is BUILT from a path, so the sites that
//! cannot yet be handed one by their caller have a single adapter to call.
//!
//! The deferred half of W1 is that `nros build` and `nros configure` compute
//! the scan ONCE and pass it down. Their files are being rewritten by
//! phase-530, so until phase-530 W2 lands the sites they reach build their own
//! index here, at the points marked
//! `// phase-545 W1: threaded from build/configure after phase-530 W2`.
//! The process-wide memo below keeps that interim cost to one scan per
//! workspace per invocation, which is what the threaded form will give.
//!
//! The search path is the workspace's provider search path with root 0
//! narrowed to `packages/rmw/` ([`provider_scan::InTreeScope::Rmw`]). A caller
//! with no workspace scans the SDK root alone ([`for_sdk_root`]).

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, OnceLock},
};

use cargo_nano_ros::provider_scan;
pub use cargo_nano_ros::rmw_resolver::{ResolvedRmwIn, RmwIndex, RmwLookupError};
use eyre::{Result, WrapErr};

/// The index over `workspace`'s RMW search path: the SDK's `packages/rmw/`,
/// then the workspace's source roots, then `NROS_PACKAGE_PATH`.
///
/// Memoised per canonical workspace path for the life of the process, so the
/// sites that each build one (until the deferred step threads it) share one
/// scan.
///
/// # Errors
///
/// An unreadable `nros.toml`, or a root that cannot be walked.
pub fn for_workspace(workspace: &Path) -> Result<Arc<RmwIndex>> {
    let key = workspace
        .canonicalize()
        .unwrap_or_else(|_| workspace.to_path_buf());
    memoised(Scope::Workspace, key, build_for_workspace)
}

/// The index over the SDK root `root` alone, memoised like [`for_workspace`].
///
/// The answer for a directory that sits in no workspace ([`for_dir`]): it
/// resolves every in-tree backend, and an out-of-tree one is refused with the
/// search named, never guessed from the process's current directory.
///
/// # Errors
///
/// The root cannot be walked.
pub fn for_sdk_root_path(root: &Path) -> Result<Arc<RmwIndex>> {
    let key = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    memoised(Scope::SdkRoot, key, RmwIndex::over_sdk_root)
}

/// [`for_workspace`] for the workspace `dir` sits in (a bringup, an entry, a
/// node package), or `dir` itself when it holds a `system.toml` (a
/// single-package leaf is its own workspace). Anywhere else there is no
/// workspace to scan, so the index is the SDK root's alone: a directory that
/// merely contains the caller (a home directory, `/tmp`) is never walked.
///
/// # Errors
///
/// As [`for_workspace`], or no SDK root resolves when there is no workspace.
pub fn for_dir(dir: &Path) -> Result<Arc<RmwIndex>> {
    if let Some(ws) = crate::cmd::image_facts::workspace_root_from(dir) {
        return for_workspace(&ws);
    }
    if dir.join("system.toml").is_file() {
        return for_workspace(dir);
    }
    let root = super::nano_ros_root::resolve(None, dir)
        .ok_or_else(|| eyre::eyre!("{}", super::nano_ros_root::not_found_help()))?;
    for_sdk_root_path(&root)
}

/// The index for a caller with NO workspace (`nros new`, help text): the SDK
/// root alone, narrowed to its `packages/rmw/` (RFC-0071 D14).
///
/// `cwd` is where the SDK-root ladder walks up from. No SDK root means no
/// backend sources either, so this fails with
/// [`super::nano_ros_root::not_found_help`] rather than answering from a table.
///
/// # Errors
///
/// No SDK root resolves, or it cannot be walked.
pub fn for_sdk_root(cwd: &Path) -> Result<RmwIndex> {
    let root = super::nano_ros_root::resolve(None, cwd)
        .ok_or_else(|| eyre::eyre!("{}", super::nano_ros_root::not_found_help()))?;
    RmwIndex::over_sdk_root(&root)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Scope {
    Workspace,
    SdkRoot,
}

type Memo = Mutex<HashMap<(Scope, PathBuf), Arc<RmwIndex>>>;

static MEMO: OnceLock<Memo> = OnceLock::new();

fn memoised(
    scope: Scope,
    key: PathBuf,
    build: impl FnOnce(&Path) -> Result<RmwIndex>,
) -> Result<Arc<RmwIndex>> {
    let memo = MEMO.get_or_init(Default::default);
    if let Some(hit) = memo
        .lock()
        .expect("rmw index memo")
        .get(&(scope, key.clone()))
    {
        return Ok(Arc::clone(hit));
    }
    let index = Arc::new(build(&key)?);
    memo.lock()
        .expect("rmw index memo")
        .insert((scope, key), Arc::clone(&index));
    Ok(index)
}

fn build_for_workspace(workspace: &Path) -> Result<RmwIndex> {
    let path = crate::cmd::ws::rmw_search_path_for(workspace, &[])?;
    let scan = provider_scan::scan_search_path(&path)
        .wrap_err_with(|| format!("scanning rmw providers for {}", workspace.display()))?;
    for e in &scan.errors {
        eprintln!("warning: {}: {}", e.path.display(), e.message);
    }
    Ok(RmwIndex::new(scan, path.paths()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_workspace_provider_is_found_beside_the_in_tree_backends() {
        let tmp = tempfile::tempdir().unwrap();
        cargo_nano_ros::rmw_resolver::test_support::write_acme_provider(
            &tmp.path().join("src/acme_rmw"),
        );
        let idx = for_workspace(tmp.path()).unwrap();
        assert_eq!(idx.resolve("acme").unwrap().package, "acme_rmw");
        assert_eq!(idx.resolve("zenoh").unwrap().declared, "zenoh");
        // Root 0 is the SDK's `packages/rmw/`, not the whole checkout.
        assert!(
            idx.roots()[0].ends_with("packages/rmw"),
            "{:?}",
            idx.roots()
        );
    }

    #[test]
    fn the_sdk_root_alone_answers_the_in_tree_backends_only() {
        let idx = for_sdk_root(Path::new(env!("CARGO_MANIFEST_DIR"))).unwrap();
        assert!(idx.resolve("cyclonedds").is_ok());
        let e = idx.resolve("acme").unwrap_err().to_string();
        assert!(e.contains("acme") && e.contains("searched"), "{e}");
    }
}
