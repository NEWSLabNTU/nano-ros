//! Relative-path arithmetic for generated build files (phase-383 W3.c).
//!
//! Extracted because it was about to have a THIRD copy. `cargo_root` and
//! `cmake_root` each grew one, and the entry emitter needs the same thing —
//! which is exactly the shape CLAUDE.md names as this repository's recurring
//! defect: *"add ONE shared helper rather than a second spelling."* Three
//! copies of a path calculation drift silently, and the symptom is an absolute
//! path in a generated file that only fails on someone else's machine.
//!
//! Every generated file this crate writes must carry relative paths only.
//! Reproducible builds require bit-identical output across machines, and a
//! path under a developer's home directory is the single most common way
//! that fails.

use std::path::{Path, PathBuf};

/// Path from `from` to `to`, using `/` separators, both absolute.
///
/// Returns `None` when the two share no prefix at all — on Unix that means one
/// of them was not absolute. **A caller must treat `None` as fatal, never as a
/// reason to fall back to an absolute path**: that fallback is precisely the
/// bug this module exists to prevent, and it would pass every test on the
/// machine that wrote it.
#[must_use]
pub fn relative(from: &Path, to: &Path) -> Option<String> {
    let f: Vec<_> = from.components().collect();
    let t: Vec<_> = to.components().collect();
    let common = f.iter().zip(t.iter()).take_while(|(a, b)| a == b).count();
    if common == 0 {
        return None;
    }
    let mut parts: Vec<String> = std::iter::repeat_n("..".to_string(), f.len() - common).collect();
    for c in &t[common..] {
        parts.push(c.as_os_str().to_string_lossy().into_owned());
    }
    Some(if parts.is_empty() {
        ".".to_string()
    } else {
        parts.join("/")
    })
}

/// [`relative`], with an error naming both paths and the rule.
///
/// The message matters: an author who sees it is usually holding a path that
/// escaped the workspace, and "cannot express X relative to Y" tells them
/// which two.
pub fn relative_or_err(from: &Path, to: &Path) -> Result<String, String> {
    relative(from, to).ok_or_else(|| {
        format!(
            "cannot express {} relative to {} — a generated file must carry no \
             absolute path (phase-383 W3.c)",
            to.display(),
            from.display()
        )
    })
}

/// Collapse `.` and `..` without touching the filesystem.
///
/// Needed because the entry's path deps are written relative to the ENTRY
/// (`../../../../../packages/api/nros`), and joining that onto the entry dir
/// leaves the `..`s embedded. Cargo reads such a path fine, but [`relative`]
/// compares components to find the common prefix, and an un-collapsed `..`
/// makes every path look divergent — producing a correct-but-absurd
/// `../../../src/linux_entry/../../../../../packages/...`.
#[must_use]
pub fn lexical_normalize(p: &Path) -> PathBuf {
    let mut out: Vec<std::path::Component<'_>> = Vec::new();
    for c in p.components() {
        match c {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                // Only pop a real name; `/..` and a leading `..` stay put.
                if matches!(out.last(), Some(std::path::Component::Normal(_))) {
                    out.pop();
                } else {
                    out.push(c);
                }
            }
            other => out.push(other),
        }
    }
    out.iter().collect()
}

/// Re-express `rel`, written relative to `from_dir`, relative to `to_dir`
/// (issue 2175). Both directories absolute; an absolute `rel` is returned
/// unchanged, since it never depended on where it was written.
pub fn rebase_relative(from_dir: &Path, to_dir: &Path, rel: &str) -> Result<String, String> {
    if Path::new(rel).is_absolute() {
        return Ok(rel.to_string());
    }
    let target = lexical_normalize(&from_dir.join(rel));
    relative_or_err(&lexical_normalize(to_dir), &target)
}

/// The link a generated cargo entry carries to its own workspace (issue 2322).
///
/// cargo's `-C metadata` hashes a path dependency by its path RELATIVE to the
/// cargo root when it can strip the root's prefix, and by its ABSOLUTE path
/// when it cannot (`SourceId::stable_hash`). A generated entry is its own root
/// (`<ws>/build/<coord>/<entry>/`, RFC-0098 D9), so every `src/` and
/// `generated/` crate it reached sat outside it and was hashed absolutely: the
/// same project built in two directories differed in every mangled name and in
/// its codegen-unit split, and no path remapping could repair that (measured,
/// 20,446 symbol lines). Reaching the workspace through a link INSIDE the root
/// makes those paths strip: cargo normalises a path dependency lexically and
/// never canonicalises it, so `_ws/src/talker` stays under the root and hashes
/// the same wherever the workspace lives. The link is relative (`../../..`),
/// so it too is the same bytes in every directory.
pub const WS_LINK: &str = "_ws";

/// The workspace a generated entry belongs to: `<ws>` for
/// `<ws>/build/<coord>/<entry>`, and `None` for any other shape (a single
/// package's own manifest, a west application), which keeps its paths as they
/// are.
#[must_use]
pub fn entry_workspace(entry_dir: &Path) -> Option<&Path> {
    let build = entry_dir.parent()?.parent()?;
    (build.file_name()? == "build").then_some(())?;
    build.parent()
}

/// `to`, re-rooted under the entry's [`WS_LINK`] when it lies inside the
/// entry's workspace — the ONE spelling every cargo-root path to a workspace
/// crate goes through (issue 2322). Anything else (the SDK, a registry crate's
/// patch, a non-entry `entry_dir`) is returned unchanged; such a path is the
/// same on one host wherever the workspace sits.
#[must_use]
pub fn through_ws_link(entry_dir: &Path, to: &Path) -> PathBuf {
    let to = lexical_normalize(to);
    let Some(ws) = entry_workspace(entry_dir) else {
        return to;
    };
    match to.strip_prefix(ws) {
        // Never route the entry's own build tree through the link: a crate
        // under `build/` is generated output beside the entry, not a workspace
        // source, and `_ws/build/...` would loop back into the root.
        Ok(rel) if !rel.starts_with("build") => entry_dir.join(WS_LINK).join(rel),
        _ => to,
    }
}

/// The path a generated entry's manifest writes for a dependency on `to`:
/// [`relative_or_err`] from the entry, after [`through_ws_link`]. Every
/// entry-relative dependency path goes through this, so no site can reach a
/// workspace crate around the link (issue 2322).
pub fn entry_dep_rel(entry_dir: &Path, to: &Path) -> Result<String, String> {
    relative_or_err(entry_dir, &through_ws_link(entry_dir, to))
}

/// Create (or repair) the entry's [`WS_LINK`]. A no-op for a directory that is
/// not a generated entry. The target is relative, so moving the workspace
/// carries the link with it.
pub fn ensure_ws_link(entry_dir: &Path) -> std::io::Result<()> {
    if entry_workspace(entry_dir).is_none() {
        return Ok(());
    }
    let link = entry_dir.join(WS_LINK);
    let target = Path::new("../../..");
    if std::fs::read_link(&link).is_ok_and(|t| t == target) {
        return Ok(());
    }
    if link.symlink_metadata().is_ok() {
        std::fs::remove_file(&link)?;
    }
    std::fs::create_dir_all(entry_dir)?;
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(target, &link)
    }
    #[cfg(not(unix))]
    {
        Err(std::io::Error::other(
            "a generated cargo entry links its workspace with a symlink; this host has none",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Issue 2322: a workspace crate is reached through the entry's link, so
    /// cargo hashes it relative to the root; the SDK and the build tree are not.
    #[test]
    fn workspace_crates_route_through_the_entry_link() {
        let entry = Path::new("/w/build/posix-x/linux_entry");
        assert_eq!(entry_workspace(entry), Some(Path::new("/w")));
        assert_eq!(
            through_ws_link(entry, Path::new("/w/src/talker")),
            Path::new("/w/build/posix-x/linux_entry/_ws/src/talker")
        );
        assert_eq!(
            through_ws_link(
                entry,
                Path::new("/w/build/posix-x/linux_entry/../../../generated/std_msgs")
            ),
            Path::new("/w/build/posix-x/linux_entry/_ws/generated/std_msgs")
        );
        assert_eq!(
            through_ws_link(entry, Path::new("/sdk/packages/api/nros")),
            Path::new("/sdk/packages/api/nros")
        );
        assert_eq!(
            through_ws_link(entry, Path::new("/w/build/other")),
            Path::new("/w/build/other")
        );
        let not_entry = Path::new("/w/src/pkg");
        assert_eq!(entry_workspace(not_entry), None);
        assert_eq!(
            through_ws_link(not_entry, Path::new("/w/src/talker")),
            Path::new("/w/src/talker")
        );
    }
    use std::path::PathBuf;

    #[test]
    fn rebasing_one_level_shallower_drops_one_step_up() {
        // issue 2175's measured case: build/posix-zenoh/linux_entry -> src/linux_entry.
        let from = Path::new("/ws/build/posix-zenoh/linux_entry");
        let to = Path::new("/ws/src/linux_entry");
        assert_eq!(
            rebase_relative(from, to, "../../../src/ctrl_pkg").as_deref(),
            Ok("../ctrl_pkg")
        );
        assert_eq!(
            rebase_relative(from, to, "../../../../../packages/api/nros").as_deref(),
            Ok("../../../../packages/api/nros")
        );
        assert_eq!(
            rebase_relative(from, to, "/abs/elsewhere").as_deref(),
            Ok("/abs/elsewhere")
        );
    }

    #[test]
    fn a_sibling_needs_one_step_up() {
        assert_eq!(
            relative(Path::new("/ws/build/linux"), Path::new("/ws/src/talker")).as_deref(),
            Some("../../src/talker")
        );
    }

    #[test]
    fn a_descendant_needs_no_dot_dot() {
        assert_eq!(
            relative(Path::new("/ws"), Path::new("/ws/src/talker")).as_deref(),
            Some("src/talker")
        );
    }

    #[test]
    fn the_same_directory_is_a_single_dot() {
        assert_eq!(
            relative(Path::new("/ws"), Path::new("/ws")).as_deref(),
            Some(".")
        );
    }

    #[test]
    fn an_ancestor_is_all_dot_dots() {
        assert_eq!(
            relative(Path::new("/ws/a/b"), Path::new("/ws")).as_deref(),
            Some("../..")
        );
    }

    #[test]
    fn escaping_the_workspace_still_resolves_when_a_root_is_shared() {
        // A vendored nano-ros outside the workspace is normal — both projects
        // that motivated this phase do it.
        assert_eq!(
            relative(
                Path::new("/opt/u/ws/build/linux"),
                Path::new("/opt/u/nano-ros/packages/api/nros")
            )
            .as_deref(),
            Some("../../../nano-ros/packages/api/nros")
        );
    }

    #[test]
    fn a_relative_input_yields_none_rather_than_a_wrong_answer() {
        // No shared root component. Returning None forces the caller to fail
        // rather than silently emit something host-specific.
        assert!(relative(Path::new("build/linux"), Path::new("/ws/src/a")).is_none());
    }

    #[test]
    fn the_error_names_both_paths_and_the_rule() {
        let e = relative_or_err(Path::new("rel"), Path::new("/abs")).expect_err("must fail");
        assert!(e.contains("/abs"), "{e}");
        assert!(e.contains("rel"), "{e}");
        assert!(e.contains("W3.c"), "cites the rule: {e}");
    }

    #[test]
    fn output_always_uses_forward_slashes() {
        // Cargo and CMake both accept `/` on every platform we build on, and a
        // generated file that differed by host separator would not be
        // byte-identical across machines.
        let r = relative(
            &PathBuf::from("/ws/build/linux"),
            &PathBuf::from("/ws/src/a/b/c"),
        )
        .expect("resolves");
        assert!(!r.contains('\\'), "{r}");
    }
}
