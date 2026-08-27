//! Stage 5 — hand the build to the native tool (phase-383 W2.e, RFC-0065 D1).
//!
//! ## Why this file is one function and a lot of prose
//!
//! `nros build` is admissible at all only because of what happens here.
//! RFC-0024 §2.4 marks "nros never a build verb" LOCKED, and §9 rejects it as
//! *"re-creates colcon's wrapping anti-pattern; hides cargo/cmake
//! diagnostics."* RFC-0065 amends that in SCOPE, not in principle, on one
//! technical claim:
//!
//! > Stage 5 is `exec`, not a pipe. The native tool REPLACES the process. A
//! > rustc error is byte-identical to `cargo build`'s, because nothing is
//! > capturing it.
//!
//! **If this ever becomes a pipe, the amendment is void.** Not "degraded" —
//! void, because the objection it answers is exactly "the wrapper swallowed my
//! diagnostics". The temptations are ordinary and will arrive: a progress bar,
//! a log file, a summary line, colour stripping, an exit-code remap. Each one
//! needs `Stdio::piped()`, and each one re-creates what phase-222 deleted.
//!
//! This is also precisely what colcon CANNOT do — its per-package task model
//! must capture output in order to attribute it to a package. The thing that
//! makes our diagnostics honest is the same thing that makes colcon's
//! unavoidable.
//!
//! ## What `exec` buys, concretely
//!
//! * stdout/stderr are the terminal's, so colour detection, line buffering and
//!   `isatty` behave exactly as they do under a bare `cargo build`;
//! * the exit code is the tool's own, not a remap;
//! * signals (Ctrl-C, SIGTERM) reach the compiler directly — no orphaned
//!   ninja, no zombie child holding a build lock;
//! * there is no second process in `ps`, so a hung build is attributable.
//!
//! ## Non-Unix
//!
//! `execvp` does not exist on Windows. nano-ros builds host-side on Unix only,
//! so rather than silently falling back to a spawn-and-wait — which would be a
//! pipe-shaped hole in the guarantee on exactly the platform nobody tests —
//! the non-Unix path refuses and says why.

use std::{ffi::OsString, path::PathBuf};

/// A fully-resolved native build command, ready to replace this process.
///
/// Deliberately inert: constructing one performs no I/O, so a caller can build
/// it, print it (`--dry-run`), assert on it in a test, and only then hand over.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Handoff {
    /// Program to exec — `cargo`, `cmake`, `west`, `idf.py`. Resolved through
    /// `PATH` by `execvp`, deliberately: the user's activated environment is
    /// what should decide which `west` runs, not a path we baked at build time.
    pub program: OsString,
    /// Arguments, NOT including argv[0].
    pub args: Vec<OsString>,
    /// Working directory to change into first. `None` keeps the caller's.
    pub cwd: Option<PathBuf>,
    /// Environment additions applied before the exec. Never a full env
    /// replacement: a build needs the user's `PATH`, `HOME`, `SSH_AUTH_SOCK`
    /// and the whole activated toolchain environment.
    pub env: Vec<(OsString, OsString)>,
}

impl Handoff {
    /// A handoff to `program` with `args`.
    pub fn new<P, A, I>(program: P, args: I) -> Self
    where
        P: Into<OsString>,
        A: Into<OsString>,
        I: IntoIterator<Item = A>,
    {
        Self {
            program: program.into(),
            args: args.into_iter().map(Into::into).collect(),
            cwd: None,
            env: Vec::new(),
        }
    }

    /// Run the command from `dir`.
    #[must_use]
    pub fn in_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.cwd = Some(dir.into());
        self
    }

    /// Add one environment variable.
    #[must_use]
    pub fn with_env(mut self, key: impl Into<OsString>, value: impl Into<OsString>) -> Self {
        self.env.push((key.into(), value.into()));
        self
    }

    /// Render the command the way a user could retype it.
    ///
    /// For `--dry-run` and for error messages. NOT shell-quoted, and it must
    /// not become the thing a shell runs: this is for a human to read.
    #[must_use]
    pub fn display(&self) -> String {
        let mut s = String::new();
        for (k, v) in &self.env {
            s.push_str(&format!("{}={} ", k.to_string_lossy(), v.to_string_lossy()));
        }
        s.push_str(&self.program.to_string_lossy());
        for a in &self.args {
            s.push(' ');
            s.push_str(&a.to_string_lossy());
        }
        s
    }
}

/// Replace this process with the native build tool. **Never returns on
/// success.**
///
/// The return type says so: an `Ok` value is unconstructible, so a caller
/// cannot accidentally write code that assumes control comes back. Every
/// The same command as a `std::process::Command`, for a step that must RETURN.
///
/// Stage 5 execs and never comes back, which is the guarantee RFC-0065 D1
/// rests on. A driver that needs a configure FIRST (cmake: `cmake --build` on
/// an unconfigured tree fails, and configure+build is two invocations at our
/// 3.22 floor) needs to run one command and survive it. Same struct, so the
/// configure a `--dry-run` prints is the configure that runs.
impl Handoff {
    #[must_use]
    pub fn command(&self) -> std::process::Command {
        let mut cmd = std::process::Command::new(&self.program);
        cmd.args(&self.args);
        if let Some(dir) = &self.cwd {
            cmd.current_dir(dir);
        }
        for (k, v) in &self.env {
            cmd.env(k, v);
        }
        cmd
    }
}

/// return is an error that happened BEFORE the handover — a missing `cwd`, or
/// a program `execvp` could not find.
#[cfg(unix)]
pub fn exec(handoff: &Handoff) -> Result<std::convert::Infallible, String> {
    use std::os::unix::process::CommandExt;

    let mut cmd = std::process::Command::new(&handoff.program);
    cmd.args(&handoff.args);
    if let Some(dir) = &handoff.cwd {
        if !dir.is_dir() {
            return Err(format!(
                "build directory {} does not exist — stage 4 should have created it",
                dir.display()
            ));
        }
        cmd.current_dir(dir);
    }
    for (k, v) in &handoff.env {
        cmd.env(k, v);
    }

    // NO Stdio::piped() ANYWHERE. See the module docs: the whole RFC-0024 §2.4
    // amendment rests on this. Inheriting is the default, and it is stated
    // rather than assumed so that a future edit has to delete a comment to
    // break it.
    //
    // `exec` returns only on failure — on success this process IS the compiler.
    let err = cmd.exec();
    Err(format!(
        "could not exec `{}`: {err}",
        handoff.program.to_string_lossy()
    ))
}

/// Non-Unix: refuse rather than silently degrade to spawn-and-wait.
#[cfg(not(unix))]
pub fn exec(handoff: &Handoff) -> Result<std::convert::Infallible, String> {
    Err(format!(
        "`nros build` hands off with execvp, which this platform does not \
         provide, so the diagnostic guarantee (RFC-0065 D1) cannot be kept \
         here. Run the native command directly:\n  {}",
        handoff.display()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_handoff_is_inert_until_exec() {
        // Constructing one must do no I/O: `--dry-run` prints it, tests assert
        // on it, and only then does anyone hand over.
        let h = Handoff::new("cargo", ["build", "-p", "native_entry"])
            .in_dir("/nonexistent/build/dir")
            .with_env("CARGO_TERM_COLOR", "always");
        assert_eq!(h.program, OsString::from("cargo"));
        assert_eq!(h.args.len(), 3);
        assert_eq!(
            h.cwd.as_deref(),
            Some(std::path::Path::new("/nonexistent/build/dir"))
        );
    }

    #[test]
    fn display_is_retypable_by_a_human() {
        let h = Handoff::new("west", ["build", "-b", "native_sim/native/64"])
            .with_env("ZEPHYR_BASE", "/opt/zephyr");
        assert_eq!(
            h.display(),
            "ZEPHYR_BASE=/opt/zephyr west build -b native_sim/native/64"
        );
    }

    #[test]
    fn a_missing_build_dir_fails_before_handing_over() {
        // The error must name stage 4, because that is who was supposed to
        // create the directory.
        let h = Handoff::new("cargo", ["build"]).in_dir("/definitely/not/here");
        let e = exec(&h).expect_err("must refuse");
        assert!(e.contains("/definitely/not/here"), "{e}");
        assert!(
            e.contains("stage 4"),
            "points at the responsible stage: {e}"
        );
    }

    #[test]
    fn an_unfindable_program_reports_the_program_name() {
        let h = Handoff::new("nros-no-such-build-tool-exists", ["build"]);
        let e = exec(&h).expect_err("must fail");
        assert!(e.contains("nros-no-such-build-tool-exists"), "{e}");
    }

    /// The guard for the whole RFC-0024 §2.4 amendment.
    ///
    /// A source scan, deliberately: the property is "this file never captures
    /// output", and no runtime test can prove a negative about code paths it
    /// does not take. A reviewer adding a progress bar has to delete this test
    /// to land it, which is the point.
    #[test]
    fn stage_five_never_pipes_output() {
        let src = include_str!("handoff.rs");
        // Only CODE is scanned. Doc comments and `//` lines legitimately name
        // the forbidden constructs — that is how a reader learns why they are
        // forbidden — so a naive substring scan over the whole file would fire
        // on its own explanation.
        let code: String = src
            .lines()
            .filter(|l| {
                let t = l.trim_start();
                !t.starts_with("//") && !t.starts_with("*") && !t.starts_with("#!")
            })
            // And drop this test's own body, whose literals are the pattern.
            .take_while(|l| !l.contains("fn stage_five_never_pipes_output"))
            .collect::<Vec<_>>()
            .join("\n");
        for forbidden in ["Stdio::piped", "Stdio::null", ".output()"] {
            assert!(
                !code.contains(forbidden),
                "stage 5 must never capture the native tool's output — found \
                 `{forbidden}`. RFC-0024 §2.4 is amended ONLY because this \
                 hands off with exec; a pipe voids that. See the module docs."
            );
        }
        assert!(
            code.contains("cmd.exec()"),
            "the handoff must still be an exec"
        );
    }
}
