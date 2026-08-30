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
        board_metadata::load_provisioning_contract,
        sdk_index::{SdkIndex, ToolPackage, host_key},
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

    /// #0390 — provision the whole build-stage source UNION the repo's
    /// `just test` / `build-test-fixtures` need (every RMW's `-sys` source + the
    /// platform sources the workspace graph path-deps), independent of the
    /// per-board `nros setup <board>` slice. The index's top-level `build_sources`
    /// is the SSOT. With `--check`, only VERIFY they are present and name
    /// `nros setup --source <name>` per missing — the preflight the two recipes run.
    #[arg(long = "build-sources")]
    pub build_sources: bool,

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

    /// phase-327 W2 (RFC-0062) — resolve the `[system.*]` OS-package closure
    /// for the detected package manager and PRINT the one native install
    /// command (never runs it without `--sudo`). With `--check`, run each
    /// entry's presence probe instead and exit non-zero if anything is
    /// missing (the doctor surface).
    #[arg(long)]
    pub system: bool,

    /// Run presence probes and report present/missing/unknown instead of
    /// installing/printing; exits 1 when anything is missing. With
    /// `--system`, probes only the `[system.*]` class; alone, walks EVERY
    /// declared class — `[system.*]`, `[rust.*]`, `[python.*]` — printing a
    /// remedy COMPUTED from each entry (phase-327 W3: doctor derives from
    /// the index, remedies are never hand-written).
    #[arg(long)]
    pub check: bool,

    /// With `--system`: EXECUTE the composed native install command (which
    /// invokes sudo where the manager needs it) instead of printing it.
    #[arg(long, requires = "system", conflicts_with = "check")]
    pub sudo: bool,
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
    if args.system {
        // The index lives AT the repo root, so its parent is the base a
        // `path` probe resolves against (phase-398 W2). Derived rather than
        // re-discovered, so the probe and the provisioner cannot disagree
        // about where a checkout is.
        let root = args.index.parent().map(std::path::Path::to_path_buf);
        return run_system(&index, args.check, args.sudo, root.as_deref());
    }
    // #0390 — must precede the generic `--check` below: `--build-sources --check`
    // is its own preflight, not the all-deps doctor pass.
    if args.build_sources {
        return run_build_sources(
            &index,
            &args.index,
            args.check,
            args.dry_run,
            shallow_override(&args),
        );
    }
    // `--tool <name> --check` asks about ONE tool. Without this the generic
    // `--check` below swallowed it and walked everything, so the targeted
    // question issue 0466 asks for ("is THIS tool at its pin?") could not be
    // put — which is why a drifted tool kept being discovered downstream.
    if args.check && args.tool.is_some() {
        return run_check_tool(&index, args.tool.as_deref().unwrap());
    }
    if args.check {
        return run_check_all(&index);
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
    // RFC-0048 §6 / phase-287 W5 — store bin dirs to fold onto the emitted
    // CMakePreset's environment.PATH (so the cross-compiler resolves).
    let mut bin_dirs: Vec<PathBuf> = Vec::new();

    // issue 0374 — resolve the whole plan first so the source builds can be
    // announced together, before the first fetch. The loop below prints and
    // installs one package at a time, so without this pre-pass the user learns
    // about a long build only as it starts.
    let source_builds = source_build_names(&index, &packages, &root, &host);
    warn_source_builds(&source_builds, &host);

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
        bin_dirs.push(prefix.join("bin"));
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

    // RFC-0048 §6 / phase-287 W5 — emit the CMakePreset for this board so
    // `nros init` + `cmake --preset <board>` cross-configure with no hand-set
    // toolchain. Best-effort: a failure here never fails provisioning.
    if !args.dry_run
        && let Err(e) = emit_board_cmake_preset(board, &workspace, &bin_dirs)
    {
        eprintln!("nros setup: CMakePreset not written: {e:#}");
    }
    Ok(())
}

/// Resolve the board's `[board.cmake] toolchain_file` and write
/// `~/.nros/presets/<board>.json`. Skips boards that have no preset shape (e.g.
/// Zephyr, which uses `west build`, not `cmake --preset`).
fn emit_board_cmake_preset(board: &str, workspace: &Path, bin_dirs: &[PathBuf]) -> Result<()> {
    use crate::orchestration::{
        board_descriptor::{BoardCatalog, PlatformKind},
        cmake_preset,
    };

    // Presets are `include`d from an arbitrary project dir, so every emitted path
    // must be absolute. Canonicalize the repo root (fall back to the given path if
    // it can't be resolved).
    let workspace_abs =
        std::fs::canonicalize(workspace).unwrap_or_else(|_| workspace.to_path_buf());
    let workspace = workspace_abs.as_path();

    let catalog =
        BoardCatalog::load(workspace).map_err(|e| eyre::eyre!("load board catalog: {e}"))?;
    let Some(desc) = catalog
        .descriptors()
        .iter()
        .find(|d| d.names.iter().any(|n| n == board))
    else {
        // Not a board descriptor (tool-only setup, alias mismatch) — no preset.
        return Ok(());
    };

    let toolchain_abs = desc
        .cmake
        .as_ref()
        .map(|c| workspace.join(&c.toolchain_file));
    // Cross boards carry a toolchain file; host posix emits a toolchain-less
    // preset (just `nano_ros_ROOT`). Everything else (Zephyr, …) has no
    // `cmake --preset` flow — skip.
    let is_host = matches!(desc.platform, PlatformKind::Posix);
    if toolchain_abs.is_none() && !is_host {
        return Ok(());
    }

    let path =
        cmake_preset::emit_board_preset(board, workspace, toolchain_abs.as_deref(), bin_dirs)?;
    eprintln!("nros setup: wrote CMakePreset {}", path.display());
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

    // 3. Read the board's provisioning contract — through the same
    //    bundle-aware resolver `nros board info` uses (issue 0729: this verb
    //    hand-built `packages/boards/nros-board-<name>` and so could not see
    //    any conf-bundle board, i.e. any in-tree Zephyr board post-337 W9.a).
    let crate_dir = crate::cmd::board::locate_board_crate(&root, &args.name)
        .wrap_err("nros setup board (`nros board list` enumerates the known boards)")?;
    let meta = load_provisioning_contract(&crate_dir)
        .wrap_err_with(|| format!("read provisioning contract from {}", crate_dir.display()))?;

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
    eprintln!(
        "  contract: zephyr_line={zephyr_line}, requires_rust={}, rmw_source={}, rust_targets=[{}]",
        meta.requires_rust.unwrap_or(false),
        meta.rmw_source.as_deref().unwrap_or("-"),
        meta.rust_targets.join(", "),
    );

    // (a) Fetch the board's RMW source — index-driven, into nano-ros's own
    //     tree (the consumer links it via `nano_ros_use_board()` /
    //     `add_subdirectory(packages/rmw/cyclonedds/...)`), same as `just zephyr setup`.
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
            let index = SdkIndex::load(&index_path)
                .wrap_err_with(|| format!("load SDK index from {}", index_path.display()))?;
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
        eprintln!(
            "nros setup board {}: (--dry-run: nothing changed)",
            args.name
        );
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
    // issue 0374 — same heads-up as the board path; a single `--tool` install
    // hits the identical source-build cost (sccache, play_launch_parser, …).
    if matches!(action, InstallAction::Source { .. }) {
        warn_source_builds(&[name], &host);
    }
    if dry_run {
        eprintln!("(--dry-run: nothing installed)");
        return Ok(());
    }
    // phase-327 W4 (issue 0368 F3) — the tool's declared system deps: a
    // dist's RUNTIME libs (libslirp) or a source recipe's BUILD deps (glib
    // for qemu's meson). Probe BEFORE doing any work, so the failure surface
    // is "install these packages" rather than a bare loader error out of a
    // later smoke check — or a dead configure 40 minutes into a source build.
    // Through `prereqs()`, never `index.system` — a consumer reading one table
    // sees half the SSoT while `[prereq.*]` and `[system.*]` coexist.
    let prereqs = index.prereqs();
    let mut missing_sys: Vec<&str> = Vec::new();
    for key in &tool.system {
        if let Some(dep) = prereqs.get(key)
            && run_probe(dep.check.as_ref()) == ProbeResult::Missing
        {
            missing_sys.push(key);
        }
    }
    if !missing_sys.is_empty() {
        let entries: Vec<(&String, &crate::orchestration::sdk_index::PrereqDep)> = prereqs
            .iter()
            .filter(|(k, _)| missing_sys.contains(&k.as_str()))
            .collect();
        let hint = detect_package_manager()
            .map(|mgr| native_install_command(mgr, &compose_packages(&entries, mgr)))
            .unwrap_or_else(|| "<no supported package manager detected>".to_string());
        bail!(
            "nros setup --tool {name}: needs {} system package(s) this host is \
             missing: {}.\n  Install with:  {hint}\n  \
             (declared as [tool.{name}] system = [..]; probes via [system.*].check)",
            missing_sys.len(),
            missing_sys.join(", "),
        );
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

/// #0390 — is a `[source.*]` provisioned on disk? An uninitialised submodule is
/// an absent or EMPTY directory; a provisioned one has entries. `dest` is
/// workspace-relative.
fn source_present(src: &crate::orchestration::sdk_index::SourcePackage, workspace: &Path) -> bool {
    let Some(dest) = src.dest.as_deref() else {
        return false;
    };
    std::fs::read_dir(workspace.join(dest))
        .map(|mut d| d.next().is_some())
        .unwrap_or(false)
}

/// #0390 — provision (or with `check`, VERIFY) the repo build stage's source
/// UNION (the index's top-level `build_sources`). `just test` links every RMW's
/// `-sys` and `build-test-fixtures` resolves graphs path-depping platform
/// sources, so the contributor build needs the whole set regardless of the
/// per-board slice `nros setup <board>` provisions. The `--check` mode is the
/// preflight those recipes run: it names `nros setup --source <name>` per missing
/// and exits non-zero, instead of letting the build die deep in a raw cargo /
/// build-script error naming a path with no mention of setup.
fn run_build_sources(
    index: &SdkIndex,
    index_path: &Path,
    check: bool,
    dry_run: bool,
    shallow: Option<bool>,
) -> Result<()> {
    if index.build_sources.is_empty() {
        bail!("nros setup --build-sources: the index declares no top-level `build_sources`");
    }
    // A name in `build_sources` with no `[source.*]` is a data bug — catch it
    // before it looks like a provisioning miss.
    for name in &index.build_sources {
        if !index.source.contains_key(name.as_str()) {
            bail!(
                "nros setup --build-sources: `{name}` is in `build_sources` but has no [source.{name}]"
            );
        }
    }

    if check {
        let workspace = index_workspace(index_path);
        let missing: Vec<&str> = index
            .build_sources
            .iter()
            .filter(|n| !source_present(&index.source[n.as_str()], &workspace))
            .map(String::as_str)
            .collect();
        if missing.is_empty() {
            eprintln!(
                "nros setup --build-sources: all {} build-stage source(s) present",
                index.build_sources.len()
            );
            return Ok(());
        }
        eprintln!(
            "nros setup: {} build-stage source(s) not provisioned — the workspace build needs them:",
            missing.len()
        );
        for name in &missing {
            eprintln!("  [MISSING] {name}    run: nros setup --source {name}");
        }
        eprintln!("  (or provision them all:  nros setup --build-sources)");
        bail!(
            "{} build-stage source(s) missing — see the `nros setup --source` line(s) above",
            missing.len()
        );
    }

    let names: Vec<String> = index.build_sources.clone();
    provision_named_sources(index, index_path, &names, dry_run, shallow)
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

/// Names of the `[tool.*]` packages in `packages` that this host will BUILD
/// FROM SOURCE — no `dist.<host>` row in the index, and not already installed.
/// `[source.*]` submodule packages are excluded: they are a git checkout, not a
/// compile. Split out of the board path so the selection is unit-testable
/// (issue 0374).
fn source_build_names<'p>(
    index: &SdkIndex,
    packages: &[&'p str],
    root: &Path,
    host: &str,
) -> Vec<&'p str> {
    packages
        .iter()
        .filter(|name| {
            index.tool.get(**name).is_some_and(|tool| {
                let prefix = tool_prefix(root, name, &tool.version);
                matches!(
                    plan_install(tool, host, &prefix),
                    InstallAction::Source { .. }
                )
            })
        })
        .copied()
        .collect()
}

/// Heads-up printed BEFORE any fetching starts when the index has no prebuilt
/// for this host and `nros setup` will therefore build the tool from source.
///
/// issue 0374 — installation.md promises "prebuilt toolchains per platform per
/// RMW", and for the book's own headline board that is not what happens:
/// `[tool.zenohd]` carries no `dist.<host>` row, so `nros setup native` cargo-
/// builds zenoh 1.7.2. On a first run that took minutes, pulled a SECOND rust
/// toolchain (the zenoh checkout pins its own), and left 792 MB in the store —
/// none of it announced. The per-package plan line ("source build … (no
/// prebuilt for …)") is accurate but reads as a routing detail rather than as
/// "this will take a while", and a user who did not think to pass `--dry-run`
/// meets it as an unexplained stall. Say the cost in words, once, up front.
fn warn_source_builds(names: &[&str], host: &str) {
    if names.is_empty() {
        return;
    }
    eprintln!(
        "nros setup: {} package(s) have no prebuilt for {host} — BUILDING FROM SOURCE: {}",
        names.len(),
        names.join(", ")
    );
    eprintln!(
        "  Expect minutes (tens of minutes for a large recipe) and hundreds of MB under {} — \
         the source checkout and its build dir stay in the store.",
        store_root().display()
    );
    // Issue 0374 d4 — this used to say a recipe pinning its own Rust toolchain
    // "also makes rustup fetch that toolchain". It no longer does by default:
    // source recipes build with the workspace channel (RUSTUP_TOOLCHAIN), so
    // the only recipes that still pull one are those that opted out.
    eprintln!(
        "  Rust recipes build with this workspace's pinned channel, so no extra \
         toolchain is downloaded — unless a recipe sets `respect_toolchain = true`."
    );
    eprintln!("  `nros setup … --dry-run` prints the full plan and fetches nothing.");
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

// Listings go to STDOUT (pipeable: `nros setup --list | grep …`);
// progress/diagnostics elsewhere in this file stay on stderr.
fn print_list(index: &SdkIndex) {
    println!("nros setup --list:");
    for (name, t) in &index.tool {
        println!("  [tool]   {name:<22} {}", t.version);
    }
    for (name, s) in &index.source {
        println!("  [source] {name:<22} {}", s.version);
    }
    for (name, g) in &index.gated {
        println!("  [gated]  {name:<22} {} (${})", g.version, g.env);
    }
}

fn print_licenses(index: &SdkIndex) {
    if index.gated.is_empty() {
        println!("nros setup --licenses: no license-gated packages");
        return;
    }
    println!("nros setup --licenses (install these yourself; never fetched):");
    for (name, g) in &index.gated {
        println!(
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

// ---------------------------------------------------------------------------
// phase-327 W2 (RFC-0062) — the `[system.*]` OS-package layer.
// ---------------------------------------------------------------------------

/// Detect the host's package manager: `/etc/os-release` `ID`/`ID_LIKE`
/// first (works even when several managers are installed), then a
/// `command -v` probe as fallback.
///
/// No macOS arm (issue 0916). This returned `Some("brew")` for
/// `env::consts::OS == "macos"`, added by phase-327 — after phase-260 dropped
/// macOS as a host, whose acceptance reads "No `apple`/`darwin`/`APPLE` branch
/// in nano-ros source/CMake/CI". `"macos"` is the same claim in a spelling that
/// criterion did not name, which is why it survived the sweep.
///
/// Removing it rather than keeping it is the honest end: nano-ros does not
/// BUILD on macOS (`nros-platform-posix/src/timer.c` calls `timer_create`,
/// which macOS does not implement; `nros-board-linux` calls
/// `sched_setaffinity`, which libc does not define for apple), so naming a
/// package manager there offers a first step down a road with no second one.
/// A macOS host now falls through to `None`, which callers already render as
/// "<no supported package manager detected>" — the same shape phase-260 W3
/// chose for the planner: "an explicit unsupported-host error, not a silent
/// route".
pub(crate) fn detect_package_manager() -> Option<&'static str> {
    if let Ok(os_release) = std::fs::read_to_string("/etc/os-release") {
        let mut ids = String::new();
        for line in os_release.lines() {
            if let Some(v) = line
                .strip_prefix("ID=")
                .or_else(|| line.strip_prefix("ID_LIKE="))
            {
                ids.push(' ');
                ids.push_str(v.trim_matches('"'));
            }
        }
        for token in ids.split_whitespace() {
            match token {
                "debian" | "ubuntu" => return Some("apt"),
                "fedora" | "rhel" | "centos" => return Some("dnf"),
                "arch" => return Some("pacman"),
                _ => {}
            }
        }
    }
    for (cmd, mgr) in [
        ("apt-get", "apt"),
        ("dnf", "dnf"),
        ("pacman", "pacman"),
        ("brew", "brew"),
    ] {
        if command_exists(cmd) {
            return Some(mgr);
        }
    }
    None
}

fn command_exists(cmd: &str) -> bool {
    let Some(path) = std::env::var_os("PATH") else {
        return false;
    };
    std::env::split_paths(&path).any(|dir| {
        let p = dir.join(cmd);
        p.is_file()
            && std::fs::metadata(&p)
                .map(|m| {
                    use std::os::unix::fs::PermissionsExt;
                    m.permissions().mode() & 0o111 != 0
                })
                .unwrap_or(false)
    })
}

/// The composed native install command for `manager` over `packages`.
/// One command, the user's to run — sudo is spelled out where the manager
/// needs it (brew must NOT run under sudo).
pub(crate) fn native_install_command(manager: &str, packages: &[String]) -> String {
    let list = packages.join(" ");
    match manager {
        "apt" => format!("sudo apt-get install -y {list}"),
        "dnf" => format!("sudo dnf install -y {list}"),
        "pacman" => format!("sudo pacman -S --needed {list}"),
        "brew" => format!("brew install {list}"),
        other => format!("<install via {other}: {list}>"),
    }
}

/// One entry's probe result.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum ProbeResult {
    Present,
    Missing,
    /// No probe declared, or the probe kind is not answerable on this host
    /// (e.g. `sharedlib` off Linux) — composed into the plan, not counted
    /// as missing.
    Unknown,
}

/// issue 0487 — probes are OR-ed: PRESENT if any declared probe finds the
/// dependency, MISSING only if at least one could answer and none did, UNKNOWN
/// if none was answerable on this host.
///
/// The schema used to allow exactly one probe, on the assumption that every
/// dependency has one right existence test. libgcrypt refuted it — Arch's 1.12
/// ships `libgcrypt.pc` and no `libgcrypt-config`, Ubuntu 22.04's 1.9 ships the
/// script and no `.pc` — so either probe alone is a false negative on one of
/// the two hosts. A false negative here does not degrade gracefully: it hard-
/// blocks `nros setup --tool esp32-qemu` and prints a sudo command for a
/// package that is already installed.
fn run_probe(check: Option<&crate::orchestration::sdk_index::CheckProbe>) -> ProbeResult {
    run_probe_in(check, None)
}

/// [`run_probe`] with the base directory a `path` probe resolves against.
///
/// phase-398 W2. Split rather than threaded through all nine call sites,
/// because only a provider that HAS a checkout can answer a `path` probe — the
/// OS-package callers have no base and must keep passing none, which then reads
/// `Unknown` rather than inventing a root to test against.
fn run_probe_in(
    check: Option<&crate::orchestration::sdk_index::CheckProbe>,
    base: Option<&std::path::Path>,
) -> ProbeResult {
    let Some(check) = check else {
        return ProbeResult::Unknown;
    };
    let mut answered_missing = false;
    if let Some(cmd) = &check.cmd {
        if command_exists(cmd) {
            return ProbeResult::Present;
        }
        answered_missing = true;
    }
    if let Some(lib) = &check.sharedlib
        && std::env::consts::OS == "linux"
        && let Ok(out) = std::process::Command::new("ldconfig").arg("-p").output()
    {
        let listing = String::from_utf8_lossy(&out.stdout);
        // PREFIX match: distros version the soname differently
        // (libclang-14.so.1 vs libclang.so.16), so `sharedlib = "libclang"`
        // matches any of them; an exact soname still works as a probe.
        if listing
            .lines()
            .any(|l| l.trim_start().starts_with(lib.as_str()))
        {
            return ProbeResult::Present;
        }
        answered_missing = true;
    }
    if let Some(hdr) = &check.header
        && std::env::consts::OS == "linux"
    {
        // issue 0603 — the probe for a `-dev` package whose consumer COMPILES
        // against it. `sharedlib` cannot answer this: it prefix-matches the
        // versioned SONAME the RUNTIME package ships, so mbedTLS read as
        // present on a host carrying only `libmbedtls14`, and the sweep died
        // twenty minutes later inside a vendored TU.
        //
        // Default search path only, and no compiler invocation: this runs for
        // every entry on every `nros setup`, and a probe that shells a compiler
        // would cost more than the check is worth. A package that installs
        // outside these roots wants `pkg_config` or `sharedlib` instead.
        let multiarch = ["aarch64-linux-gnu", "x86_64-linux-gnu"];
        let mut roots: Vec<std::path::PathBuf> = vec![
            std::path::PathBuf::from("/usr/include"),
            std::path::PathBuf::from("/usr/local/include"),
        ];
        roots.extend(
            multiarch
                .iter()
                .map(|m| std::path::PathBuf::from("/usr/include").join(m)),
        );
        if roots.iter().any(|r| r.join(hdr).exists()) {
            return ProbeResult::Present;
        }
        answered_missing = true;
    }
    if let Some(pc) = &check.pkg_config {
        // No pkg-config on the host: this probe cannot answer, which is NOT the
        // same as the dependency being absent — leave `answered_missing` alone.
        if let Ok(status) = std::process::Command::new("pkg-config")
            .args(["--exists", pc])
            .status()
        {
            if status.success() {
                return ProbeResult::Present;
            }
            answered_missing = true;
        }
    }
    // phase-398 W2 — `runs`: the resolved binary EXECUTES. `cmd` above is a
    // PATH lookup and cannot see a store dist at all; this is the probe the
    // libslirp failure needed, where the path existed and the loader disagreed.
    if let Some(line) = &check.runs {
        let mut it = line.split_whitespace();
        if let Some(exe) = it.next() {
            let args: Vec<&str> = it.collect();
            match std::process::Command::new(exe)
                .args(&args)
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status()
            {
                Ok(st) if st.success() => return ProbeResult::Present,
                // Ran and failed: a loader error, a missing sub-dependency, a
                // broken dist. That IS absent for our purposes.
                Ok(_) => answered_missing = true,
                // Could not spawn at all. `command_exists` distinguishes "not
                // installed" from "here but unrunnable"; without it this would
                // report a foreign-platform tool as broken rather than
                // unanswerable.
                Err(_) if command_exists(exe) => answered_missing = true,
                Err(_) => {}
            }
        }
    }
    // phase-398 W2 — `path`: a file inside the provider's checkout. Answerable
    // only with a base; without one the probe abstains rather than guessing a
    // root, which is the same `Unknown` discipline `sharedlib` uses off Linux.
    if let Some(rel) = &check.path {
        if let Some(base) = base {
            if base.join(rel).exists() {
                return ProbeResult::Present;
            }
            answered_missing = true;
        }
    }
    if answered_missing {
        ProbeResult::Missing
    } else {
        ProbeResult::Unknown
    }
}

/// `nros setup --system [--check|--sudo]`.
///
/// Print mode composes ONE native install command for the packages whose
/// probe does not report present, plus a per-key `why` listing so the user
/// can prune — and exits 0 (a missing OS package must never abort the
/// sudo-less remainder of a setup flow; issue 0368 F1). `--check` reports
/// probe results and exits 1 on any missing (the doctor surface). `--sudo`
/// executes the composed command.
fn run_system(
    index: &SdkIndex,
    check_only: bool,
    run_sudo: bool,
    repo_root: Option<&std::path::Path>,
) -> Result<()> {
    // W4's retirement notice, WIRED — the step phase-383 W1.f skipped, which is
    // why that deprecation reached nobody.
    for w in index
        .deprecated_system_table_warnings(crate::orchestration::image::deprecation_suppressed())
    {
        eprintln!("nros setup: {w}");
    }
    for k in index.duplicate_prereq_keys() {
        eprintln!(
            "nros setup: `{k}` is declared in BOTH [prereq.{k}] and [system.{k}]. \
             The [prereq.*] entry wins; delete the [system.*] one."
        );
    }

    let prereqs = index.prereqs();
    if prereqs.is_empty() {
        println!("nros setup --system: index declares no [prereq.*] / [system.*] entries.");
        return Ok(());
    }
    let manager = detect_package_manager();

    let mut missing: Vec<(&String, &crate::orchestration::sdk_index::PrereqDep)> = Vec::new();
    let mut unknown: Vec<&String> = Vec::new();
    let mut present = 0usize;
    for (key, dep) in &prereqs {
        let base = repo_root.and_then(|r| index.prereq_checkout_dir(key, dep, r));
        match run_probe_in(dep.check.as_ref(), base.as_deref()) {
            ProbeResult::Present => present += 1,
            ProbeResult::Missing => missing.push((key, dep)),
            ProbeResult::Unknown => unknown.push(key),
        }
    }

    if check_only {
        println!(
            "nros setup --system --check: {present} present, {} missing, {} unprobed",
            missing.len(),
            unknown.len()
        );
        for (key, dep) in &missing {
            println!(
                "  [MISSING] {key}{}",
                dep.why
                    .as_deref()
                    .map(|w| format!(" — {w}"))
                    .unwrap_or_default()
            );
        }
        for key in &unknown {
            println!("  [UNPROBED] {key} (no check declared / not answerable here)");
        }
        if !missing.is_empty() {
            let Some(mgr) = manager else {
                bail!(
                    "{} system package(s) missing and no package manager detected",
                    missing.len()
                );
            };
            let pkgs = compose_packages(&missing, mgr);
            bail!(
                "{} system package(s) missing. Install with:\n  {}",
                missing.len(),
                native_install_command(mgr, &pkgs)
            );
        }
        return Ok(());
    }

    let Some(mgr) = manager else {
        println!(
            "nros setup --system: no supported package manager detected \
             (apt/dnf/pacman/brew). The index declares {} [system.*] entries; \
             map your platform in nros-sdk-index.toml.",
            prereqs.len()
        );
        return Ok(());
    };

    // Compose over the entries not already present (probe-first, so re-runs
    // shrink the command instead of repeating it).
    let prereqs = index.prereqs();
    let to_install: Vec<(&String, &crate::orchestration::sdk_index::PrereqDep)> = prereqs
        .iter()
        .filter(|(_, dep)| run_probe(dep.check.as_ref()) != ProbeResult::Present)
        .collect();
    if to_install.is_empty() {
        println!("nros setup --system: every probed [system.*] entry is present.");
        return Ok(());
    }
    // phase-398 W5 — the rosdep backend that used to fill an unmapped manager
    // here is DELETED (RFC-0062, amended 2026-08-29). It answered for one
    // provider of four, could not carry a `check`, and being consulted only
    // where it happened to be installed made one tree resolve two ways. A key
    // this index does not map for this host is now simply unmapped, and says so.
    let mut unmapped: Vec<&String> = Vec::new();
    for (key, dep) in &to_install {
        if dep.packages_for(mgr).is_empty() {
            unmapped.push(key);
        }
    }
    let mut pkgs = compose_packages(&to_install, mgr);
    pkgs.sort();
    pkgs.dedup();

    println!(
        "nros setup --system ({mgr}): {} entr(ies) not confirmed present:",
        to_install.len()
    );
    for (key, dep) in &to_install {
        println!(
            "  {key:<28} {}",
            dep.why.as_deref().unwrap_or("(no why recorded)")
        );
    }
    if !unmapped.is_empty() {
        println!(
            "  ({} entr(ies) have no {mgr} mapping and are omitted: {} — map them \
             in nros-sdk-index.toml)",
            unmapped.len(),
            unmapped
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
    if pkgs.is_empty() {
        println!("nros setup --system: nothing to compose for {mgr}.");
        return Ok(());
    }
    let cmd = native_install_command(mgr, &pkgs);
    if run_sudo {
        println!("nros setup --system --sudo: running:\n  {cmd}");
        let status = std::process::Command::new("sh")
            .args(["-c", &cmd])
            .status()
            .wrap_err("spawn the native package manager")?;
        if !status.success() {
            bail!("system package install failed ({status})");
        }
    } else {
        println!("Install with (or re-run with --sudo):\n  {cmd}");
    }
    Ok(())
}

/// Is `[tool.<name>]` installed at the version the index pins? — issue 0466.
///
/// TWO store layouts are live and they carry DIFFERENT version vocabularies,
/// both declared in the index, so this reads them rather than normalising one
/// into the other:
///
///   * `nros setup --tool` installs a VERSIONED prefix,
///     `<store>/<tool>/<version>/`, keyed on the index's repack id
///     (`0.6.1-nros1`).
///   * the `just workspace install-*` recipes install an UNVERSIONED prefix and
///     stamp `<store>/<tool>/.installed-version` with the UPSTREAM tag
///     (`v0.6.1`) — which the index also declares, as `upstream`.
///
/// Returns `(present, what the store actually holds)`. The second half is the
/// diagnosis for "but I ran the installer": #0493's corrosion sat at 0.5.1
/// against a 0.6.1 pin and surfaced hours later as duplicate `#[no_mangle]`
/// symbols at link time.
///
/// Scope, stated rather than implied: this asks about the SHARED STORE. A tool
/// installed with `--prefix` (the workspace-local `build/qemu`)
/// is deliberately outside it and is not answered here — `--prefix` exists so a
/// checkout can pin its own copy, so "absent from the store" is the correct
/// answer for those, not a false negative.
fn tool_pin_status(index: &SdkIndex, name: &str, tool: &ToolPackage) -> (bool, Vec<String>) {
    let store = crate::orchestration::sdk_store::store_root();
    let _ = index;
    let versioned = crate::orchestration::sdk_store::tool_prefix(&store, name, &tool.version);
    let stamp = store.join(name).join(".installed-version");
    let stamped = std::fs::read_to_string(&stamp)
        .ok()
        .map(|v| v.trim().to_string());

    if versioned.is_dir() {
        return (true, Vec::new());
    }
    if let (Some(found), Some(want)) = (stamped.as_deref(), tool.upstream.as_deref())
        && found == want
    {
        return (true, Vec::new());
    }

    let mut held: Vec<String> = std::fs::read_dir(store.join(name))
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().is_dir())
        .filter_map(|e| e.file_name().into_string().ok())
        .collect();
    if let Some(found) = stamped {
        held.push(format!(".installed-version={found}"));
    }
    held.sort();
    (false, held)
}

/// The first `[tool.*].smoke` entry that does not work, with what it printed.
///
/// issue 0929. `system = [..]` proves the libraries resolve; this proves the
/// binary does something. `arm-none-eabi-gdb` is why both are needed: it links
/// nothing missing, exits 0, and prints NOTHING, because ARM's embedded Python
/// aborts during init. An exit-status probe calls that healthy.
///
/// Absent `smoke` means no opinion, not a pass — most dists have none yet, and
/// silence must not read as coverage.
fn failing_smoke(
    index: &SdkIndex,
    name: &str,
    tool: &crate::orchestration::sdk_index::ToolPackage,
) -> Option<(String, String)> {
    let _ = index;
    let store = crate::orchestration::sdk_store::store_root();
    let prefix = crate::orchestration::sdk_store::tool_prefix(&store, name, &tool.version);
    for probe in &tool.smoke {
        let mut argv = probe.run.split_whitespace();
        let Some(exe) = argv.next() else { continue };
        let out = std::process::Command::new(prefix.join(exe))
            .args(argv)
            // A smoke check must measure the DIST, not the caller's shell. On a
            // host with ROS sourced, `LD_LIBRARY_PATH` shadows a bundled
            // library and `PYTHONPATH` redirects an embedded interpreter — both
            // measured while diagnosing 0929, where the ROS path in gdb's error
            // text sent the first diagnosis down a blind alley (issue 0774's
            // class).
            .env_remove("LD_LIBRARY_PATH")
            .env_remove("PYTHONPATH")
            .env_remove("PYTHONHOME")
            .output();
        let (text, note) = match out {
            Ok(o) => (
                format!(
                    "{}{}",
                    String::from_utf8_lossy(&o.stdout),
                    String::from_utf8_lossy(&o.stderr)
                ),
                None,
            ),
            Err(e) => (String::new(), Some(format!("could not run it: {e}"))),
        };
        if let Some(n) = note {
            return Some((probe.run.clone(), n));
        }
        if !text.contains(&probe.expect) {
            let shown = if text.trim().is_empty() {
                format!(
                    "printed NOTHING; expected output containing `{}`",
                    probe.expect
                )
            } else {
                format!("expected `{}`, got:\n{}", probe.expect, text.trim())
            };
            return Some((probe.run.clone(), shown));
        }
    }
    None
}

/// Issue 0466 finding (b) — is ONE tool at the version the index pins?
///
/// The generic `--check` walks every class and answers "is anything missing".
/// This answers the question a build actually raises: the pin moved, or the
/// store holds a stale prefix, and the consequence appears somewhere else
/// (#0493: corrosion 0.5.1 against a 0.6.1 pin, surfacing as duplicate
/// `#[no_mangle]` symbols at link time). Exit 1 when the pinned prefix is
/// absent, and NAME what the store holds instead — that list is the whole
/// diagnosis for "but I ran the installer".
fn run_check_tool(index: &SdkIndex, name: &str) -> Result<()> {
    let Some(tool) = index.tool.get(name) else {
        bail!("nros setup: no [tool.{name}] in the index (see `nros setup --list`)");
    };
    let (present, held) = tool_pin_status(index, name, tool);
    if present {
        // issue 0926 — PINNED is not USABLE. The `system = [..]` probe below
        // lived only in the install path, behind the already-present
        // short-circuit, so a dist that is provisioned and cannot start
        // reported `[OK]` here. That is the gap issue 0368 F3 was filed for,
        // one path over: "nothing consulted it on the path where the tool is
        // used". Measured on a 22.04 host — `openocd` present at the pinned
        // version and dead on `libftdi.so.1`, `arm-none-eabi-gdb` dead on
        // `libncursesw.so.5`.
        let prereqs = index.prereqs();
        let missing: Vec<&str> = tool
            .system
            .iter()
            .filter(|k| {
                prereqs
                    .get(k.as_str())
                    .is_some_and(|d| run_probe(d.check.as_ref()) == ProbeResult::Missing)
            })
            .map(|k| k.as_str())
            .collect();
        if missing.is_empty() {
            // issue 0929 — every library resolves; now ask whether it RUNS.
            // Not the same question, and the difference is what let a toolchain
            // with a dead debugger report `[OK]`.
            if let Some((cmd, why)) = failing_smoke(index, name, tool) {
                println!(
                    "  [BROKEN]  tool    {name} {} — installed, every declared \
                     library present, and `{cmd}` does not work:",
                    tool.version
                );
                for line in why.lines().take(4) {
                    println!("            {line}");
                }
                bail!("nros setup --tool {name} --check: present but not working");
            }
            println!("  [OK]      tool    {name} {}", tool.version);
            return Ok(());
        }
        let entries: Vec<(&String, &crate::orchestration::sdk_index::PrereqDep)> = prereqs
            .iter()
            .filter(|(k, _)| missing.contains(&k.as_str()))
            .collect();
        let hint = detect_package_manager()
            .map(|mgr| native_install_command(mgr, &compose_packages(&entries, mgr)))
            .unwrap_or_else(|| "<no supported package manager detected>".to_string());
        println!(
            "  [BROKEN]  tool    {name} {} — installed, but {} system \
             package(s) it needs are missing: {}",
            tool.version,
            missing.len(),
            missing.join(", ")
        );
        println!("            Install with:  {hint}");
        bail!("nros setup --tool {name} --check: present but unusable");
    }
    println!(
        "  [MISSING] tool    {name} {} (run: nros setup --tool {name})",
        tool.version
    );
    if !held.is_empty() {
        println!("            the store holds: {}", held.join(", "));
        println!(
            "            Installed is not the same as PINNED — that gap is how a landed\n\
             \x20           fix stays inert (issue 0493), and the store ACCUMULATES, so a\n\
             \x20           stale prefix can shadow the pin (issue 0500)."
        );
    }
    bail!("nros setup --tool {name} --check: not at the pinned version")
}

/// phase-327 W3 — the generic walker: probe every declared class and print a
/// remedy COMPUTED from the entry. Exit 1 when anything is missing.
fn run_check_all(index: &SdkIndex) -> Result<()> {
    let mut missing = 0usize;
    // Separate from `missing` only because the `report` closure below captures
    // that one mutably; both are summed at the end.
    let mut broken = 0usize;
    let mut report = |class: &str, name: &str, ok: ProbeResult, remedy: String| match ok {
        ProbeResult::Present => println!("  [OK]      {class:<7} {name}"),
        ProbeResult::Missing => {
            println!("  [MISSING] {class:<7} {name} (run: {remedy})");
            missing += 1;
        }
        ProbeResult::Unknown => println!("  [UNPROBED] {class:<6} {name}"),
    };

    // Prerequisites — the composed native command is the remedy. Through
    // `prereqs()` so the doctor surface covers `[prereq.*]` and `[system.*]`
    // alike; a doctor that walked one table would report a shrinking half of
    // the SSoT as the migration runs.
    let manager = detect_package_manager();
    let prereqs = index.prereqs();
    for (key, dep) in &prereqs {
        let remedy = match manager {
            Some(mgr) if !dep.packages_for(mgr).is_empty() => {
                native_install_command(mgr, dep.packages_for(mgr))
            }
            _ => format!("map [prereq.{key}] for this host in nros-sdk-index.toml"),
        };
        report("system", key, run_probe(dep.check.as_ref()), remedy);
    }

    // [tool.*] — issue 0466 finding (b): "a landed fix is not an applied fix".
    // This class was the ONE `run_check_all` never walked, so a provisioned tool
    // that had drifted behind its pin was invisible here and surfaced later as
    // something else entirely — #0493's corrosion 0.5.1-vs-0.6.1 presented as a
    // duplicate-symbol LINK failure, hours from its cause.
    //
    // Presence is asked of the PINNED version's prefix specifically, not of the
    // tool in general, because the store ACCUMULATES: `~/.nros/sdk/<tool>/` can
    // hold several versions at once and `find_package` takes the first prefix
    // that resolves (issue 0500). So an installed-but-wrong version is reported
    // as MISSING with the pin named, and any OTHER versions present are listed —
    // that list is the thing which explains a "but I installed it" build
    // failure.
    for (name, tool) in &index.tool {
        let (present, held) = tool_pin_status(index, name, tool);
        let label = if held.is_empty() {
            format!("{name} {}", tool.version)
        } else {
            format!("{name} {} (store holds: {})", tool.version, held.join(", "))
        };
        // issue 0929 — the same two questions this class already learned to ask
        // one verb over: is the pin THERE, and does it WORK. Reporting a tool
        // whose binaries are dead as `[OK]` is what made a broken debugger
        // invisible to `nros setup --check` as well as to `--tool X --check`;
        // a fix on one path only would have left the other lying.
        let smoke_failure = present.then(|| failing_smoke(index, name, tool)).flatten();
        if let Some((cmd, why)) = smoke_failure {
            println!("  [BROKEN]  tool    {label} — `{cmd}` does not work");
            for line in why.lines().take(2) {
                println!("            {line}");
            }
            broken += 1;
            continue;
        }
        let ok = if present {
            ProbeResult::Present
        } else {
            ProbeResult::Missing
        };
        report("tool", &label, ok, format!("nros setup --tool {name}"));
    }

    // [rust.toolchain.*] — `rustup toolchain list` + per-component listing.
    let toolchain_list = command_stdout("rustup", &["toolchain", "list"]);
    for (alias, tc) in &index.rust.toolchain {
        let installed = toolchain_list.lines().any(|l| {
            l.split_whitespace().next() == Some(tc.channel.as_str())
                || l.starts_with(&format!("{}-", tc.channel))
        });
        let present = if installed {
            let comps = command_stdout(
                "rustup",
                &[
                    "component",
                    "list",
                    "--toolchain",
                    &tc.channel,
                    "--installed",
                ],
            );
            tc.components.iter().all(|c| {
                comps
                    .lines()
                    .any(|l| l == c.as_str() || l.starts_with(&format!("{c}-")))
            })
        } else {
            false
        };
        report(
            "rust",
            &format!("toolchain {alias} ({})", tc.channel),
            if present {
                ProbeResult::Present
            } else {
                ProbeResult::Missing
            },
            format!(
                "rustup toolchain install --profile minimal {}{}",
                tc.channel,
                tc.components
                    .iter()
                    .map(|c| format!(" && rustup component add --toolchain {} {c}", tc.channel))
                    .collect::<String>()
            ),
        );
    }

    // [rust.target.*] — installed-target listing per toolchain.
    for (alias, target) in &index.rust.target {
        let channel = target
            .toolchain
            .as_ref()
            .and_then(|a| index.rust.toolchain.get(a))
            .map(|tc| tc.channel.as_str());
        let mut rustup_args = vec!["target", "list", "--installed"];
        if let Some(ch) = channel {
            rustup_args.extend(["--toolchain", ch]);
        }
        let listing = command_stdout("rustup", &rustup_args);
        let present = listing.lines().any(|l| l.trim() == target.triple);
        report(
            "rust",
            &format!("target {alias} ({})", target.triple),
            if present {
                ProbeResult::Present
            } else {
                ProbeResult::Missing
            },
            format!(
                "rustup {}target add {}",
                channel.map(|c| format!("+{c} ")).unwrap_or_default(),
                target.triple
            ),
        );
    }

    // [rust.cargo-tool.*] — the declared probe, else `cargo install --list`.
    let cargo_installed = command_stdout("cargo", &["install", "--list"]);
    for (alias, tool) in &index.rust.cargo_tool {
        let probed = match &tool.check {
            Some(c) => run_probe(Some(c)),
            None => {
                if cargo_installed
                    .lines()
                    .any(|l| l.starts_with(&format!("{} ", tool.crate_name)))
                {
                    ProbeResult::Present
                } else {
                    ProbeResult::Missing
                }
            }
        };
        report(
            "rust",
            &format!("cargo-tool {alias} ({})", tool.crate_name),
            probed,
            format!(
                "cargo install {}{}{}",
                tool.crate_name,
                tool.version
                    .as_deref()
                    .map(|v| format!(" --version {v}"))
                    .unwrap_or_default(),
                if tool.locked { " --locked" } else { "" }
            ),
        );
    }

    // [python.*].
    for (alias, py) in &index.python {
        report(
            "python",
            &format!("{alias} ({})", py.pip),
            run_probe(py.check.as_ref()),
            format!(
                "pip3 install --user {}{}",
                py.pip,
                py.version
                    .as_deref()
                    .map(|v| format!("=={v}"))
                    .unwrap_or_default()
            ),
        );
    }

    if missing + broken > 0 {
        if broken > 0 && missing == 0 {
            bail!(
                "nros setup --check: {broken} tool(s) installed but NOT WORKING \
                 (above). Nothing is missing — reinstalling will not help."
            );
        }
        bail!(
            "nros setup --check: {missing} declared dependenc(ies) missing, \
             {broken} installed but not working (remedies above)"
        );
    }
    println!("nros setup --check: every probed declared dependency is present.");
    Ok(())
}

/// Captured stdout of a probe command; empty string when it cannot run (the
/// caller's listing check then reports missing, which carries the right
/// remedy anyway).
fn command_stdout(cmd: &str, args: &[&str]) -> String {
    std::process::Command::new(cmd)
        .args(args)
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
        .unwrap_or_default()
}

fn compose_packages(
    entries: &[(&String, &crate::orchestration::sdk_index::PrereqDep)],
    manager: &str,
) -> Vec<String> {
    let mut pkgs: Vec<String> = entries
        .iter()
        .flat_map(|(_, dep)| dep.packages_for(manager).iter().cloned())
        .collect();
    pkgs.sort();
    pkgs.dedup();
    pkgs
}

#[cfg(test)]
mod tests {
    use super::*;

    /// issue 0603 — a `header` probe answers about the `-dev` package, where
    /// `sharedlib` answers about the runtime one.
    ///
    /// The regression this pins: `sharedlib` PREFIX-matches, so
    /// `sharedlib = "libmbedtls.so"` accepted the `libmbedtls.so.14` that the
    /// RUNTIME package ships, and the gate reported a header-less host as ready.
    /// A header the compiler can actually reach is the honest test.
    #[test]
    fn header_probe_answers_missing_for_an_absent_header() {
        use crate::orchestration::sdk_index::CheckProbe;
        if std::env::consts::OS != "linux" {
            return; // probe is Linux-only by construction; `Unknown` elsewhere.
        }
        let absent = CheckProbe {
            header: Some("nros-nonexistent-probe/should-not-exist.h".into()),
            ..Default::default()
        };
        assert_eq!(run_probe(Some(&absent)), ProbeResult::Missing);

        // A header every Linux toolchain has, to prove the probe is not simply
        // always-missing — which would trade a false positive for a false
        // negative and hard-block `nros setup` on an already-provisioned host.
        let present = CheckProbe {
            header: Some("stdio.h".into()),
            ..Default::default()
        };
        assert_eq!(run_probe(Some(&present)), ProbeResult::Present);
    }

    fn board_index() -> SdkIndex {
        SdkIndex::parse(
            "[board.qemu-arm-freertos]\npackages=[\"arm-none-eabi-gcc\",\"qemu\",\"freertos-kernel\",\"lwip\"]\n\
             [board.qemu-riscv64-threadx]\npackages=[\"riscv-none-elf-gcc\",\"qemu\",\"threadx\"]\n\
             [board.qemu-esp32-baremetal]\narch=\"riscv32\"\npackages=[]\n\
             [board.native]\npackages=[\"zenohd\"]\n\
             [board.gated-example]\npackages=[\"arm-none-eabi-gcc\",\"a-gated-sdk\"]\n",
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

        // ESP32-C3 QEMU: declared arch riscv32, no index host-tool (rustup target).
        assert!(
            resolve_packages(&idx, "qemu-esp32-baremetal")
                .unwrap()
                .is_empty()
        );
        assert_eq!(resolve_packages(&idx, "native").unwrap(), vec!["zenohd"]);
        // A board whose set mixes a hosted tool with a license-gated one.
        let gated = resolve_packages(&idx, "gated-example").unwrap();
        assert!(gated.contains(&"arm-none-eabi-gcc") && gated.contains(&"a-gated-sdk"));

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
        let ws = crate::test_support::scratch_dir("idx");
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
        let ws = crate::test_support::scratch_dir("noidx");
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

    /// issue 0374 — the up-front "BUILDING FROM SOURCE" heads-up must name
    /// exactly the tools this host compiles: a tool with a dist for the host is
    /// prebuilt, one without falls back to source, and `[source.*]` submodule
    /// packages are a checkout rather than a build. Getting this wrong in
    /// either direction is bad: a missed name is the unannounced multi-minute
    /// stall the issue was filed for, a spurious one trains users to ignore the
    /// warning.
    #[test]
    fn source_build_names_lists_only_host_source_builds() {
        // qemu: dist for linux + a source fallback — prebuilt on linux, built
        // on any other host. zenohd: source only, built everywhere (the real
        // [tool.zenohd] shape that prompted this issue).
        let idx = SdkIndex::parse(
            "[tool.qemu]\nversion=\"11.0\"\ndist.linux-x86_64={url=\"u\",sha256=\"h\"}\n\
             [tool.qemu.source]\ngit=\"g\"\nref=\"r\"\n\
             [tool.zenohd]\nversion=\"1.7.2\"\n[tool.zenohd.source]\ngit=\"g\"\nref=\"r\"\n\
             [source.mbedtls]\nversion=\"3.x\"\n",
        )
        .unwrap();
        // An empty store, so nothing resolves to `Present`.
        let root = crate::test_support::scratch_dir("source-build-names");

        let pkgs = ["qemu", "zenohd", "mbedtls"];
        assert_eq!(
            source_build_names(&idx, &pkgs, &root, "linux-x86_64"),
            vec!["zenohd"],
            "only the dist-less [tool.*] entry is a source build on this host"
        );

        // Same index, a host the prebuilt does not cover: qemu joins the list.
        assert_eq!(
            source_build_names(&idx, &pkgs, &root, "macos-arm64"),
            vec!["qemu", "zenohd"]
        );

        // Nothing to build => nothing to warn about (warn_source_builds is a
        // no-op on an empty slice).
        assert!(source_build_names(&idx, &["mbedtls"], &root, "linux-x86_64").is_empty());
    }
}

#[cfg(test)]
mod probe_kind_tests {
    use super::*;
    use crate::orchestration::sdk_index::CheckProbe;

    fn probe(c: CheckProbe, base: Option<&std::path::Path>) -> ProbeResult {
        run_probe_in(Some(&c), base)
    }

    /// `runs` distinguishes three states, and the third is the point: a tool
    /// that cannot be executed here (foreign platform, no emulator) is NOT
    /// absent, and must not vote (issue 0487's rule).
    #[test]
    fn runs_separates_broken_from_unrunnable() {
        assert_eq!(
            probe(
                CheckProbe {
                    runs: Some("true".into()),
                    ..Default::default()
                },
                None
            ),
            ProbeResult::Present,
        );
        // Present on PATH, exits non-zero: a broken dist, a loader failure —
        // the libslirp shape. That IS absent for our purposes.
        assert_eq!(
            probe(
                CheckProbe {
                    runs: Some("false".into()),
                    ..Default::default()
                },
                None
            ),
            ProbeResult::Missing,
        );
        // Not on PATH at all: unanswerable, not broken.
        assert_eq!(
            probe(
                CheckProbe {
                    runs: Some("nros-no-such-binary --x".into()),
                    ..Default::default()
                },
                None
            ),
            ProbeResult::Unknown,
        );
    }

    /// `path` answers only with a base. Without one it abstains rather than
    /// testing against a guessed root — the same discipline `sharedlib` uses
    /// off Linux.
    #[test]
    fn path_needs_a_base_and_abstains_without_one() {
        let d = std::env::temp_dir().join(format!("nros-w2-{}-{}", std::process::id(), line!()));
        std::fs::create_dir_all(&d).expect("mkdir");
        std::fs::write(d.join("present.h"), "").expect("write");

        assert_eq!(
            probe(
                CheckProbe {
                    path: Some("present.h".into()),
                    ..Default::default()
                },
                Some(&d)
            ),
            ProbeResult::Present,
        );
        // The uninitialised-submodule case: the directory exists, the file does
        // not. That is exactly what "the path exists" could never catch.
        assert_eq!(
            probe(
                CheckProbe {
                    path: Some("absent.h".into()),
                    ..Default::default()
                },
                Some(&d)
            ),
            ProbeResult::Missing,
        );
        assert_eq!(
            probe(
                CheckProbe {
                    path: Some("present.h".into()),
                    ..Default::default()
                },
                None
            ),
            ProbeResult::Unknown,
            "no base ⇒ abstain, never guess a root",
        );
        std::fs::remove_dir_all(&d).ok();
    }

    /// Probes stay OR-ed across the new kinds too: one answering PRESENT wins,
    /// which is issue 0487's libgcrypt rule applied to a stronger probe.
    #[test]
    fn the_new_kinds_compose_with_the_old_or() {
        assert_eq!(
            probe(
                CheckProbe {
                    cmd: Some("nros-no-such-binary".into()),
                    runs: Some("true".into()),
                    ..Default::default()
                },
                None
            ),
            ProbeResult::Present,
            "a failing cmd must not veto a passing runs",
        );
    }
}
