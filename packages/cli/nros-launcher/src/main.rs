//! `nros-launcher` — the binary users install, and the only one that outlives a
//! toolchain (RFC-0097 D4).
//!
//! Installed as `$NROS_STORE/bin/nros`, so what a user types is `nros`. It
//! resolves the project's pin, ensures that toolchain, and `exec`s it. That is
//! the whole program.
//!
//! ## It parses no `argv`, with ONE exception, and the exception is safe
//!
//! RFC-0095 D8: *"it must keep working when it is OLDER than what it
//! launches."* An older launcher does not know a newer toolchain's flags, so
//! anything it parses it can get wrong on behalf of a binary that would have
//! got it right. Every argument is therefore forwarded byte for byte.
//!
//! The exception is a leading `--version` / `-V`, and it does NOT change what
//! the child receives: the launcher prints one extra line and forwards the
//! argument anyway. It exists because the split has to be observable — a user
//! looking at one version string cannot tell which of the two artifacts
//! answered, and "the launcher has its own release cadence" is not a claim
//! anyone can check from the outside without it:
//!
//! ```console
//! $ nros --version
//! nros-launcher 0.1.0
//! nros 0.5.0
//! ```
//!
//! The second line is the toolchain's, printed after the `exec`. With no
//! toolchain installed the first line still appears — which is the point of
//! testing that state rather than assuming it.
//!
//! ## Exit codes
//!
//! `0` when it handed over (the child's code, after `exec`) or when it answered
//! `--version`; `1` for every terminal [`launch::Plan`] arm, each of which
//! prints what to do next. A launcher that cannot run the command must not exit
//! 0 — a CI job would read that as a build that passed.

use std::{ffi::OsString, process::ExitCode};

use nros_launcher::{
    LAUNCHER_VERSION, dispatch,
    launch::{self, Plan},
    session::Session,
    store_root,
};

/// Is the first argument a bare version query?
///
/// FIRST only, and an exact match. `nros build --version-file x` must not be
/// mistaken for one, and a subcommand's own `--version` (clap's
/// `propagate_version`) is the toolchain's business, not ours.
fn is_version_query(args: &[OsString]) -> bool {
    args.first()
        .and_then(|a| a.to_str())
        .is_some_and(|a| a == "--version" || a == "-V")
}

fn main() -> ExitCode {
    let args: Vec<OsString> = std::env::args_os().skip(1).collect();
    let version_query = is_version_query(&args);
    if version_query {
        // Before the resolution, so it is printed even when the resolution
        // fails — "which launcher is this?" is a question whose answer must not
        // depend on the store being in a good state.
        println!("nros-launcher {LAUNCHER_VERSION}");
    }

    let ctx = launch::Context {
        exe: std::env::current_exe().unwrap_or_else(|_| std::path::PathBuf::from("nros")),
        cwd: std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from(".")),
        store: store_root::root(),
        dispatched: std::env::var(dispatch::DISPATCHED_ENV).ok(),
        session: Session::detect(),
    };

    let plan = match launch::resolve(&ctx) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("nros: {e:?}");
            return ExitCode::FAILURE;
        }
    };

    // RFC-0097 D11's launcher half, printed BEFORE the handover because after
    // the `exec` this process is gone. `None` for every other plan.
    if let Some(w) = launch::warning(&plan) {
        eprintln!("nros: {w}");
    }

    let bin = match &plan {
        Plan::Exec { bin, .. } => bin.clone(),
        Plan::Missing {
            version,
            pin_path,
            looked_in,
        } => match dispatch::fetch(&ctx.exe, &ctx.store, version, pin_path, looked_in) {
            Ok(b) => b,
            Err(e) => {
                eprintln!("nros: {e:?}");
                return ExitCode::FAILURE;
            }
        },
        terminal => {
            eprintln!("nros: {}", launch::explain(terminal));
            // A version query was ANSWERED — the launcher's own line is above,
            // and the note explains why no toolchain line follows it. Reporting
            // failure there would make `nros --version` a broken command on a
            // host that simply has nothing installed yet.
            return if version_query {
                ExitCode::SUCCESS
            } else {
                ExitCode::FAILURE
            };
        }
    };

    let version = nros_launcher::pin::running_version(&bin)
        .map(|(v, _)| v)
        .unwrap_or_default();
    match dispatch::exec(&bin, &version, &args) {
        // `exec` does not return on success, so reaching here is the failure.
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("nros: {e:?}");
            ExitCode::FAILURE
        }
    }
}
