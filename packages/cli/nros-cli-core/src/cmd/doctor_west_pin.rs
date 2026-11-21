//! Issue 2771 -- `nros doctor` compares a project's toolchain pin with the
//! `nano-ros` revision its `west.yml` fetches.
//!
//! A Zephyr consumer names nano-ros twice: the toolchain pin in
//! `nros-toolchain.toml` (issue 2645: `version`, `git` + `rev`, or `path`) and
//! the `nano-ros` project's `revision` in its west manifest, which west fetches
//! as a module. Nothing compared them, so a consumer kept a lockstep script of
//! its own. This is that comparison. West stays the owner of its pin: the
//! doctor only reports, and never edits either file.
//!
//! What each pin form is compared with:
//!
//! - `git` + `rev`: the pin's commit.
//! - `path`: the checkout's `HEAD`, read when the doctor runs.
//! - `version`: nothing. The SDK index records no commit for a release, so a
//!   release pin cannot be compared, and the doctor says so rather than
//!   guessing.
//!
//! A west revision that is a commit is compared by PREFIX in either direction,
//! so a short sha in `west.yml` agrees with the full one in the pin. A branch or
//! tag cannot be compared with a commit without fetching, so it is reported and
//! not counted.

use std::path::{Path, PathBuf};

use crate::orchestration::pin::{Pin, PinSource};

/// The west manifest's file name, found from the working directory upward.
pub const WEST_MANIFEST: &str = "west.yml";

/// Names a west project may carry for nano-ros. West project names are the
/// consumer's choice; these are the two spellings the repository itself goes
/// by (`nano-ros` the project, `nano_ros` the C header and CMake names).
const NANO_ROS_PROJECT_NAMES: &[&str] = &["nano-ros", "nano_ros"];

/// The comparison's verdict, as the line the doctor prints.
#[derive(Debug, PartialEq, Eq)]
pub struct Lockstep {
    /// The report line, in the install report's `  [..] <topic>  ...` shape.
    pub line: String,
    /// Whether it counts as a problem: only a measured disagreement does.
    pub problem: bool,
}

/// The `west.yml` nearest `start`, walking up like [`crate::orchestration::pin::find`].
#[must_use]
pub fn find_west_manifest(start: &Path) -> Option<PathBuf> {
    start
        .ancestors()
        .map(|d| d.join(WEST_MANIFEST))
        .find(|p| p.is_file())
}

/// The `nano-ros` project's revision in a west manifest's text.
///
/// `Ok(None)` when the manifest lists no nano-ros project (a west project
/// that does not fetch nano-ros has nothing to agree with). A project with no
/// `revision` of its own takes `defaults: revision`, and with neither, west's
/// own default, `master`.
pub fn nano_ros_revision(manifest: &str) -> Result<Option<String>, String> {
    let doc: serde_yaml_ng::Value =
        serde_yaml_ng::from_str(manifest).map_err(|e| format!("not YAML: {e}"))?;
    let m = doc
        .get("manifest")
        .ok_or_else(|| "no top-level `manifest` key".to_string())?;
    let Some(projects) = m.get("projects").and_then(|p| p.as_sequence()) else {
        return Ok(None);
    };
    let Some(project) = projects.iter().find(|p| {
        p.get("name")
            .and_then(|n| n.as_str())
            .is_some_and(|n| NANO_ROS_PROJECT_NAMES.contains(&n))
    }) else {
        return Ok(None);
    };
    let revision = project
        .get("revision")
        .or_else(|| m.get("defaults").and_then(|d| d.get("revision")))
        .map(|r| match r {
            // An all-digit sha (rare, but legal) reads as a YAML number.
            serde_yaml_ng::Value::Number(n) => n.to_string(),
            other => other.as_str().unwrap_or_default().to_string(),
        })
        .unwrap_or_else(|| "master".to_string());
    Ok(Some(revision.trim().to_string()))
}

/// Whether a west revision is a commit (7 to 40 hex digits) rather than a
/// branch or a tag.
fn is_commit(rev: &str) -> bool {
    (7..=40).contains(&rev.len()) && rev.bytes().all(|b| b.is_ascii_hexdigit())
}

/// Whether two commit spellings name the same commit: one is a prefix of the
/// other, compared without case.
fn same_commit(a: &str, b: &str) -> bool {
    let (a, b) = (a.to_ascii_lowercase(), b.to_ascii_lowercase());
    a.starts_with(&b) || b.starts_with(&a)
}

/// Compare `pin` with the `nano-ros` revision of the `west.yml` nearest `cwd`.
///
/// `None` when there is nothing to compare: no `west.yml`, or one that lists
/// no nano-ros project. `head_of` reads a checkout's `HEAD` (a parameter, so a
/// test needs no git repository).
pub fn check(pin: &Pin, cwd: &Path, head_of: &dyn Fn(&Path) -> Option<String>) -> Option<Lockstep> {
    let manifest = find_west_manifest(cwd)?;
    let text = match std::fs::read_to_string(&manifest) {
        Ok(t) => t,
        Err(e) => {
            return Some(Lockstep {
                line: format!(
                    "  [!!] west pin   {} cannot be read: {e}",
                    manifest.display()
                ),
                problem: true,
            });
        }
    };
    let west_rev = match nano_ros_revision(&text) {
        Ok(Some(r)) => r,
        Ok(None) => return None,
        Err(e) => {
            return Some(Lockstep {
                line: format!(
                    "  [!!] west pin   {} cannot be read as a west manifest: {e}",
                    manifest.display()
                ),
                problem: true,
            });
        }
    };

    let (wanted, what) = match &pin.source {
        PinSource::Git { rev, .. } => (rev.clone(), "commit"),
        PinSource::Path(dir) => match head_of(dir) {
            Some(h) => (h, "checkout HEAD"),
            None => {
                return Some(Lockstep {
                    line: format!(
                        "  [--] west pin   {} pins the checkout {}, whose HEAD cannot be read, \
                         so it is not compared with {} (nano-ros at {west_rev})",
                        pin.path.display(),
                        dir.display(),
                        manifest.display()
                    ),
                    problem: false,
                });
            }
        },
        PinSource::Release => {
            return Some(Lockstep {
                line: format!(
                    "  [--] west pin   {} pins the release {}, which records no commit, so it \
                     is not compared with {} (nano-ros at {west_rev})",
                    pin.path.display(),
                    pin.version,
                    manifest.display()
                ),
                problem: false,
            });
        }
    };

    if !is_commit(&west_rev) {
        return Some(Lockstep {
            line: format!(
                "  [--] west pin   {} fetches nano-ros at `{west_rev}`, a branch or tag, which \
                 cannot be compared with the {what} {} that {} names",
                manifest.display(),
                short(&wanted),
                pin.path.display()
            ),
            problem: false,
        });
    }
    if same_commit(&wanted, &west_rev) {
        Some(Lockstep {
            line: format!(
                "  [OK] west pin   {} and {} both name nano-ros {}",
                pin.path.display(),
                manifest.display(),
                short(&wanted)
            ),
            problem: false,
        })
    } else {
        Some(Lockstep {
            line: format!(
                "  [!!] west pin   the toolchain and the west module disagree on nano-ros:\n\
                 \x20              {} names the {what} {wanted}\n\
                 \x20              {} fetches revision {west_rev}\n\
                 \x20              the build would run one nano-ros's CLI over another's \
                 sources; move one of the two so they name the same commit",
                pin.path.display(),
                manifest.display()
            ),
            problem: true,
        })
    }
}

/// A commit's first ten digits, as `Pin::describe` prints one.
fn short(rev: &str) -> &str {
    &rev[..rev.len().min(10)]
}

/// `git -C <dir> rev-parse HEAD`, or `None` when it does not answer.
#[must_use]
pub fn git_head(dir: &Path) -> Option<String> {
    let out = crate::source_stamp::nros_git_command("git")
        .arg("-C")
        .arg(dir)
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8(out.stdout).ok()?.trim().to_string();
    (!s.is_empty()).then_some(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SHA: &str = "09219edc75a1b2c3d4e5f60718293a4b5c6d7e8f";
    const OTHER: &str = "1111111111a1b2c3d4e5f60718293a4b5c6d7e8f";

    fn git_pin(dir: &Path, rev: &str) -> Pin {
        Pin {
            path: dir.join("nros-toolchain.toml"),
            version: format!("git-{rev}"),
            source: PinSource::Git {
                url: "https://example.invalid/nano-ros.git".into(),
                rev: rev.into(),
            },
        }
    }

    fn west(dir: &Path, revision_line: &str) {
        std::fs::write(
            dir.join(WEST_MANIFEST),
            format!(
                "manifest:\n  projects:\n    - name: zephyr\n      revision: v3.7.2\n    \
                 - name: nano-ros\n      url: https://example.invalid/nano-ros\n{revision_line}"
            ),
        )
        .unwrap();
    }

    fn no_head(_: &Path) -> Option<String> {
        None
    }

    #[test]
    fn a_git_pin_and_west_naming_the_same_commit_agree() {
        let tmp = tempfile::tempdir().unwrap();
        west(tmp.path(), &format!("      revision: {SHA}\n"));
        let v = check(&git_pin(tmp.path(), SHA), tmp.path(), &no_head).unwrap();
        assert!(!v.problem, "{}", v.line);
        assert!(v.line.contains("[OK]"), "{}", v.line);
    }

    #[test]
    fn a_git_pin_and_west_naming_different_commits_are_an_error_naming_both() {
        let tmp = tempfile::tempdir().unwrap();
        west(tmp.path(), &format!("      revision: {OTHER}\n"));
        let pin = git_pin(tmp.path(), SHA);
        let v = check(&pin, tmp.path(), &no_head).unwrap();
        assert!(v.problem, "a disagreement must count:\n{}", v.line);
        for needle in [
            SHA,
            OTHER,
            &pin.path.display().to_string(),
            &tmp.path().join(WEST_MANIFEST).display().to_string(),
        ] {
            assert!(
                v.line.contains(needle),
                "the error must name `{needle}`:\n{}",
                v.line
            );
        }
    }

    #[test]
    fn a_short_sha_in_west_agrees_by_prefix() {
        let tmp = tempfile::tempdir().unwrap();
        west(tmp.path(), &format!("      revision: {}\n", &SHA[..12]));
        let v = check(&git_pin(tmp.path(), SHA), tmp.path(), &no_head).unwrap();
        assert!(!v.problem, "{}", v.line);
        // ...and a short sha of ANOTHER commit still disagrees.
        west(tmp.path(), &format!("      revision: {}\n", &OTHER[..12]));
        let v = check(&git_pin(tmp.path(), SHA), tmp.path(), &no_head).unwrap();
        assert!(v.problem, "{}", v.line);
    }

    #[test]
    fn with_no_west_manifest_there_is_nothing_to_compare() {
        let tmp = tempfile::tempdir().unwrap();
        assert_eq!(check(&git_pin(tmp.path(), SHA), tmp.path(), &no_head), None);
    }

    #[test]
    fn a_west_manifest_without_nano_ros_has_nothing_to_compare() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join(WEST_MANIFEST),
            "manifest:\n  projects:\n    - name: zephyr\n      revision: v3.7.2\n",
        )
        .unwrap();
        assert_eq!(check(&git_pin(tmp.path(), SHA), tmp.path(), &no_head), None);
    }

    #[test]
    fn the_manifest_is_found_from_a_subdirectory() {
        let tmp = tempfile::tempdir().unwrap();
        west(tmp.path(), &format!("      revision: {OTHER}\n"));
        let sub = tmp.path().join("app/src");
        std::fs::create_dir_all(&sub).unwrap();
        let v = check(&git_pin(tmp.path(), SHA), &sub, &no_head).unwrap();
        assert!(v.problem, "{}", v.line);
    }

    #[test]
    fn a_path_pin_is_compared_with_its_checkout_head() {
        let tmp = tempfile::tempdir().unwrap();
        west(tmp.path(), &format!("      revision: {SHA}\n"));
        let checkout = tmp.path().join("nano-ros");
        let pin = Pin {
            path: tmp.path().join("nros-toolchain.toml"),
            version: format!("path:{}", checkout.display()),
            source: PinSource::Path(checkout.clone()),
        };
        let at = |head: &'static str| {
            let c = checkout.clone();
            move |d: &Path| (d == c).then(|| head.to_string())
        };
        let agree = check(&pin, tmp.path(), &at(SHA)).unwrap();
        assert!(!agree.problem, "{}", agree.line);
        let disagree = check(&pin, tmp.path(), &at(OTHER)).unwrap();
        assert!(disagree.problem, "{}", disagree.line);
        assert!(disagree.line.contains("checkout HEAD"), "{}", disagree.line);
    }

    #[test]
    fn a_release_pin_is_reported_and_not_compared() {
        let tmp = tempfile::tempdir().unwrap();
        west(tmp.path(), &format!("      revision: {SHA}\n"));
        let pin = Pin::release(tmp.path().join("nros-toolchain.toml"), "0.6.0-nros11");
        let v = check(&pin, tmp.path(), &no_head).unwrap();
        assert!(!v.problem, "{}", v.line);
        assert!(v.line.contains("records no commit"), "{}", v.line);
    }

    #[test]
    fn a_branch_or_tag_in_west_is_reported_and_not_counted() {
        let tmp = tempfile::tempdir().unwrap();
        west(tmp.path(), "      revision: main\n");
        let v = check(&git_pin(tmp.path(), SHA), tmp.path(), &no_head).unwrap();
        assert!(!v.problem, "{}", v.line);
        assert!(v.line.contains("branch or tag"), "{}", v.line);
    }

    #[test]
    fn a_project_without_a_revision_takes_the_manifest_default() {
        let doc = format!(
            "manifest:\n  defaults:\n    revision: {SHA}\n  projects:\n    - name: nano-ros\n"
        );
        assert_eq!(nano_ros_revision(&doc).unwrap().as_deref(), Some(SHA));
        let bare = "manifest:\n  projects:\n    - name: nano-ros\n";
        assert_eq!(nano_ros_revision(bare).unwrap().as_deref(), Some("master"));
    }
}
