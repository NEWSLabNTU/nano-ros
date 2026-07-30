//! `nros ws …` — workspace-level msg-pkg surface.
//!
//! Phase 210.B.3 + 210.D.1 (locked design). Subcommands:
//!
//! * `env` — print shell export for `NROS_INTERFACE_SEARCH_PATH`.
//! * `sync` — scan workspace, codegen msg pkgs into
//!   `generated/<pkg>/`, write `[patch.crates-io]`
//!   block into the patch authority Cargo.toml so plain `cargo build`
//!   resolves `local_msgs = "*"` to the generated crate.
//!
//! **Dual-mode (`cargo`-style):** every subcommand works on BOTH layouts —
//! a multi-pkg colcon workspace (`<root>/src/<pkg>/package.xml`) AND a
//! single standalone pkg (`<root>/package.xml`). Detection runs at command
//! time:
//!
//!   * **colcon-mode** iff `<root>/src/` exists AND at least one
//!     immediate subdir contains `package.xml`.
//!   * **single-pkg mode** iff `<root>/package.xml` exists and the colcon
//!     check fails.
//!
//! Mirrors `cargo build` which works at either a workspace root or a
//! standalone pkg dir without special arg.
//!
//! See `docs/roadmap/phase-210-ros-convention-codegen.md` for the
//! full design (patch authority detection, colcon-shape build dir,
//! the chicken-egg motivation for a pre-cargo sync step).

use clap::{Args as ClapArgs, Subcommand, ValueEnum};
use eyre::{Result, WrapErr, bail, eyre};
use rosidl_bindgen::ament::Package;
use rosidl_codegen::RosEdition;
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
};

#[derive(Debug, ClapArgs)]
pub struct Args {
    #[command(subcommand)]
    pub command: Sub,
}

#[derive(Debug, Subcommand)]
pub enum Sub {
    /// Print shell export adding <dir> (default `./src`) to
    /// `NROS_INTERFACE_SEARCH_PATH`. `eval "$(nros ws env)"`.
    Env(EnvArgs),

    /// DEPRECATED alias for `nros sync` (phase-265 W5). Kept for one release
    /// cycle. Hidden from help; emits a one-line deprecation note then runs
    /// the same codegen + `.cargo/config.toml` patch sync.
    #[command(hide = true)]
    Sync(SyncArgs),

    /// List discovered msg + rust-consumer pkgs in the workspace (or
    /// single pkg). Prints kind, name, dir per row. (Phase 210.F.3.)
    List(ListArgs),

    /// Freshness check — non-fatal sibling of `sync --check`. Prints a
    /// one-line summary of `n up-to-date / n stale / n missing`.
    Status(StatusArgs),

    /// Remove `generated/` + the auto-managed
    /// `[patch.crates-io]` block from each Rust consumer's patch authority
    /// Cargo.toml. Leaves user-written sections alone.
    Clean(CleanArgs),

    /// Lint workspace pkgs: warn on missing `<member_of_group>
    /// rosidl_interface_packages</member_of_group>` markers, malformed
    /// `package.xml`, stale patch blocks. Mirrors the sync detection.
    Doctor(DoctorArgs),
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum Shell {
    /// POSIX-shell `export VAR=…` (bash/zsh/sh).
    Posix,
    /// Fish-shell `set -gx VAR …`.
    Fish,
}

#[derive(Debug, ClapArgs)]
pub struct EnvArgs {
    /// Workspace root containing pkg subdirs with `package.xml`. Defaults
    /// to `./src` (the colcon-standard layout).
    pub workspace: Option<PathBuf>,

    /// Output shell flavour.
    #[arg(long, value_enum, default_value = "posix")]
    pub shell: Shell,
}

#[derive(Debug, ClapArgs)]
pub struct SyncArgs {
    /// Workspace root (the dir containing `src/`). Defaults to cwd.
    pub workspace: Option<PathBuf>,

    /// Output dir for generated msg crates (Phase 212 convention is `generated/`).
    #[arg(long, default_value = "generated")]
    pub build_dir: PathBuf,

    /// ROS 2 edition (`humble` | `iron` | `jazzy`). When omitted,
    /// inherits `[system].ros_edition` from a `system.toml` at the workspace root
    /// (RFC-0056 W2b auto-lowering), else `humble`.
    #[arg(long)]
    pub ros_edition: Option<String>,

    /// Don't write — just print what would happen.
    #[arg(long)]
    pub dry_run: bool,

    /// Exit non-zero if any patch block is missing or stale (CI hook;
    /// also used by `nros ws status`).
    #[arg(long)]
    pub check: bool,

    /// Verbose codegen output.
    #[arg(short, long)]
    pub verbose: bool,

    /// phase-307 W2 — skip the source-metadata refresh. The refresh compiles a
    /// host probe per Node pkg, which is the slow part of a cold sync; skipping
    /// it leaves any existing sidecars untouched and makes bakes fall back to
    /// the SystemModel's entity lower bound.
    #[arg(long)]
    pub no_metadata: bool,

    /// Path to the nano-ros source tree. Accepted for back-compat but
    /// currently a NO-OP since post-212 alignment: the canonical 212
    /// shape carries nros-* runtime crates as path-deps in the user's
    /// own `[dependencies]`, so duplicating them in the patch block
    /// triggers cargo's "patch unused" warnings. Falls back to the env
    /// var `NROS_REPO_DIR` (cmake-side contract) when the flag is
    /// omitted.
    #[arg(long)]
    pub nano_ros_path: Option<PathBuf>,
}

#[derive(Debug, ClapArgs)]
pub struct ListArgs {
    /// Workspace root (cwd or first ancestor containing `src/`). Defaults
    /// to cwd.
    pub workspace: Option<PathBuf>,
}

#[derive(Debug, ClapArgs)]
pub struct StatusArgs {
    pub workspace: Option<PathBuf>,
    #[arg(long, default_value = "generated")]
    pub build_dir: PathBuf,
}

#[derive(Debug, ClapArgs)]
pub struct CleanArgs {
    pub workspace: Option<PathBuf>,
    #[arg(long, default_value = "generated")]
    pub build_dir: PathBuf,
    /// Don't write — just print what would be removed.
    #[arg(long)]
    pub dry_run: bool,
}

#[derive(Debug, ClapArgs)]
pub struct DoctorArgs {
    pub workspace: Option<PathBuf>,
    #[arg(long, default_value = "generated")]
    pub build_dir: PathBuf,
}

pub fn run(args: Args) -> Result<()> {
    match args.command {
        Sub::Env(a) => run_env(a),
        Sub::Sync(a) => {
            eprintln!("note: `nros ws sync` is deprecated; use `nros sync` (phase-265).");
            run_sync(a)
        }
        Sub::List(a) => run_list(a),
        Sub::Status(a) => run_status(a),
        Sub::Clean(a) => run_clean(a),
        Sub::Doctor(a) => run_doctor(a),
    }
}

// =============================================================================
// `nros ws env`
// =============================================================================

fn run_env(args: EnvArgs) -> Result<()> {
    let abs = resolve_env_root(args.workspace.as_deref())?;
    let abs_s = abs.display().to_string();
    match args.shell {
        Shell::Posix => {
            println!(
                "export NROS_INTERFACE_SEARCH_PATH=\"{abs_s}:${{NROS_INTERFACE_SEARCH_PATH:-}}\""
            );
        }
        Shell::Fish => {
            println!("set -gx NROS_INTERFACE_SEARCH_PATH \"{abs_s}\" $NROS_INTERFACE_SEARCH_PATH");
        }
    }
    Ok(())
}

/// Resolve the dir the cmake-side smart Find-stub will scan as a
/// `NROS_INTERFACE_SEARCH_PATH` entry. Mirrors `sync`'s dual-mode
/// detection so a `cd <my_pkg> && eval "$(nros ws env)"` from inside a
/// standalone pkg works the same as one run at a colcon workspace root.
///
/// Resolution order:
///   1. Explicit path arg → use it.
///   2. `<cwd>/src/<sub>/package.xml` exists → use `<cwd>/src`.
///   3. `<cwd>/package.xml` exists → use `<cwd>/..` (so smart Find-stub
///      finds `<parent>/<my_pkg>/package.xml` from there).
///   4. Fallback → `<cwd>/src` (legacy default; may not exist).
fn resolve_env_root(arg: Option<&Path>) -> Result<PathBuf> {
    if let Some(p) = arg {
        return std::fs::canonicalize(p).map_err(|e| eyre!("ws env: {}: {e}", p.display()));
    }
    let cwd = std::env::current_dir()?;
    let src = cwd.join("src");
    if src.is_dir() && has_pkg_subdir(&src) {
        return std::fs::canonicalize(&src).map_err(|e| eyre!("ws env: {}: {e}", src.display()));
    }
    if cwd.join("package.xml").is_file() {
        let parent = cwd.parent().ok_or_else(|| {
            eyre!(
                "ws env: cwd {} is a standalone pkg but has no parent",
                cwd.display()
            )
        })?;
        return std::fs::canonicalize(parent)
            .map_err(|e| eyre!("ws env: {}: {e}", parent.display()));
    }
    // Fallback — caller might not be in a pkg/workspace dir. Use ./src
    // and surface the error from canonicalize if it doesn't exist.
    std::fs::canonicalize(&src).map_err(|e| {
        eyre!(
            "ws env: {}: {e}\n\
                            (no `src/<pkg>/package.xml` colcon layout and no `package.xml` \
                            at cwd — pass an explicit path arg)",
            src.display()
        )
    })
}

// =============================================================================
// `nros ws sync` — pre-cargo codegen + patch-table writer
// =============================================================================

/// Scanned workspace pkg.
#[derive(Debug, Clone)]
struct WsPkg {
    name: String,
    dir: PathBuf,
    manifest: PathBuf,
    /// True iff msg pkg (member_of_group=rosidl_interface_packages OR
    /// msg/srv/action dirs).
    is_msg_pkg: bool,
    /// True iff `Cargo.toml` at root.
    is_rust_pkg: bool,
    /// Pkg names declared in `<*depend>` tags (filtered for ROS-meta).
    deps: Vec<String>,
    /// Phase 212.M-F.21 — `false` for path-dep targets imported into
    /// `scan` purely so their `<*depend>` rows can be unioned into the
    /// consumer's dep set. These pkgs are NOT cargo-build entry points
    /// and must not become `[patch.crates-io]` authorities. `true` for
    /// the originally-requested single-pkg dir or every workspace-mode
    /// scan hit.
    is_patch_consumer: bool,
}

impl WsPkg {
    /// True iff this pkg needs a `[patch.crates-io]` authority — a Rust pkg
    /// that builds against the generated msg crates / nros-* runtime via
    /// cargo, so cargo must resolve those path-patches from its authority.
    ///
    /// Phase-265 W5b: a pkg that BOTH defines msgs (`is_msg_pkg`, e.g. an
    /// inline `msg/` dir) AND carries a hand `Cargo.toml` is still a consumer
    /// — `native/custom-msg`, `zephyr .../talker-aemv8r`. The old filter
    /// excluded `is_msg_pkg`, silently dropping these ("no Rust consumer
    /// pkgs"). Pure interface packages never carry a *source* `Cargo.toml`
    /// (the crate is generated into `generated/`), so `is_rust_pkg` already
    /// excludes them without the `!is_msg_pkg` guard. `is_patch_consumer`
    /// still excludes path-dep import targets (the Entry→Component walk).
    fn needs_patch_authority(&self) -> bool {
        self.is_rust_pkg && self.is_patch_consumer
    }
}

/// phase-267 W1c/C3e — generate `<bringup>/nros-bridge.toml` for every bringup
/// whose `system.toml` declares a `[[bridge]]`. Plans the bringup (resolving each
/// bridge topic NAME to its ROS type from the node pkgs' synthetic `publishes`
/// metadata — pre-build, no sidecar), then renders the runtime bridge config the
/// entry's `nros_bridge::run_from_config` consumes. No bridge ⇒ no file written
/// (and a stale one is removed). Non-bridge workspaces never plan here.
/// R-code UX — materialize each bringup's SystemModel as part of `nros
/// sync`, so the user's canonical flow (sync → west/cargo/cmake) never
/// hand-runs the resolver. For every pkg with a `launch/` dir: resolve
/// `config/system_model.yaml` when it is missing or older than any input
/// (launch XMLs, system.toml). When the helper is absent, a model that needs no
/// refresh is used as-is; a model that DOES need one is a hard error, never a
/// silent staleness.
/// Multi-launch bringups also refresh per-launch `config/<name>_model.yaml`
/// siblings that were previously committed (variant models stay opt-in:
/// only refreshed, never created, for non-default launches).
///
/// Issue 0320 — content-addressed staleness. A committed model records a
/// `sha256` for every input under `meta.inputs`. This re-hashes each recorded
/// input against the file on disk and returns `Some(reason)` when the recorded
/// provenance no longer holds: a non-portable absolute path (which regenerates
/// the machine-specific legacy models on any checkout), a recorded input that
/// no longer exists, or a hash that has changed (an input the mtime gate does
/// not watch — a sibling include or the `--sched` platform file). `None` means
/// the model's provenance is intact. A model that cannot be parsed returns
/// `None` so the caller falls back to the mtime gate rather than force-churning.
///
/// Relative paths resolve against `bringup_dir` (the package root), matching
/// how the resolver strips the launch file's grandparent as the base and how
/// `main_macro` re-joins them.
fn model_provenance_stale(model_path: &Path, bringup_dir: &Path) -> Option<String> {
    let raw = std::fs::read_to_string(model_path).ok()?;
    let model = ros_launch_manifest_model::SystemModel::from_yaml_str(&raw).ok()?;
    for input in &model.meta.inputs {
        let recorded = Path::new(&input.path);
        if recorded.is_absolute() {
            return Some(format!("non-portable absolute input path `{}`", input.path));
        }
        let resolved = bringup_dir.join(recorded);
        let Ok(bytes) = std::fs::read(&resolved) else {
            return Some(format!("recorded input missing `{}`", input.path));
        };
        let digest = format!("{:x}", Sha256::digest(&bytes));
        if digest != input.sha256 {
            return Some(format!("input hash changed `{}`", input.path));
        }
    }
    None
}

fn resolve_system_models(scan: &[WsPkg], verbose: bool) -> Result<()> {
    // Issue 0285 — resolve the helper by ABSOLUTE PATH, never through PATH.
    //
    // This used to run `play_launch` by bare name. `play_launch` is also an
    // unrelated ROS 2 record/replay tool, so on a host that had that one the
    // wrong binary won and every `nros sync` died with "unrecognized
    // subcommand 'resolve'", taking the whole fixture build with it. Probing
    // the capability instead of the name made that degrade rather than fail,
    // but it could not make the RIGHT tool findable.
    //
    // Now we ship our own `nros-launch-resolve`, built from the pinned
    // play_launch submodule and versioned with this CLI, and look for it next
    // to the running `nros` binary. Nothing on PATH can shadow it — and,
    // equally deliberate, we never put it ON PATH, so we cannot shadow a
    // user's real `play_launch` either.
    //
    // Absent used to be a DEGRADE: warn once, use whatever models are committed,
    // carry on. That is wrong, and it cost a full fixture sweep to notice. The
    // helper is a setup step (`just setup-launch-resolve`, now reached from
    // `just setup` and `just build-test-fixtures`), so absent means the tree is
    // mis-provisioned — not that the user chose to skip refreshing.
    //
    // Worse, the degrade was silent in the only way that matters: it printed
    // per-workspace noise whether or not anything was actually stale, so the
    // message carried no signal, and a genuinely stale model sailed through it
    // into a build. Museum SystemModels are exactly the fixture-mtime treadmill
    // this repo keeps getting bitten by.
    //
    // The rule is now: refresh, or fail. Absence is only tolerated when nothing
    // needs refreshing, in which case it is not a degrade at all and says
    // nothing. `blocked` collects everything that DID need the helper so one
    // error names them all rather than the user fixing them one run at a time.
    let play_launch = launch_resolver_path();
    let mut blocked: Vec<String> = Vec::new();
    for pkg in scan {
        let launch_dir = pkg.dir.join("launch");
        if !launch_dir.is_dir() {
            continue;
        }
        let cfg_dir = pkg.dir.join("config");
        let system_toml = pkg.dir.join("system.toml");
        // Input mtime horizon: launch XMLs + system.toml.
        let mut newest_input: Option<std::time::SystemTime> = None;
        let mut launches: Vec<std::path::PathBuf> = Vec::new();
        if let Ok(rd) = std::fs::read_dir(&launch_dir) {
            for e in rd.flatten() {
                let p = e.path();
                if p.extension().and_then(|s| s.to_str()) == Some("xml") {
                    if let Ok(md) = p.metadata()
                        && let Ok(mt) = md.modified()
                    {
                        newest_input = Some(newest_input.map_or(mt, |c| c.max(mt)));
                    }
                    launches.push(p);
                }
            }
        }
        if launches.is_empty() {
            continue;
        }
        if let Ok(md) = system_toml.metadata()
            && let Ok(mt) = md.modified()
        {
            newest_input = Some(newest_input.map_or(mt, |c| c.max(mt)));
        }
        let stale = |model: &std::path::Path| -> bool {
            match (model.metadata().and_then(|m| m.modified()), newest_input) {
                (Ok(mm), Some(ni)) => mm < ni,
                (Err(_), _) => true,
                _ => false,
            }
        };
        // Targets: the default model always; committed variant models refresh.
        let mut targets: Vec<(std::path::PathBuf, std::path::PathBuf)> = Vec::new();
        let default_launch = {
            // `[system] default_launch` else system.launch.xml else the single file.
            let named = std::fs::read_to_string(&system_toml)
                .ok()
                .and_then(|raw| toml::from_str::<toml::Value>(&raw).ok())
                .and_then(|v| {
                    v.get("system")?
                        .get("default_launch")?
                        .as_str()
                        .map(|s| launch_dir.join(s))
                });
            named
                .filter(|p| p.is_file())
                .or_else(|| {
                    let sys = launch_dir.join("system.launch.xml");
                    sys.is_file().then_some(sys)
                })
                .or_else(|| (launches.len() == 1).then(|| launches[0].clone()))
        };
        if let Some(dl) = &default_launch {
            targets.push((dl.clone(), cfg_dir.join("system_model.yaml")));
        }
        for lf in &launches {
            if Some(lf) == default_launch.as_ref() {
                continue;
            }
            let stem = lf
                .file_name()
                .and_then(|s| s.to_str())
                .map(|s| s.trim_end_matches(".launch.xml").to_string())
                .unwrap_or_default();
            let variant = cfg_dir.join(format!("{stem}_model.yaml"));
            if variant.exists() {
                targets.push((lf.clone(), variant));
            }
        }
        for (launch, model) in targets {
            // Issue 0320 — staleness is BOTH mtime AND content-addressed. The
            // mtime gate watches only `launch/*.xml` + `system.toml`, but a
            // committed model's `meta.inputs` hashes more (sibling includes, the
            // `--sched` file) and can carry a non-portable absolute path from the
            // machine that generated it. Re-hashing the recorded inputs catches
            // both the wider input set (issue 0196 class) and the 43 legacy
            // absolute-path models, which are otherwise never mtime-stale.
            let provenance = model
                .exists()
                .then(|| model_provenance_stale(&model, &pkg.dir))
                .flatten();
            if !stale(&model) && provenance.is_none() {
                continue;
            }
            let Some(pl) = &play_launch else {
                // Reached only when this model is stale or missing — `stale()`
                // already let the current ones through above.
                blocked.push(format!(
                    "  {} — {} is {} ({})",
                    pkg.name,
                    model.strip_prefix(&pkg.dir).unwrap_or(&model).display(),
                    match (&provenance, model.exists()) {
                        (Some(why), _) => why.as_str(),
                        (None, true) => "older than its inputs",
                        (None, false) => "missing",
                    },
                    launch
                        .file_name()
                        .and_then(|s| s.to_str())
                        .unwrap_or("launch"),
                ));
                continue;
            };
            std::fs::create_dir_all(&cfg_dir)
                .wrap_err_with(|| format!("ws sync: create {}", cfg_dir.display()))?;
            let mut cmd = std::process::Command::new(pl);
            cmd.arg(&launch);
            // Issue 0320 — state the bringup package root explicitly so
            // `meta.inputs[].path` are recorded relative to it structurally,
            // rather than the resolver inferring it as the launch file's
            // grandparent (which emits absolute paths for a non-standard layout).
            cmd.arg("--bringup-root").arg(&pkg.dir);
            if system_toml.is_file() {
                cmd.arg("--system").arg(&system_toml);
            }
            cmd.arg("-o").arg(&model);
            let out = cmd
                .output()
                .wrap_err_with(|| format!("ws sync: spawn nros-launch-resolve for {}", pkg.name))?;
            if !out.status.success() {
                eyre::bail!(
                    "ws sync: nros-launch-resolve failed for `{}` ({}):\n{}",
                    pkg.name,
                    launch.display(),
                    String::from_utf8_lossy(&out.stderr),
                );
            }
            if verbose {
                println!("ws sync: resolved {}", model.display());
            } else {
                println!(
                    "ws sync: resolved {} → {}",
                    launch
                        .file_name()
                        .and_then(|s| s.to_str())
                        .unwrap_or("launch"),
                    model.strip_prefix(&pkg.dir).unwrap_or(&model).display()
                );
            }
        }
    }
    if !blocked.is_empty() {
        eyre::bail!(
            "ws sync: {} SystemModel(s) need resolving but `nros-launch-resolve` \
             is not next to the `nros` binary:\n{}\n\n\
             Build it:  just setup-launch-resolve\n\
             (If the submodule is missing:  git submodule update --init --recursive \
             packages/cli/third-party/ros-launch-resolve)\n\n\
             Refusing to continue with stale models — a museum SystemModel builds \
             clean and then places nodes wrong at runtime.",
            blocked.len(),
            blocked.join("\n"),
        );
    }
    Ok(())
}

/// Locate `nros-launch-resolve` (issue 0285).
///
/// Mirrors `nros_cli_bin()` in `scripts/build/cargo.sh` — the repo's existing
/// SSoT for finding the CLI — with ONE deliberate omission: no `$PATH` step.
/// A PATH lookup is precisely the bug this fixes, since an unrelated ROS 2
/// `play_launch` won that race. Resolution order:
///
///   1. `$NROS_LAUNCH_RESOLVE` — explicit override, the twin of `$NROS_CLI`
///      (packaging, CI, and tests that ship the helper elsewhere);
///   2. a sibling of the running `nros` — the installed layout;
///   3. `$NROS_REPO_DIR/packages/cli/nros-launch-resolve/target/release/…`,
///      then the same path derived by walking up from the running binary —
///      the per-checkout build, which `cargo.sh` also prefers so each worktree
///      carries its own tools with no cross-tree skew.
///
/// The helper is its OWN cargo workspace, so its binary is under
/// `nros-launch-resolve/target/release/`, not beside `nros` in
/// `packages/cli/target/release/`.
fn launch_resolver_path() -> Option<std::path::PathBuf> {
    resolver_beside(&std::env::current_exe().ok()?)
}

/// The lookup itself, parameterised on the `nros` binary's own path so it can
/// be tested without spawning anything.
///
/// Two locations, both derived from `exe` — never `$PATH`:
/// 1. a sibling (installed layout: `nros` and the helper side by side);
/// 2. `../../nros-launch-resolve/target/release/` (in-tree: the helper is its
///    own cargo workspace, so it does NOT land in `packages/cli/target/`).
const LAUNCH_RESOLVER: &str = "nros-launch-resolve";

fn resolver_beside(exe: &std::path::Path) -> Option<std::path::PathBuf> {
    resolver_from(
        exe,
        std::env::var_os("NROS_LAUNCH_RESOLVE").map(std::path::PathBuf::from),
        std::env::var_os("NROS_REPO_DIR").map(std::path::PathBuf::from),
    )
}

/// The search itself, pure in its inputs.
///
/// Taking the two env values as arguments rather than reading them keeps this
/// hermetic: the tests below would otherwise race each other through the
/// process-wide environment, and would also see a real `$NROS_REPO_DIR` from
/// the developer's shell.
fn resolver_from(
    exe: &std::path::Path,
    explicit: Option<std::path::PathBuf>,
    repo_dir: Option<std::path::PathBuf>,
) -> Option<std::path::PathBuf> {
    // 1. Explicit override, mirroring `$NROS_CLI`. A non-existent override
    //    falls through rather than failing: the caller degrades to the
    //    committed model, and a stale env var must not be harder to diagnose
    //    than a missing tool.
    if let Some(p) = explicit
        && p.is_file()
    {
        return Some(p);
    }

    let dir = exe.parent()?;

    // 2. Installed layout — beside the CLI we shipped.
    let sibling = dir.join(LAUNCH_RESOLVER);
    if sibling.is_file() {
        return Some(sibling);
    }

    // 3. Per-checkout build, preferred by cargo.sh for the same reason: each
    //    worktree carries its own tools, with no cross-tree skew.
    let in_checkout = |root: &std::path::Path| {
        root.join("packages")
            .join("cli")
            .join(LAUNCH_RESOLVER)
            .join("target")
            .join("release")
            .join(LAUNCH_RESOLVER)
    };
    if let Some(root) = repo_dir {
        let p = in_checkout(&root);
        if p.is_file() {
            return Some(p);
        }
    }
    // `dir` is <repo>/packages/cli/target/release, so <repo> is four
    // ancestors up (target, cli, packages, repo).
    dir.ancestors()
        .nth(4)
        .map(in_checkout)
        .filter(|p| p.is_file())
}

/// phase-315 W1 — write one selection facade per ENTRY package.
///
/// The bringup owns the declaration, the entry consumes it, and the two are
/// different packages, so this needs both: it finds the workspace's
/// `system.toml` (the bringup) and then every package carrying
/// `[package.metadata.nros.entry]`.
///
/// A workspace with no `system.toml` has nothing to derive from and is left
/// alone — that is the STANDALONE shape, where the build command is the
/// selector (`cargo build --features …`, the twin of C++'s `-DNANO_ROS_RMW=…`)
/// and a facade would have no input. See phase-315 W3.
fn generate_facade_crates(
    ws_root: &std::path::Path,
    scan: &[WsPkg],
    build_root: &std::path::Path,
    verbose: bool,
) -> Result<()> {
    // The bringup: the package that declares the system. More than one is a
    // multi-system workspace, which the facade shape does not yet model — say
    // so rather than silently picking the first.
    let bringups: Vec<&WsPkg> = scan
        .iter()
        .filter(|p| p.dir.join("system.toml").is_file())
        .collect();
    let bringup = match bringups.as_slice() {
        [] => return Ok(()),
        [one] => *one,
        many => {
            eprintln!(
                "ws sync: {} bringups declare a system ({}); selection facades \
                 are not generated for multi-system workspaces (phase-315 W1 \
                 models one declaration per workspace). Entry manifests keep \
                 their hand-written features.",
                many.len(),
                many.iter()
                    .map(|p| p.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", "),
            );
            return Ok(());
        }
    };

    let system_toml = bringup.dir.join("system.toml");
    let raw = std::fs::read_to_string(&system_toml)
        .wrap_err_with(|| format!("ws sync: read {}", system_toml.display()))?;
    let sys: crate::orchestration::cargo_metadata_schema::SystemToml = toml::from_str(&raw)
        .wrap_err_with(|| format!("ws sync: parse {}", system_toml.display()))?;

    // Entry packages come from CARGO's member list, not from `scan`.
    //
    // `scan` is ament-driven: a package enters it by having a `package.xml`.
    // Nine workspace entries do not have one — they are cargo workspace members
    // and nothing else, which is legal (the workspace ROOT is their patch
    // authority, so the rest of sync works on them). Keying facade generation
    // off `scan` silently skipped exactly those nine, and the skip was
    // invisible: sync succeeded, and the entries kept their hand-written
    // features, which is the state that looks correct.
    //
    // Cargo's `members` list is the truth for "what is in this workspace" here,
    // because the facade's whole mechanism is cargo feature unification.
    let mut candidates: Vec<(String, PathBuf)> = scan
        .iter()
        .filter(|p| p.is_rust_pkg)
        .map(|p| (p.name.clone(), p.dir.clone()))
        .collect();
    for dir in cargo_workspace_members(ws_root) {
        if !candidates.iter().any(|(_, d)| *d == dir) {
            let name = dir
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or_default()
                .to_string();
            candidates.push((name, dir));
        }
    }

    let facade_root = build_root.join("nros-selection");
    for (pkg_name, pkg_dir) in &candidates {
        // The CARGO manifest — NOT `WsPkg::manifest`, which is the ament
        // `package.xml`, and which the cargo-only members do not have at all.
        let cargo_toml = pkg_dir.join("Cargo.toml");
        if !cargo_toml.is_file() {
            continue;
        }
        let Some(f) = crate::orchestration::facade::write_facade(
            pkg_name,
            pkg_dir,
            &cargo_toml,
            &sys,
            &facade_root,
        )
        .wrap_err_with(|| format!("ws sync: facade for {pkg_name}"))?
        else {
            continue;
        };
        if f.changed || verbose {
            println!(
                "ws sync: selection facade {} → nros[{}] {}",
                f.entry,
                f.nros_features.join(", "),
                if f.board_features.is_empty() {
                    String::new()
                } else {
                    format!("board[{}]", f.board_features.join(", "))
                },
            );
        }
    }
    Ok(())
}

/// Cargo workspace members of `ws_root`, as absolute directories.
///
/// Deliberately simple: `members` entries are literal relative paths in every
/// nano-ros example workspace. Glob members (`src/*`) are expanded, since cargo
/// allows them and one of these workspaces could grow one; anything else is
/// skipped rather than guessed at.
fn cargo_workspace_members(ws_root: &std::path::Path) -> Vec<PathBuf> {
    let Ok(raw) = std::fs::read_to_string(ws_root.join("Cargo.toml")) else {
        return Vec::new();
    };
    let Ok(v) = toml::from_str::<toml::Value>(&raw) else {
        return Vec::new();
    };
    let Some(members) = v
        .get("workspace")
        .and_then(|w| w.get("members"))
        .and_then(|m| m.as_array())
    else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for m in members.iter().filter_map(|m| m.as_str()) {
        if let Some(prefix) = m.strip_suffix("/*") {
            if let Ok(rd) = std::fs::read_dir(ws_root.join(prefix)) {
                out.extend(rd.flatten().map(|e| e.path()).filter(|p| p.is_dir()));
            }
        } else {
            let p = ws_root.join(m);
            if p.is_dir() {
                out.push(p);
            }
        }
    }
    out
}

fn generate_bridge_configs(
    ws_root: &std::path::Path,
    scan: &[WsPkg],
    build_root: &std::path::Path,
    verbose: bool,
) -> Result<()> {
    for pkg in scan {
        let system_toml = pkg.dir.join("system.toml");
        if !system_toml.is_file() {
            continue;
        }
        let raw = std::fs::read_to_string(&system_toml).unwrap_or_default();
        let has_bridge = toml::from_str::<toml::Value>(&raw)
            .ok()
            .and_then(|v| {
                v.get("bridge")
                    .and_then(|b| b.as_array())
                    .map(|a| !a.is_empty())
            })
            .unwrap_or(false);
        let dest = pkg.dir.join("nros-bridge.toml");
        if !has_bridge {
            continue;
        }

        // R-code — plan from the bringup's committed SystemModel when
        // present (every in-tree bridge workspace has one); launch-synth
        // resolution survives only for modelless bringups. Both temp guards
        // (synth XML / synthesized record) live in `_guards` through
        // plan_system.
        let model_path = pkg.dir.join("config/system_model.yaml");
        let mut _guard_record = None;
        let (plan_launch_file, plan_record_file) = if model_path.exists() {
            let model = crate::orchestration::model_ingest::load_model(&model_path)?;
            let record = crate::orchestration::model_ingest::plan_record_from_model(&model);
            let tmp = tempfile::NamedTempFile::new()?;
            std::fs::write(tmp.path(), serde_json::to_string_pretty(&record)?)?;
            let rec_path = tmp.path().to_path_buf();
            _guard_record = Some(tmp);
            (model_path.clone(), Some(rec_path))
        } else {
            // R-code.1 — the launch-synth fallback is deleted; a bridge
            // bringup declares system semantics, so it must resolve a model.
            eyre::bail!(
                "ws sync: bridge bringup `{}` has no committed SystemModel \
                 (config/system_model.yaml) — the launch-synth fallback was \
                 removed (phase-296 R4); resolve one with `play_launch \
                 resolve … --system {}/system.toml`",
                pkg.name,
                pkg.dir.display()
            );
        };
        let output = crate::orchestration::planner::plan_system(
            crate::orchestration::planner::PlanOptions {
                system_pkg: pkg.name.clone(),
                workspace_root: ws_root.to_path_buf(),
                launch_file: plan_launch_file,
                record_file: plan_record_file,
                out_root: build_root.join(&pkg.name).join("nros-bridge-plan"),
                metadata_files: Vec::new(),
                manifest_files: Vec::new(),
                launch_args: Vec::new(),
                rmw: None,
                target: None,
            },
        )
        .wrap_err_with(|| format!("ws sync: plan bridge bringup {}", pkg.name))?;

        let plan_json = std::fs::read_to_string(&output.plan_path)?;
        let plan: crate::orchestration::plan::NrosPlan = serde_json::from_str(&plan_json)
            .wrap_err_with(|| format!("ws sync: parse plan for bridge bringup {}", pkg.name))?;

        match crate::orchestration::bridge_gen::render_bridge_runtime_config(&plan, ws_root) {
            Some(cfg) => {
                std::fs::write(&dest, cfg)
                    .wrap_err_with(|| format!("ws sync: write {}", dest.display()))?;
                if verbose {
                    println!("ws sync: wrote {}", dest.display());
                }
            }
            // A `[[bridge]]` whose plan carried no resolvable bridge — drop any
            // stale file so the entry doesn't boot an outdated config.
            None => {
                let _ = std::fs::remove_file(&dest);
            }
        }
    }
    Ok(())
}

/// Join `rel` onto `base`, folding `.` and `..` textually.
///
/// `Path::join` + `is_file()` would walk the real filesystem, which fails when
/// an intermediate component does not exist yet. Nothing here touches disk.
fn lexically_join(base: &Path, rel: &Path) -> PathBuf {
    let mut out = base.to_path_buf();
    for part in rel.components() {
        match part {
            std::path::Component::ParentDir => {
                out.pop();
            }
            std::path::Component::CurDir => {}
            other => out.push(other.as_os_str()),
        }
    }
    out
}

pub fn run_sync(args: SyncArgs) -> Result<()> {
    // phase-308 W1 — captured up front: `args.workspace` is moved just below,
    // and the C/C++-only early return still needs the probe's nano-ros path.
    let nano_ros_for_probes = nano_ros_path_for(&args);
    let ws_root: PathBuf = match args.workspace {
        Some(p) => {
            std::fs::canonicalize(&p).wrap_err_with(|| format!("ws sync: {}", p.display()))?
        }
        None => std::env::current_dir()?,
    };
    // Two layouts supported:
    //  * `src/`-based: workspace root has src/, src/<pkg>/ subdirs (colcon
    //    standard).
    //  * Single-pkg: workspace root IS the pkg dir (package.xml at root).
    //    Common for ported standalone examples (`examples/native/rust/talker`).
    // Heuristic: colcon-style layout iff `src/` exists AND has at least one
    // immediate subdir with `package.xml`. Falls through to single-pkg mode
    // when the workspace root itself carries `package.xml` (the standalone
    // example shape; `src/` may exist as the cargo source dir).
    let colcon_layout = ws_root.join("src").is_dir() && has_pkg_subdir(&ws_root.join("src"));
    let single_pkg_mode = !colcon_layout && ws_root.join("package.xml").is_file();
    let src_root = if colcon_layout {
        ws_root.join("src")
    } else if single_pkg_mode {
        ws_root.clone()
    } else {
        bail!(
            "ws sync: no `src/<pkg>/package.xml` and no `package.xml` at root \
             under {} — expected colcon-style workspace or single-pkg dir",
            ws_root.display()
        );
    };
    let build_root = if args.build_dir.is_absolute() {
        args.build_dir.clone()
    } else {
        ws_root.join(&args.build_dir)
    };

    let mut scan = Vec::new();
    if single_pkg_mode {
        scan_one_pkg_dir(&src_root, &mut scan)?;
    } else {
        scan_workspace(&src_root, &mut scan)?;
    }
    if scan.is_empty() {
        println!("ws sync: no pkgs under {}", src_root.display());
        return Ok(());
    }
    // Phase 212.M-F.21 — Rust consumer's transitive msg deps via path-deps.
    // The pkg.xml `<*depend>` tags drive AMENT codegen + patch table,
    // but Entry pkgs typically don't list msg deps directly — they
    // inherit them through a path-dep on a Component pkg. Walk each
    // Rust consumer's `Cargo.toml [dependencies]`, resolve path-deps
    // against the scan, and union the dependent pkg's `deps` in. The
    // patch authority for the Entry pkg then carries every msg patch
    // the transitive build needs.
    augment_rust_consumer_deps_via_path_deps(&mut scan)?;
    let msg_pkgs: Vec<&WsPkg> = scan.iter().filter(|p| p.is_msg_pkg).collect();
    let topo = topo_sort_msg_pkgs(&msg_pkgs)?;

    if args.verbose || args.dry_run {
        println!(
            "ws sync: scanned {} pkgs ({} msg, {} rust) under {}",
            scan.len(),
            msg_pkgs.len(),
            scan.iter().filter(|p| p.is_rust_pkg).count(),
            src_root.display()
        );
        println!("ws sync: topo order: {topo:?}");
    }

    if args.check {
        return check_freshness(&ws_root, &build_root, &scan, &topo);
    }

    if args.dry_run {
        for name in &topo {
            let pkg = scan.iter().find(|p| &p.name == name).unwrap();
            let out = build_root.join(name);
            println!(
                "ws sync: WOULD codegen {} from {} → {}",
                name,
                pkg.manifest.display(),
                out.display()
            );
        }
        return Ok(());
    }

    // Captured before `args` is partially moved below (the nano_ros_path take).
    let no_metadata = args.no_metadata;
    let verbose = args.verbose;

    let edition = resolve_sync_edition(args.ros_edition.as_deref(), &ws_root)?;

    // Track every pkg we generate so a later iteration (or AMENT-dep walk)
    // skips already-emitted ones. Keyed by pkg name.
    let mut emitted: HashSet<String> = HashSet::new();

    for name in &topo {
        let pkg = scan.iter().find(|p| &p.name == name).unwrap();
        // First materialize any AMENT-resolved cross-deps so the workspace
        // pkg's deps closure exists in build/ too. Skips workspace deps
        // (those are handled by topo order itself).
        codegen_ament_deps_for(
            &pkg.deps,
            &scan,
            &build_root,
            edition,
            &mut emitted,
            args.verbose,
        )?;
        // Now generate the workspace pkg itself directly from its dir.
        if !emitted.contains(name) {
            codegen_workspace_pkg(pkg, &build_root, edition, args.verbose)?;
            emitted.insert(name.clone());
        }
    }
    // Also generate AMENT deps for every Rust consumer (pkg.xml deps).
    let rust_consumers: Vec<&WsPkg> = scan.iter().filter(|p| p.needs_patch_authority()).collect();
    for c in &rust_consumers {
        codegen_ament_deps_for(
            &c.deps,
            &scan,
            &build_root,
            edition,
            &mut emitted,
            args.verbose,
        )?;
    }

    // phase-267 W1c/C3e — for each bringup declaring a `[[bridge]]`, plan it
    // (topic names→types resolve from the node pkgs' synthetic `publishes`
    // metadata, no build) and write `<bringup>/nros-bridge.toml` — the file the
    // entry's `nros_bridge::run_from_config` consumes at runtime.
    // R-code UX — resolve/refresh each bringup's committed SystemModel first
    // (the canonical input; bridge planning below consumes it).
    resolve_system_models(&scan, args.verbose)?;
    generate_bridge_configs(&ws_root, &scan, &build_root, args.verbose)?;
    generate_facade_crates(&ws_root, &scan, &build_root, args.verbose)?;

    if rust_consumers.is_empty() {
        println!("ws sync: no Rust consumer pkgs — patch tables not written.");
        // phase-308 W1 — a C/C++-only workspace has no Rust consumers, but it
        // DOES have components to probe. Returning here skipped the metadata
        // refresh entirely for exactly the workspaces the C/C++ producer
        // exists to serve.
        refresh_source_metadata(&ws_root, nano_ros_for_probes, no_metadata, verbose)?;
        println!("ws sync: done.");
        return Ok(());
    }

    // Group consumers by patch authority. Cargo workspace covers many
    // consumers via one umbrella; standalone pkgs are their own authority.
    let all_emitted: Vec<String> = {
        let mut v: Vec<String> = emitted.iter().cloned().collect();
        v.sort();
        v
    };
    let mut authority_to_pkgs: HashMap<PathBuf, Vec<String>> = HashMap::new();
    for c in &rust_consumers {
        let authority = find_patch_authority(&c.dir, &ws_root)?;
        // Workspace mode keeps the locked shared-root topology (`3f07dd9f7`):
        // every consumer's authority carries the full emitted set. Single-pkg
        // mode is dependency-aware — only the msg crates this consumer
        // transitively depends on (its `<depend>` closure), so a node's
        // unconsumed self-codegen crate never lands a broken patch entry.
        let pkgs_for: Vec<String> = if single_pkg_mode {
            emitted_msg_dep_closure(&c.deps, &all_emitted, &build_root)
        } else {
            all_emitted.clone()
        };
        authority_to_pkgs
            .entry(authority)
            .or_default()
            .extend(pkgs_for);
    }
    let nano_ros_path = args
        .nano_ros_path
        .or_else(|| std::env::var_os("NROS_REPO_DIR").map(PathBuf::from))
        .or_else(|| autodetect_nano_ros_path(&ws_root));

    // Phase 220.E — collect the union of `nros-*` (+ `nros` + `cyclonedds-sys`)
    // registry-style deps across every Rust consumer pointing at this
    // authority. Each authority gets a single patch block; if any
    // consumer references `nros-rmw-zenoh = "*"`, the authority's
    // block must carry the matching path entry — otherwise cargo
    // can't resolve the dep at all (it'll search crates.io and fail).
    let mut authority_to_extra: HashMap<PathBuf, Vec<String>> = HashMap::new();
    for c in &rust_consumers {
        let authority = find_patch_authority(&c.dir, &ws_root)?;
        let cargo_toml = c.dir.join("Cargo.toml");
        let extras = match std::fs::read_to_string(&cargo_toml) {
            Ok(body) => extract_consumer_registry_nros_deps(&body),
            Err(_) => Vec::new(),
        };
        authority_to_extra
            .entry(authority)
            .or_default()
            .extend(extras);
    }

    // Phase 287 W9 (option E) — regenerate the central patch file once per
    // sync; every authority's config then reaches the universal-trio patches
    // through a single `include` line instead of per-leaf relative paths.
    let central_patch: Option<PathBuf> = match nano_ros_path.as_deref() {
        Some(nrp) => Some(write_central_patch_file(nrp)?),
        None => None,
    };
    // #272 — the central patch is reached via an `include = [...]` config key
    // that was NIGHTLY-ONLY before cargo 1.93 (stabilized there). On an older
    // build toolchain cargo silently drops the include and the build dies with
    // an unexplained `no matching package named 'nros'`. Warn LOUDLY once when
    // the workspace's effective cargo predates 1.93 — the reachability check
    // above only catches a missing FILE, not a cargo too old to read it.
    if central_patch.is_some() {
        warn_if_cargo_predates_config_include(&ws_root);
    }

    for (authority, pkgs) in authority_to_pkgs {
        let mut unique = pkgs;
        unique.sort();
        unique.dedup();
        let mut extras = authority_to_extra.remove(&authority).unwrap_or_default();
        extras.sort();
        extras.dedup();
        write_patch_block(
            &authority,
            &build_root,
            &unique,
            nano_ros_path.as_deref(),
            &extras,
            central_patch.as_deref(),
        )?;
    }

    refresh_source_metadata(&ws_root, nano_ros_path.clone(), no_metadata, verbose)?;

    println!("ws sync: done.");
    Ok(())
}

/// phase-308 W1 — resolve the nano-ros checkout the metadata probes build
/// against. Mirrors the resolution the patch-table path already does.
fn nano_ros_path_for(args: &SyncArgs) -> Option<PathBuf> {
    args.nano_ros_path
        .clone()
        .or_else(|| std::env::var_os("NROS_REPO_DIR").map(PathBuf::from))
}
/// phase-307 W2 — the producer trigger.
///
/// Runs LAST, and the order is load-bearing: the metadata harness compiles the
/// Node pkg for real, so the generated interface crates must already exist
/// (codegen, above) and the `[patch.crates-io]` tables that redirect
/// `example_interfaces = "*"` at them must already be written (immediately
/// above). Running this any earlier fails to resolve the interface deps.
fn refresh_source_metadata(
    ws_root: &Path,
    nano_ros_path: Option<PathBuf>,
    no_metadata: bool,
    verbose: bool,
) -> Result<()> {
    if no_metadata {
        return Ok(());
    }
    let report = crate::orchestration::metadata_refresh::refresh_stale_sidecars(
        ws_root,
        nano_ros_path.as_deref(),
        verbose,
    )?;
    if report.total() == 0 && report.unsupported.is_empty() {
        return Ok(());
    }
    println!(
        "ws sync: source metadata — {} rebuilt, {} already current",
        report.rebuilt.len(),
        report.fresh.len()
    );
    // Never silent: a component with no producer is a component whose entity
    // count a bake cannot know, and that is exactly how issue 0257's executor
    // ran out of callback slots at boot.
    for what in &report.unsupported {
        println!("ws sync: source metadata — no producer for {what}");
    }
    Ok(())
}

fn parse_edition(s: &str) -> Result<RosEdition> {
    RosEdition::parse(s)
        .ok_or_else(|| eyre::eyre!("ws sync: unknown ROS edition '{s}' (humble | iron | jazzy)"))
}

/// Resolve the codegen edition (phase-304 W2b): an explicit `--ros-edition`
/// wins; otherwise auto-lower `[system].ros_edition` from a `system.toml` at the
/// workspace root (declare once, RFC-0056); neither → humble (byte-identical).
/// The baked type_hash then matches the runtime `ros-<edition>` keyexpr feature.
fn resolve_sync_edition(cli: Option<&str>, ws_root: &Path) -> Result<RosEdition> {
    if let Some(s) = cli {
        return parse_edition(s);
    }
    let sys_toml = ws_root.join("system.toml");
    if sys_toml.is_file() {
        let raw = std::fs::read_to_string(&sys_toml)
            .wrap_err_with(|| format!("ws sync: read {}", sys_toml.display()))?;
        let sys: crate::orchestration::cargo_metadata_schema::SystemToml = toml::from_str(&raw)
            .wrap_err_with(|| format!("ws sync: parse {}", sys_toml.display()))?;
        return sys
            .system
            .ros_edition()
            .wrap_err_with(|| format!("ws sync: [system].ros_edition in {}", sys_toml.display()));
    }
    Ok(RosEdition::Humble)
}

// The interface index (ament, when a ROS 2 env is sourced, merged over the
// bundled share dirs at packages/cli/interfaces/) — loaded once per process.
// Used both to codegen AMENT dep pkgs and, on Iron+, to resolve cross-package
// nested `.msg` types for the REP-2011 type hash.
fn interface_index() -> Option<&'static rosidl_bindgen::ament::AmentIndex> {
    static AMENT_INDEX: std::sync::OnceLock<Option<rosidl_bindgen::ament::AmentIndex>> =
        std::sync::OnceLock::new();
    AMENT_INDEX
        .get_or_init(|| cargo_nano_ros::load_index_with_fallback(false).ok())
        .as_ref()
}

// A cross-package `.msg` resolver over the interface index (RIHS01 type-hash
// DAG closure). `generate_package` resolves same-package nested types itself;
// this covers `std_msgs` / `builtin_interfaces` / etc. Consulted only on Iron+
// (Humble emits a placeholder hash and never calls it).
fn ament_msg_resolver() -> impl Fn(&str) -> Option<rosidl_parser::Message> {
    move |fqn: &str| {
        let idx = interface_index()?;
        let mut parts = fqn.split('/');
        let pkg = parts.next()?;
        let name = parts.next_back()?;
        let package = idx.packages().get(pkg)?;
        let content = std::fs::read_to_string(package.get_message_path(name)).ok()?;
        rosidl_parser::parse_message(&content).ok()
    }
}

// Generate the workspace pkg directly (using its dir as a synthetic share_dir
// — `Package::from_share_dir` reads `package.xml` + scans msg/srv/action).
fn codegen_workspace_pkg(
    pkg: &WsPkg,
    build_root: &Path,
    edition: RosEdition,
    verbose: bool,
) -> Result<()> {
    let out_dir = build_root;
    std::fs::create_dir_all(&out_dir)
        .wrap_err_with(|| format!("ws sync: mkdir {}", out_dir.display()))?;
    if verbose {
        println!(
            "ws sync: codegen workspace pkg {} → {}",
            pkg.name,
            out_dir.display()
        );
    } else {
        println!("ws sync: codegen {}", pkg.name);
    }
    let package = Package::from_share_dir(pkg.dir.clone())
        .wrap_err_with(|| format!("ws sync: read pkg {}", pkg.dir.display()))?;
    // Per-field capacity config (RFC-0033), discovered from the pkg source dir.
    let resolver = rosidl_codegen::CapacityResolver::discover(&pkg.dir, None)?;
    let msg_resolve = ament_msg_resolver();
    rosidl_bindgen::generator::generate_package(
        &package,
        &out_dir,
        edition,
        &resolver,
        &msg_resolve,
    )
    .wrap_err_with(|| format!("ws sync: generate_package failed for {}", pkg.name))?;
    // Codegen emits <out_dir>/<pkg>/{Cargo.toml,src/} with sibling `path =
    // "../<dep>"` deps. We keep that flat layout (no extra `rust/`
    // nesting) so the relative paths between generated crates resolve
    // correctly without a rewrite pass. Our `nros_generator_rs` prefix
    // already namespaces by language — the extra `rust/` colcon adds is
    // there to coexist with `<pkg>/c/`, `<pkg>/cpp/`, etc. inside the
    // same generator's output, which we don't have.
    Ok(())
}

// Resolve AMENT-side deps (the per-pkg.xml `<depend>` tags not in workspace)
// and codegen each via Package::from_share_dir over its AMENT share path.
fn codegen_ament_deps_for(
    deps: &[String],
    scan: &[WsPkg],
    build_root: &Path,
    edition: RosEdition,
    emitted: &mut HashSet<String>,
    verbose: bool,
) -> Result<()> {
    // Pre-load the interface index once per invocation: the ament index
    // (when a ROS 2 env is sourced) merged over the bundled share dirs at
    // packages/cli/interfaces/ — so a host WITHOUT ROS 2 still resolves
    // std_msgs/builtin_interfaces instead of letting cargo fall through to
    // crates.io's yanked ROS crates (#204 probe finding).
    let Some(idx) = interface_index() else {
        return Ok(());
    };

    let in_workspace: HashSet<&str> = scan.iter().map(|p| p.name.as_str()).collect();
    let mut to_resolve: Vec<String> = deps
        .iter()
        .filter(|d| !in_workspace.contains(d.as_str()))
        .cloned()
        .collect();

    while let Some(dep) = to_resolve.pop() {
        if emitted.contains(&dep) {
            continue;
        }
        let Some(amented) = idx.packages().get(&dep).cloned() else {
            // AMENT doesn't know — silently skip (smart-stub semantics).
            continue;
        };
        // Codegen the AMENT pkg.
        let out_dir = build_root;
        std::fs::create_dir_all(&out_dir)?;
        if verbose {
            println!(
                "ws sync: codegen AMENT pkg {} → {}",
                amented.name,
                out_dir.display()
            );
        } else {
            println!("ws sync: codegen {}", amented.name);
        }
        let resolver = rosidl_codegen::CapacityResolver::discover(&amented.share_dir, None)?;
        let msg_resolve = ament_msg_resolver();
        rosidl_bindgen::generator::generate_package(
            &amented,
            &out_dir,
            edition,
            &resolver,
            &msg_resolve,
        )
        .wrap_err_with(|| format!("ws sync: generate_package failed for {}", amented.name))?;
        emitted.insert(amented.name.clone());
        // Queue this pkg's own deps (parse its package.xml).
        let pxml = amented.share_dir.join("package.xml");
        if pxml.is_file() {
            let body = std::fs::read_to_string(&pxml).unwrap_or_default();
            for d in extract_pkg_deps(&body) {
                if !in_workspace.contains(d.as_str()) && !emitted.contains(&d) {
                    to_resolve.push(d);
                }
            }
        }
    }
    Ok(())
}

// --- Scan ----------------------------------------------------------------------

fn has_pkg_subdir(dir: &Path) -> bool {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return false;
    };
    for e in entries.flatten() {
        if let Ok(t) = e.file_type() {
            if t.is_dir() && e.path().join("package.xml").is_file() {
                return true;
            }
        }
    }
    false
}

fn scan_one_pkg_dir(pkg_dir: &Path, out: &mut Vec<WsPkg>) -> Result<()> {
    scan_one_pkg_dir_inner(pkg_dir, out, true)
}

fn scan_one_pkg_dir_inner(
    pkg_dir: &Path,
    out: &mut Vec<WsPkg>,
    is_patch_consumer: bool,
) -> Result<()> {
    let manifest = pkg_dir.join("package.xml");
    let body = std::fs::read_to_string(&manifest)?;
    let Some(name) = extract_pkg_name(&body) else {
        bail!(
            "ws sync: single-pkg mode: package.xml at {} has no <name>",
            manifest.display()
        );
    };
    let is_msg_pkg = body.contains("rosidl_interface_packages")
        || pkg_dir.join("msg").is_dir()
        || pkg_dir.join("srv").is_dir()
        || pkg_dir.join("action").is_dir();
    let is_rust_pkg = pkg_dir.join("Cargo.toml").is_file();
    let deps = extract_pkg_deps(&body);
    // Phase 212.M-F.21 — when single-pkg mode lands on an Entry pkg
    // (or any Rust consumer that path-deps on a sibling Component pkg),
    // walk those path-deps + add the targets as siblings in `out` so
    // `augment_rust_consumer_deps_via_path_deps` can union their msg
    // `<*depend>` rows. Without this, single-pkg mode's `scan` only
    // contains the Entry pkg itself + the transitive walk has no msg
    // pkgs to discover. Imports are flagged `is_patch_consumer=false` —
    // cargo only respects `[patch.crates-io]` from the pkg it invokes,
    // so writing patches into a path-dep target is dead weight (and the
    // wrong-direction relative paths corrupt the target's manifest).
    if is_rust_pkg && let Ok(cargo_body) = std::fs::read_to_string(pkg_dir.join("Cargo.toml")) {
        for path in extract_cargo_path_deps(&cargo_body) {
            let target = pkg_dir.join(&path);
            if target.join("package.xml").is_file()
                && std::fs::canonicalize(&target).ok() != std::fs::canonicalize(pkg_dir).ok()
            {
                scan_one_pkg_dir_inner(&target, out, false)?;
            }
        }
    }
    out.push(WsPkg {
        name,
        dir: pkg_dir.to_path_buf(),
        manifest,
        is_msg_pkg,
        is_rust_pkg,
        deps,
        is_patch_consumer,
    });
    Ok(())
}

fn scan_workspace(src_root: &Path, out: &mut Vec<WsPkg>) -> Result<()> {
    for entry in std::fs::read_dir(src_root)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let dir = entry.path();
        let manifest = dir.join("package.xml");
        if !manifest.is_file() {
            continue;
        }
        let body = std::fs::read_to_string(&manifest)?;
        let Some(name) = extract_pkg_name(&body) else {
            continue;
        };
        let is_msg_pkg = body.contains("rosidl_interface_packages")
            || dir.join("msg").is_dir()
            || dir.join("srv").is_dir()
            || dir.join("action").is_dir();
        let is_rust_pkg = dir.join("Cargo.toml").is_file();
        let deps = extract_pkg_deps(&body);
        out.push(WsPkg {
            name,
            dir,
            manifest,
            is_msg_pkg,
            is_rust_pkg,
            deps,
            is_patch_consumer: true,
        });
    }
    Ok(())
}

fn extract_pkg_name(body: &str) -> Option<String> {
    let start = body.find("<name>")? + "<name>".len();
    let end = body[start..].find("</name>")? + start;
    Some(body[start..end].trim().to_string())
}

/// Phase 212.M-F.21 — walk each Rust consumer's `Cargo.toml [dependencies]`
/// + sibling `[dev-dependencies]` / `[build-dependencies]` tables for
/// `path = "..."` entries that resolve (by directory) to another `WsPkg`
/// in `scan`. For each such hit, union the target pkg's `deps` into the
/// consumer's `deps`. Idempotent — re-running deduplicates.
///
/// Concretely unblocks the Entry-pkg → Component-pkg path: the Entry
/// pkg's `package.xml` typically has no `<depend>` rows but its
/// `Cargo.toml` carries `freertos_rs_talker = { path = "../talker" }`.
/// The Component pkg's `package.xml` lists `<depend>std_msgs</depend>`
/// etc. — those msg deps need to land in the Entry pkg's patch table
/// (the patch authority cargo invokes).
fn augment_rust_consumer_deps_via_path_deps(scan: &mut Vec<WsPkg>) -> Result<()> {
    // Index by canonical directory so we can resolve path-dep targets.
    let dir_to_pkg: std::collections::HashMap<PathBuf, usize> = scan
        .iter()
        .enumerate()
        .filter_map(|(i, p)| std::fs::canonicalize(&p.dir).ok().map(|d| (d, i)))
        .collect();

    // Snapshot pre-augmentation deps so transitivity is single-hop per pass.
    // (Multi-hop chains converge after a small fixed number of passes; we
    // keep it deterministic + bounded.)
    for _ in 0..4 {
        let snapshot: Vec<Vec<String>> = scan.iter().map(|p| p.deps.clone()).collect();
        let mut changed = false;
        for i in 0..scan.len() {
            if !scan[i].is_rust_pkg {
                continue;
            }
            let cargo_toml = scan[i].dir.join("Cargo.toml");
            let Ok(body) = std::fs::read_to_string(&cargo_toml) else {
                continue;
            };
            for path in extract_cargo_path_deps(&body) {
                let target = scan[i].dir.join(&path);
                let Ok(canon) = std::fs::canonicalize(&target) else {
                    continue;
                };
                let Some(&j) = dir_to_pkg.get(&canon) else {
                    continue;
                };
                if i == j {
                    continue;
                }
                let target_deps = &snapshot[j];
                for d in target_deps {
                    if !scan[i].deps.contains(d) {
                        scan[i].deps.push(d.clone());
                        changed = true;
                    }
                }
            }
            scan[i].deps.sort();
            scan[i].deps.dedup();
        }
        if !changed {
            break;
        }
    }
    Ok(())
}

/// Extract `path = "<rel>"` values from `[dependencies]` /
/// `[dev-dependencies]` / `[build-dependencies]` tables. Loose TOML
/// scanner — handles single-line `pkg = { path = "..." }` form which
/// is the convention across nano-ros fixtures. Multi-line tables are
/// rare in fixture Cargo.tomls and skipped silently.
fn extract_cargo_path_deps(body: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut in_deps = false;
    for line in body.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            in_deps = matches!(
                trimmed,
                "[dependencies]" | "[dev-dependencies]" | "[build-dependencies]"
            );
            continue;
        }
        if !in_deps {
            continue;
        }
        // Match `<name> = { path = "<rel>", ... }` form.
        let Some(eq) = trimmed.find('=') else {
            continue;
        };
        let rhs = trimmed[eq + 1..].trim_start();
        if !rhs.starts_with('{') {
            continue;
        }
        if let Some(p) = rhs.find("path") {
            let after = &rhs[p + 4..];
            let after = after.trim_start().trim_start_matches('=').trim_start();
            if let Some(rest) = after.strip_prefix('"')
                && let Some(end) = rest.find('"')
            {
                out.push(rest[..end].to_string());
            }
        }
    }
    out
}

/// Phase 212.M-F.21 — walk up from `ws_root` looking for a nano-ros
/// source tree (marker: `packages/core/nros-core/Cargo.toml`). Used as
/// a fallback when neither `--nros-repo` nor `NROS_REPO_DIR` is set.
/// In-tree fixtures + examples sit several levels below the nano-ros
/// root, so this turns the most common "I forgot to set NROS_REPO_DIR"
/// case into a no-op — patches still flow.
fn autodetect_nano_ros_path(ws_root: &Path) -> Option<PathBuf> {
    let mut cur: Option<&Path> = Some(ws_root);
    while let Some(p) = cur {
        if p.join("packages/core/nros-core/Cargo.toml").is_file() {
            return Some(p.to_path_buf());
        }
        cur = p.parent();
    }
    None
}

fn extract_pkg_deps(body: &str) -> Vec<String> {
    let mut deps = Vec::new();
    for tag in &[
        "<depend>",
        "<build_depend>",
        "<exec_depend>",
        "<run_depend>",
        "<build_export_depend>",
    ] {
        let close = tag.replace("<", "</");
        let mut cursor = 0;
        while let Some(rel) = body[cursor..].find(tag) {
            let start = cursor + rel + tag.len();
            let Some(rel_close) = body[start..].find(close.as_str()) else {
                break;
            };
            let end = start + rel_close;
            let name = body[start..end].trim().to_string();
            if !name.is_empty() && !is_ros_meta_pkg(&name) {
                deps.push(name);
            }
            cursor = end;
        }
    }
    deps.sort();
    deps.dedup();
    deps
}

fn is_ros_meta_pkg(name: &str) -> bool {
    name.starts_with("rosidl")
        || name.starts_with("ament")
        || name == "rclcpp"
        || name == "rclpy"
        || name.starts_with("rcl")
        || name.starts_with("rmw")
        || name.starts_with("launch")
        || name == "catkin"
}

fn topo_sort_msg_pkgs(pkgs: &[&WsPkg]) -> Result<Vec<String>> {
    let names: std::collections::HashSet<&str> = pkgs.iter().map(|p| p.name.as_str()).collect();
    let mut remaining: Vec<&&WsPkg> = pkgs.iter().collect();
    let mut emitted: Vec<String> = Vec::new();
    while !remaining.is_empty() {
        let pick_idx = remaining.iter().position(|p| {
            p.deps
                .iter()
                .filter(|d| names.contains(d.as_str()))
                .all(|d| emitted.contains(d))
        });
        match pick_idx {
            Some(idx) => emitted.push(remaining.remove(idx).name.clone()),
            None => {
                let names: Vec<&str> = remaining.iter().map(|p| p.name.as_str()).collect();
                bail!("ws sync: dependency cycle (or missing dep) among {names:?}");
            }
        }
    }
    Ok(emitted)
}

// --- Patch authority -----------------------------------------------------------

fn find_patch_authority(start: &Path, ws_root: &Path) -> Result<PathBuf> {
    let mut cur = start.to_path_buf();
    loop {
        let cargo = cur.join("Cargo.toml");
        if cargo.is_file() {
            let body = std::fs::read_to_string(&cargo)?;
            if has_workspace_table(&body) {
                return Ok(cargo);
            }
        }
        if cur == *ws_root {
            return Ok(start.join("Cargo.toml"));
        }
        match cur.parent() {
            Some(p) => cur = p.to_path_buf(),
            None => return Ok(start.join("Cargo.toml")),
        }
    }
}

fn has_workspace_table(body: &str) -> bool {
    body.lines().any(|l| {
        let t = l.trim();
        t == "[workspace]" || t.starts_with("[workspace]")
    })
}

// --- Patch block writer --------------------------------------------------------

const BEGIN: &str = "# === BEGIN nros-managed [patch.crates-io] ===";
const END: &str = "# === END nros-managed [patch.crates-io] ===";

/// Phase 287 W9 (option E) — the crates whose `[patch.crates-io]` entries move
/// to the sync-generated CENTRAL `<checkout>/nros-patch.toml` (reached from each
/// leaf via one `include = [...]` line) instead of being emitted per-leaf.
///
/// Membership rule: a crate may live centrally only if it is registry-named in
/// EVERY Rust consumer's dependency graph — cargo warns "patch … was not used in
/// the crate graph" for a patch entry whose crate the graph never resolves
/// registry-style, and the central file is shared by all leaves. That limits the
/// set to the universal trio: `nros` (named by every consumer Cargo.toml) and
/// `nros-core`/`nros-serdes` (the hardcoded base of the managed set, named by
/// every generated msg crate). RMW crates are NOT universal (verified 2026-07-14:
/// a freertos entry's slim graph lacks `nros-rmw-cyclonedds-sys`/`-xrce-cffi`
/// and warns); board/driver/PAC crates even less so. Those stay per-leaf.
const CENTRAL_PATCH_CRATES: &[&str] = &["nros", "nros-core", "nros-serdes"];

/// File name of the central patch file at the nano-ros checkout root.
const CENTRAL_PATCH_FILE: &str = "nros-patch.toml";

/// Phase 287 W9 (option E) — write `<nano_ros_path>/nros-patch.toml`: the
/// central `[patch.crates-io]` for [`CENTRAL_PATCH_CRATES`], ABSOLUTE paths so
/// one generated file serves every leaf regardless of depth. Idempotent
/// (skip-write when content is unchanged, so repeated syncs don't churn the
/// mtime); atomic temp + rename otherwise.
/// #272 — the minor version of cargo that stabilized the `include` config key.
/// Before this, `include` is `-Z config-include` (nightly-only) and stable cargo
/// silently ignores it — dropping the central `[patch.crates-io]` and failing the
/// build with `no matching package named 'nros'`.
const CONFIG_INCLUDE_STABLE_MINOR: u64 = 93;

/// Parse the minor version out of `cargo --version` output
/// (`"cargo 1.96.0 (abc 2026-..)"` → `Some(96)`). `None` when the shape is
/// unrecognised (a custom/edge build) — the caller then stays quiet rather than
/// warn on a version it cannot read.
fn parse_cargo_minor(version_line: &str) -> Option<u64> {
    let ver = version_line.split_whitespace().nth(1)?; // "1.96.0"
    let mut parts = ver.split('.');
    let _major = parts.next()?;
    parts.next()?.parse::<u64>().ok()
}

/// Warn once if the workspace's effective cargo predates the `include` config-key
/// stabilization (1.93), so an external consumer on an old pinned toolchain gets
/// a clear diagnostic instead of a silent patch drop (#272). Best-effort: any
/// failure to run/parse `cargo --version` stays silent (never blocks sync).
fn warn_if_cargo_predates_config_include(ws_root: &Path) {
    // Run in the workspace root so a `rust-toolchain.toml` there selects the
    // SAME cargo the build will use, not whatever invoked `nros`.
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
    let Ok(out) = std::process::Command::new(cargo)
        .arg("--version")
        .current_dir(ws_root)
        .output()
    else {
        return;
    };
    if !out.status.success() {
        return;
    }
    let line = String::from_utf8_lossy(&out.stdout);
    let Some(minor) = parse_cargo_minor(&line) else {
        return;
    };
    if minor < CONFIG_INCLUDE_STABLE_MINOR {
        eprintln!(
            "warning: this workspace's cargo ({}) predates the `include` config-key \
             stabilization (cargo 1.{CONFIG_INCLUDE_STABLE_MINOR}). nros sync writes the \
             nros/nros-core/nros-serdes [patch.crates-io] rows into a central file reached \
             via `include = [\"…/nros-patch.toml\"]`, which stable cargo < 1.{CONFIG_INCLUDE_STABLE_MINOR} \
             SILENTLY IGNORES — the build will then fail `no matching package named 'nros'`. \
             Upgrade to cargo >= 1.{CONFIG_INCLUDE_STABLE_MINOR}, or add those three `path = ` rows \
             to `[patch.crates-io]` by hand.",
            line.trim(),
        );
    }
}

fn write_central_patch_file(nano_ros_path: &Path) -> Result<PathBuf> {
    let nrp = nano_ros_path
        .canonicalize()
        .unwrap_or_else(|_| nano_ros_path.to_path_buf());
    let mut body = String::from(
        "# Generated by `nros sync` — do not edit, do not commit (gitignored).\n\
         # Phase 287 W9 (RFC-0048 option E): the central [patch.crates-io] every\n\
         # Rust leaf reaches via one `include = [...]` line in .cargo/config.toml.\n\
         # Absolute paths: re-run `nros sync` after moving the checkout.\n\
         [patch.crates-io]\n",
    );
    for name in CENTRAL_PATCH_CRATES {
        let sub = nros_crate_subpath(name).expect("central crates are in the lookup table");
        let crate_root = nrp.join(&sub);
        if !crate_root.join("Cargo.toml").is_file() {
            continue;
        }
        body.push_str(&format!(
            "{name} = {{ path = \"{}\" }}\n",
            crate_root.display()
        ));
    }
    let dst = nrp.join(CENTRAL_PATCH_FILE);
    if std::fs::read_to_string(&dst).ok().as_deref() == Some(body.as_str()) {
        return Ok(dst);
    }
    let tmp = dst.with_file_name(format!(".{CENTRAL_PATCH_FILE}.tmp.{}", std::process::id()));
    std::fs::write(&tmp, &body).wrap_err_with(|| format!("ws sync: write {}", tmp.display()))?;
    std::fs::rename(&tmp, &dst)
        .wrap_err_with(|| format!("ws sync: rename {} -> {}", tmp.display(), dst.display()))?;
    Ok(dst)
}

fn write_patch_block(
    authority: &Path,
    build_root: &Path,
    pkgs: &[String],
    nano_ros_path: Option<&Path>,
    extra_runtime_crates: &[String],
    central_patch: Option<&Path>,
) -> Result<()> {
    let authority_dir = authority.parent().unwrap();
    let mut entries = render_managed_entries(
        authority,
        build_root,
        pkgs,
        nano_ros_path,
        extra_runtime_crates,
    );
    // #272 — how this leaf reaches the central trio (`nros`/`nros-core`/
    // `nros-serdes`) depends on whether it lives INSIDE the nano-ros checkout:
    //
    // - IN-TREE example leaf: its `.cargo/config.toml` is COMMITTED, so it uses
    //   the relative `include = ["…/nros-patch.toml"]` line (a host-absolute path
    //   would break every other checkout). The reachability bail below turns the
    //   include's silent-drop failure modes into a loud error.
    // - OUT-OF-TREE consumer (colcon / autoware_sentinel): NOT committed, and the
    //   `include` has three fragile preconditions (cargo ≥ 1.93, a correct
    //   relative path, a present central file) — tripping any one fails the build
    //   with an unexplained `no matching package named 'nros'`. So inline the trio
    //   with ABSOLUTE paths directly and skip the include entirely — the failure
    //   class cannot occur.
    let external = match nano_ros_path {
        Some(nrp) => {
            let nrp_c = nrp.canonicalize().unwrap_or_else(|_| nrp.to_path_buf());
            let auth_c = authority_dir
                .canonicalize()
                .unwrap_or_else(|_| authority_dir.to_path_buf());
            !auth_c.starts_with(&nrp_c)
        }
        None => false,
    };

    let include_rel: Option<String> = if external {
        // Replace render's RELATIVE trio rows (nros-core/nros-serdes) with the
        // full ABSOLUTE trio (incl. `nros`, which render never emits). No include.
        if let Some(nrp) = nano_ros_path {
            let nrp_c = nrp.canonicalize().unwrap_or_else(|_| nrp.to_path_buf());
            entries.retain(|(name, _)| !CENTRAL_PATCH_CRATES.contains(&name.as_str()));
            for name in CENTRAL_PATCH_CRATES {
                let Some(sub) = nros_crate_subpath(name) else {
                    continue;
                };
                let crate_root = nrp_c.join(&sub);
                if crate_root.join("Cargo.toml").is_file() {
                    entries.push((name.to_string(), crate_root.display().to_string()));
                }
            }
        }
        None
    } else {
        // In-tree: crates served by the central file drop out of the per-leaf
        // emit; the relative `include` line carries them instead.
        central_patch.map(|cp| {
            entries.retain(|(name, _)| !CENTRAL_PATCH_CRATES.contains(&name.as_str()));
            // Cargo resolves a relative `include` against the INCLUDING file's
            // directory (`<authority_dir>/.cargo/`).
            let cfg_dir = authority_dir.join(".cargo");
            pathdiff::diff_paths(cp, &cfg_dir)
                .unwrap_or_else(|| cp.to_path_buf())
                .display()
                .to_string()
        })
    };

    // 1) Write the managed [patch.crates-io] into `<authority_dir>/.cargo/config.toml`
    //    (phase-265: never the consumer Cargo.toml). Format-preserving toml_edit DOM.
    // #272 — fail loud when the include target is unreachable from this leaf
    // (cargo would silently drop the patch and the build would die with an
    // unexplained `no matching package named 'nros'`).
    if let Some(inc) = include_rel.as_deref() {
        let target = std::path::Path::new(inc);
        let resolved = if target.is_absolute() {
            target.to_path_buf()
        } else {
            // Resolve the `..` segments LEXICALLY. The include is relative to
            // the config file, i.e. `<authority>/.cargo/`, and on a first sync
            // that directory does not exist yet — so a filesystem-walking
            // `is_file()` on the unnormalised path fails through the missing
            // component and reports a perfectly readable central patch file as
            // unreachable. phase-307 hit exactly that adding a new example
            // workspace: `nros sync` refused a workspace whose only sin was
            // being new.
            lexically_join(&authority_dir.join(".cargo"), target)
        };
        if !resolved.is_file() {
            bail!(
                "ws sync: central patch file `{}` is not readable from `{}` — \
                 cargo ignores a missing `include` WITHOUT warning and the build \
                 would fail `no matching package named 'nros'`. Re-run `nros sync` \
                 from the nano-ros checkout (the file is gitignored + regenerated).",
                resolved.display(),
                authority_dir.display(),
            );
        }
    }
    write_patch_config(authority_dir, &entries, include_rel.as_deref())?;

    // 2) Migrate: vacate any legacy nros-managed `[patch.crates-io]` block from the
    //    consumer Cargo.toml (one-time; the patch now lives in config.toml). User
    //    patch rows + the rest of the manifest are preserved. Atomic temp + rename
    //    (the parallel-RMW-variant race the splice writer guarded still applies).
    let body = std::fs::read_to_string(authority)
        .wrap_err_with(|| format!("ws sync: read {}", authority.display()))?;
    let migrated = strip_managed_patch_from_cargo(&body);
    if migrated != body {
        let fname = authority
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("Cargo.toml");
        let tmp =
            authority.with_file_name(format!(".{fname}.nros-sync-tmp.{}", std::process::id()));
        std::fs::write(&tmp, migrated)
            .wrap_err_with(|| format!("ws sync: write {}", tmp.display()))?;
        std::fs::rename(&tmp, authority).wrap_err_with(|| {
            format!(
                "ws sync: rename {} -> {}",
                tmp.display(),
                authority.display()
            )
        })?;
    }

    println!(
        "ws sync: wrote [patch.crates-io] → {}",
        authority_dir.join(".cargo/config.toml").display()
    );
    Ok(())
}

/// Phase 265 (W3) — migrate a consumer Cargo.toml off the legacy nros-managed
/// `[patch.crates-io]` block (now that patches live in `.cargo/config.toml`).
/// Text-level (NOT toml_edit) so the rest of the hand-authored manifest is byte-
/// preserved: (1) remove every `BEGIN…END` managed region; (2) if a now-empty
/// `[patch.crates-io]` header remains (nothing but blanks until the next section /
/// EOF), drop the header + its trailing blanks too. User patch rows are kept.
fn strip_managed_patch_from_cargo(body: &str) -> String {
    let stripped = strip_managed_block(body);
    drop_empty_patch_crates_io_header(&stripped)
}

/// Remove a `[patch.crates-io]` (bare or quoted) header that has no entries before
/// the next `[section]` / EOF — only blank lines. Leaves a populated table intact.
fn drop_empty_patch_crates_io_header(body: &str) -> String {
    let lines: Vec<&str> = body.lines().collect();
    let mut out: Vec<&str> = Vec::with_capacity(lines.len());
    let mut i = 0usize;
    while i < lines.len() {
        if is_patch_crates_io_header(lines[i]) {
            // Look ahead: is the table body empty (only blanks) until the next
            // section header / EOF?
            let mut j = i + 1;
            let mut empty = true;
            while j < lines.len() {
                let t = lines[j].trim();
                if t.is_empty() {
                    j += 1;
                    continue;
                }
                // Next table header → table ended; anything else → non-empty.
                empty = t.starts_with('[');
                break;
            }
            if empty {
                // Skip the header + the run of blank lines after it; also drop one
                // trailing blank separator already in `out` for a minimal diff.
                if out.last().map(|l| l.trim().is_empty()).unwrap_or(false) {
                    out.pop();
                }
                i += 1;
                while i < lines.len() && lines[i].trim().is_empty() {
                    i += 1;
                }
                continue;
            }
        }
        out.push(lines[i]);
        i += 1;
    }
    let mut s = out.join("\n");
    if body.ends_with('\n') && !s.ends_with('\n') {
        s.push('\n');
    }
    s
}

/// Phase 220.E — static lookup of every nano-ros runtime crate the
/// `ws sync` writer knows how to emit a `[patch.crates-io]` path entry
/// for. Mirrors the workspace layout under `<NROS_REPO_DIR>/packages/`.
///
/// If a consumer references an `nros-*` crate not in this table, the
/// writer logs a warning + skips (so a third-party `nros-foo` extension
/// doesn't break sync — the user can hand-patch outside the managed
/// region).
///
/// Order here doesn't matter; the emission pass dedupes + sorts
/// alphabetically for diff-stable output.
const fn nros_crate_path_lookup() -> &'static [(&'static str, &'static str)] {
    &[
        // Core runtime
        ("nros", "packages/core/nros"),
        ("nros-core", "packages/core/nros-core"),
        ("nros-serdes", "packages/core/nros-serdes"),
        ("nros-platform", "packages/core/nros-platform"),
        ("nros-platform-api", "packages/core/nros-platform-api"),
        ("nros-platform-cffi", "packages/core/nros-platform-cffi"),
        ("nros-node", "packages/core/nros-node"),
        ("nros-rmw", "packages/core/nros-rmw"),
        ("nros-rmw-cffi", "packages/core/nros-rmw-cffi"),
        ("nros-log", "packages/core/nros-log"),
        ("nros-macros", "packages/core/nros-macros"),
        ("nros-params", "packages/core/nros-params"),
        // Phase 277 W6 — crates the standalone examples reference registry-style
        // after the path-dep flip but that don't ride the `nros-board-*` generic
        // fallback (a support crate under core/, a driver, and a board PAC whose
        // package name has no `nros-` prefix).
        (
            "nros-platform-critical-section",
            "packages/core/nros-platform-critical-section",
        ),
        // phase-291 (#211) — the zephyr-leaf build.rs bake helper; a
        // [build-dependencies] row in every zephyr rust example / ws entry.
        ("nros-zephyr-build", "packages/core/nros-zephyr-build"),
        (
            "nros-transport-callbacks",
            "packages/drivers/nros-transport-callbacks",
        ),
        ("mps2-an385-pac", "packages/boards/mps2-an385-pac"),
        // RMW backends
        ("nros-rmw-zenoh", "packages/rmw/zenoh/nros-rmw-zenoh"),
        (
            "nros-rmw-zenoh-staticlib",
            "packages/rmw/zenoh/nros-rmw-zenoh-staticlib",
        ),
        (
            "nros-rmw-cyclonedds",
            "packages/rmw/cyclonedds/nros-rmw-cyclonedds",
        ),
        (
            "nros-rmw-cyclonedds-sys",
            "packages/rmw/cyclonedds/nros-rmw-cyclonedds-sys",
        ),
        ("nros-rmw-xrce-cffi", "packages/xrce/nros-rmw-xrce-cffi"),
        (
            "nros-rmw-xrce-cffi-staticlib",
            "packages/xrce/nros-rmw-xrce-cffi-staticlib",
        ),
        // Transport / SDKs that consumers regularly reference as `version = "*"`
        ("cyclonedds-sys", "packages/rmw/cyclonedds/cyclonedds-sys"),
    ]
}

/// Phase 220.E — scan a consumer `Cargo.toml` body for `nros-*`,
/// `nros`, or `cyclonedds-sys` deps declared registry-style (`version =
/// "*"` or bare `"*"`). Returns crate names sorted + deduped.
///
/// Walks `[dependencies]`, `[dev-dependencies]`, `[build-dependencies]`,
/// and any `[target.<cfg>.dependencies]`-shaped table. Loose TOML scanner
/// matching the existing `extract_cargo_path_deps` style — handles the
/// single-line `name = { version = "*", ... }` form which is the only
/// shape current nano-ros examples use.
///
/// Path-style deps (`path = "..."`) are intentionally skipped — the
/// user already pinned a concrete location, no patch needed.
fn extract_consumer_registry_nros_deps(body: &str) -> Vec<String> {
    use toml_edit::{DocumentMut, Item, Value};

    // Phase 265 (W2) — toml_edit DOM walk. The inline `name = { version = … }`
    // and explicit `[dependencies.<name>]` (dotted) forms collapse to the SAME
    // DOM shape (a table-like dep item), so issue #94 case B disappears. A
    // malformed manifest (won't parse) yields no extras — same as the old loose
    // scanner finding nothing.
    let doc: DocumentMut = match body.parse() {
        Ok(d) => d,
        Err(_) => return Vec::new(),
    };

    // Registry-style iff a version is declared (bare string, or a table — inline
    // or dotted — carrying a `version` key). A path-only table (canonical 212
    // path-dep) is skipped; a `version` + `path` table counts (the version
    // registers the dep in the crates.io namespace `[patch.crates-io]` operates on).
    fn is_registry_style(item: &Item) -> bool {
        match item {
            Item::Value(Value::String(_)) => true,
            Item::Value(Value::InlineTable(t)) => t.contains_key("version"),
            Item::Table(t) => t.contains_key("version"),
            _ => false,
        }
    }
    fn scan_deps(deps: Option<&Item>, out: &mut Vec<String>) {
        let Some(tbl) = deps.and_then(|i| i.as_table_like()) else {
            return;
        };
        for (name, item) in tbl.iter() {
            if is_managed_runtime_crate_name(name) && is_registry_style(item) {
                out.push(name.to_string());
            }
        }
    }

    let mut out: Vec<String> = Vec::new();
    let root = doc.as_table();
    for kind in ["dependencies", "dev-dependencies", "build-dependencies"] {
        scan_deps(root.get(kind), &mut out);
    }
    // `[target.<cfg>.<kind>]` tables.
    if let Some(target) = root.get("target").and_then(|i| i.as_table_like()) {
        for (_cfg, cfg_item) in target.iter() {
            if let Some(cfg_tbl) = cfg_item.as_table_like() {
                for kind in ["dependencies", "dev-dependencies", "build-dependencies"] {
                    scan_deps(cfg_tbl.get(kind), &mut out);
                }
            }
        }
    }
    out.sort();
    out.dedup();
    out
}

/// True iff `name` is a crate the patch-block writer knows a workspace
/// path for. Restricts the 220.E extension surface to vetted names.
fn is_managed_runtime_crate_name(name: &str) -> bool {
    nros_crate_path_lookup().iter().any(|(n, _)| *n == name)
        // RFC-0040 D-Q3 — board crates are managed too (a scaffolded embedded
        // project deps `nros-board-<x> = "*"`). Their path is derived uniformly
        // (`packages/boards/<name>`), not enumerated in the static table.
        || name.starts_with("nros-board-")
}

/// RFC-0040 D-Q3 — map a managed crate name to its `<NROS_REPO_DIR>`-relative
/// subpath. Core/RMW crates come from the static [`nros_crate_path_lookup`]
/// table; board crates follow the uniform `packages/boards/<name>` convention,
/// so any current or future `nros-board-*` resolves without a table entry.
fn nros_crate_subpath(name: &str) -> Option<String> {
    if let Some((_, p)) = nros_crate_path_lookup().iter().find(|(n, _)| *n == name) {
        Some((*p).to_string())
    } else if name.starts_with("nros-board-") {
        Some(format!("packages/boards/{name}"))
    } else {
        None
    }
}

/// Crate names in a generated msg crate's `[dependencies]` /
/// `[build-dependencies]` / `[dev-dependencies]` tables (registry + path).
/// Used to walk the emitted msg-crate dep graph. toml_edit, like W2.
fn cargo_dependency_names(cargo_body: &str) -> Vec<String> {
    let Ok(doc) = cargo_body.parse::<toml_edit::DocumentMut>() else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for table in ["dependencies", "build-dependencies", "dev-dependencies"] {
        if let Some(t) = doc.get(table).and_then(|i| i.as_table_like()) {
            for (k, _) in t.iter() {
                out.push(k.to_string());
            }
        }
    }
    out
}

/// Phase-265 W5b — the transitive closure of `seeds` over the emitted msg-crate
/// dependency graph, intersected with `emitted`. A standalone consumer's patch
/// should carry only the generated msg crates it actually depends on (its
/// `package.xml` `<depend>` rows + their transitive msg deps) — NOT every crate
/// the sync run emitted. This excludes a node's own auto-generated self-crate
/// when nothing consumes it (e.g. `native/custom-msg` hand-codes its msgs inline
/// and uses `std_msgs`; its `msg/` dir still triggers self-codegen, but the
/// unconsumed self-crate must not land a broken `[patch.crates-io]` path entry).
fn emitted_msg_dep_closure(seeds: &[String], emitted: &[String], build_root: &Path) -> Vec<String> {
    let set: HashSet<&str> = emitted.iter().map(String::as_str).collect();
    let mut result: HashSet<String> = HashSet::new();
    let mut stack: Vec<String> = seeds
        .iter()
        .filter(|s| set.contains(s.as_str()))
        .cloned()
        .collect();
    while let Some(c) = stack.pop() {
        if !result.insert(c.clone()) {
            continue;
        }
        if let Ok(body) = std::fs::read_to_string(build_root.join(&c).join("Cargo.toml")) {
            for d in cargo_dependency_names(&body) {
                if set.contains(d.as_str()) && !result.contains(&d) {
                    stack.push(d);
                }
            }
        }
    }
    let mut v: Vec<String> = result.into_iter().collect();
    v.sort();
    v
}

/// Phase 265 (issue 0094) — the managed `(crate_name, relative_path)` patch entries
/// for a consumer authority, in emit order (generated msg crates first, then the
/// deduped + alphabetised runtime crates). Single source of the managed-set + path
/// policy behind the toml_edit `write_patch_config`. Paths are relative to the
/// authority's directory.
fn render_managed_entries(
    authority: &Path,
    build_root: &Path,
    pkgs: &[String],
    nano_ros_path: Option<&Path>,
    extra_runtime_crates: &[String],
) -> Vec<(String, String)> {
    let authority_dir = authority.parent().unwrap();
    let mut out: Vec<(String, String)> = Vec::new();

    // 1) Generated msg crates (path = generated/<pkg>).
    for pkg in pkgs {
        let crate_root = build_root.join(pkg);
        let rel = pathdiff::diff_paths(&crate_root, authority_dir).unwrap_or(crate_root);
        out.push((pkg.clone(), rel.display().to_string()));
    }

    if let Some(nrp) = nano_ros_path {
        let mut wanted: Vec<String> = vec!["nros-core".to_string(), "nros-serdes".to_string()];
        // Phase 244 E3 — scan each generated pkg's Cargo.toml for registry-style
        // runtime deps the consumer never names directly.
        let mut gen_extras: Vec<String> = Vec::new();
        for pkg in pkgs {
            if let Ok(gen_body) = std::fs::read_to_string(build_root.join(pkg).join("Cargo.toml")) {
                gen_extras.extend(extract_consumer_registry_nros_deps(&gen_body));
            }
        }
        for extra in extra_runtime_crates.iter().chain(gen_extras.iter()) {
            if nros_crate_subpath(extra).is_some() {
                if !wanted.iter().any(|w| w == extra) {
                    wanted.push(extra.clone());
                }
            } else {
                eprintln!(
                    "ws sync: unknown runtime crate `{extra}` referenced as registry dep; \
                     no path mapping in the nros lookup table — skipping patch entry."
                );
            }
        }
        wanted.sort();
        wanted.dedup();
        for cname in &wanted {
            let sub = nros_crate_subpath(cname).expect("cname is a managed crate; subpath exists");
            let crate_root = nrp.join(&sub);
            if !crate_root.join("Cargo.toml").is_file() {
                continue;
            }
            let rel = pathdiff::diff_paths(&crate_root, authority_dir).unwrap_or(crate_root);
            out.push((cname.clone(), rel.display().to_string()));
        }
    }
    out
}

/// Phase 265 (issue 0094) — decor suffix tagging a sync-owned `[patch.crates-io]`
/// entry in a `.cargo/config.toml`. Distinguishes managed entries from user keys
/// (a hand `libc` patch, etc.) so re-sync evicts only its own.
const NROS_MANAGED_TAG: &str = "nros-managed";

/// True if a `[patch.crates-io]` value carries the `# nros-managed` decor marker.
fn item_is_nros_managed(item: &toml_edit::Item) -> bool {
    item.as_value()
        .and_then(|v| v.decor().suffix())
        .and_then(|s| s.as_str())
        .map(|s| s.contains(NROS_MANAGED_TAG))
        .unwrap_or(false)
}

/// Phase 265 (issue 0094) — write the managed `[patch.crates-io]` entries into
/// `<authority_dir>/.cargo/config.toml` via a format-preserving `toml_edit` DOM
/// (replacing the line-based `Cargo.toml` splice). Each managed entry is tagged
/// with a `# nros-managed` decor suffix; on re-sync only tagged keys are evicted,
/// so user content (a hand `libc` patch, `[target]`/`[env]` sections) is preserved.
/// Atomic temp + rename. Creates `.cargo/config.toml` if absent; removes an emptied
/// `[patch.crates-io]` / `[patch]` table.
fn write_patch_config(
    authority_dir: &Path,
    managed: &[(String, String)],
    include_rel: Option<&str>,
) -> Result<()> {
    let cfg_dir = authority_dir.join(".cargo");
    let cfg = cfg_dir.join("config.toml");
    let text = std::fs::read_to_string(&cfg).unwrap_or_default();
    let out = render_patch_config(&text, managed, include_rel)
        .wrap_err_with(|| format!("ws sync: edit {}", cfg.display()))?;

    // Atomic write (create `.cargo/` first).
    std::fs::create_dir_all(&cfg_dir)
        .wrap_err_with(|| format!("ws sync: mkdir {}", cfg_dir.display()))?;
    let tmp = cfg.with_file_name(format!(".config.toml.nros-sync-tmp.{}", std::process::id()));
    std::fs::write(&tmp, out).wrap_err_with(|| format!("ws sync: write {}", tmp.display()))?;
    std::fs::rename(&tmp, &cfg)
        .wrap_err_with(|| format!("ws sync: rename {} -> {}", tmp.display(), cfg.display()))?;
    Ok(())
}

/// Pure DOM transform behind [`write_patch_config`]: given the existing
/// `.cargo/config.toml` text (empty string if absent) + the managed entries, return
/// the rewritten text with `[patch.crates-io]`'s nros-managed keys replaced. Format-
/// preserving (`toml_edit`); user keys + `[target]`/`[env]` untouched. No fs — pure +
/// unit-testable.
fn render_patch_config(
    existing: &str,
    managed: &[(String, String)],
    include_rel: Option<&str>,
) -> Result<String> {
    use toml_edit::{DocumentMut, Item, Table, Value, value};

    let mut doc: DocumentMut = existing.parse().wrap_err("parse .cargo/config.toml")?;

    // W9 option E — manage the top-level `include = [...]` array. Our entry is
    // recognised by its `nros-patch.toml` basename (evicted + re-added each
    // sync so a checkout-depth change re-points it); user include entries are
    // preserved, and an array left empty is removed. toml_edit keeps root
    // scalar keys ahead of tables when rendering, so the key lands in a valid
    // position even in a config that already carries [patch]/[env] tables.
    {
        let inc_item = doc
            .as_table_mut()
            .entry("include")
            .or_insert_with(|| value(toml_edit::Array::new()));
        let arr = inc_item
            .as_value_mut()
            .and_then(|v| v.as_array_mut())
            .ok_or_else(|| eyre!("ws sync: `include` is not an array"))?;
        arr.retain(|v| {
            v.as_str()
                .map(|s| !s.ends_with(CENTRAL_PATCH_FILE))
                .unwrap_or(true)
        });
        if let Some(rel) = include_rel {
            arr.insert(0, rel);
        }
        if arr.is_empty() {
            doc.as_table_mut().remove("include");
        }
    }

    // Ensure [patch] then [patch.crates-io] tables exist.
    let patch_item = doc
        .as_table_mut()
        .entry("patch")
        .or_insert_with(|| Item::Table(Table::new()));
    let patch_tbl = patch_item
        .as_table_mut()
        .ok_or_else(|| eyre!("ws sync: [patch] is not a table"))?;
    patch_tbl.set_implicit(true);
    let cio_item = patch_tbl
        .entry("crates-io")
        .or_insert_with(|| Item::Table(Table::new()));
    let cio = cio_item
        .as_table_mut()
        .ok_or_else(|| eyre!("ws sync: [patch.crates-io] is not a table"))?;

    // Evict prior nros-managed keys (preserve user keys + their decor).
    let stale: Vec<String> = cio
        .iter()
        .filter(|(_, v)| item_is_nros_managed(v))
        .map(|(k, _)| k.to_string())
        .collect();
    for k in stale {
        cio.remove(&k);
    }

    // Insert the current managed set, alphabetised + deduped, each tagged.
    let mut sorted: Vec<(String, String)> = managed.to_vec();
    sorted.sort_by(|a, b| a.0.cmp(&b.0));
    sorted.dedup_by(|a, b| a.0 == b.0);
    for (name, rel) in &sorted {
        let mut it = toml_edit::InlineTable::new();
        it.insert("path", Value::from(rel.as_str()));
        let mut item = value(Value::InlineTable(it));
        if let Some(v) = item.as_value_mut() {
            v.decor_mut().set_suffix(format!("  # {NROS_MANAGED_TAG}"));
        }
        cio.insert(name, item);
    }

    // Drop emptied tables so an empty managed set leaves no bare header (0094 F).
    if cio.is_empty() {
        patch_tbl.remove("crates-io");
    }
    if patch_tbl.is_empty() {
        doc.as_table_mut().remove("patch");
    }

    Ok(doc.to_string())
}

/// Remove EVERY contiguous BEGIN..END region from `body` (including both
/// marker lines). Returns `body` unchanged if no markers found.
///
/// Issue #94 case C — a prior crash or concurrent writer can leave more
/// than one managed block; strip them all so the next sync self-heals
/// instead of indefinitely carrying a stale duplicate.
fn strip_managed_block(body: &str) -> String {
    let mut out = body.to_string();
    while let Some(next) = strip_first_managed_block(&out) {
        out = next;
    }
    out
}

/// Remove the FIRST BEGIN..END region from `body`. Returns `None` when no
/// complete region is present (no BEGIN, or BEGIN without a following END).
fn strip_first_managed_block(body: &str) -> Option<String> {
    let begin_idx = body.find(BEGIN)?;
    let after_begin = begin_idx + BEGIN.len();
    let end_rel = body[after_begin..].find(END)?;
    let end_idx = after_begin + end_rel;
    let end_line_end = end_idx + END.len();
    // Consume the newline after END if present.
    let tail_start = if body[end_line_end..].starts_with('\n') {
        end_line_end + 1
    } else {
        end_line_end
    };
    let mut out = String::new();
    out.push_str(&body[..begin_idx]);
    // Drop a single trailing blank line above BEGIN if it was emitted as
    // a separator by a previous sync (keeps diffs minimal across re-runs).
    if out.ends_with("\n\n") {
        out.pop();
    }
    out.push_str(&body[tail_start..]);
    Some(out)
}

/// True iff `line` is a `[patch.crates-io]` table header, tolerating the
/// TOML-equivalent quoted form `[patch."crates-io"]` (or single-quoted) and
/// any trailing inline comment. Issue #94 case A — cargo/toml_edit and hand
/// edits both occur, and the bare-`starts_with` match missed the quoted form,
/// causing a duplicate header to be emitted (which cargo rejects).
fn is_patch_crates_io_header(line: &str) -> bool {
    let t = line.trim_start();
    let Some(rest) = t.strip_prefix('[') else {
        return false;
    };
    let Some(close) = rest.find(']') else {
        return false;
    };
    let inner = &rest[..close];
    let segs: Vec<&str> = inner.split('.').collect();
    segs.len() == 2
        && strip_toml_key_quotes(segs[0].trim()) == "patch"
        && strip_toml_key_quotes(segs[1].trim()) == "crates-io"
}

/// Strip surrounding quotes from a TOML bare key wrapped in `"..."` or
/// `'...'`. Bare keys pass through unchanged.
fn strip_toml_key_quotes(key: &str) -> &str {
    let trimmed = key.trim();
    if trimmed.len() >= 2 {
        let bytes = trimmed.as_bytes();
        let first = bytes[0];
        let last = bytes[trimmed.len() - 1];
        if (first == b'"' && last == b'"') || (first == b'\'' && last == b'\'') {
            return &trimmed[1..trimmed.len() - 1];
        }
    }
    trimmed
}

// --- Check / freshness ---------------------------------------------------------

fn check_freshness(
    ws_root: &Path,
    build_root: &Path,
    scan: &[WsPkg],
    topo: &[String],
) -> Result<()> {
    let mut stale = false;
    for name in topo {
        let pkg = scan.iter().find(|p| &p.name == name).unwrap();
        let crate_root = build_root.join(name);
        let cargo = crate_root.join("Cargo.toml");
        if !cargo.is_file() {
            eprintln!(
                "ws sync --check: stale: {name} — no Cargo.toml at {}",
                cargo.display()
            );
            stale = true;
            continue;
        }
        let cargo_mt = std::fs::metadata(&cargo)?.modified()?;
        for subdir in &["msg", "srv", "action"] {
            let d = pkg.dir.join(subdir);
            if !d.is_dir() {
                continue;
            }
            for entry in std::fs::read_dir(d)? {
                let entry = entry?;
                if !entry.file_type()?.is_file() {
                    continue;
                }
                let mt = entry.metadata()?.modified()?;
                if mt > cargo_mt {
                    eprintln!(
                        "ws sync --check: stale: {name} — {} newer than generated crate",
                        entry
                            .path()
                            .strip_prefix(ws_root)
                            .unwrap_or(&entry.path())
                            .display()
                    );
                    stale = true;
                }
            }
        }
    }
    if stale {
        bail!("ws sync --check: some pkgs stale — run `nros ws sync` first.");
    }
    println!("ws sync --check: all good.");
    Ok(())
}

// =============================================================================
// Phase 210.F.3 — `nros ws {list,status,clean,doctor}` sibling subcommands.
// All dual-mode (single-pkg + colcon-style workspace), same detection as sync.
// =============================================================================

/// Run sync's scan+resolve step without codegen — for list/status/clean/
/// doctor. Returns the workspace root + scanned pkgs + the resolved
/// build_root. The optional `build_dir` arg defaults to `<ws_root>/build`.
fn scan_for_query(
    workspace: Option<&Path>,
    build_dir: &Path,
) -> Result<(PathBuf, Vec<WsPkg>, PathBuf)> {
    let ws_root: PathBuf = match workspace {
        Some(p) => std::fs::canonicalize(p).wrap_err_with(|| format!("ws: {}", p.display()))?,
        None => std::env::current_dir()?,
    };
    let colcon_layout = ws_root.join("src").is_dir() && has_pkg_subdir(&ws_root.join("src"));
    let single_pkg_mode = !colcon_layout && ws_root.join("package.xml").is_file();
    let src_root = if colcon_layout {
        ws_root.join("src")
    } else if single_pkg_mode {
        ws_root.clone()
    } else {
        bail!(
            "ws: no `src/<pkg>/package.xml` and no `package.xml` at root \
             under {} — expected colcon-style workspace or single-pkg dir",
            ws_root.display()
        );
    };
    let mut scan = Vec::new();
    if single_pkg_mode {
        scan_one_pkg_dir(&src_root, &mut scan)?;
    } else {
        scan_workspace(&src_root, &mut scan)?;
    }
    let build_root = if build_dir.is_absolute() {
        build_dir.to_path_buf()
    } else {
        ws_root.join(build_dir)
    };
    Ok((ws_root, scan, build_root))
}

// --- list ---------------------------------------------------------------------

fn run_list(args: ListArgs) -> Result<()> {
    // build_dir doesn't matter for list; use the default for the scan
    // helper's signature.
    let (ws_root, scan, _build_root) =
        scan_for_query(args.workspace.as_deref(), Path::new("build"))?;
    if scan.is_empty() {
        println!("ws list: no pkgs found.");
        return Ok(());
    }
    println!("ws list ({}):", ws_root.display());
    let mut kinds = (0usize, 0usize); // (msg, rust)
    for p in &scan {
        let kind = match (p.is_msg_pkg, p.is_rust_pkg) {
            (true, true) => "msg+rust",
            (true, false) => "msg",
            (false, true) => "rust",
            (false, false) => "other",
        };
        if p.is_msg_pkg {
            kinds.0 += 1;
        }
        if p.needs_patch_authority() {
            kinds.1 += 1;
        }
        println!(
            "  {kind:9}  {:24}  {}",
            p.name,
            p.dir.strip_prefix(&ws_root).unwrap_or(&p.dir).display()
        );
    }
    println!("ws list: {} msg, {} rust consumer", kinds.0, kinds.1);
    Ok(())
}

// --- status -------------------------------------------------------------------

fn run_status(args: StatusArgs) -> Result<()> {
    let (ws_root, scan, build_root) = scan_for_query(args.workspace.as_deref(), &args.build_dir)?;
    let msg_pkgs: Vec<&WsPkg> = scan.iter().filter(|p| p.is_msg_pkg).collect();
    if msg_pkgs.is_empty() {
        println!("ws status: no msg pkgs.");
        return Ok(());
    }
    let mut up_to_date = 0;
    let mut stale = 0;
    let mut missing = 0;
    for pkg in &msg_pkgs {
        let crate_root = build_root.join(&pkg.name);
        let cargo = crate_root.join("Cargo.toml");
        if !cargo.is_file() {
            missing += 1;
            continue;
        }
        let cargo_mt = match std::fs::metadata(&cargo).and_then(|m| m.modified()) {
            Ok(t) => t,
            Err(_) => {
                missing += 1;
                continue;
            }
        };
        let mut pkg_stale = false;
        for subdir in &["msg", "srv", "action"] {
            let d = pkg.dir.join(subdir);
            if !d.is_dir() {
                continue;
            }
            for e in std::fs::read_dir(d)?.flatten() {
                if e.file_type().map(|t| t.is_file()).unwrap_or(false) {
                    if let Ok(mt) = e.metadata().and_then(|m| m.modified()) {
                        if mt > cargo_mt {
                            pkg_stale = true;
                            break;
                        }
                    }
                }
            }
            if pkg_stale {
                break;
            }
        }
        if pkg_stale {
            stale += 1;
        } else {
            up_to_date += 1;
        }
    }
    let _ = ws_root;
    println!(
        "ws status: {up_to_date} up-to-date, {stale} stale, {missing} missing \
         (of {} msg pkgs)",
        msg_pkgs.len()
    );
    Ok(())
}

// --- clean --------------------------------------------------------------------

fn run_clean(args: CleanArgs) -> Result<()> {
    let (ws_root, scan, build_root) = scan_for_query(args.workspace.as_deref(), &args.build_dir)?;
    let gen_dir = build_root;
    if gen_dir.is_dir() {
        if args.dry_run {
            println!("ws clean: WOULD rm -rf {}", gen_dir.display());
        } else {
            std::fs::remove_dir_all(&gen_dir)
                .wrap_err_with(|| format!("ws clean: rm {}", gen_dir.display()))?;
            println!("ws clean: removed {}", gen_dir.display());
        }
    } else {
        println!("ws clean: {} not present, skip", gen_dir.display());
    }
    // Phase 265 — strip the auto-managed `[patch.crates-io]` entries from every Rust
    // consumer's patch-authority `.cargo/config.toml` (the patch now lives there, not
    // the Cargo.toml). User keys (a hand `libc` patch) + `[target]`/`[env]` are kept.
    let rust_consumers: Vec<&WsPkg> = scan.iter().filter(|p| p.is_rust_pkg).collect();
    let mut authorities: std::collections::HashSet<PathBuf> = std::collections::HashSet::new();
    for c in &rust_consumers {
        if let Ok(a) = find_patch_authority(&c.dir, &ws_root) {
            authorities.insert(a);
        }
    }
    for authority in authorities {
        let cfg = authority
            .parent()
            .unwrap_or(&authority)
            .join(".cargo/config.toml");
        let body = match std::fs::read_to_string(&cfg) {
            Ok(b) => b,
            Err(_) => continue,
        };
        if !body.contains(NROS_MANAGED_TAG) {
            continue;
        }
        if args.dry_run {
            println!(
                "ws clean: WOULD strip managed patches from {}",
                cfg.display()
            );
            continue;
        }
        // Re-render with an empty managed set + no include → evicts every
        // nros-managed key AND the W9 central-patch include entry; drops an
        // emptied table/array; preserves user content.
        let cleaned = render_patch_config(&body, &[], None)
            .wrap_err_with(|| format!("ws clean: edit {}", cfg.display()))?;
        std::fs::write(&cfg, cleaned)
            .wrap_err_with(|| format!("ws clean: write {}", cfg.display()))?;
        println!("ws clean: stripped managed patches from {}", cfg.display());
    }
    Ok(())
}

// --- doctor -------------------------------------------------------------------

fn run_doctor(args: DoctorArgs) -> Result<()> {
    let (ws_root, scan, build_root) = scan_for_query(args.workspace.as_deref(), &args.build_dir)?;
    let mut warnings = 0;
    println!("ws doctor ({})", ws_root.display());
    for pkg in &scan {
        // (a) package.xml well-formed?
        let body = match std::fs::read_to_string(&pkg.manifest) {
            Ok(b) => b,
            Err(e) => {
                eprintln!("  ✗ {}: package.xml read error: {e}", pkg.name);
                warnings += 1;
                continue;
            }
        };
        // (b) msg pkg without member_of_group=rosidl_interface_packages
        let has_iface_group = body.contains("rosidl_interface_packages");
        let has_msg_dirs = pkg.dir.join("msg").is_dir()
            || pkg.dir.join("srv").is_dir()
            || pkg.dir.join("action").is_dir();
        if has_msg_dirs && !has_iface_group {
            eprintln!(
                "  ⚠ {}: has msg/srv/action dirs but pkg.xml lacks \
                 <member_of_group>rosidl_interface_packages</member_of_group> \
                 — upstream colcon won't classify it as an interface pkg",
                pkg.name
            );
            warnings += 1;
        }
        // (c) rust consumer: is the patch authority config sane?
        if pkg.needs_patch_authority() {
            match find_patch_authority(&pkg.dir, &ws_root) {
                Ok(a) => {
                    let cfg = a
                        .parent()
                        .map(|d| d.join(".cargo/config.toml"))
                        .unwrap_or_default();
                    let body = std::fs::read_to_string(&cfg).unwrap_or_default();
                    if !body.contains(NROS_MANAGED_TAG) {
                        eprintln!(
                            "  ⚠ {}: no nros-managed [patch.crates-io] entries in \
                             patch authority config ({}). Run `nros ws sync`.",
                            pkg.name,
                            cfg.display()
                        );
                        warnings += 1;
                    }
                }
                Err(e) => {
                    eprintln!("  ⚠ {}: patch authority resolve failed: {e}", pkg.name);
                    warnings += 1;
                }
            }
        }
    }
    // (d) stale msg pkgs (same logic as status).
    let _ = build_root;
    if warnings == 0 {
        println!("ws doctor: no issues.");
    } else {
        println!("ws doctor: {warnings} warning(s).");
    }
    Ok(())
}
// =============================================================================
// Phase 210.D.1 regression tests — `[patch.crates-io]` dedup writer.
// =============================================================================

#[cfg(test)]
mod config_include_version_tests {
    use super::*;

    #[test]
    fn parses_minor_from_cargo_version_line() {
        assert_eq!(parse_cargo_minor("cargo 1.96.0 (abc 2026-01-01)"), Some(96));
        assert_eq!(parse_cargo_minor("cargo 1.93.0"), Some(93));
        assert_eq!(
            parse_cargo_minor("cargo 1.90.1 (deadbeef 2025-06-01)"),
            Some(90)
        );
    }

    #[test]
    fn unrecognised_version_line_parses_to_none() {
        assert_eq!(parse_cargo_minor(""), None);
        assert_eq!(parse_cargo_minor("cargo"), None);
        assert_eq!(parse_cargo_minor("cargo weird-build"), None);
    }

    #[test]
    fn stable_boundary_is_1_93() {
        // The warn gate: < 93 warns, >= 93 stays quiet. Lock the boundary so a
        // refactor can't silently move it off the actual stabilization release.
        assert_eq!(CONFIG_INCLUDE_STABLE_MINOR, 93);
        assert!(parse_cargo_minor("cargo 1.92.0").unwrap() < CONFIG_INCLUDE_STABLE_MINOR);
        assert!(parse_cargo_minor("cargo 1.93.0").unwrap() >= CONFIG_INCLUDE_STABLE_MINOR);
    }
}

#[cfg(test)]
mod launch_resolver_tests {
    use super::*;

    fn touch(path: &std::path::Path) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, "#!/bin/sh\n").unwrap();
    }

    /// Installed layout: the helper sits beside the `nros` binary.
    #[test]
    fn finds_the_helper_beside_the_nros_binary() {
        let tmp = tempfile::tempdir().unwrap();
        let exe = tmp.path().join("bin").join("nros");
        touch(&exe);
        let helper = tmp.path().join("bin").join(LAUNCH_RESOLVER);
        touch(&helper);

        assert_eq!(resolver_from(&exe, None, None), Some(helper));
    }

    /// Per-checkout layout: `nros` is at `packages/cli/target/release/nros`,
    /// but the helper is its OWN cargo workspace, so it lands under
    /// `packages/cli/nros-launch-resolve/target/release/` instead. Found via
    /// `$NROS_REPO_DIR` and via the walk-up, matching `nros_cli_bin()`.
    #[test]
    fn finds_the_in_tree_helper_by_repo_dir_and_by_walk_up() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let exe = root
            .join("packages")
            .join("cli")
            .join("target")
            .join("release")
            .join("nros");
        touch(&exe);
        let helper = root
            .join("packages")
            .join("cli")
            .join(LAUNCH_RESOLVER)
            .join("target")
            .join("release")
            .join(LAUNCH_RESOLVER);
        touch(&helper);

        assert_eq!(
            resolver_from(&exe, None, Some(root.to_path_buf())),
            Some(helper.clone()),
            "$NROS_REPO_DIR should locate the per-checkout helper"
        );
        assert_eq!(
            resolver_from(&exe, None, None),
            Some(helper),
            "and the walk-up should find it without the env var"
        );
    }

    /// `$NROS_LAUNCH_RESOLVE` wins, mirroring `$NROS_CLI`; a non-existent
    /// override falls through instead of failing.
    #[test]
    fn env_override_wins_and_a_bad_one_falls_through() {
        let tmp = tempfile::tempdir().unwrap();
        let exe = tmp.path().join("bin").join("nros");
        touch(&exe);
        let sibling = tmp.path().join("bin").join(LAUNCH_RESOLVER);
        touch(&sibling);
        let packaged = tmp.path().join("packaged").join(LAUNCH_RESOLVER);
        touch(&packaged);

        assert_eq!(
            resolver_from(&exe, Some(packaged.clone()), None),
            Some(packaged),
            "the override must win over the sibling"
        );
        assert_eq!(
            resolver_from(&exe, Some(tmp.path().join("nope")), None),
            Some(sibling),
            "a bad override must fall through to the normal search"
        );
    }

    /// Issue 0285, the property the whole fix exists for: resolution NEVER
    /// consults `$PATH`. A helper reachable only through PATH must not be
    /// found — that is exactly how an unrelated `play_launch` hijacked this
    /// call and took every platform's fixture build down with it.
    #[test]
    fn a_helper_only_on_path_is_not_found() {
        let tmp = tempfile::tempdir().unwrap();
        let exe = tmp.path().join("bin").join("nros");
        touch(&exe);
        // Exists, but somewhere only PATH would reach.
        touch(
            &tmp.path()
                .join("usr")
                .join("local")
                .join("bin")
                .join(LAUNCH_RESOLVER),
        );

        assert_eq!(
            resolver_from(&exe, None, None),
            None,
            "a helper reachable only via PATH must NOT be used"
        );
    }

    /// No helper anywhere is a clean `None`, so the caller degrades to the
    /// committed model rather than failing the build.
    #[test]
    fn absent_helper_is_none() {
        let tmp = tempfile::tempdir().unwrap();
        let exe = tmp.path().join("bin").join("nros");
        touch(&exe);
        assert_eq!(resolver_from(&exe, None, None), None);
    }
}

#[cfg(test)]
mod patch_block_tests {
    use super::*;

    /// `strip_managed_block` is a no-op when no BEGIN marker is present.
    #[test]
    fn strip_managed_block_noop_without_markers() {
        let body = "[package]\nname = \"x\"\n";
        assert_eq!(strip_managed_block(body), body);
    }

    fn wspkg(name: &str, is_msg: bool, is_rust: bool, is_consumer: bool) -> WsPkg {
        WsPkg {
            name: name.to_string(),
            dir: PathBuf::from(format!("/ws/{name}")),
            manifest: PathBuf::from(format!("/ws/{name}/package.xml")),
            is_msg_pkg: is_msg,
            is_rust_pkg: is_rust,
            deps: Vec::new(),
            is_patch_consumer: is_consumer,
        }
    }

    /// Phase-265 W5b — a Rust node that ALSO defines msgs (inline `msg/` dir,
    /// e.g. `native/custom-msg`) is still a patch consumer; the old
    /// `!is_msg_pkg` guard wrongly dropped it ("no Rust consumer pkgs").
    #[test]
    fn node_with_msg_dir_is_a_patch_consumer() {
        // is_rust + is_msg + consumer → needs an authority (the fix).
        assert!(wspkg("custom_msg", true, true, true).needs_patch_authority());
        // pure interface pkg (no source Cargo.toml) → excluded by is_rust.
        assert!(!wspkg("std_msgs", true, false, true).needs_patch_authority());
        // plain rust consumer → included.
        assert!(wspkg("talker", false, true, true).needs_patch_authority());
        // path-dep import target (Entry→Component walk) → not an authority.
        assert!(!wspkg("component", false, true, false).needs_patch_authority());
    }

    /// `cargo_dependency_names` collects keys across the three dep tables.
    #[test]
    fn cargo_dependency_names_spans_all_dep_tables() {
        let body = r#"
[dependencies]
std_msgs = "*"
nros = { path = "../nros" }
[build-dependencies]
cc = "1"
[dev-dependencies]
proptest = "1"
"#;
        let mut got = cargo_dependency_names(body);
        got.sort();
        assert_eq!(got, vec!["cc", "nros", "proptest", "std_msgs"]);
    }

    /// The closure keeps only seeds reachable through the emitted graph and
    /// drops a node's unconsumed self-crate. Hermetic: writes a tiny
    /// `generated/<crate>/Cargo.toml` graph under a temp build root.
    #[test]
    fn closure_excludes_unconsumed_self_crate() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        // std_msgs depends on builtin_interfaces; the self-crate depends on
        // nothing emitted and is referenced by no one.
        for (c, deps) in [
            ("std_msgs", "builtin_interfaces = \"*\"\n"),
            ("builtin_interfaces", ""),
            ("native_rs_custom_msg", ""),
        ] {
            let dir = root.join(c);
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(
                dir.join("Cargo.toml"),
                format!("[package]\nname = \"{c}\"\n\n[dependencies]\n{deps}"),
            )
            .unwrap();
        }
        let emitted = vec![
            "builtin_interfaces".to_string(),
            "native_rs_custom_msg".to_string(),
            "std_msgs".to_string(),
        ];
        // Seed with the consumer's `<depend>` (std_msgs only).
        let got = emitted_msg_dep_closure(&["std_msgs".to_string()], &emitted, root);
        assert_eq!(
            got,
            vec!["builtin_interfaces".to_string(), "std_msgs".to_string()],
            "closure must reach builtin_interfaces but exclude the unconsumed self-crate"
        );
    }

    /// Phase 220.E — consumer Cargo.toml scanner finds every
    /// `nros-*` / `nros` / `cyclonedds-sys` dep with a registry-style
    /// version (`"*"` or `{ version = "*", ... }`), even when other
    /// shapes appear in the same `[dependencies]` table. Path-style
    /// deps (no `version` key) are excluded.
    #[test]
    fn extract_consumer_registry_deps_basic() {
        let body = r#"
[package]
name = "demo"

[dependencies]
zephyr = "0.1"
log = "0.4"
nros = { version = "*", default-features = false }
nros-rmw-zenoh = { version = "*", optional = true }
nros-rmw-cyclonedds-sys = { path = "../foo/nros-rmw-cyclonedds-sys" }
std_msgs = { version = "*", default-features = false }
"#;
        let got = extract_consumer_registry_nros_deps(body);
        // nros + nros-rmw-zenoh recognized (registry).
        // nros-rmw-cyclonedds-sys EXCLUDED (path-only, no version key).
        // zephyr/log/std_msgs ignored (not in lookup table).
        assert_eq!(got, vec!["nros".to_string(), "nros-rmw-zenoh".to_string()]);
    }

    /// Bare-string version form `name = "*"` recognized.
    #[test]
    fn extract_consumer_registry_deps_bare_version() {
        let body = r#"
[dependencies]
nros-core = "*"
nros-serdes = "0.4"
"#;
        let got = extract_consumer_registry_nros_deps(body);
        assert_eq!(
            got,
            vec!["nros-core".to_string(), "nros-serdes".to_string()]
        );
    }

    /// Both `version` AND `path` is treated as registry-style (cargo
    /// workspace shape — version key wins for `[patch.crates-io]`
    /// matching purposes).
    #[test]
    fn extract_consumer_registry_deps_version_plus_path() {
        let body = r#"
[dependencies]
nros = { version = "0.4", path = "../core/nros" }
"#;
        let got = extract_consumer_registry_nros_deps(body);
        assert_eq!(got, vec!["nros".to_string()]);
    }

    /// Target-cfg-scoped `[target.<cfg>.dependencies]` tables are
    /// walked too — common shape for platform-specific deps.
    #[test]
    fn extract_consumer_registry_deps_target_cfg() {
        let body = r#"
[dependencies]
log = "0.4"

[target.'cfg(target_os = "linux")'.dependencies]
nros-rmw-zenoh = { version = "*" }
"#;
        let got = extract_consumer_registry_nros_deps(body);
        assert_eq!(got, vec!["nros-rmw-zenoh".to_string()]);
    }

    /// `cyclonedds-sys` lives under `packages/rmw/cyclonedds/` and is intentionally
    /// in the lookup table — it's the most common non-`nros-*`-prefixed
    /// runtime crate consumers reference registry-style.
    #[test]
    fn extract_consumer_registry_deps_cyclonedds_sys() {
        let body = r#"
[dependencies]
cyclonedds-sys = { version = "*" }
nros-foo-extension = { version = "*" }
"#;
        let got = extract_consumer_registry_nros_deps(body);
        // `cyclonedds-sys` in lookup, `nros-foo-extension` is not.
        assert_eq!(got, vec!["cyclonedds-sys".to_string()]);
    }

    /// Path-only deps (the canonical Phase 212 shape) produce an empty
    /// scan — no patch entries needed since cargo resolves them directly.
    #[test]
    fn extract_consumer_registry_deps_path_only_empty() {
        let body = r#"
[dependencies]
nros = { path = "../../../packages/core/nros" }
nros-rmw-zenoh = { path = "../../../packages/rmw/zenoh/nros-rmw-zenoh" }
"#;
        let got = extract_consumer_registry_nros_deps(body);
        assert!(got.is_empty(), "expected no registry deps, got: {got:?}");
    }

    /// The lookup table covers every name the Phase 220 brief enumerates.
    #[test]
    fn lookup_table_covers_phase_220_e_minimum_set() {
        let must_have = [
            "nros",
            "nros-core",
            "nros-serdes",
            "nros-platform",
            "nros-platform-cffi",
            "nros-node",
            "nros-rmw",
            "nros-rmw-cffi",
            "nros-log",
            "nros-macros",
            "nros-rmw-zenoh",
            "nros-rmw-cyclonedds-sys",
            "nros-rmw-xrce-cffi",
            "cyclonedds-sys",
        ];
        for name in &must_have {
            assert!(
                is_managed_runtime_crate_name(name),
                "lookup table missing `{name}`"
            );
        }
    }

    /// Phase 277 W2.e — board crates (`packages/boards/*`) must resolve to a
    /// `[patch.crates-io]` path so `nros sync` can rewrite a scaffolded
    /// project's registry-style `nros-board-<x> = "*"` dep (W6 flips example
    /// board deps to registry-style). Board crates are NOT enumerated in the
    /// static [`nros_crate_path_lookup`] table — `is_managed_runtime_crate_name`
    /// / `nros_crate_subpath` recognize any `nros-board-`-prefixed name
    /// generically (RFC-0040 D-Q3) and derive `packages/boards/<name>`
    /// uniformly, since every current board crate's Cargo package name
    /// equals its directory name under `packages/boards/`. This test locks
    /// that resolution in for the concrete crates phase-277 cares about
    /// (verified against each crate's actual `Cargo.toml` `name =` field —
    /// note the bare-metal board crate is `nros-board-bare-metal`, not
    /// `nros-board-baremetal-cortex-m`).
    #[test]
    fn board_crates_resolve_via_generic_fallback() {
        let boards = [
            "nros-board-native",
            "nros-board-freertos",
            "nros-board-mps2-an385-freertos",
            "nros-board-threadx",
            "nros-board-threadx-qemu-riscv64",
            "nros-board-bare-metal",
        ];
        for name in &boards {
            assert!(
                is_managed_runtime_crate_name(name),
                "board crate `{name}` not recognized as managed"
            );
            assert_eq!(
                nros_crate_subpath(name),
                Some(format!("packages/boards/{name}")),
                "board crate `{name}` resolved to an unexpected subpath"
            );
        }
    }

    /// Phase 277 W6 — the standalone-example manifest flip (path-dep →
    /// `version = "*"`) references three nros-owned crates that neither the
    /// pre-W6 static table nor the `nros-board-*` generic fallback covered:
    /// the critical-section support crate, the custom-transport driver crate,
    /// and the MPS2 PAC (a `packages/boards/` crate WITHOUT the `nros-board-`
    /// name prefix). Lock their table entries in so `nros sync` emits patch
    /// rows for them instead of the "unknown runtime crate" skip warning.
    #[test]
    fn lookup_table_covers_w6_example_flip_extras() {
        let extras = [
            (
                "nros-platform-critical-section",
                "packages/core/nros-platform-critical-section",
            ),
            (
                "nros-transport-callbacks",
                "packages/drivers/nros-transport-callbacks",
            ),
            ("mps2-an385-pac", "packages/boards/mps2-an385-pac"),
        ];
        for (name, subpath) in &extras {
            assert!(
                is_managed_runtime_crate_name(name),
                "lookup table missing `{name}`"
            );
            assert_eq!(
                nros_crate_subpath(name),
                Some((*subpath).to_string()),
                "`{name}` resolved to an unexpected subpath"
            );
        }
    }

    /// Issue #94 case B — explicit dependency-table form
    /// `[dependencies.<name>]` (and target-scoped variants) must be scanned:
    /// a `version`-carrying entry needs a `[patch.crates-io]` path, a
    /// path-only entry does not.
    #[test]
    fn extract_consumer_registry_deps_explicit_table_form() {
        let body = r#"
[dependencies]
log = "0.4"

[dependencies.nros]
version = "*"
default-features = false

[dependencies.nros-rmw-zenoh]
path = "../rmw/zenoh/nros-rmw-zenoh"

[target.'cfg(target_os = "linux")'.dependencies.nros-core]
version = "*"
"#;
        let got = extract_consumer_registry_nros_deps(body);
        // nros + nros-core carry a version → registry → patched.
        // nros-rmw-zenoh is path-only → skipped.
        assert_eq!(got, vec!["nros".to_string(), "nros-core".to_string()]);
    }

    /// Issue #94 case C — `strip_managed_block` removes EVERY managed
    /// region, not just the first, so a doubled block (from a prior crash
    /// or concurrent writer) is self-healed on the next sync.
    #[test]
    fn strip_managed_block_removes_all_blocks() {
        let body = format!(
            "[package]\nname = \"x\"\n\n{BEGIN}\nnros-core = {{ path = \"a\" }}\n{END}\n\n\
             {BEGIN}\nnros-serdes = {{ path = \"b\" }}\n{END}\n"
        );
        let out = strip_managed_block(&body);
        assert!(!out.contains(BEGIN), "leftover BEGIN marker:\n{out}");
        assert!(!out.contains(END), "leftover END marker:\n{out}");
        assert!(out.contains("name = \"x\""), "package head lost:\n{out}");
    }

    // --- phase-265: render_patch_config (.cargo/config.toml, toml_edit) ---

    fn mng(items: &[(&str, &str)]) -> Vec<(String, String)> {
        items
            .iter()
            .map(|(n, p)| (n.to_string(), p.to_string()))
            .collect()
    }

    #[test]
    fn config_writer_creates_table_with_markers() {
        // Empty/absent config → one [patch.crates-io] with each managed key tagged.
        let out = render_patch_config(
            "",
            &mng(&[
                ("nros-core", "../nros-core"),
                ("std_msgs", "generated/std_msgs"),
            ]),
            None,
        )
        .unwrap();
        let doc: toml_edit::DocumentMut = out.parse().unwrap();
        let cio = doc["patch"]["crates-io"].as_table().unwrap();
        assert_eq!(
            cio.get("std_msgs").unwrap()["path"].as_str(),
            Some("generated/std_msgs")
        );
        assert_eq!(
            cio.get("nros-core").unwrap()["path"].as_str(),
            Some("../nros-core")
        );
        assert!(
            item_is_nros_managed(cio.get("nros-core").unwrap()),
            "managed key not tagged:\n{out}"
        );
        // Alphabetised: nros-core before std_msgs.
        let keys: Vec<&str> = cio.iter().map(|(k, _)| k).collect();
        assert_eq!(keys, vec!["nros-core", "std_msgs"], "not sorted:\n{out}");
    }

    /// Build a fake nano-ros checkout with the trio crate manifests present.
    #[cfg(test)]
    fn fake_checkout() -> tempfile::TempDir {
        let nrp = tempfile::tempdir().unwrap();
        for name in CENTRAL_PATCH_CRATES {
            let d = nrp
                .path()
                .join(nros_crate_subpath(name).expect("trio in lookup table"));
            std::fs::create_dir_all(&d).unwrap();
            std::fs::write(
                d.join("Cargo.toml"),
                format!("[package]\nname = \"{name}\"\n"),
            )
            .unwrap();
        }
        nrp
    }

    /// #272 — an OUT-OF-TREE consumer inlines the trio with ABSOLUTE paths and
    /// emits NO `include` line (which would otherwise silently drop on cargo
    /// < 1.93 / a wrong relative path / a missing central file).
    #[test]
    fn external_consumer_inlines_absolute_trio_no_include() {
        let nrp = fake_checkout();
        let central = write_central_patch_file(nrp.path()).unwrap();

        let ext = tempfile::tempdir().unwrap();
        let authority = ext.path().join("Cargo.toml");
        std::fs::write(&authority, "[package]\nname = \"consumer\"\n").unwrap();
        let build_root = ext.path().join("build");
        std::fs::create_dir_all(&build_root).unwrap();

        write_patch_block(
            &authority,
            &build_root,
            &[],
            Some(nrp.path()),
            &[],
            Some(&central),
        )
        .unwrap();

        let cfg = std::fs::read_to_string(ext.path().join(".cargo/config.toml")).unwrap();
        assert!(
            !cfg.contains("include"),
            "external must not use `include`:\n{cfg}"
        );
        let doc: toml_edit::DocumentMut = cfg.parse().unwrap();
        let cio = doc["patch"]["crates-io"].as_table().unwrap();
        for name in CENTRAL_PATCH_CRATES {
            let p = cio
                .get(*name)
                .unwrap_or_else(|| panic!("trio `{name}` not inlined:\n{cfg}"))["path"]
                .as_str()
                .unwrap();
            assert!(
                std::path::Path::new(p).is_absolute(),
                "`{name}` path not absolute: {p}"
            );
        }
    }

    /// #272 — an IN-TREE example leaf (under the checkout) keeps the relative
    /// `include` line and does NOT inline the trio (its config is committed).
    #[test]
    fn in_tree_leaf_uses_relative_include() {
        let nrp = fake_checkout();
        let central = write_central_patch_file(nrp.path()).unwrap();

        let authority = nrp.path().join("examples/foo/Cargo.toml");
        std::fs::create_dir_all(authority.parent().unwrap()).unwrap();
        std::fs::write(&authority, "[package]\nname = \"foo\"\n").unwrap();
        let build_root = nrp.path().join("build");
        std::fs::create_dir_all(&build_root).unwrap();

        write_patch_block(
            &authority,
            &build_root,
            &[],
            Some(nrp.path()),
            &[],
            Some(&central),
        )
        .unwrap();

        let cfg =
            std::fs::read_to_string(nrp.path().join("examples/foo/.cargo/config.toml")).unwrap();
        assert!(
            cfg.contains("include"),
            "in-tree leaf must use `include`:\n{cfg}"
        );
        let doc: toml_edit::DocumentMut = cfg.parse().unwrap();
        if let Some(cio) = doc
            .get("patch")
            .and_then(|p| p.get("crates-io"))
            .and_then(|c| c.as_table())
        {
            assert!(
                cio.get("nros").is_none(),
                "trio must be served by the include, not inlined:\n{cfg}"
            );
        }
    }

    #[test]
    fn config_writer_preserves_user_keys_and_sections() {
        // A hand `libc` patch + a [target] section must survive; libc stays UNtagged.
        let existing = "\
[target.thumbv7m-none-eabi]\n\
runner = \"qemu\"\n\n\
[patch.crates-io]\n\
libc = { path = \"../../third-party/nuttx/libc\" }\n";
        let out =
            render_patch_config(existing, &mng(&[("nros-core", "../nros-core")]), None).unwrap();
        let doc: toml_edit::DocumentMut = out.parse().unwrap();
        assert!(doc.get("target").is_some(), "[target] lost:\n{out}");
        let cio = doc["patch"]["crates-io"].as_table().unwrap();
        assert!(cio.get("libc").is_some(), "user libc patch evicted:\n{out}");
        assert!(
            !item_is_nros_managed(cio.get("libc").unwrap()),
            "user libc wrongly tagged:\n{out}"
        );
        assert!(
            item_is_nros_managed(cio.get("nros-core").unwrap()),
            "managed not tagged:\n{out}"
        );
    }

    #[test]
    fn config_writer_evicts_only_managed_on_resync() {
        // First sync, then re-sync with a DIFFERENT managed set: old managed keys gone,
        // a new one present, user key untouched.
        let existing = "[patch.crates-io]\nlibc = { path = \"x\" }\n";
        let s1 = render_patch_config(
            existing,
            &mng(&[
                ("std_msgs", "generated/std_msgs"),
                ("nros-core", "../nros-core"),
            ]),
            None,
        )
        .unwrap();
        // re-sync: std_msgs dropped (no longer generated), nros-serdes added.
        let s2 = render_patch_config(
            &s1,
            &mng(&[
                ("nros-core", "../nros-core"),
                ("nros-serdes", "../nros-serdes"),
            ]),
            None,
        )
        .unwrap();
        let doc: toml_edit::DocumentMut = s2.parse().unwrap();
        let cio = doc["patch"]["crates-io"].as_table().unwrap();
        assert!(
            cio.get("std_msgs").is_none(),
            "stale managed std_msgs not evicted:\n{s2}"
        );
        assert!(
            cio.get("nros-serdes").is_some(),
            "new managed missing:\n{s2}"
        );
        assert!(
            cio.get("libc").is_some(),
            "user libc lost on re-sync:\n{s2}"
        );
    }

    #[test]
    fn config_writer_idempotent() {
        let existing = "[patch.crates-io]\nlibc = { path = \"x\" }\n";
        let m = mng(&[
            ("nros-core", "../nros-core"),
            ("std_msgs", "generated/std_msgs"),
        ]);
        let a = render_patch_config(existing, &m, None).unwrap();
        let b = render_patch_config(&a, &m, None).unwrap();
        assert_eq!(a, b, "re-render not idempotent:\n--a--\n{a}\n--b--\n{b}");
    }

    #[test]
    fn config_writer_include_added_evicted_and_user_preserved() {
        // W9 option E — fresh config: include lands as a top-level array ahead
        // of the [patch.crates-io] table.
        let out = render_patch_config(
            "",
            &mng(&[("std_msgs", "generated/std_msgs")]),
            Some("../../../../nros-patch.toml"),
        )
        .unwrap();
        assert!(
            out.contains("include = [\"../../../../nros-patch.toml\"]"),
            "include missing:\n{out}"
        );
        assert!(
            out.find("include").unwrap() < out.find("[patch").unwrap(),
            "include must precede the patch table:\n{out}"
        );
        assert!(out.parse::<toml_edit::DocumentMut>().is_ok());

        // Re-sync at a different depth re-points OUR entry, preserves a user one.
        let with_user = out.replace(
            "include = [\"../../../../nros-patch.toml\"]",
            "include = [\"../../../../nros-patch.toml\", \"user.toml\"]",
        );
        let repointed = render_patch_config(
            &with_user,
            &mng(&[("std_msgs", "generated/std_msgs")]),
            Some("../../nros-patch.toml"),
        )
        .unwrap();
        assert!(repointed.contains("\"../../nros-patch.toml\""));
        assert!(
            !repointed.contains("../../../../nros-patch.toml"),
            "stale include entry not evicted:\n{repointed}"
        );
        assert!(
            repointed.contains("\"user.toml\""),
            "user include entry lost:\n{repointed}"
        );

        // ws clean shape (no include) drops OUR entry; array with only user
        // entries survives; array left empty is removed entirely.
        let cleaned = render_patch_config(&repointed, &[], None).unwrap();
        assert!(!cleaned.contains("nros-patch.toml"));
        assert!(cleaned.contains("\"user.toml\""));
        let ours_only = render_patch_config("", &[], Some("../nros-patch.toml")).unwrap();
        let emptied = render_patch_config(&ours_only, &[], None).unwrap();
        assert!(
            !emptied.contains("include"),
            "emptied include array not removed:\n{emptied}"
        );
    }

    #[test]
    fn config_writer_empty_managed_removes_table() {
        // No managed entries + no user keys → [patch.crates-io] (and [patch]) removed (0094 F).
        let out = render_patch_config(
            "[patch.crates-io]\nnros-core = { path = \"x\" }  # nros-managed\n",
            &[],
            None,
        )
        .unwrap();
        assert!(
            !out.contains("[patch"),
            "empty managed left a patch table:\n{out}"
        );
    }

    #[test]
    fn migrate_strips_managed_block_and_empty_header() {
        // In-tree example shape: [patch.crates-io] holds ONLY the managed BEGIN/END
        // block → migration removes the block AND the now-empty header.
        let body = format!(
            "[package]\nname = \"x\"\n\n[dependencies]\nnros = \"*\"\n\n[patch.crates-io]\n{BEGIN}\n\
             # banner\nnros-core = {{ path = \"a\" }}\n{END}\n"
        );
        let out = strip_managed_patch_from_cargo(&body);
        assert!(
            !out.contains("[patch.crates-io]"),
            "empty patch header left:\n{out}"
        );
        assert!(
            !out.contains(BEGIN) && !out.contains(END),
            "markers left:\n{out}"
        );
        assert!(
            out.contains("name = \"x\"") && out.contains("nros = \"*\""),
            "manifest body lost:\n{out}"
        );
    }

    #[test]
    fn migrate_keeps_user_patch_rows() {
        // A user (non-managed) patch row alongside the managed block: keep the row +
        // header, drop only the managed block.
        let body = format!(
            "[package]\nname = \"x\"\n\n[patch.crates-io]\nlibc = {{ path = \"z\" }}\n{BEGIN}\n\
             nros-core = {{ path = \"a\" }}\n{END}\n"
        );
        let out = strip_managed_patch_from_cargo(&body);
        assert!(
            out.contains("[patch.crates-io]"),
            "header wrongly dropped (had user row):\n{out}"
        );
        assert!(
            out.contains("libc = { path = \"z\" }"),
            "user row lost:\n{out}"
        );
        assert!(!out.contains(BEGIN), "managed block left:\n{out}");
    }

    #[test]
    fn config_writer_quoted_user_header_no_duplicate() {
        // Pre-existing quoted [patch."crates-io"] + user key → still ONE table via DOM;
        // managed merged in (0094 A immune by construction).
        let existing = "[patch.\"crates-io\"]\nlibc = { path = \"x\" }\n";
        let out =
            render_patch_config(existing, &mng(&[("nros-core", "../nros-core")]), None).unwrap();
        let doc: toml_edit::DocumentMut = out.parse().unwrap(); // parses = no duplicate table
        let cio = doc["patch"]["crates-io"].as_table().unwrap();
        assert!(
            cio.get("libc").is_some() && cio.get("nros-core").is_some(),
            "merge failed:\n{out}"
        );
    }
}

#[cfg(test)]
mod provenance_tests {
    // Issue 0320 — content-addressed staleness for committed SystemModels.
    use super::*;

    fn sha(bytes: &[u8]) -> String {
        format!("{:x}", Sha256::digest(bytes))
    }

    fn write_model(dir: &Path, inputs: Vec<(String, String)>) -> PathBuf {
        let mut m = ros_launch_manifest_model::SystemModel::default();
        m.meta.version = ros_launch_manifest_model::SCHEMA_VERSION;
        m.meta.inputs = inputs
            .into_iter()
            .map(|(path, sha256)| ros_launch_manifest_model::InputHash { path, sha256 })
            .collect();
        let p = dir.join("system_model.yaml");
        std::fs::write(&p, serde_yaml_ng::to_string(&m).unwrap()).unwrap();
        p
    }

    #[test]
    fn intact_provenance_is_not_stale() {
        let tmp = tempfile::tempdir().unwrap();
        let bringup = tmp.path();
        let content = b"[system]\n";
        std::fs::write(bringup.join("system.toml"), content).unwrap();
        let model = write_model(bringup, vec![("system.toml".into(), sha(content))]);
        assert_eq!(model_provenance_stale(&model, bringup), None);
    }

    #[test]
    fn changed_hash_is_stale() {
        let tmp = tempfile::tempdir().unwrap();
        let bringup = tmp.path();
        std::fs::write(bringup.join("system.toml"), b"new\n").unwrap();
        let model = write_model(bringup, vec![("system.toml".into(), sha(b"old\n"))]);
        assert!(
            model_provenance_stale(&model, bringup)
                .unwrap()
                .contains("hash changed")
        );
    }

    /// The 43 legacy models: an absolute path is non-portable and must
    /// regenerate even when the file it points at still exists and matches.
    #[test]
    fn absolute_path_is_stale_even_when_file_matches() {
        let tmp = tempfile::tempdir().unwrap();
        let bringup = tmp.path();
        let abs = bringup.join("system.toml");
        std::fs::write(&abs, b"x\n").unwrap();
        let model = write_model(bringup, vec![(abs.display().to_string(), sha(b"x\n"))]);
        assert!(
            model_provenance_stale(&model, bringup)
                .unwrap()
                .contains("absolute")
        );
    }

    #[test]
    fn missing_input_is_stale() {
        let tmp = tempfile::tempdir().unwrap();
        let bringup = tmp.path();
        let model = write_model(bringup, vec![("gone.toml".into(), sha(b"x"))]);
        assert!(
            model_provenance_stale(&model, bringup)
                .unwrap()
                .contains("missing")
        );
    }
}
