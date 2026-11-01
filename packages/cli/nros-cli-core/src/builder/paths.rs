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

#[cfg(test)]
mod tests {
    use super::*;
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
