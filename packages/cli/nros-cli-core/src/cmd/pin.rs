//! `nros pin` — read, and move, this project's `nros-toolchain.toml`
//! (RFC-0097 D7, phase-443 W2; the pin itself is phase-440 W7).
//!
//! ```text
//!   nros pin                      what does this project pin, and what is it made of
//!   nros pin <store version>      move the pin — reporting the codegen delta FIRST
//! ```
//!
//! ## The one question this verb exists to answer
//!
//! A user upgrading nano-ros wants to know one thing: *does this force me to
//! regenerate?* RFC-0097 measured why that is not the same question as "did the
//! version change" — `packages/cli` moved 710 times in 60 days,
//! `NROS_CODEGEN_VERSION` twice in the project's life. So the verdict comes
//! from the `codegen` field of each toolchain's own
//! `share/nros/manifest.toml`, never from comparing version STRINGS, and never
//! from an asset filename.
//!
//! Two releases declaring the same `codegen` produce
//! [`CodegenDelta::Same`] however far apart their versions are; one declaring a
//! different `codegen` produces a warning that names `nros sync`.
//!
//! ## Why the report is printed before the pin is written
//!
//! D7's acceptance is that a codegen change "warns before doing anything". That
//! is an ORDERING claim, so it is structural here rather than a matter of care:
//! [`execute`] builds the whole [`Survey`], prints it, and only then writes. A
//! test asserts the ordering on the returned outcome rather than by reading a
//! terminal.
//!
//! It warns rather than refuses because the runtime declares a RANGE
//! (`nros_core::codegen_version`), so a bump is not always breaking — and
//! because a refusal a user cannot override is how `--force` flags get added.
//! What must never happen is silence: drifted generated code compiles.
//!
//! ## Moving a pin is a source edit
//!
//! `pin::write` refuses to overwrite, which is deliberate (issues 0359/0378 one
//! layer up: a pin moves only when a dev means it). Typing `nros pin <version>`
//! IS a dev meaning it, so this verb goes through [`super::super::orchestration::pin::set`],
//! the overwriting spelling, and nothing else in the crate calls it.

use std::path::{Path, PathBuf};

use clap::Args as ClapArgs;
use eyre::{Result, bail};

use crate::orchestration::{
    pin::{self, Pin},
    release_manifest::{self, CodegenDelta, Found},
    store,
};

#[derive(Debug, ClapArgs)]
pub struct Args {
    /// The store version to pin to, exactly as `toolchains/<version>` spells it
    /// (`0.7.9-nros1`) — not the crate version `nros --version` prints. Omit to
    /// report the current pin and change nothing.
    //
    // `id`, and no `long`: the binary sets `propagate_version = true`, so clap
    // generates a `--version` flag on every subcommand and a field named
    // `version` collides with it — a startup `debug_assert` panic on this verb
    // alone, in every build. `nros toolchain uninstall` carries the same note
    // for the same reason.
    #[arg(id = "pin-version", value_name = "VERSION")]
    pub version: Option<String>,

    /// The project directory. Defaults to the working directory; the pin is
    /// searched for at or above it, the way cargo finds a workspace root.
    #[arg(long, value_name = "PATH")]
    pub dir: Option<PathBuf>,

    /// Store root. Defaults to `$NROS_STORE`, else `$NROS_HOME`, else
    /// `~/.nros` — never an absolute literal (RFC-0095 D2).
    #[arg(long, value_name = "PATH")]
    pub root: Option<PathBuf>,

    /// Report the delta and write nothing.
    #[arg(long)]
    pub dry_run: bool,
}

/// Everything the verb learned before it decided anything — gathered once, so
/// the decision is a pure function of it and its tests need no installed store.
#[derive(Clone, Debug)]
pub struct Survey {
    /// The directory the pin search started from.
    pub project: PathBuf,
    /// The pin that is there now, if any.
    pub current: Option<Pin>,
    /// The manifest of the currently pinned toolchain, if it is installed and
    /// declares one.
    pub current_manifest: Option<Found>,
    /// The version asked for, if this is a bump.
    pub target: Option<String>,
    /// The manifest of the target toolchain, if it is installed and declares
    /// one.
    pub target_manifest: Option<Found>,
}

impl Survey {
    /// The codegen verdict for a bump. `None` when nothing was asked for — a
    /// bare report compares nothing.
    #[must_use]
    pub fn delta(&self) -> Option<CodegenDelta> {
        self.target.as_ref()?;
        Some(CodegenDelta::between(
            self.current_manifest.as_ref().map(Found::codegen),
            self.target_manifest.as_ref().map(Found::codegen),
        ))
    }
}

/// Gather. Reads the pin file and at most two manifests; writes nothing.
pub fn survey(project: &Path, root: &Path, target: Option<&str>) -> Result<Survey> {
    let current = pin::find_and_load(project)?;
    let current_manifest = match &current {
        Some(p) => release_manifest::for_store_version(root, &p.version)?,
        None => None,
    };
    let target_manifest = match target {
        Some(v) => release_manifest::for_store_version(root, v)?,
        None => None,
    };
    Ok(Survey {
        project: project.to_path_buf(),
        current,
        current_manifest,
        target: target.map(str::to_string),
        target_manifest,
    })
}

/// The lines this verb prints, in order. Returned rather than printed so the
/// ORDERING claim — the warning comes before the write — is assertable.
#[must_use]
pub fn report(s: &Survey) -> Vec<String> {
    let mut out = Vec::new();
    match &s.current {
        Some(p) => {
            out.push(format!("pinned: {} ({})", p.version, p.path.display()));
            out.push(describe_manifest("  ", p, s.current_manifest.as_ref()));
        }
        None => out.push(format!(
            "pinned: NOTHING — no {} at or above {}.\n\
             \x20   This project floats: `nros build` will write a pin \
             (RFC-0095 D9), and until it does, firmware built here is \
             reproducible only by accident.",
            pin::FILE_NAME,
            s.project.display()
        )),
    }
    let Some(target) = &s.target else {
        return out;
    };
    out.push(match &s.target_manifest {
        Some(f) => format!(
            "target: {target}\n\x20   codegen {}   ({})",
            f.codegen(),
            f.path.display()
        ),
        None => format!(
            "target: {target} — not installed in this store, or it declares no \
             share/nros/{}.",
            release_manifest::FILE_NAME
        ),
    });
    if let Some(delta) = s.delta() {
        let from = s
            .current
            .as_ref()
            .map_or("(unpinned)", |p| p.version.as_str());
        out.push(release_manifest::describe_delta(&delta, from, target));
    }
    out
}

fn describe_manifest(indent: &str, p: &Pin, m: Option<&Found>) -> String {
    match m {
        Some(f) => {
            let mut s = format!("{indent}codegen {}", f.codegen());
            if let Some(i) = &f.manifest.index {
                s.push_str(&format!("   index {i}"));
            }
            if let Some(n) = &f.manifest.nano_ros {
                s.push_str(&format!("   nano-ros {n}"));
            }
            s.push_str(&format!("\n{indent}{}", f.path.display()));
            s
        }
        None => format!(
            "{indent}codegen UNKNOWN — {} is not installed in this store, or it \
             predates share/nros/{} (RFC-0097 D7).",
            p.version,
            release_manifest::FILE_NAME
        ),
    }
}

/// What [`execute`] did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// A bare report. Nothing was touched.
    Reported,
    /// `--dry-run`: the report was produced and the pin left alone.
    WouldWrite { path: PathBuf, version: String },
    /// The pin now names `version`.
    Wrote { path: PathBuf, version: String },
}

/// Report, then act — in that order, which is the point.
///
/// `emit` is called with each line AS IT IS PRODUCED, not handed a `Vec`
/// afterwards. That distinction is the whole test surface: a version of this
/// that wrote the pin first and then appended the same lines to a buffer
/// produces an identical transcript, and an ordering assertion over that
/// transcript passes — measured, 2026-09-10, against exactly that mutation. A
/// sink lets a test observe the FILESYSTEM at the moment each line arrives, so
/// "warns before doing anything" is checked against the thing it is about.
pub fn execute(
    project: &Path,
    root: &Path,
    target: Option<&str>,
    dry_run: bool,
    emit: &mut dyn FnMut(&str),
) -> Result<Outcome> {
    let s = survey(project, root, target)?;
    for line in report(&s) {
        emit(&line);
    }
    let Some(version) = s.target.clone() else {
        return Ok(Outcome::Reported);
    };
    if s.current.as_ref().is_some_and(|c| c.version == version) {
        emit(&format!("pin already names {version} — unchanged."));
        return Ok(Outcome::Reported);
    }
    let path = project.join(pin::FILE_NAME);
    if dry_run {
        emit(&format!("--dry-run: would write {}", path.display()));
        return Ok(Outcome::WouldWrite { path, version });
    }
    let written = pin::set(project, &version)?;
    emit(&format!(
        "wrote {} — this project now pins nano-ros {version}. Commit it.",
        written.display()
    ));
    Ok(Outcome::Wrote {
        path: written,
        version,
    })
}

pub fn run(args: Args) -> Result<()> {
    let project = match args.dir {
        Some(d) => d,
        None => std::env::current_dir()?,
    };
    if !project.is_dir() {
        bail!("{} is not a directory", project.display());
    }
    let root = args.root.clone().unwrap_or_else(store::root);
    // Printed as produced, for the same reason the sink exists: the report must
    // have REACHED the user before the pin moves, and a buffer flushed at the
    // end cannot promise that when the write fails half way.
    execute(
        &project,
        &root,
        args.version.as_deref(),
        args.dry_run,
        &mut |line: &str| println!("{line}"),
    )
    .map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a store holding two toolchains with the given codegen numbers.
    fn store_with(root: &Path, entries: &[(&str, Option<u32>)]) {
        for (version, codegen) in entries {
            let prefix = root.join("toolchains").join(version);
            std::fs::create_dir_all(prefix.join("share/nros")).unwrap();
            std::fs::write(prefix.join("bin_placeholder"), b"").unwrap();
            if let Some(c) = codegen {
                std::fs::write(
                    release_manifest::path_in_prefix(&prefix),
                    release_manifest::render(&release_manifest::ReleaseManifest {
                        version: (*version).to_string(),
                        codegen: *c,
                        index: Some("2026-09-10".into()),
                        nano_ros: Some("abc1234".into()),
                    }),
                )
                .unwrap();
            }
        }
    }

    /// Run `execute` and capture, for every line, WHAT THE PIN FILE HELD AT THE
    /// MOMENT IT WAS EMITTED.
    ///
    /// A plain `Vec<String>` is not enough, and that is measured rather than
    /// assumed: a mutation that wrote the pin first and then appended the same
    /// lines produced an identical transcript, so the ordering assertion over it
    /// passed. Recording the filesystem beside each line is what makes "warns
    /// before doing anything" a claim about doing rather than about printing.
    fn run_capturing(
        project: &Path,
        root: &Path,
        target: Option<&str>,
        dry_run: bool,
    ) -> (Outcome, Vec<(String, Option<String>)>) {
        let pin_path = project.join(pin::FILE_NAME);
        let mut seen: Vec<(String, Option<String>)> = Vec::new();
        let outcome = {
            let mut sink = |line: &str| {
                seen.push((line.to_string(), std::fs::read_to_string(&pin_path).ok()));
            };
            execute(project, root, target, dry_run, &mut sink).unwrap()
        };
        (outcome, seen)
    }

    /// Just the lines.
    fn lines_of(seen: &[(String, Option<String>)]) -> Vec<String> {
        seen.iter().map(|(l, _)| l.clone()).collect()
    }

    /// The feature: same `codegen`, different everything else, no warning and
    /// no re-emit instruction anywhere in the report.
    #[test]
    fn a_bump_between_equal_codegens_does_not_warn() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("store");
        let project = dir.path().join("proj");
        std::fs::create_dir_all(&project).unwrap();
        store_with(&root, &[("0.7.1-nros1", Some(7)), ("0.7.9-nros4", Some(7))]);
        std::fs::write(project.join(pin::FILE_NAME), pin::render("0.7.1-nros1")).unwrap();

        let (out, seen) = run_capturing(&project, &root, Some("0.7.9-nros4"), false);
        let joined = lines_of(&seen).join("\n");
        assert!(
            !joined.contains("warning"),
            "equal codegen must not warn:\n{joined}"
        );
        assert!(joined.contains("UNCHANGED"), "{joined}");
        assert!(!joined.contains("nros sync"), "{joined}");
        assert!(matches!(out, Outcome::Wrote { .. }), "{out:?}");
        assert_eq!(
            pin::load(&project.join(pin::FILE_NAME)).unwrap().version,
            "0.7.9-nros4"
        );
    }

    /// The ordering claim, asserted against the FILESYSTEM: at the instant the
    /// warning reaches the user, the pin still names the old toolchain.
    #[test]
    fn a_differing_codegen_warns_before_the_pin_is_written() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("store");
        let project = dir.path().join("proj");
        std::fs::create_dir_all(&project).unwrap();
        store_with(&root, &[("0.7.9-nros1", Some(7)), ("0.8.0-nros1", Some(8))]);
        std::fs::write(project.join(pin::FILE_NAME), pin::render("0.7.9-nros1")).unwrap();

        let (out, seen) = run_capturing(&project, &root, Some("0.8.0-nros1"), false);
        let (warn_line, pin_when_warned) = seen
            .iter()
            .find(|(l, _)| l.starts_with("warning: codegen 7 -> 8"))
            .unwrap_or_else(|| panic!("no codegen warning in {seen:#?}"));
        assert!(warn_line.contains("nros sync"), "{warn_line}");
        // THE claim: when the user was warned, the pin still named the OLD
        // toolchain. Nothing had been done yet.
        assert_eq!(
            pin_when_warned.as_deref(),
            Some(pin::render("0.7.9-nros1").as_str()),
            "the pin had already moved by the time the warning was emitted"
        );
        // And the move did then happen.
        assert!(matches!(out, Outcome::Wrote { .. }), "{out:?}");
        assert_eq!(
            pin::load(&project.join(pin::FILE_NAME)).unwrap().version,
            "0.8.0-nros1"
        );
    }

    /// A toolchain that predates manifests leaves the question unanswered, and
    /// the verb says so rather than assuming it is fine.
    #[test]
    fn a_toolchain_without_a_manifest_reports_unknown_not_same() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("store");
        let project = dir.path().join("proj");
        std::fs::create_dir_all(&project).unwrap();
        store_with(&root, &[("0.6.0-nros1", None), ("0.7.0-nros1", Some(7))]);
        std::fs::write(project.join(pin::FILE_NAME), pin::render("0.6.0-nros1")).unwrap();

        let s = survey(&project, &root, Some("0.7.0-nros1")).unwrap();
        assert!(matches!(
            s.delta(),
            Some(CodegenDelta::Unknown {
                from_known: false,
                to_known: true
            })
        ));
        let joined = report(&s).join("\n");
        assert!(joined.contains("UNKNOWN"), "{joined}");
        assert!(joined.contains("nros sync"), "{joined}");
    }

    /// `--dry-run` reports and writes nothing — the half that proves the report
    /// is not a consequence of the write.
    #[test]
    fn dry_run_reports_the_delta_and_leaves_the_pin_alone() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("store");
        let project = dir.path().join("proj");
        std::fs::create_dir_all(&project).unwrap();
        store_with(&root, &[("0.7.9-nros1", Some(7)), ("0.8.0-nros1", Some(8))]);
        let pin_path = project.join(pin::FILE_NAME);
        std::fs::write(&pin_path, pin::render("0.7.9-nros1")).unwrap();
        let before = std::fs::read_to_string(&pin_path).unwrap();

        let (out, seen) = run_capturing(&project, &root, Some("0.8.0-nros1"), true);
        assert!(matches!(out, Outcome::WouldWrite { .. }), "{out:?}");
        assert_eq!(std::fs::read_to_string(&pin_path).unwrap(), before);
        // Every line was emitted against an unchanged pin, not just the last.
        for (line, at) in &seen {
            assert_eq!(at.as_deref(), Some(before.as_str()), "{line}");
        }
        assert!(
            lines_of(&seen)
                .iter()
                .any(|l| l.starts_with("warning: codegen 7 -> 8"))
        );
    }

    /// A bare report compares nothing and touches nothing.
    #[test]
    fn a_bare_report_names_the_pin_and_its_codegen() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("store");
        let project = dir.path().join("proj");
        std::fs::create_dir_all(&project).unwrap();
        store_with(&root, &[("0.7.9-nros1", Some(7))]);
        std::fs::write(project.join(pin::FILE_NAME), pin::render("0.7.9-nros1")).unwrap();

        let (out, seen) = run_capturing(&project, &root, None, false);
        assert_eq!(out, Outcome::Reported);
        let joined = lines_of(&seen).join("\n");
        assert!(joined.contains("pinned: 0.7.9-nros1"), "{joined}");
        assert!(joined.contains("codegen 7"), "{joined}");
        assert!(joined.contains("index 2026-09-10"), "{joined}");
    }

    /// An unpinned project is a reported state, not an error.
    #[test]
    fn an_unpinned_project_says_so_and_can_be_pinned() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("store");
        let project = dir.path().join("proj");
        std::fs::create_dir_all(&project).unwrap();
        store_with(&root, &[("0.7.9-nros1", Some(7))]);

        let (out, seen) = run_capturing(&project, &root, Some("0.7.9-nros1"), false);
        let lines = lines_of(&seen);
        assert!(lines[0].contains("pinned: NOTHING"), "{:?}", lines[0]);
        // Reported as floating BEFORE the pin existed — not after writing one.
        assert_eq!(seen[0].1, None, "the pin existed when the report started");
        assert!(matches!(out, Outcome::Wrote { .. }), "{out:?}");
    }
}
