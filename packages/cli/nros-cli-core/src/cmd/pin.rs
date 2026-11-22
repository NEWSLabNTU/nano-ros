//! `nros pin` — read, and move, this project's `nros-toolchain.toml`
//! (RFC-0097 D7, phase-443 W2; the pin itself is phase-440 W7).
//!
//! ```text
//!   nros pin                      what does this project pin, and what is it made of
//!   nros pin <store version>      move the pin — reporting the codegen delta FIRST
//!   nros pin --rev <sha> [--git <url>]   pin one commit (issue 2770)
//!   nros pin --path <dir>         pin a nano-ros checkout on this machine
//! ```
//!
//! Each form replaces the other two, so the file always names exactly one.
//! In an automated session (`$CI`) every form refuses to write, the rule
//! `nros build` already keeps (RFC-0097 D11): a pin is a source edit.
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
//! different `codegen` produces a warning that names `nros configure`.
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

use nros_launcher::session::Session;

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
    #[arg(id = "pin-version", value_name = "VERSION", conflicts_with_all = ["rev", "path"])]
    pub version: Option<String>,

    /// Pin one commit instead: the full 40-hex sha. A branch, a tag or a
    /// short sha is refused, since none of them is sure to name the same
    /// commit tomorrow.
    #[arg(long, value_name = "SHA", conflicts_with = "path")]
    pub rev: Option<String>,

    /// The repository `--rev` is a commit of. Default: the pin's current
    /// `git`, else the nano-ros repository.
    #[arg(long, value_name = "URL", requires = "rev")]
    pub git: Option<String>,

    /// Pin a nano-ros checkout on this machine instead: the root of a clone.
    /// The pin records it relative to the pin file.
    #[arg(long, value_name = "DIR")]
    pub path: Option<PathBuf>,

    /// The project directory. Defaults to the working directory; the pin is
    /// searched for at or above it, the way cargo finds a workspace root.
    #[arg(long, value_name = "PATH")]
    pub dir: Option<PathBuf>,

    /// Store root. Defaults to `$NROS_HOME`, else `~/.nros`.
    // Never an absolute literal (RFC-0095 D2).
    #[arg(long, value_name = "PATH")]
    pub root: Option<PathBuf>,

    /// Report the delta and write nothing.
    #[arg(long)]
    pub dry_run: bool,
}

/// The pin a bump writes: one of the three forms, already checked.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Target {
    /// A store version.
    Release(String),
    /// One commit of a repository.
    Git { url: String, rev: String },
    /// A checkout: `written` is the `path` the file records, `abs` the
    /// directory it names.
    Path { written: String, abs: PathBuf },
}

impl Target {
    /// The key [`Pin::version`] reads back for this form.
    #[must_use]
    pub fn key(&self) -> String {
        match self {
            Target::Release(v) => v.clone(),
            Target::Git { rev, .. } => pin::git_store_name(rev),
            Target::Path { abs, .. } => format!("path:{}", abs.display()),
        }
    }

    /// What the pin file gets.
    #[must_use]
    pub fn spec(&self) -> pin::PinSpec {
        match self {
            Target::Release(v) => pin::PinSpec::Release(v.clone()),
            Target::Git { url, rev } => pin::PinSpec::Git {
                url: url.clone(),
                rev: rev.clone(),
            },
            Target::Path { written, .. } => pin::PinSpec::Path(written.clone()),
        }
    }

    /// Whether `current` already names exactly this: the same form and the
    /// same value, so a git pin of the same commit from another URL moves.
    #[must_use]
    pub fn is(&self, current: &Pin) -> bool {
        match (self, &current.source) {
            (Target::Release(v), pin::PinSource::Release) => current.version == *v,
            (Target::Git { url, rev }, pin::PinSource::Git { url: u, rev: r }) => {
                url == u && rev == r
            }
            (Target::Path { abs, .. }, pin::PinSource::Path(d)) => abs == d,
            _ => false,
        }
    }
}

/// The checked target of the arguments, or `None` for a bare report.
///
/// `--rev` takes the loader's own rule ([`pin::full_rev`]); `--path` is
/// recorded relative to `project`, where the pin is written, and refused
/// unless it is a nano-ros checkout root ([`pin::path_spec`]).
pub fn target_of(
    project: &Path,
    current: Option<&Pin>,
    version: Option<&str>,
    rev: Option<&str>,
    git: Option<&str>,
    path: Option<&Path>,
) -> Result<Option<Target>> {
    if let Some(rev) = rev {
        let rev = pin::full_rev(rev)
            .map_err(|what| eyre::eyre!("`--rev {rev}` looks like {what}. {}", pin::REV_RULE))?;
        let url = match (git, current.map(|c| &c.source)) {
            (Some(u), _) => u.to_string(),
            (None, Some(pin::PinSource::Git { url, .. })) => url.clone(),
            (None, _) => pin::DEFAULT_GIT_URL.to_string(),
        };
        if url.is_empty() {
            bail!("`--git` names an empty URL.");
        }
        return Ok(Some(Target::Git { url, rev }));
    }
    if let Some(dir) = path {
        let written = pin::path_spec(project, dir)?;
        let abs = std::fs::canonicalize(dir)?;
        return Ok(Some(Target::Path { written, abs }));
    }
    Ok(version.map(|v| Target::Release(v.to_string())))
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
    // A checkout has no store entry to read a manifest from.
    let target_manifest = match target {
        Some(v) if !v.starts_with("path:") => release_manifest::for_store_version(root, v)?,
        _ => None,
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
            // See RFC-0095 D9.
            "pinned: NOTHING — no {} at or above {}.\n\
             \x20   This project floats: `nros build` will write a pin, and until it does, firmware built here is \
             reproducible only by accident.",
            pin::FILE_NAME,
            s.project.display()
        )),
    }
    let Some(target) = &s.target else {
        return out;
    };
    if let Some(dir) = target.strip_prefix("path:") {
        out.push(format!(
            "target: the nano-ros checkout {dir}\n\x20   codegen is that checkout's; the next \
             `nros build` regenerates the bindings if it differs."
        ));
        return out;
    }
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
            // See RFC-0097 D7.
            "{indent}codegen UNKNOWN — {} is not installed in this store, or it \
             predates share/nros/{}.",
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
    target: Option<&Target>,
    dry_run: bool,
    session: &Session,
    emit: &mut dyn FnMut(&str),
) -> Result<Outcome> {
    let key = target.map(Target::key);
    let s = survey(project, root, key.as_deref())?;
    for line in report(&s) {
        emit(&line);
    }
    let (Some(target), Some(version)) = (target, key) else {
        return Ok(Outcome::Reported);
    };
    if s.current.as_ref().is_some_and(|c| target.is(c)) {
        emit(&format!("pin already names {version} — unchanged."));
        return Ok(Outcome::Reported);
    }
    let path = project.join(pin::FILE_NAME);
    if dry_run {
        emit(&format!("--dry-run: would write {}", path.display()));
        return Ok(Outcome::WouldWrite { path, version });
    }
    // RFC-0097 D11: a pin is a source edit, and an automated session does not
    // make one. After the report, so CI still sees what would change.
    if let Some(var) = session.ci_var() {
        bail!(
            "refusing to write {} in an automated session (`{var}` is set): a pin is \
             a source edit. Move it at a keyboard and commit it, or set {}=1 if this job \
             is meant to.",
            path.display(),
            nros_launcher::session::ALLOW_ENV
        );
    }
    let written = pin::set_spec(project, &target.spec())?;
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
    let current = pin::find_and_load(&project)?;
    let target = target_of(
        &project,
        current.as_ref(),
        args.version.as_deref(),
        args.rev.as_deref(),
        args.git.as_deref(),
        args.path.as_deref(),
    )?;
    // Printed as produced, for the same reason the sink exists: the report must
    // have REACHED the user before the pin moves, and a buffer flushed at the
    // end cannot promise that when the write fails half way.
    execute(
        &project,
        &root,
        target.as_ref(),
        args.dry_run,
        &Session::detect(),
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
            let target = target.map(|v| Target::Release(v.to_string()));
            execute(
                project,
                root,
                target.as_ref(),
                dry_run,
                &Session::interactive(),
                &mut sink,
            )
            .unwrap()
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
        assert!(!joined.contains("nros configure"), "{joined}");
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
        assert!(warn_line.contains("nros configure"), "{warn_line}");
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
        assert!(joined.contains("nros configure"), "{joined}");
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

    // ---- issue 2770: the commit and checkout forms ----------------------

    const SHA: &str = "09219edc75c9b9a788590c6ac586a7ba45185eab";

    /// A directory the loader accepts as a nano-ros checkout root.
    fn checkout(at: &Path) -> PathBuf {
        let marker = at.join(nros_launcher::checkout::MONOREPO_MARKER);
        std::fs::create_dir_all(marker.parent().unwrap()).unwrap();
        std::fs::write(marker, "").unwrap();
        std::fs::canonicalize(at).unwrap()
    }

    /// Run `execute` with a checked target, at a keyboard or in CI.
    fn move_to(project: &Path, root: &Path, t: &Target, session: &Session) -> Result<Outcome> {
        execute(project, root, Some(t), false, session, &mut |_| {})
    }

    /// The `[toolchain]` keys the file holds: exactly one form's.
    fn keys(project: &Path) -> Vec<String> {
        let raw = std::fs::read_to_string(project.join(pin::FILE_NAME)).unwrap();
        let v: toml::Value = toml::from_str(&raw).unwrap();
        v["toolchain"].as_table().unwrap().keys().cloned().collect()
    }

    fn rev_target(
        project: &Path,
        current: Option<&Pin>,
        rev: &str,
        git: Option<&str>,
    ) -> Result<Option<Target>> {
        target_of(project, current, None, Some(rev), git, None)
    }

    #[test]
    fn rev_writes_git_and_rev_and_replaces_a_release_pin() {
        let dir = tempfile::tempdir().unwrap();
        let (root, project) = (dir.path().join("store"), dir.path().join("proj"));
        std::fs::create_dir_all(&project).unwrap();
        std::fs::write(project.join(pin::FILE_NAME), pin::render("0.7.9-nros1")).unwrap();
        let current = pin::find_and_load(&project).unwrap();

        // Upper case is the same commit; the file holds it lowercased.
        let t = rev_target(&project, current.as_ref(), &SHA.to_uppercase(), None)
            .unwrap()
            .unwrap();
        assert!(matches!(
            move_to(&project, &root, &t, &Session::interactive()).unwrap(),
            Outcome::Wrote { .. }
        ));
        assert_eq!(keys(&project), ["git", "rev"]);
        let back = pin::load(&project.join(pin::FILE_NAME)).unwrap();
        assert_eq!(
            back.source,
            pin::PinSource::Git {
                url: pin::DEFAULT_GIT_URL.to_string(),
                rev: SHA.to_string()
            }
        );
    }

    #[test]
    fn rev_refuses_a_short_sha_a_branch_and_a_tag() {
        let dir = tempfile::tempdir().unwrap();
        for (rev, what) in [
            ("09219edc75", "10-character sha"),
            ("main", "branch or tag name"),
            ("nros-0.6.0-nros11", "branch or tag name"),
            ("", "branch or tag name"),
        ] {
            let e = rev_target(dir.path(), None, rev, None)
                .unwrap_err()
                .to_string();
            assert!(e.contains(what), "{rev:?}: {e}");
            assert!(e.contains("git rev-parse"), "{rev:?}: {e}");
        }
        let e = rev_target(dir.path(), None, SHA, Some(""))
            .unwrap_err()
            .to_string();
        assert!(e.contains("empty URL"), "{e}");
    }

    #[test]
    fn rev_keeps_the_pin_s_own_git_unless_told_otherwise() {
        let dir = tempfile::tempdir().unwrap();
        let mirror = "https://example.invalid/mirror/nano-ros";
        let project = dir.path();
        pin::set_spec(
            project,
            &pin::PinSpec::Git {
                url: mirror.into(),
                rev: SHA.into(),
            },
        )
        .unwrap();
        let current = pin::find_and_load(project).unwrap();
        let other = "a".repeat(40);
        let Some(Target::Git { url, .. }) =
            rev_target(project, current.as_ref(), &other, None).unwrap()
        else {
            panic!("not a git target");
        };
        assert_eq!(url, mirror);
        let Some(Target::Git { url, .. }) = rev_target(
            project,
            current.as_ref(),
            &other,
            Some("https://x.invalid/n"),
        )
        .unwrap() else {
            panic!("not a git target");
        };
        assert_eq!(url, "https://x.invalid/n");
    }

    /// The same commit from another repository is a move, not "unchanged".
    #[test]
    fn the_same_rev_from_another_git_moves_the_pin() {
        let dir = tempfile::tempdir().unwrap();
        let (root, project) = (dir.path().join("store"), dir.path().join("proj"));
        std::fs::create_dir_all(&project).unwrap();
        pin::set_spec(
            &project,
            &pin::PinSpec::Git {
                url: pin::DEFAULT_GIT_URL.into(),
                rev: SHA.into(),
            },
        )
        .unwrap();
        let same = Target::Git {
            url: pin::DEFAULT_GIT_URL.into(),
            rev: SHA.into(),
        };
        assert_eq!(
            move_to(&project, &root, &same, &Session::interactive()).unwrap(),
            Outcome::Reported
        );
        let mirror = Target::Git {
            url: "https://example.invalid/n".into(),
            rev: SHA.into(),
        };
        assert!(matches!(
            move_to(&project, &root, &mirror, &Session::interactive()).unwrap(),
            Outcome::Wrote { .. }
        ));
    }

    #[test]
    fn path_writes_the_checkout_relative_to_the_pin_and_replaces_a_git_pin() {
        let dir = tempfile::tempdir().unwrap();
        let (root, project) = (dir.path().join("store"), dir.path().join("ws/proj"));
        std::fs::create_dir_all(&project).unwrap();
        let co = checkout(&dir.path().join("modules/nros"));
        pin::set_spec(
            &project,
            &pin::PinSpec::Git {
                url: pin::DEFAULT_GIT_URL.into(),
                rev: SHA.into(),
            },
        )
        .unwrap();

        let t = target_of(&project, None, None, None, None, Some(&co))
            .unwrap()
            .unwrap();
        assert!(matches!(
            move_to(&project, &root, &t, &Session::interactive()).unwrap(),
            Outcome::Wrote { .. }
        ));
        assert_eq!(keys(&project), ["path"]);
        let raw = std::fs::read_to_string(project.join(pin::FILE_NAME)).unwrap();
        assert!(raw.contains("path = \"../../modules/nros\""), "{raw}");
        assert_eq!(
            pin::load(&project.join(pin::FILE_NAME)).unwrap().source,
            pin::PinSource::Path(co)
        );

        // And a release pin replaces it in turn.
        let t = Target::Release("0.7.9-nros1".into());
        move_to(&project, &root, &t, &Session::interactive()).unwrap();
        assert_eq!(keys(&project), ["version"]);
    }

    #[test]
    fn path_refuses_a_directory_that_is_not_a_checkout_root() {
        let dir = tempfile::tempdir().unwrap();
        let not = dir.path().join("plain");
        std::fs::create_dir_all(&not).unwrap();
        let e = target_of(dir.path(), None, None, None, None, Some(&not))
            .unwrap_err()
            .to_string();
        assert!(e.contains("not a nano-ros checkout"), "{e}");
        let e = target_of(
            dir.path(),
            None,
            None,
            None,
            None,
            Some(&dir.path().join("missing")),
        )
        .unwrap_err()
        .to_string();
        assert!(e.contains("does not exist"), "{e}");
    }

    /// RFC-0097 D11: in CI every form reports and then refuses, and the pin is
    /// untouched; `--dry-run` still answers.
    #[test]
    fn in_ci_every_form_refuses_to_write() {
        let dir = tempfile::tempdir().unwrap();
        let (root, project) = (dir.path().join("store"), dir.path().join("proj"));
        std::fs::create_dir_all(&project).unwrap();
        std::fs::write(project.join(pin::FILE_NAME), pin::render("0.7.9-nros1")).unwrap();
        let before = std::fs::read_to_string(project.join(pin::FILE_NAME)).unwrap();
        let co = checkout(&dir.path().join("nros"));
        let ci = Session::automated("CI");
        for t in [
            Target::Release("0.8.0-nros1".into()),
            Target::Git {
                url: pin::DEFAULT_GIT_URL.into(),
                rev: SHA.into(),
            },
            target_of(&project, None, None, None, None, Some(&co))
                .unwrap()
                .unwrap(),
        ] {
            let e = move_to(&project, &root, &t, &ci).unwrap_err().to_string();
            assert!(e.contains("automated session") && e.contains("`CI`"), "{e}");
            assert_eq!(
                std::fs::read_to_string(project.join(pin::FILE_NAME)).unwrap(),
                before
            );
            let dry = execute(&project, &root, Some(&t), true, &ci, &mut |_| {}).unwrap();
            assert!(matches!(dry, Outcome::WouldWrite { .. }), "{dry:?}");
        }
    }
}
