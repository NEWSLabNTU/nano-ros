//! `nros setup` — Phase 187.2: resolve a board's toolchain/SDK package set from
//! the index and report the install plan. The actual fetch / source-build /
//! cache is Phase 187.3; this verb does the CLI + board→package resolution +
//! `--list` / `--licenses` / the per-host disposition plan.
//!
//! See `docs/design/0014-nros-setup-toolchain-management.md`.

use std::{
    path::{Path, PathBuf},
    process::Command,
};

use clap::{Args as ClapArgs, Subcommand};
use eyre::{Result, WrapErr, bail};

use crate::{
    cmd::board::find_workspace_root,
    orchestration::{
        board_metadata::parse_board_metadata,
        sdk_index::{SdkIndex, host_key},
        sdk_store::{
            InstallAction, LOCK_FILE, SdkLock, SourceDisposition, execute, plan_install,
            provision_source, store_root, tool_prefix,
        },
    },
};

#[derive(Debug, ClapArgs)]
#[command(args_conflicts_with_subcommands = true)]
pub struct Args {
    /// Provisioning subcommands. `nros setup board <name> --zephyr-workspace
    /// <dir>` provisions a DOWNSTREAM Zephyr consumer's tree (Phase 215.J.2);
    /// omit it for the legacy host-toolchain `nros setup <board>` flow below.
    #[command(subcommand)]
    pub command: Option<SetupCommand>,

    /// Board to set up (resolves its toolchain/SDK package set from the index
    /// `[board.*]` table).
    pub board: Option<String>,

    /// List every package in the index + its version.
    #[arg(long)]
    pub list: bool,

    /// Show the license-gated packages + how to install them.
    #[arg(long)]
    pub licenses: bool,

    /// Install a single tool by name (instead of a board's whole set), e.g.
    /// `--tool qemu`. The `just <module> setup` recipes call this.
    #[arg(long)]
    pub tool: Option<String>,

    /// Provision a single `[source.*]` package by name from the index (Phase
    /// 195.B), e.g. `--source freertos-kernel`. Repeatable. The index is the
    /// SSOT — `dest`/`ref`/`submodule` come from data, never a hardcoded path.
    /// The `just <module> setup` recipes call this instead of inlining
    /// `git submodule update <path>`.
    #[arg(long = "source")]
    pub sources: Vec<String>,

    /// Install prefix override (only with `--tool`): place the tool here instead
    /// of the shared store, e.g. `--prefix build/qemu` so the test harness finds
    /// it where it already looks. Layout is identical (`<prefix>/bin/…`).
    #[arg(long)]
    pub prefix: Option<PathBuf>,

    /// Path to the SDK index.
    #[arg(long, default_value = "nros-sdk-index.toml")]
    pub index: PathBuf,

    /// RMW backend whose host daemon/tool to also provision — orthogonal to the
    /// board (Phase 191.6.a): `zenoh` | `xrce` | `cyclonedds`. Defaults to
    /// `zenoh`. Resolves `board.packages ∪ rmw.packages`.
    #[arg(long)]
    pub rmw: Option<String>,

    /// Resolve + print the plan without fetching/building anything.
    #[arg(long)]
    pub dry_run: bool,

    /// Provision full git history instead of the per-source shallow default
    /// (`--depth 1`). Use when you want `git log` / `blame` / branching in a
    /// provisioned source or submodule. Overrides the index `shallow` for this
    /// invocation only — no shared-file edit. (An already-shallow checkout is
    /// deepened in place with `git -C <path> fetch --unshallow`.)
    #[arg(long, conflicts_with = "shallow")]
    pub full: bool,

    /// Force shallow (`--depth 1`) even for sources that set `shallow = false`
    /// in the index. The inverse of `--full`.
    #[arg(long)]
    pub shallow: bool,
}

/// `nros setup <subcommand>` (Phase 215.J.2).
#[derive(Debug, Subcommand)]
pub enum SetupCommand {
    /// Provision a downstream Zephyr consumer's tree for a nano-ros board:
    /// fetch the board's RMW source, apply the zephyr-line patch set, add the
    /// rust targets, and check the `zephyr-lang-rust` pin — board-driven from
    /// the board's provisioning contract (`board.cmake` /
    /// `[package.metadata.nros.board]`). See RFC-0014 §"Downstream Zephyr
    /// consumer provisioning" + phase-215.J.
    Board(BoardSetupArgs),
}

#[derive(Debug, ClapArgs)]
pub struct BoardSetupArgs {
    /// Board crate suffix (after `nros-board-`), e.g. `fvp-aemv8r-smp`.
    pub name: String,

    /// The downstream consumer's Zephyr workspace dir (the tree containing
    /// `zephyr/` + `modules/lang/rust/`). The patch set is applied here.
    #[arg(long)]
    pub zephyr_workspace: PathBuf,

    /// nano-ros workspace root (where the board crate, `scripts/zephyr/`, and
    /// `nros-sdk-index.toml` live). Auto-detected by walking up from cwd, or
    /// via `NROS_WORKSPACE_ROOT`, if omitted.
    #[arg(long)]
    pub workspace: Option<PathBuf>,

    /// Path to the SDK index (relative paths resolve against the nano-ros
    /// workspace root). The RMW source's `dest` is index-driven.
    #[arg(long, default_value = "nros-sdk-index.toml")]
    pub index: PathBuf,

    /// Resolve + print the provisioning plan without fetching/patching.
    #[arg(long)]
    pub dry_run: bool,
}

/// Per-invocation shallow override from `--full` / `--shallow`: `None` = use the
/// per-source index default, `Some(false)` = full history, `Some(true)` = force
/// shallow.
fn shallow_override(args: &Args) -> Option<bool> {
    if args.full {
        Some(false)
    } else if args.shallow {
        Some(true)
    } else {
        None
    }
}

pub fn run(args: Args) -> Result<()> {
    if let Some(command) = args.command {
        return match command {
            SetupCommand::Board(b) => run_board(b),
        };
    }

    let index = SdkIndex::load(&args.index)?;
    let host = host_key();

    if args.list {
        print_list(&index);
        return Ok(());
    }
    if args.licenses {
        print_licenses(&index);
        return Ok(());
    }

    if let Some(tool) = args.tool.as_deref() {
        return install_single_tool(&index, tool, args.prefix.as_deref(), args.dry_run);
    }

    if !args.sources.is_empty() {
        return provision_named_sources(
            &index,
            &args.index,
            &args.sources,
            args.dry_run,
            shallow_override(&args),
        );
    }

    let board = match args.board.as_deref() {
        Some(b) => b,
        None => {
            bail!("nros setup: give a <board>, `--tool <name>`, `--list`, or `--licenses`")
        }
    };

    let packages = resolve_packages_with_rmw(&index, board, args.rmw.as_deref())?;
    eprintln!(
        "nros setup: {board} (rmw {}) needs {} package(s):",
        args.rmw.as_deref().unwrap_or("zenoh"),
        packages.len()
    );

    let root = store_root();
    let workspace = index_workspace(&args.index);
    let lock_path = PathBuf::from(LOCK_FILE);
    let mut lock = SdkLock::load(&lock_path)?;
    let mut installed = false;

    for name in &packages {
        // `[tool.*]` packages install into the shared store; `[source.*]` are
        // provisioned into their index-declared `dest` (Phase 195.B);
        // `[gated.*]` are user-installed.
        let Some(tool) = index.tool.get(*name) else {
            if let Some(src) = index.source.get(*name) {
                let disp =
                    provision_source(name, src, &workspace, args.dry_run, shallow_override(&args))
                        .wrap_err_with(|| format!("provision source {name}"))?;
                eprintln!("  {:<22} {}", name, describe_source(src, &disp));
                if matches!(disp, SourceDisposition::Provisioned) {
                    installed = true;
                }
            } else {
                eprintln!("  {:<22} {}", name, disposition(&index, name, &host));
            }
            continue;
        };
        let prefix = tool_prefix(&root, name, &tool.version);
        let action = plan_install(tool, &host, &prefix);
        eprintln!("  {:<22} {}", name, describe(&action, &tool.version, &host));

        if args.dry_run {
            continue;
        }
        match action {
            InstallAction::Unavailable => {
                bail!(
                    "nros setup: {name} {} has no prebuilt for {host} and no source recipe \
                     (add one to the index, or set up that host's toolchain manually)",
                    tool.version
                );
            }
            other => {
                let provenance = execute(&other, name, &tool.version, &prefix)
                    .wrap_err_with(|| format!("install {name} {}", tool.version))?;
                lock.record(name, &provenance);
                installed = true;
                eprintln!("    → {}", prefix.display());
            }
        }
    }

    if args.dry_run {
        eprintln!("(--dry-run: nothing installed)");
    } else if installed {
        lock.save(&lock_path)?;
        eprintln!(
            "nros setup: {board} ready; locked in {}",
            lock_path.display()
        );
    } else {
        eprintln!("nros setup: {board} — all packages already present");
    }
    Ok(())
}

/// Verify the consumer's Zephyr checkout matches the board's declared
/// `zephyr_line` (major.minor), reading `<ws>/zephyr/VERSION`. Hard-errors on a
/// mismatch: the line-specific patch set (`patches/<line>.sh`) and nano-ros's
/// platform-zephyr code target that exact Zephyr API, so applying them to a
/// different line drifts silently into deep compile errors (issue 0054). A
/// missing/unparseable VERSION is a warning, not a hard stop (don't block an
/// unusual-but-valid layout).
fn verify_zephyr_line(zephyr_ws: &Path, zephyr_line: &str) -> Result<()> {
    let version_file = zephyr_ws.join("zephyr").join("VERSION");
    let text = match std::fs::read_to_string(&version_file) {
        Ok(t) => t,
        Err(e) => {
            eprintln!(
                "  warning: cannot read {} ({e}) — skipping Zephyr-line check \
                 (board declares {zephyr_line}).",
                version_file.display()
            );
            return Ok(());
        }
    };
    let field = |key: &str| -> Option<String> {
        text.lines().find_map(|l| {
            let (k, v) = l.split_once('=')?;
            (k.trim() == key).then(|| v.trim().to_string())
        })
    };
    let (Some(major), Some(minor)) = (field("VERSION_MAJOR"), field("VERSION_MINOR")) else {
        eprintln!(
            "  warning: {} has no VERSION_MAJOR/MINOR — skipping Zephyr-line check.",
            version_file.display()
        );
        return Ok(());
    };
    let actual = format!("{major}.{minor}");
    if actual != zephyr_line {
        bail!(
            "nros setup board: consumer Zephyr is v{actual} but this board declares \
             zephyr_line={zephyr_line}. The {zephyr_line} patch set and nano-ros's \
             platform-zephyr code target the {zephyr_line} API; provisioning v{actual} \
             would drift into deep compile errors. Pin the consumer's zephyr to the \
             {zephyr_line} line (west.yml revision / submodule) and re-run."
        );
    }
    eprintln!("  zephyr check: consumer is v{actual} — matches board line {zephyr_line}");
    Ok(())
}

/// `nros setup board <name> --zephyr-workspace <dir>` (Phase 215.J.2).
///
/// Provisions a DOWNSTREAM Zephyr consumer's tree for a nano-ros board, driven
/// entirely by the board's provisioning contract (`board.cmake` /
/// `[package.metadata.nros.board]` — Phase 215.J.1). Reuses the existing
/// index-driven `--source` fetch + the workspace-parameterized patch scripts
/// (`scripts/zephyr/patches/<line>.sh $WORKSPACE`); no consumer-side
/// duplication, no forked index logic. Idempotent: source provisioning skips
/// when present, the patch scripts self-detect prior application, and
/// `rustup target add` is a no-op when installed.
fn run_board(args: BoardSetupArgs) -> Result<()> {
    // 1. Resolve the nano-ros workspace root (board crate + scripts + index).
    let root = match args.workspace {
        Some(p) => p,
        None => find_workspace_root().wrap_err(
            "nros setup board: could not locate the nano-ros workspace root \
             (pass --workspace <path> or set NROS_WORKSPACE_ROOT)",
        )?,
    };

    // 2. Validate the consumer's Zephyr workspace.
    let zephyr_ws = &args.zephyr_workspace;
    if !zephyr_ws.join("zephyr").is_dir() {
        bail!(
            "nros setup board: --zephyr-workspace `{}` does not look like a Zephyr \
             workspace (no `zephyr/` subdir). Point it at the consumer's west \
             topdir (the tree containing `zephyr/` + `modules/`).",
            zephyr_ws.display()
        );
    }

    // 3. Read the board's provisioning contract.
    let crate_dir = root
        .join("packages")
        .join("boards")
        .join(format!("nros-board-{}", args.name));
    let cargo_toml = crate_dir.join("Cargo.toml");
    if !cargo_toml.is_file() {
        bail!(
            "nros setup board: no board crate at `{}` (check the board name; \
             `nros board list` enumerates them)",
            crate_dir.display()
        );
    }
    let meta = parse_board_metadata(&cargo_toml)
        .wrap_err_with(|| format!("read provisioning contract from {}", cargo_toml.display()))?;

    let Some(zephyr_line) = meta.zephyr_line.as_deref() else {
        bail!(
            "nros setup board: `{}` has no `zephyr_line` in its provisioning \
             contract — it is not a Zephyr consumer board (nothing to provision). \
             Non-Zephyr boards are consumed via cargo path-deps, not `nros setup board`.",
            args.name
        );
    };

    // Verify the consumer's Zephyr matches the board's declared line BEFORE
    // touching the tree — the line-specific patch set + nano-ros's
    // platform-zephyr code target that exact Zephyr API, so a mismatched
    // checkout drifts silently into deep compile errors (issue 0054).
    verify_zephyr_line(zephyr_ws, zephyr_line)?;

    eprintln!(
        "nros setup board {}: provisioning consumer Zephyr tree at {}",
        args.name,
        zephyr_ws.display()
    );
    eprintln!("  contract: zephyr_line={zephyr_line}, requires_rust={}, rmw_source={}, rust_targets=[{}]",
        meta.requires_rust.unwrap_or(false),
        meta.rmw_source.as_deref().unwrap_or("-"),
        meta.rust_targets.join(", "),
    );

    // (a) Fetch the board's RMW source — index-driven, into nano-ros's own
    //     tree (the consumer links it via `nano_ros_use_board()` /
    //     `add_subdirectory(packages/dds/...)`), same as `just zephyr setup`.
    if let Some(rmw_source) = meta.rmw_source.as_deref() {
        let index_path = if args.index.is_absolute() {
            args.index.clone()
        } else {
            root.join(&args.index)
        };
        eprintln!("  (a) RMW source: nros setup --source {rmw_source}");
        if args.dry_run {
            eprintln!("      (--dry-run: skipped)");
        } else {
            let index = SdkIndex::load(&index_path).wrap_err_with(|| {
                format!("load SDK index from {}", index_path.display())
            })?;
            provision_named_sources(
                &index,
                &index_path,
                std::slice::from_ref(&rmw_source.to_string()),
                false,
                None,
            )?;
        }
    }

    // (b) Apply the zephyr-line patch set to the CONSUMER's tree. The patch
    //     scripts already take the workspace dir as $1.
    let patch_script = root
        .join("scripts")
        .join("zephyr")
        .join("patches")
        .join(format!("{zephyr_line}.sh"));
    if !patch_script.is_file() {
        bail!(
            "nros setup board: no patch set for Zephyr line `{zephyr_line}` at {} \
             (add scripts/zephyr/patches/{zephyr_line}.sh — see patches/README.md)",
            patch_script.display()
        );
    }
    eprintln!(
        "  (b) zephyr patches: bash {} {}",
        patch_script.display(),
        zephyr_ws.display()
    );
    if !args.dry_run {
        let status = Command::new("bash")
            .arg(&patch_script)
            .arg(zephyr_ws)
            .status()
            .wrap_err_with(|| format!("spawn {}", patch_script.display()))?;
        if !status.success() {
            bail!(
                "nros setup board: patch set {} exited with {status}",
                patch_script.display()
            );
        }
    }

    // (c) rustup target add (when the board requires Rust).
    if meta.requires_rust.unwrap_or(false) && !meta.rust_targets.is_empty() {
        eprintln!(
            "  (c) rust targets: rustup target add {}",
            meta.rust_targets.join(" ")
        );
        if !args.dry_run {
            for target in &meta.rust_targets {
                let status = Command::new("rustup")
                    .args(["target", "add", target])
                    .status()
                    .wrap_err("spawn rustup (is it on PATH?)")?;
                if !status.success() {
                    bail!("nros setup board: `rustup target add {target}` failed ({status})");
                }
            }
        }
    }

    // (d) Ensure the zephyr-lang-rust module is in the consumer's tree. The
    //     module fetch itself is west-native (the board ships a
    //     `west-downstream.yml` `import:false` fragment — Phase 215.J.3); here
    //     we only verify + instruct, never edit the consumer's manifest.
    if meta.requires_rust.unwrap_or(false) {
        ensure_lang_rust_module(&crate_dir, zephyr_ws);
    }

    if args.dry_run {
        eprintln!("nros setup board {}: (--dry-run: nothing changed)", args.name);
    } else {
        eprintln!(
            "nros setup board {}: consumer Zephyr tree provisioned",
            args.name
        );
    }
    Ok(())
}

/// Phase 215.J.2 step (d) — verify the `zephyr-lang-rust` module is present in
/// the consumer's tree; warn + point at the board's `west-downstream.yml`
/// import fragment (Phase 215.J.3) when it is not. Never mutates the consumer's
/// west manifest — the consumer keeps manifest authority.
fn ensure_lang_rust_module(crate_dir: &Path, zephyr_ws: &Path) {
    let module = zephyr_ws.join("modules").join("lang").join("rust");
    if module.join("Kconfig").is_file() {
        eprintln!("  (d) zephyr-lang-rust: present at {}", module.display());
        return;
    }
    let fragment = crate_dir.join("west-downstream.yml");
    eprintln!(
        "  (d) zephyr-lang-rust: MISSING at {}.\n\
         \x20     Add the board's import fragment to your west manifest, then `west update`:\n\
         \x20       manifest:\n\
         \x20         self:\n\
         \x20           import:\n\
         \x20             - file: {}\n\
         \x20     (board-shipped `import:false` fragment — pins zephyr-lang-rust at\n\
         \x20      nano-ros's supported rev; `name-allowlist` keeps it to that one module.)",
        module.display(),
        fragment.display()
    );
}

/// Install one tool by name (`nros setup --tool <name>`). `prefix_override`
/// (from `--prefix`) places it outside the shared store — e.g. `build/qemu`, the
/// location the test harness already reads, so `just <module> setup` can delegate
/// here with no harness change and no script-side path resolution. Prebuilt-or-
/// source per the index (187.3); the lockfile is only updated for shared-store
/// installs (a `--prefix` placement is workspace-local).
fn install_single_tool(
    index: &SdkIndex,
    name: &str,
    prefix_override: Option<&Path>,
    dry_run: bool,
) -> Result<()> {
    let host = host_key();
    let tool = index
        .tool
        .get(name)
        .ok_or_else(|| eyre::eyre!("nros setup --tool: no [tool.{name}] in the index"))?;
    let root = store_root();
    let prefix = prefix_override
        .map(Path::to_path_buf)
        .unwrap_or_else(|| tool_prefix(&root, name, &tool.version));

    let action = plan_install(tool, &host, &prefix);
    eprintln!(
        "nros setup --tool {name}: {} → {}",
        describe(&action, &tool.version, &host),
        prefix.display()
    );
    if dry_run {
        eprintln!("(--dry-run: nothing installed)");
        return Ok(());
    }
    match action {
        InstallAction::Present => {}
        InstallAction::Unavailable => bail!(
            "nros setup --tool {name} {}: no prebuilt for {host} and no source recipe",
            tool.version
        ),
        other => {
            let prov = execute(&other, name, &tool.version, &prefix)
                .wrap_err_with(|| format!("install {name} {}", tool.version))?;
            // Only the shared store is tracked by the lock; --prefix is local.
            if prefix_override.is_none() {
                let lock_path = PathBuf::from(LOCK_FILE);
                let mut lock = SdkLock::load(&lock_path)?;
                lock.record(name, &prov);
                lock.save(&lock_path)?;
            }
        }
    }
    Ok(())
}

/// Provision one or more `[source.*]` packages by name (`nros setup --source
/// <name> …`) — the index-driven replacement for inline `git submodule update
/// <path>` in the `just <module> setup` recipes (Phase 195.B; mirrors what
/// 187.6 did for `qemu`/`zenohd` via `--tool`). The index is the SSOT: `dest`,
/// `ref`, and `submodule` all come from data.
fn provision_named_sources(
    index: &SdkIndex,
    index_path: &Path,
    names: &[String],
    dry_run: bool,
    shallow_override: Option<bool>,
) -> Result<()> {
    let workspace = index_workspace(index_path);
    for name in names {
        let src = index
            .source
            .get(name.as_str())
            .ok_or_else(|| eyre::eyre!("nros setup --source: no [source.{name}] in the index"))?;
        let disp = provision_source(name, src, &workspace, dry_run, shallow_override)
            .wrap_err_with(|| format!("provision source {name}"))?;
        eprintln!(
            "nros setup --source {name}: {}",
            describe_source(src, &disp)
        );
    }
    Ok(())
}

/// Phase 187.6 — lazy install support: resolve the
/// board's index tools and install any not already in the store, so a first
/// platform build needs no separate `nros setup` (the PlatformIO auto-install
/// ergonomic). Only `[tool.*]` packages are installed; `[source.*]` build with
/// the app and `[gated.*]` are user-provided. Opt out with `NROS_NO_AUTO_SETUP`.
/// No-op (empty) when no index is found; an unavailable tool warns rather than
/// fails so the downstream platform build surfaces the real miss (e.g. a system-installed
/// toolchain the index doesn't host).
///
/// Returns the `bin/` dirs of the resolved tools present in the store — Method A
/// callers ([`activate_store_path`]) prepend these to the env so every spawned
/// child finds the toolchain, without any non-`nros` script resolving paths.
pub fn ensure_tools(board: &str, workspace: Option<&Path>) -> Result<Vec<PathBuf>> {
    if std::env::var_os("NROS_NO_AUTO_SETUP").is_some() {
        return Ok(Vec::new());
    }
    let Some(index_path) = locate_index(workspace) else {
        return Ok(Vec::new());
    };
    let index = SdkIndex::load(&index_path)?;
    let host = host_key();
    let root = store_root();
    let ws = index_workspace(&index_path);
    let lock_path = PathBuf::from(LOCK_FILE);
    let mut lock = SdkLock::load(&lock_path)?;
    let mut installed = false;
    let mut bin_dirs = Vec::new();

    // Unknown board ⇒ no known package set — warn + skip (lazy auto-setup is
    // best-effort; the user provides tools). `nros setup` errors instead.
    // Auto-setup defaults to the zenoh RMW host set (rmw=None). The default
    // keeps the historical behaviour (e.g. native pulls `zenohd`).
    let packages = match resolve_packages_with_rmw(&index, board, None) {
        Ok(p) => p,
        Err(_) => {
            eprintln!(
                "nros: board '{board}' not in the SDK index — skipping auto-setup \
                 (provide its tools yourself, or add a [board.{board}] entry)"
            );
            return Ok(Vec::new());
        }
    };
    for name in packages {
        let Some(tool) = index.tool.get(name) else {
            // Phase 195.B — provision `[source.*]` into its index `dest` so a
            // first build/deploy gets the kernel/lib source with no `just`.
            if let Some(src) = index.source.get(name) {
                // Lazy auto-setup uses the index per-source default (no
                // `--full`/`--shallow` to thread here).
                match provision_source(name, src, &ws, false, None) {
                    Ok(SourceDisposition::Provisioned) => {
                        eprintln!(
                            "nros: provisioned source {name} → {}",
                            src.dest.as_deref().unwrap_or("-")
                        );
                        installed = true;
                    }
                    Ok(_) => {}
                    Err(e) => eprintln!(
                        "nros: source {name} provisioning failed ({e}) — provide it yourself if the build needs it"
                    ),
                }
            }
            continue; // gated / not-in-index — not a store tool
        };
        let prefix = tool_prefix(&root, name, &tool.version);
        match plan_install(tool, &host, &prefix) {
            InstallAction::Present => {}
            InstallAction::Unavailable => {
                eprintln!(
                    "nros: {name} {} unavailable for {host} (no prebuilt, no source) — \
                     install it yourself if the build needs it",
                    tool.version
                );
                continue; // not in the store → nothing to add to PATH
            }
            action => {
                eprintln!(
                    "nros: auto-installing {name} {} (set NROS_NO_AUTO_SETUP to skip)",
                    tool.version
                );
                let prov = execute(&action, name, &tool.version, &prefix)
                    .wrap_err_with(|| format!("auto-setup {name} {}", tool.version))?;
                lock.record(name, &prov);
                installed = true;
                eprintln!("    → {}", prefix.display());
            }
        }
        let bin = prefix.join("bin");
        if bin.is_dir() {
            bin_dirs.push(bin);
        }
    }
    if installed {
        lock.save(&lock_path)?;
    }
    Ok(bin_dirs)
}

/// Method A — prepend the store `bin/` dirs (from [`ensure_tools`]) to this
/// process's `PATH` so child platform-tool invocations (cargo, cmake,
/// west, the `build[]`/`package[]` steps) find the toolchain on `PATH`. `nros`
/// is the single resolver; non-`nros` scripts/code never hunt for SDK paths.
/// A no-op when `dirs` is empty (no store tools / auto-setup skipped).
pub fn activate_store_path(dirs: &[PathBuf]) {
    if dirs.is_empty() {
        return;
    }
    let mut parts: Vec<PathBuf> = dirs.to_vec();
    if let Some(cur) = std::env::var_os("PATH") {
        parts.extend(std::env::split_paths(&cur));
    }
    if let Ok(joined) = std::env::join_paths(parts) {
        // SAFETY: a CLI invocation activating its own toolchain for the child
        // processes it is about to spawn; set before any thread reads the env.
        unsafe { std::env::set_var("PATH", joined) };
    }
}

/// Locate the SDK index for auto-setup: cwd, then the passed workspace, then
/// `$NROS_WORKSPACE`. `None` ⇒ auto-setup is a no-op (not every build runs near
/// a nano-ros workspace). Shared with `nros doctor`'s license-gate check (187.7).
pub(crate) fn locate_index(workspace: Option<&Path>) -> Option<PathBuf> {
    let cwd = PathBuf::from("nros-sdk-index.toml");
    if cwd.is_file() {
        return Some(cwd);
    }
    let ws = workspace
        .map(Path::to_path_buf)
        .or_else(|| std::env::var_os("NROS_WORKSPACE").map(PathBuf::from));
    ws.map(|w| w.join("nros-sdk-index.toml"))
        .filter(|p| p.is_file())
}

/// The workspace root a `[source.*]` `dest` is resolved against: the directory
/// containing the index (Phase 195.B — `dest` is workspace-relative index data,
/// never a path baked into the binary). Falls back to `.` for a bare index name.
fn index_workspace(index: &Path) -> PathBuf {
    index
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."))
}

/// One-line description of a source's provisioning outcome (Phase 195.B).
fn describe_source(
    src: &crate::orchestration::sdk_index::SourcePackage,
    disp: &SourceDisposition,
) -> String {
    use crate::orchestration::sdk_index::SourceProvision;
    let mode = match src.provision() {
        SourceProvision::Clone => format!(
            "clone {}@{}",
            src.git.as_deref().unwrap_or("?"),
            src.git_ref.as_deref().unwrap_or("?")
        ),
        SourceProvision::Submodule => {
            format!("submodule {}", src.submodule.as_deref().unwrap_or("?"))
        }
        SourceProvision::None => "built with the app".to_string(),
    };
    let outcome = match disp {
        SourceDisposition::Provisioned => "provisioned",
        SourceDisposition::AlreadyPresent => "already present (skip)",
        SourceDisposition::NoFetch => "no fetch step",
        SourceDisposition::Planned => "would provision (--dry-run)",
    };
    format!(
        "source {} — {mode} → {} [{outcome}]",
        src.version,
        src.dest.as_deref().unwrap_or("-")
    )
}

/// One-line description of the planned action (mirrors `disposition`, but for an
/// already-resolved [`InstallAction`]).
fn describe(action: &InstallAction, version: &str, host: &str) -> String {
    match action {
        InstallAction::Present => format!("present {version} (skip)"),
        InstallAction::Prebuilt { .. } => format!("prebuilt {version} (dist {host})"),
        InstallAction::Source { .. } => format!("source build {version} (no prebuilt for {host})"),
        InstallAction::Unavailable => {
            format!("UNAVAILABLE {version} (no prebuilt for {host}, no source)")
        }
    }
}

/// Resolve a board to its SDK package set from the index `[board.*]` table — the
/// board→toolchain SSOT (Phase 191.1). No board-name guessing: an unknown board
/// is a clear error listing the known boards, not a silent wrong package set
/// (the failure mode the old keyword heuristic had — it mis-resolved ESP32-C3 as
/// Xtensa). Adding a board is a `[board.<name>]` entry, no code change.
pub fn resolve_packages<'i>(index: &'i SdkIndex, board: &str) -> Result<Vec<&'i str>> {
    match index.board.get(board) {
        Some(entry) => Ok(entry.packages.iter().map(String::as_str).collect()),
        None => {
            let mut known: Vec<&str> = index.board.keys().map(String::as_str).collect();
            known.sort_unstable();
            bail!(
                "nros setup: unknown board '{board}'. Known boards: {}. \
                 Add a [board.{board}] entry to nros-sdk-index.toml.",
                if known.is_empty() {
                    "(none in index)".to_string()
                } else {
                    known.join(", ")
                }
            )
        }
    }
}

/// Resolve `board.packages ∪ rmw.packages` (Phase 191.6.a). RMW is an axis
/// orthogonal to the board: the board contributes its platform/toolchain
/// packages, the chosen RMW its host daemon/tool (`zenohd` / `xrce-agent` /
/// `cyclonedds`) — no `board×rmw` pair enumeration. `rmw=None` defaults to
/// `zenoh`. A legacy index with no `[rmw.*]` table returns the board set
/// unchanged; an unknown RMW name errors (listing the known ones).
pub fn resolve_packages_with_rmw<'i>(
    index: &'i SdkIndex,
    board: &str,
    rmw: Option<&str>,
) -> Result<Vec<&'i str>> {
    let mut packages = resolve_packages(index, board)?;
    if index.rmw.is_empty() {
        return Ok(packages); // legacy index without the RMW axis
    }
    let rmw = rmw.unwrap_or("zenoh");
    match index.rmw.get(rmw) {
        Some(entry) => {
            for pkg in &entry.packages {
                let p = pkg.as_str();
                if !packages.contains(&p) {
                    packages.push(p);
                }
            }
        }
        None => {
            let mut known: Vec<&str> = index.rmw.keys().map(String::as_str).collect();
            known.sort_unstable();
            bail!(
                "nros setup: unknown rmw '{rmw}'. Known RMWs: {}.",
                known.join(", ")
            );
        }
    }
    Ok(packages)
}

/// How `name` would be provisioned on `host`, per the index.
fn disposition(index: &SdkIndex, name: &str, host: &str) -> String {
    if let Some(tool) = index.tool.get(name) {
        if tool.dist_for(host).is_some() {
            format!("prebuilt {} (dist {host})", tool.version)
        } else if tool.source.is_some() {
            format!("source build {} (no prebuilt for {host})", tool.version)
        } else {
            format!(
                "UNAVAILABLE {} (no prebuilt for {host}, no source)",
                tool.version
            )
        }
    } else if let Some(src) = index.source.get(name) {
        format!("source {} (built with the app)", src.version)
    } else if let Some(g) = index.gated.get(name) {
        format!(
            "license-gated {} (set ${}{})",
            g.version,
            g.env,
            g.installer
                .as_deref()
                .map(|i| format!(", via {i}"))
                .unwrap_or_default()
        )
    } else {
        "NOT in index (add to nros-sdk-index.toml — 187.5)".to_string()
    }
}

fn print_list(index: &SdkIndex) {
    eprintln!("nros setup --list:");
    for (name, t) in &index.tool {
        eprintln!("  [tool]   {name:<22} {}", t.version);
    }
    for (name, s) in &index.source {
        eprintln!("  [source] {name:<22} {}", s.version);
    }
    for (name, g) in &index.gated {
        eprintln!("  [gated]  {name:<22} {} (${})", g.version, g.env);
    }
}

fn print_licenses(index: &SdkIndex) {
    if index.gated.is_empty() {
        eprintln!("nros setup --licenses: no license-gated packages");
        return;
    }
    eprintln!("nros setup --licenses (install these yourself; never fetched):");
    for (name, g) in &index.gated {
        eprintln!(
            "  {name:<16} {} — set ${}{}",
            g.version,
            g.env,
            g.installer
                .as_deref()
                .map(|i| format!(" (via {i})"))
                .unwrap_or_default()
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn board_index() -> SdkIndex {
        SdkIndex::parse(
            "[board.qemu-arm-freertos]\npackages=[\"arm-none-eabi-gcc\",\"qemu\",\"freertos-kernel\",\"lwip\"]\n\
             [board.qemu-riscv64-threadx]\npackages=[\"riscv-none-elf-gcc\",\"qemu\",\"threadx\"]\n\
             [board.esp32]\narch=\"riscv32\"\npackages=[]\n\
             [board.native]\npackages=[\"zenohd\"]\n\
             [board.orin-spe]\npackages=[\"arm-none-eabi-gcc\",\"nv-spe-fsp\"]\n",
        )
        .unwrap()
    }

    #[test]
    fn resolves_board_package_sets_from_index() {
        let idx = board_index();
        let fr = resolve_packages(&idx, "qemu-arm-freertos").unwrap();
        assert!(fr.contains(&"arm-none-eabi-gcc") && fr.contains(&"qemu"));
        assert!(fr.contains(&"freertos-kernel") && fr.contains(&"lwip"));

        let tx = resolve_packages(&idx, "qemu-riscv64-threadx").unwrap();
        assert!(
            tx.contains(&"riscv-none-elf-gcc") && tx.contains(&"qemu") && tx.contains(&"threadx")
        );

        // ESP32-C3: declared arch riscv32, no index host-tool (rustup target).
        assert!(resolve_packages(&idx, "esp32").unwrap().is_empty());
        assert_eq!(resolve_packages(&idx, "native").unwrap(), vec!["zenohd"]);
        let orin = resolve_packages(&idx, "orin-spe").unwrap();
        assert!(orin.contains(&"arm-none-eabi-gcc") && orin.contains(&"nv-spe-fsp"));

        // Unknown board → error (no silent wrong guess), lists known boards.
        let err = resolve_packages(&idx, "totally-unknown")
            .unwrap_err()
            .to_string();
        assert!(err.contains("unknown board") && err.contains("native"));
    }

    #[test]
    fn resolve_with_rmw_unions_board_and_rmw_packages() {
        let idx = SdkIndex::parse(
            "[tool.zenohd]\nversion=\"1\"\n[tool.xrce-agent]\nversion=\"1\"\n\
             [rmw.zenoh]\npackages=[\"zenohd\"]\n[rmw.xrce]\npackages=[\"xrce-agent\"]\n\
             [board.native]\npackages=[]\n[board.qemu-arm-freertos]\npackages=[\"qemu\"]\n",
        )
        .unwrap();
        // Default RMW is zenoh.
        assert_eq!(
            resolve_packages_with_rmw(&idx, "native", None).unwrap(),
            vec!["zenohd"]
        );
        // Explicit RMW swaps the daemon, board contributes the rest.
        assert_eq!(
            resolve_packages_with_rmw(&idx, "native", Some("xrce")).unwrap(),
            vec!["xrce-agent"]
        );
        let fr = resolve_packages_with_rmw(&idx, "qemu-arm-freertos", Some("xrce")).unwrap();
        assert!(fr.contains(&"qemu") && fr.contains(&"xrce-agent"));
        // Unknown RMW errors (lists known).
        assert!(resolve_packages_with_rmw(&idx, "native", Some("nope")).is_err());
        // Legacy index without an [rmw.*] table → board set unchanged.
        let legacy = SdkIndex::parse("[board.native]\npackages=[\"zenohd\"]\n").unwrap();
        assert_eq!(
            resolve_packages_with_rmw(&legacy, "native", None).unwrap(),
            vec!["zenohd"]
        );
    }

    #[test]
    fn locate_index_falls_back_to_workspace() {
        let n = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let ws = std::env::temp_dir().join(format!("nros_idx_{n}"));
        std::fs::create_dir_all(&ws).unwrap();
        // No index in the workspace yet → None (cwd has none under `cargo test`).
        assert_eq!(locate_index(Some(&ws)), None);
        // With one present → resolves to the workspace copy.
        let idx = ws.join("nros-sdk-index.toml");
        std::fs::write(&idx, "[tool.qemu]\nversion=\"1\"\n").unwrap();
        assert_eq!(locate_index(Some(&ws)), Some(idx));
        std::fs::remove_dir_all(&ws).ok();
    }

    #[test]
    fn ensure_tools_noop_without_index() {
        // No index near a temp workspace ⇒ Ok no-op.
        let n = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let ws = std::env::temp_dir().join(format!("nros_noidx_{n}"));
        std::fs::create_dir_all(&ws).unwrap();
        assert!(ensure_tools("native", Some(&ws)).is_ok());
        std::fs::remove_dir_all(&ws).ok();
    }

    #[test]
    fn disposition_reflects_index_state() {
        let idx = SdkIndex::parse(
            "[tool.qemu]\nversion=\"11.0\"\ndist.linux-x86_64={url=\"u\",sha256=\"h\"}\n\
             [tool.riscv-none-elf-gcc]\nversion=\"14\"\n[tool.riscv-none-elf-gcc.source]\ngit=\"g\"\nref=\"r\"\n\
             [source.freertos-kernel]\nversion=\"10.6.2\"\n\
             [gated.nv-spe-fsp]\nversion=\"36.3\"\nenv=\"NV_SPE_FSP_DIR\"\n",
        )
        .unwrap();
        assert!(disposition(&idx, "qemu", "linux-x86_64").starts_with("prebuilt"));
        assert!(disposition(&idx, "qemu", "macos-arm64").starts_with("UNAVAILABLE"));
        assert!(disposition(&idx, "riscv-none-elf-gcc", "macos-arm64").starts_with("source build"));
        assert!(disposition(&idx, "freertos-kernel", "linux-x86_64").starts_with("source "));
        assert!(disposition(&idx, "nv-spe-fsp", "linux-x86_64").starts_with("license-gated"));
        assert!(disposition(&idx, "openocd", "linux-x86_64").starts_with("NOT in index"));
    }
}
