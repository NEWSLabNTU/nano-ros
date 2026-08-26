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

use std::path::Path;

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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn a_sibling_needs_one_step_up() {
        assert_eq!(
            relative(Path::new("/ws/build/native"), Path::new("/ws/src/talker")).as_deref(),
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
                Path::new("/opt/u/ws/build/native"),
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
        assert!(relative(Path::new("build/native"), Path::new("/ws/src/a")).is_none());
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
            &PathBuf::from("/ws/build/native"),
            &PathBuf::from("/ws/src/a/b/c"),
        )
        .expect("resolves");
        assert!(!r.contains('\\'), "{r}");
    }
}
