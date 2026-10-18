//! Reading cargo's error output: the first error AND why it happened.
//!
//! Issue 1898. cargo reports a failure as one `error:` line followed by a
//! `Caused by:` chain, deepest cause last:
//!
//! ```text
//! error: failed to get `ros-launch-manifest-model` as a dependency of package `nros-macros …`
//!
//! Caused by:
//!   failed to load source for dependency `ros-launch-manifest-model`
//!
//! Caused by:
//!   Unable to update https://github.com/…?tag=v0.1.57
//!
//! Caused by:
//!   failed to resolve address for github.com: Temporary failure in name resolution
//! ```
//!
//! The first line says WHAT failed, and only the last block says WHY: a network
//! timeout, an unknown tag, an `--offline`/`--locked` refusal, a missing git
//! database. Every excerpt in this repo kept the first `error` line and dropped
//! the chain, so a transient network failure and a real resolution defect
//! produced byte-identical one-line reds (the tier-2 nightly of 2026-10-14).
//!
//! So an excerpt is the first error line PLUS its root cause, appended in
//! parentheses after [`ROOT_CAUSE`]. The shell spelling of the same rule is
//! `scripts/lib/cargo-error-cause.sh`; `tests::the_shell_twin_agrees` holds the
//! two to the same answers.

/// The text that introduces the appended root cause. A reader that has to
/// shorten an excerpt keeps everything from here on ([`elide_keeping_cause`]).
pub const ROOT_CAUSE: &str = " (root cause: ";

/// The first line of `text` (trimmed) that `pick` accepts, with the root cause
/// of its `Caused by:` chain appended — or the line alone when it has no chain.
/// `None` when no line matches.
///
/// A line that already carries a [`ROOT_CAUSE`] is returned as is, so an
/// excerpt that passes through two readers is not annotated twice.
pub fn first_error_with_cause(text: &str, pick: impl Fn(&str) -> bool) -> Option<String> {
    let lines: Vec<&str> = text.lines().collect();
    let at = lines.iter().position(|l| pick(l.trim()))?;
    let line = lines[at].trim();
    if line.contains(ROOT_CAUSE) {
        return Some(line.to_string());
    }
    Some(match root_cause_after(&lines[at + 1..]) {
        Some(cause) => format!("{line}{ROOT_CAUSE}{cause})"),
        None => line.to_string(),
    })
}

/// The root cause of the FIRST cargo error in `text`: the first line of the
/// last `Caused by:` block that belongs to it. `None` when that error has no
/// chain (a rustc `error[E…]`, a `--locked` refusal, a one-line failure).
pub fn root_cause(text: &str) -> Option<String> {
    let lines: Vec<&str> = text.lines().collect();
    let at = lines.iter().position(|l| l.trim().starts_with("error"))?;
    root_cause_after(&lines[at + 1..])
}

/// Walk the chain that follows an error line. The chain ends at the next
/// top-level diagnostic (an unindented `error`/`warning` line), so a second
/// error's causes are never attributed to the first.
///
/// Both of cargo's spellings are read: `Caused by:` on its own line with the
/// cause indented below it, and `Caused by: <cause>` on one line.
fn root_cause_after(rest: &[&str]) -> Option<String> {
    let mut cause = None;
    // A bare `Caused by:` line: the cause is the next non-empty line.
    let mut want_next = false;
    for raw in rest {
        if raw.starts_with("error") || raw.starts_with("warning") {
            break;
        }
        let line = raw.trim();
        if want_next {
            if !line.is_empty() {
                cause = Some(line.to_string());
                want_next = false;
            }
            continue;
        }
        if let Some(tail) = line.strip_prefix("Caused by:") {
            let tail = tail.trim();
            if tail.is_empty() {
                want_next = true;
            } else {
                cause = Some(tail.to_string());
            }
        }
    }
    cause
}

/// Shorten `line` to at most `cap` characters, keeping its [`ROOT_CAUSE`] tail
/// whole when it has one: the cause is the part a reader cannot get back from
/// anywhere else, while the head (`failed to get X as a dependency of package
/// Y (<absolute path>)`) is mostly a path. Elided text is marked `…`.
pub fn elide_keeping_cause(line: &str, cap: usize) -> String {
    let len = line.chars().count();
    if len <= cap {
        return line.to_string();
    }
    let Some(split) = line.find(ROOT_CAUSE) else {
        let mut out: String = line.chars().take(cap).collect();
        out.push('…');
        return out;
    };
    let (head, tail) = line.split_at(split);
    let tail_len = tail.chars().count();
    if tail_len >= cap {
        let mut out: String = tail.chars().take(cap).collect();
        out.push('…');
        return out;
    }
    let mut out: String = head.chars().take(cap - tail_len).collect();
    out.push('…');
    out.push_str(tail);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The shape the 2026-10-14 tier-2 nightly printed, with the chain cargo
    /// writes beneath it (git dependency, name resolution down).
    const GIT_DEP: &str = "\
    Updating git repository `https://github.com/NEWSLabNTU/ros-launch-manifest.git`
error: failed to get `ros-launch-manifest-model` as a dependency of package `nros-macros v0.6.0 (/w/packages/core/nros-macros)`

Caused by:
  failed to load source for dependency `ros-launch-manifest-model`

Caused by:
  Unable to update https://github.com/NEWSLabNTU/ros-launch-manifest.git?tag=v0.1.57

Caused by:
  failed to resolve address for github.com: Temporary failure in name resolution; class=Net (12)
";

    fn error_line(l: &str) -> bool {
        l.starts_with("error")
    }

    #[test]
    fn a_git_dependency_failure_carries_its_root_cause() {
        let got = first_error_with_cause(GIT_DEP, error_line).unwrap();
        assert!(
            got.starts_with("error: failed to get `ros-launch-manifest-model`"),
            "{got}"
        );
        assert!(
            got.ends_with(
                "(root cause: failed to resolve address for github.com: \
                 Temporary failure in name resolution; class=Net (12))"
            ),
            "the deepest cause, not the first one: {got}"
        );
        assert!(!got.contains('\n'), "one line: {got}");
    }

    #[test]
    fn a_one_line_caused_by_is_read_too() {
        let text = "error: failed to load source for dependency `libc`\n  \
                    Caused by: unable to update /x/nuttx-libc\n  \
                    Caused by: failed to read `/x/nuttx-libc/Cargo.toml`\n";
        assert_eq!(
            root_cause(text).as_deref(),
            Some("failed to read `/x/nuttx-libc/Cargo.toml`")
        );
    }

    #[test]
    fn a_rustc_error_has_no_root_cause() {
        let text = "error[E0432]: unresolved import `std_msgs`\n --> src/main.rs:1:5\n";
        assert_eq!(root_cause(text), None);
        assert_eq!(
            first_error_with_cause(text, error_line).as_deref(),
            Some("error[E0432]: unresolved import `std_msgs`")
        );
    }

    #[test]
    fn a_later_errors_chain_is_not_the_first_errors_cause() {
        let text = "error: first\nerror: second\n\nCaused by:\n  second's cause\n";
        assert_eq!(root_cause(text), None);
    }

    #[test]
    fn an_annotated_line_is_not_annotated_twice() {
        let once = first_error_with_cause(GIT_DEP, error_line).unwrap();
        let text = format!("harness failed: {once}\n\nCaused by:\n  other\n");
        let twice = first_error_with_cause(&text, |l| l.contains("error:")).unwrap();
        assert_eq!(twice.matches(ROOT_CAUSE).count(), 1, "{twice}");
    }

    #[test]
    fn eliding_keeps_the_cause() {
        let line = first_error_with_cause(GIT_DEP, error_line).unwrap();
        let short = elide_keeping_cause(&line, 160);
        assert!(short.chars().count() <= 161, "{short}");
        assert!(short.ends_with("class=Net (12))"), "{short}");
        assert!(short.starts_with("error: failed to get"), "{short}");
        assert_eq!(elide_keeping_cause("short", 10), "short");
        assert_eq!(elide_keeping_cause("abcdefghij", 4), "abcd…");
    }

    /// The shell twin answers the same as this module on every case above.
    #[test]
    fn the_shell_twin_agrees() {
        let script = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../scripts/lib/cargo-error-cause.sh");
        assert!(script.is_file(), "{}", script.display());
        let cases = [
            GIT_DEP,
            "error[E0432]: unresolved import `std_msgs`\n --> src/main.rs:1:5\n",
            "error: first\nerror: second\n\nCaused by:\n  second's cause\n",
            "error: failed to load source for dependency `libc`\n  \
             Caused by: unable to update /x\n  Caused by: failed to read `/x/Cargo.toml`\n",
            "no error here\n",
        ];
        for text in cases {
            let out = std::process::Command::new("bash")
                .arg("-c")
                .arg(r#"source "$1"; nros_cargo_first_error"#)
                .arg("bash")
                .arg(&script)
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::piped())
                .spawn()
                .and_then(|mut c| {
                    use std::io::Write;
                    c.stdin.take().unwrap().write_all(text.as_bytes())?;
                    c.wait_with_output()
                })
                .expect("run bash");
            let shell = String::from_utf8(out.stdout).unwrap();
            let rust = first_error_with_cause(text, error_line).unwrap_or_default();
            assert_eq!(shell.trim_end_matches('\n'), rust, "case {text:?}");
        }
    }
}
