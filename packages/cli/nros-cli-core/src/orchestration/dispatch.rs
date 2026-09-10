//! The launcher: read the pin, ensure that toolchain, `exec` it (RFC-0095 D8,
//! phase-440 W7).
//!
//! ```text
//!   $NROS_STORE/bin/nros   ->  sdk/nros/<newest>/bin/nros     the launcher
//!                              read nros-toolchain.toml
//!                              ensure sdk/nros/<pinned>
//!                              exec  sdk/nros/<pinned>/bin/nros "$@"
//! ```
//!
//! D8 states the whole design in one line — *"1. read the pin; 2. ensure that
//! toolchain exists; 3. `exec` it. Everything else lives in the toolchain's own
//! binary. A shim that does more becomes a shim-versus-toolchain compatibility
//! matrix, which is the problem it exists to avoid — and it must keep working
//! when it is OLDER than what it launches."*
//!
//! ## What "older than what it launches" forces
//!
//! It forces the decision to happen BEFORE clap. An older launcher does not
//! know the newer toolchain's flags, subcommands, or defaults, so anything it
//! parses it can get wrong, and anything it gets wrong it gets wrong for a
//! binary that would have got it right. So [`redispatch`] runs at the top of
//! `main`, reads only `nros-toolchain.toml` and the process's own path, and
//! forwards `argv` byte for byte. There is no arm in this module that inspects
//! a subcommand. That is the property [`decide`] exists to make testable.
//!
//! ## When it does nothing at all, and why each case
//!
//! * **the running binary is not a store toolchain** — a contributor's
//!   `packages/cli/target/**` build. It has no store version ([`pin::running_version`]),
//!   so there is no "am I already the pinned one?" to answer and no launcher
//!   role to play. This also means a contributor never meets dispatch by
//!   accident;
//! * **the cwd is inside a nano-ros checkout** — RFC-0095 D0: a contributor's
//!   nano-ros is their clone at whatever HEAD is, and D5's ownership guard
//!   already refuses a foreign binary there. Dispatching would fight it;
//! * **no pin** — the project floats; `nros build` will write one (D9) and the
//!   binary that writes it is the one that runs;
//! * **the pin names this binary** — the common steady state, and the one that
//!   must cost nothing: one `stat` of the pin file and no exec;
//! * **`NROS_TOOLCHAIN_DISPATCH` is already set** — we ARE the exec'd
//!   toolchain. Without this a pin naming a version whose directory holds a
//!   binary that disagrees about its own version is an exec loop, which is the
//!   one failure mode a launcher must not have.
//!
//! ## Ensure
//!
//! D6's second row makes fetching the toolchain automatic, *conditional on
//! D8's release artifact* — a condition `scripts/install.sh` (phase-431 W4)
//! already meets. So ensure DELEGATES to that installer rather than
//! reimplementing "which asset, which checksum, which prefix" in Rust: the
//! release asset carries it at `share/nros/install.sh`, and this runs it with
//! `NROS_INSTALL_VERSION` set to the pin. One implementation of the fetch, the
//! same argument `cmd::sdk_front` makes for one implementation of the front.
//!
//! A launcher whose asset predates that staging finds no installer and REFUSES,
//! naming the pin, the paths it looked in, and the command that fixes it. That
//! is RFC-0065 D2's shape, and it is the honest answer: a launcher that cannot
//! fetch must not pretend the version is missing for some other reason.

use std::{
    ffi::OsString,
    path::{Path, PathBuf},
};

use eyre::{Result, bail};

use super::pin;

/// Set on the child by [`redispatch`]. Its VALUE is the version that was asked
/// for, so a child that finds it disagreeing with its own store version can say
/// so rather than silently being the wrong toolchain.
pub const DISPATCHED_ENV: &str = "NROS_TOOLCHAIN_DISPATCH";

/// The deliberate-experiment escape hatch, in the same spelling family as
/// `NROS_SKIP_STALE_CHECK`.
pub const SKIP_ENV: &str = "NROS_SKIP_TOOLCHAIN_DISPATCH";

/// Everything [`decide`] needs, gathered once so the decision itself touches no
/// process state.
#[derive(Clone, Debug)]
pub struct Context {
    /// The running binary.
    pub exe: PathBuf,
    /// Where the project is — the cwd, in production.
    pub cwd: PathBuf,
    /// The store root (`store::root()` in production).
    pub store: PathBuf,
    /// `$NROS_TOOLCHAIN_DISPATCH`, if set.
    pub dispatched: Option<String>,
    /// `$NROS_SKIP_TOOLCHAIN_DISPATCH` is set.
    pub skip: bool,
}

/// What the launcher should do. Every arm names its reason, because "nothing
/// happened" has five causes here and a user debugging one needs to know which.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Decision {
    /// Carry on in this process. The `&'static str` is why.
    Proceed(&'static str),
    /// Replace this process with `bin`, which is the pinned toolchain.
    Exec { version: String, bin: PathBuf },
    /// The pin names a version this store does not have.
    Missing {
        version: String,
        pin_path: PathBuf,
        looked_in: Vec<PathBuf>,
    },
}

/// The launcher decision — a pure function of [`Context`] and the filesystem.
///
/// Reads exactly two things off disk: whether a pin file exists on the walk up
/// from `cwd`, and whether the pinned version's binary exists in the store. It
/// never reads `argv`.
pub fn decide(ctx: &Context) -> Result<Decision> {
    if ctx.skip {
        return Ok(Decision::Proceed("$NROS_SKIP_TOOLCHAIN_DISPATCH is set"));
    }
    if ctx.dispatched.is_some() {
        return Ok(Decision::Proceed(
            "this process IS the dispatched toolchain",
        ));
    }
    // A checkout is the contributor audience (RFC-0095 D0). Asked before the
    // pin, because an in-tree workspace may legitimately carry one for a test
    // and it must not aim a contributor at the store.
    if crate::abi_guard::find_monorepo_root(&ctx.cwd).is_some() {
        return Ok(Decision::Proceed("the cwd is inside a nano-ros checkout"));
    }
    let Some((mine, _origin)) = pin::running_version(&ctx.exe) else {
        return Ok(Decision::Proceed(
            "this binary is not a store toolchain, so it is not a launcher",
        ));
    };
    let Some(p) = pin::find_and_load(&ctx.cwd)? else {
        return Ok(Decision::Proceed("this project names no toolchain"));
    };
    if p.version == mine {
        return Ok(Decision::Proceed("the pin names this binary"));
    }
    match pin::installed_bin(&ctx.store, &p.version) {
        Some(bin) => Ok(Decision::Exec {
            version: p.version,
            bin,
        }),
        None => Ok(Decision::Missing {
            looked_in: pin::candidate_bins(&ctx.store, &p.version),
            version: p.version,
            pin_path: p.path,
        }),
    }
}

/// The installer this launcher's own release asset carries, if it carries one.
///
/// `<prefix>/share/nros/install.sh`, beside the `share/nros/VERSION` the asset
/// already ships. Not a search of the filesystem and not `$PATH`: the installer
/// that fetches a toolchain must be the one that came with this launcher, or
/// "one implementation of the fetch" is not a claim this can make.
#[must_use]
pub fn bundled_installer(exe: &Path) -> Option<PathBuf> {
    let prefix = exe.parent()?.parent()?;
    let script = prefix.join("share").join("nros").join("install.sh");
    script.is_file().then_some(script)
}

/// D8 job 2, performed: fetch the pinned toolchain through the installer that
/// shipped with this launcher.
///
/// Returns the binary it installed. `Err` names the pin and the remedy — never
/// a bare "not found", because at this point the user has a pin they wrote (or
/// that `nros build` wrote for them) and a store that does not answer it, and
/// the two commands that fix it are different.
fn fetch(ctx: &Context, version: &str, pin_path: &Path, looked_in: &[PathBuf]) -> Result<PathBuf> {
    let looked = looked_in
        .iter()
        .map(|p| format!("\n\x20     {}", p.display()))
        .collect::<String>();
    let Some(installer) = bundled_installer(&ctx.exe) else {
        bail!(
            "this project pins nano-ros {version} and the store does not have it.{looked}\n\
             \x20   pinned by: {}\n\
             This `nros` ships no installer, so it cannot fetch it. Install that \
             version and build again:\n\
             \x20   curl -fsSL https://raw.githubusercontent.com/NEWSLabNTU/nano-ros/main/scripts/install.sh \
             | sh -s -- --version {version}",
            pin_path.display()
        );
    };
    eprintln!(
        "nros: {version} is pinned by {} and not installed — fetching it ({})",
        pin_path.display(),
        installer.display()
    );
    let status = std::process::Command::new("sh")
        .arg(&installer)
        .arg("--version")
        .arg(version)
        .env("NROS_INSTALL_VERSION", version)
        .status()
        .map_err(|e| eyre::eyre!("running {}: {e}", installer.display()))?;
    if !status.success() {
        bail!(
            "could not install the pinned nano-ros {version} ({} exited {status}).\n\
             \x20   pinned by: {}\n\
             Fix the pin, or install that version by hand.",
            installer.display(),
            pin_path.display()
        );
    }
    pin::installed_bin(&ctx.store, version).ok_or_else(|| {
        eyre::eyre!(
            "the installer reported success but {version} is still not in the store.{looked}\n\
             \x20   pinned by: {}",
            pin_path.display()
        )
    })
}

/// The whole launcher, for production: gather, decide, act.
///
/// On success it either RETURNS (this process continues as the toolchain) or
/// NEVER RETURNS (it has become the pinned one). Called from `main` before
/// clap — see the module header for why that is not an optimisation.
pub fn redispatch() -> Result<()> {
    let Ok(exe) = std::env::current_exe() else {
        return Ok(());
    };
    let Ok(cwd) = std::env::current_dir() else {
        return Ok(());
    };
    let ctx = Context {
        exe,
        cwd,
        store: super::store::root(),
        dispatched: std::env::var(DISPATCHED_ENV).ok(),
        skip: std::env::var_os(SKIP_ENV).is_some(),
    };
    let bin = match decide(&ctx)? {
        Decision::Proceed(_) => return Ok(()),
        Decision::Exec { bin, .. } => bin,
        Decision::Missing {
            version,
            pin_path,
            looked_in,
        } => fetch(&ctx, &version, &pin_path, &looked_in)?,
    };
    let version = pin::running_version(&bin)
        .map(|(v, _)| v)
        .unwrap_or_default();
    let args: Vec<OsString> = std::env::args_os().skip(1).collect();
    exec(&bin, &version, &args)
}

/// Become `bin`. Never returns on success.
#[cfg(unix)]
fn exec(bin: &Path, version: &str, args: &[OsString]) -> Result<()> {
    use std::os::unix::process::CommandExt;
    let err = std::process::Command::new(bin)
        .args(args)
        .env(DISPATCHED_ENV, version)
        .exec();
    Err(eyre::eyre!("exec {}: {err}", bin.display()))
}

#[cfg(not(unix))]
fn exec(bin: &Path, version: &str, args: &[OsString]) -> Result<()> {
    let status = std::process::Command::new(bin)
        .args(args)
        .env(DISPATCHED_ENV, version)
        .status()
        .map_err(|e| eyre::eyre!("run {}: {e}", bin.display()))?;
    std::process::exit(status.code().unwrap_or(1));
}
