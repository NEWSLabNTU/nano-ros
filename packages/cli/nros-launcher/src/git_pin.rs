//! Install a git pin: one nano-ros commit, built from source into the store
//! (issue 2645).
//!
//! ```text
//!   [toolchain]
//!   git = "https://github.com/NEWSLabNTU/nano-ros"
//!   rev = "<40-hex commit>"
//!
//!   $NROS_HOME/toolchains/git-<rev>/            a clone of that one commit
//!       packages/cli/target/release/nros       built by its own bootstrap.sh
//!       bin/nros -> ../packages/cli/target/release/nros
//! ```
//!
//! ## Why the store holds a checkout and not a staged prefix
//!
//! A release asset is a staged prefix (`bin/`, `share/nros/`,
//! `share/nano-ros/`) that a release workflow assembles. A commit has no such
//! asset, and re-creating that staging here would be a second release pipeline.
//! The tree's own front door, `scripts/bootstrap.sh` with no subcommand,
//! already builds a working CLI in place: the `nros` CLI, its launch resolver
//! and the companion packages of `scripts/lib/cli-companion-packages.txt`, all
//! at the paths a checkout's CLI looks for them. So the pinned toolchain IS a
//! checkout, and it behaves exactly like a contributor's: its stale guard
//! stamps its own pristine tree (nothing writes tracked files there), and its
//! runtime root is that tree.
//!
//! ## What it needs from the host
//!
//! `git`, `cargo` (a rustup toolchain; the tree's `rust-toolchain.toml` picks
//! the version), a C compiler and linker, Python 3 with headers (the resolver
//! embeds CPython through pyo3) and the network. `bootstrap.sh` installs rustup
//! when `cargo` is absent; this does NOT, and refuses instead: a toolchain
//! install that edits `~/.cargo` and a shell rc is the user's decision, not a
//! side effect of reading a pin.
//!
//! ## Atomic, and never rebuilt in place
//!
//! The build happens in a sibling `toolchains/.git-<rev>.partial-<pid>/` and is
//! renamed to `git-<rev>/` only after the build and `nros source-stamp` both
//! succeed; on failure the partial directory is deleted. The lookup path is
//! CONSTRUCTED from the pin, so `git-<rev>/` either exists complete or not at
//! all, and a crash that skips the cleanup leaves only a `.partial-*` name no
//! lookup reads; the next install deletes it once its pid is gone. Two
//! installs of one rev race to the rename; the loser deletes its copy and uses
//! the winner's. A moved pin is a new directory.

use std::{
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use eyre::{Result, WrapErr, bail, eyre};

use crate::pin;

/// `<store>/toolchains`.
fn toolchains(store: &Path) -> PathBuf {
    store.join("toolchains")
}

/// The finished directory of a git pin.
#[must_use]
pub fn store_dir(store: &Path, rev: &str) -> PathBuf {
    toolchains(store).join(pin::git_store_name(rev))
}

/// The partial-directory prefix of a rev; the pid follows it.
fn partial_prefix(rev: &str) -> String {
    format!(".{}.partial-", pin::git_store_name(rev))
}

/// Is `pid` a live process? Unknown (non-Linux) counts as live, so a partial
/// directory is never deleted from under an install that may still run.
fn pid_is_live(pid: u32) -> bool {
    if cfg!(target_os = "linux") {
        Path::new("/proc").join(pid.to_string()).exists()
    } else {
        true
    }
}

/// Delete the `.git-*.partial-<pid>` directories whose installer is gone.
/// Returns what it removed.
pub fn reap_partials(store: &Path) -> Vec<PathBuf> {
    let mut removed = Vec::new();
    let Ok(entries) = std::fs::read_dir(toolchains(store)) else {
        return removed;
    };
    for e in entries.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        if !name.starts_with(".git-") {
            continue;
        }
        let Some((_, pid)) = name.rsplit_once(".partial-") else {
            continue;
        };
        let Ok(pid) = pid.parse::<u32>() else {
            continue;
        };
        if pid != std::process::id()
            && !pid_is_live(pid)
            && std::fs::remove_dir_all(e.path()).is_ok()
        {
            removed.push(e.path());
        }
    }
    removed
}

/// Is `tool` on `path` (a `PATH` value)?
fn on_path(tool: &str, path: &std::ffi::OsStr) -> bool {
    std::env::split_paths(path).any(|d| d.join(tool).is_file())
}

/// `PATH` without the directories that hold an `nros`.
///
/// `bootstrap.sh`'s front door is a no-op when an `nros` is already on `PATH`
/// (it reads that as "already installed"), and the launcher that runs this
/// install is one. Taking those directories out makes it build this tree's.
fn path_without_nros(path: &std::ffi::OsStr) -> std::ffi::OsString {
    let kept: Vec<PathBuf> = std::env::split_paths(path)
        .filter(|d| !d.join("nros").exists())
        .collect();
    std::env::join_paths(kept).unwrap_or_default()
}

/// Run one step, its output on stderr (stdout is the toolchain's, after the
/// exec). `Err` names the step.
fn run(step: &str, cmd: &mut Command) -> Result<()> {
    let status = cmd
        .stdout(Stdio::from(std::io::stderr()))
        .status()
        .wrap_err_with(|| format!("{step}: could not start {:?}", cmd.get_program()))?;
    if !status.success() {
        bail!("{step} failed ({status})");
    }
    Ok(())
}

/// Install the git pin `url`@`rev` into `store`, or return the one that is
/// already there. Returns the binary to exec (`toolchains/git-<rev>/bin/nros`).
pub fn install(store: &Path, url: &str, rev: &str, pin_path: &Path) -> Result<PathBuf> {
    let path = std::env::var_os("PATH").unwrap_or_default();
    install_with_path(store, url, rev, pin_path, &path)
}

/// [`install`] with `PATH` passed in, so a test runs it against stub tools
/// without touching the process environment.
pub fn install_with_path(
    store: &Path,
    url: &str,
    rev: &str,
    pin_path: &Path,
    path: &std::ffi::OsStr,
) -> Result<PathBuf> {
    let done = store_dir(store, rev);
    let bin = done.join("bin").join("nros");
    if bin.is_file() {
        return Ok(bin);
    }
    let path = path.to_os_string();
    let build_path = path_without_nros(&path);
    for tool in ["git", "cargo"] {
        if !on_path(tool, &build_path) {
            bail!(
                "{} pins nano-ros git {rev}, which is built from source, and `{tool}` \
                 is not on PATH{}.\n\
                 A git pin needs git, cargo (rustup: https://rustup.rs), a C \
                 compiler, and Python 3 with headers. Installing rustup is your \
                 decision, so this does not do it. Install it and run again.",
                pin_path.display(),
                if on_path(tool, &path) {
                    " outside directories that also hold an `nros`"
                } else {
                    ""
                }
            );
        }
    }
    std::fs::create_dir_all(toolchains(store))
        .wrap_err_with(|| format!("create {}", toolchains(store).display()))?;
    reap_partials(store);
    let partial = toolchains(store).join(format!("{}{}", partial_prefix(rev), std::process::id()));
    if partial.exists() {
        std::fs::remove_dir_all(&partial).ok();
    }
    eprintln!(
        "nros: nano-ros git {rev} is pinned by {} and not installed -- \
         fetching and building it into {}",
        pin_path.display(),
        done.display()
    );
    match build(&partial, url, rev, &build_path) {
        Ok(()) => {}
        Err(e) => {
            std::fs::remove_dir_all(&partial).ok();
            return Err(e.wrap_err(format!(
                "could not install nano-ros git {rev} from {url}\n    pinned by: {}",
                pin_path.display()
            )));
        }
    }
    if let Err(e) = std::fs::rename(&partial, &done) {
        // Lost the race to another install of the same rev: theirs is complete
        // (a finished name only ever appears by rename), so use it.
        std::fs::remove_dir_all(&partial).ok();
        if !bin.is_file() {
            return Err(eyre!(
                "rename {} -> {}: {e}",
                partial.display(),
                done.display()
            ));
        }
    }
    Ok(bin)
}

/// Fetch, build and verify into `dir`.
fn build(dir: &Path, url: &str, rev: &str, build_path: &std::ffi::OsStr) -> Result<()> {
    let git = |args: &[&str]| {
        let mut c = Command::new("git");
        c.arg("-C").arg(dir).args(args).env("PATH", build_path);
        c
    };
    run(
        "git init",
        Command::new("git")
            .args(["init", "-q"])
            .arg(dir)
            .env("PATH", build_path),
    )?;
    // One commit, by sha: the server needs no ref that names it.
    run(
        &format!("git fetch {url} {rev}"),
        &mut git(&["fetch", "-q", "--depth", "1", url, rev]),
    )?;
    run(
        "git checkout",
        &mut git(&["checkout", "-q", "--detach", "FETCH_HEAD"]),
    )?;
    let head = git(&["rev-parse", "HEAD"])
        .output()
        .wrap_err("git rev-parse HEAD")?;
    let head = String::from_utf8_lossy(&head.stdout).trim().to_string();
    if head != rev {
        bail!("fetched {head}, not the pinned {rev}");
    }
    // The submodule the CLI build needs is resolved against the URL it was
    // fetched from, so a relative `.gitmodules` URL keeps working.
    run(
        "git remote add origin",
        &mut git(&["remote", "add", "origin", url]),
    )?;
    let bootstrap = dir.join("scripts").join("bootstrap.sh");
    if !bootstrap.is_file() {
        bail!(
            "{rev} has no scripts/bootstrap.sh, so it cannot be built from source \
             this way (is {url} a nano-ros repository?)"
        );
    }
    run(
        "scripts/bootstrap.sh (build the nros CLI)",
        Command::new("bash")
            .arg(&bootstrap)
            .arg("--no-prompt")
            .current_dir(dir)
            .env("PATH", build_path)
            .stdin(Stdio::null()),
    )?;
    let built = pin::checkout_cli_bin(dir);
    if !built.is_file() {
        bail!(
            "scripts/bootstrap.sh finished but built no {}",
            built.display()
        );
    }
    // The built CLI must agree with the tree it was built from, or the stale
    // guard refuses it on its first guarded verb.
    run(
        "nros source-stamp (the built CLI matches its tree)",
        Command::new(&built)
            .arg("source-stamp")
            .current_dir(dir)
            .env_remove("NROS_SKIP_STALE_CHECK"),
    )?;
    let bin_dir = dir.join("bin");
    std::fs::create_dir_all(&bin_dir)?;
    // `bin/` sits in the tree's root, so the CLI is one level up from it.
    link(
        &pin::checkout_cli_bin(Path::new("..")),
        &bin_dir.join("nros"),
    )?;
    Ok(())
}

#[cfg(unix)]
fn link(target: &Path, at: &Path) -> Result<()> {
    std::os::unix::fs::symlink(target, at)
        .wrap_err_with(|| format!("symlink {} -> {}", at.display(), target.display()))
}

#[cfg(not(unix))]
fn link(target: &Path, at: &Path) -> Result<()> {
    let from = at.parent().unwrap_or(Path::new(".")).join(target);
    std::fs::copy(&from, at)
        .map(|_| ())
        .wrap_err_with(|| format!("copy {} -> {}", from.display(), at.display()))
}
