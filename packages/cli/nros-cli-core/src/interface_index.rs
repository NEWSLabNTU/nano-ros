//! The interface types a workspace reaches, one reader for `nros interface`
//! and for the completion cache (phase-569 W4d, RFC-0133 D7).
//!
//! The search path is the one codegen resolves through
//! (`cargo_nano_ros::load_index_with_fallback`): `NROS_INTERFACE_SEARCH_PATH`,
//! then the ament environment, then the bundled interfaces. The workspace's
//! own `src/` (and its root, for a single-package leaf) is searched first, so
//! a type the user is editing shadows an installed one of the same name, as it
//! does in a build.
//!
//! It lives outside `cmd::interface` because `nros configure` writes the type
//! names into the completion cache, and reaching a leaf verb from configure
//! would put that verb in the code every build runs.

use std::path::Path;

use eyre::{Result, WrapErr};
use rosidl_bindgen::ament::AmentIndex;

/// The interface index for `workspace` (default: the current directory).
pub fn load(workspace: Option<&Path>) -> Result<AmentIndex> {
    let ws = match workspace {
        Some(w) => w.to_path_buf(),
        None => std::env::current_dir().wrap_err("resolving cwd")?,
    };
    let mut index = AmentIndex::from_path_string("")?;
    let src = ws.join("src");
    for dir in [src.as_path(), ws.as_path()] {
        if dir.is_dir()
            && let Ok(local) = AmentIndex::from_directory(dir)
        {
            index.merge(local);
        }
    }
    index.merge(cargo_nano_ros::load_index_with_fallback(false)?);
    Ok(index)
}

/// Every type `index` reaches, as `<package>/<msg|srv|action>/<Name>`,
/// sorted and without repeats.
pub fn type_names(index: &AmentIndex) -> Vec<String> {
    let mut out = std::collections::BTreeSet::new();
    for p in index.packages().values() {
        for (kind, names) in [
            ("msg", &p.interfaces.messages),
            ("srv", &p.interfaces.services),
            ("action", &p.interfaces.actions),
        ] {
            for n in names {
                out.insert(format!("{}/{kind}/{n}", p.name));
            }
        }
    }
    out.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn type_names_lists_every_kind_once() {
        let dir = tempfile::tempdir().unwrap();
        for (rel, body) in [
            ("a/msg/Inner.msg", "int32 x\n"),
            ("a/srv/Ask.srv", "---\nbool r\n"),
            ("b/action/Go.action", "int32 g\n---\n---\n"),
        ] {
            let p = dir.path().join(rel);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, body).unwrap();
        }
        let index = AmentIndex::from_directory(dir.path()).unwrap();
        assert_eq!(
            type_names(&index),
            ["a/msg/Inner", "a/srv/Ask", "b/action/Go"]
        );
    }
}
