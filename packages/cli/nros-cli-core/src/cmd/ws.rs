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

    /// Codegen workspace msg pkgs + write `[patch.crates-io]` block into
    /// each Rust consumer's patch authority Cargo.toml. Pre-cargo step;
    /// run once after editing `*.msg` files, then `cargo build` works.
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

    /// ROS 2 edition (`humble` | `iron`).
    #[arg(long, default_value = "humble")]
    pub ros_edition: String,

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
        Sub::Sync(a) => run_sync(a),
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

fn run_sync(args: SyncArgs) -> Result<()> {
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

    let edition = parse_edition(&args.ros_edition)?;

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
    let rust_consumers: Vec<&WsPkg> = scan
        .iter()
        .filter(|p| p.is_rust_pkg && !p.is_msg_pkg && p.is_patch_consumer)
        .collect();
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

    if rust_consumers.is_empty() {
        println!("ws sync: no Rust consumer pkgs — patch tables not written.");
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
        authority_to_pkgs
            .entry(authority)
            .or_default()
            .extend(all_emitted.iter().cloned());
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
        )?;
    }

    println!("ws sync: done.");
    Ok(())
}

fn parse_edition(s: &str) -> Result<RosEdition> {
    match s.to_lowercase().as_str() {
        "humble" => Ok(RosEdition::Humble),
        "iron" => Ok(RosEdition::Iron),
        other => bail!("ws sync: unknown ROS edition '{other}' (humble | iron)"),
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
    rosidl_bindgen::generator::generate_package(&package, &out_dir, edition, &resolver)
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
    // Pre-load ament index once per invocation.
    static AMENT_INDEX: std::sync::OnceLock<Option<rosidl_bindgen::ament::AmentIndex>> =
        std::sync::OnceLock::new();
    let idx = AMENT_INDEX.get_or_init(|| rosidl_bindgen::ament::AmentIndex::from_env().ok());
    let Some(idx) = idx else { return Ok(()) };

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
        rosidl_bindgen::generator::generate_package(&amented, &out_dir, edition, &resolver)
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

fn write_patch_block(
    authority: &Path,
    build_root: &Path,
    pkgs: &[String],
    nano_ros_path: Option<&Path>,
    extra_runtime_crates: &[String],
) -> Result<()> {
    let body = std::fs::read_to_string(authority)
        .wrap_err_with(|| format!("ws sync: read {}", authority.display()))?;
    let rendered = render_patch_block(
        authority,
        build_root,
        pkgs,
        nano_ros_path,
        extra_runtime_crates,
    );
    let new_body = splice_patch_block(&body, &rendered);
    // Atomic write: render to a sibling temp file, then rename over `authority`.
    // `std::fs::write` truncates-then-writes, so a concurrent reader (the native
    // fixture build runs `ws sync` on the SAME example manifest for the zenoh / xrce
    // / cyclonedds variants in parallel) can observe a half-written, truncated file
    // and splice ITS patch block onto the fragment — corrupting the manifest down to
    // a 9-line stub with no `[package]`. A rename is atomic on the same filesystem,
    // so a reader always sees a complete file (old or new). The patch block is
    // idempotent across RMW variants, so last-writer-wins is harmless.
    let fname = authority
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("Cargo.toml");
    let tmp = authority.with_file_name(format!(".{fname}.nros-sync-tmp.{}", std::process::id()));
    std::fs::write(&tmp, new_body).wrap_err_with(|| format!("ws sync: write {}", tmp.display()))?;
    std::fs::rename(&tmp, authority).wrap_err_with(|| {
        format!(
            "ws sync: rename {} -> {}",
            tmp.display(),
            authority.display()
        )
    })?;
    println!(
        "ws sync: refreshed [patch.crates-io] block in {}",
        authority.display()
    );
    Ok(())
}

/// Output of `render_patch_block` — the managed entry text plus the set of
/// crate names the block claims authority over. The names are used by
/// `splice_patch_block` to evict any duplicates from a pre-existing
/// `[patch.crates-io]` table while preserving every user-authored row.
struct RenderedBlock {
    /// Crate names this sync run is responsible for (managed set).
    managed_names: Vec<String>,
    /// Body text of the BEGIN/END region (does NOT include a
    /// `[patch.crates-io]` header — `splice_patch_block` guarantees exactly
    /// one header lives directly above it).
    block: String,
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
        // RMW backends
        ("nros-rmw-zenoh", "packages/zpico/nros-rmw-zenoh"),
        (
            "nros-rmw-zenoh-staticlib",
            "packages/zpico/nros-rmw-zenoh-staticlib",
        ),
        ("nros-rmw-cyclonedds", "packages/dds/nros-rmw-cyclonedds"),
        (
            "nros-rmw-cyclonedds-sys",
            "packages/dds/nros-rmw-cyclonedds-sys",
        ),
        ("nros-rmw-xrce-cffi", "packages/xrce/nros-rmw-xrce-cffi"),
        (
            "nros-rmw-xrce-cffi-staticlib",
            "packages/xrce/nros-rmw-xrce-cffi-staticlib",
        ),
        // Transport / SDKs that consumers regularly reference as `version = "*"`
        ("cyclonedds-sys", "packages/dds/cyclonedds-sys"),
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
    let mut out: Vec<String> = Vec::new();
    let mut in_deps = false;
    // Issue #94 case B — the explicit dotted dependency-table form
    // `[dependencies.<name>]` (and `[target.<cfg>.dependencies.<name>]`)
    // declares one dep whose `version` / `path` keys live on FOLLOWING
    // lines. Track the pending managed crate name + whether we've seen a
    // `version` key; flush it (registry-style → emit) on the next section
    // header or EOF.
    let mut explicit: Option<(String, bool)> = None;
    fn flush(explicit: &mut Option<(String, bool)>, out: &mut Vec<String>) {
        if let Some((name, true)) = explicit.take() {
            out.push(name);
        }
    }
    for line in body.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            flush(&mut explicit, &mut out);
            in_deps = is_dependencies_table(trimmed);
            explicit = explicit_dep_table_crate(trimmed)
                .filter(|n| is_managed_runtime_crate_name(n))
                .map(|n| (n, false));
            continue;
        }
        // Inside an explicit `[dependencies.<name>]` table: only the
        // `version` key matters (registry vs path-only). Other keys ignored.
        if let Some((_, has_version)) = explicit.as_mut() {
            let key = trimmed.split('=').next().map(str::trim).unwrap_or("");
            if strip_toml_key_quotes(key) == "version" {
                *has_version = true;
            }
            continue;
        }
        if !in_deps {
            continue;
        }
        let Some(eq) = trimmed.find('=') else {
            continue;
        };
        // Skip lines like `[foo` (table heads) or commented entries.
        if trimmed.starts_with('#') {
            continue;
        }
        let lhs = trimmed[..eq].trim();
        // Strip optional TOML key quotes.
        let key = strip_toml_key_quotes(lhs);
        if !is_managed_runtime_crate_name(key) {
            continue;
        }
        let rhs = trimmed[eq + 1..].trim_start();
        // Two registry-style shapes accepted:
        //   1. bare version string: `name = "*"` (or any "x.y.z")
        //   2. inline table: `name = { version = "*", ... }`
        //
        // Path-style (`name = { path = "..." }` with NO `version` key)
        // is skipped — user already pinned a path-dep, no patch needed.
        //
        // For (2) we require an explicit `version` key. A table that
        // ONLY carries `path = ...` (the canonical 212-shape path-dep)
        // is excluded. A table that carries BOTH `version` + `path`
        // (the cargo-workspace shape used in some fixtures) is treated
        // as registry-style because the version makes cargo register
        // the dep in the resolver's registry namespace, which is the
        // axis `[patch.crates-io]` operates on.
        let is_registry = if rhs.starts_with('"') {
            true
        } else if rhs.starts_with('{') {
            rhs.contains("version")
        } else {
            false
        };
        if !is_registry {
            continue;
        }
        out.push(key.to_string());
    }
    // Flush a trailing explicit `[dependencies.<name>]` table at EOF.
    flush(&mut explicit, &mut out);
    out.sort();
    out.dedup();
    out
}

/// Issue #94 case B — if `header` is an explicit single-dependency table
/// head (`[dependencies.<name>]`, `[dev-dependencies.<name>]`,
/// `[build-dependencies.<name>]`, or the `[target.<cfg>.<kind>.<name>]`
/// variants), return `<name>` (TOML quotes stripped). Returns `None` for a
/// flat `[dependencies]` head — that shape is handled by the inline scanner.
///
/// Crate names cannot contain `.`, so the name is the final dotted segment;
/// a quoted `cfg(...)` segment in the target form is skipped by anchoring on
/// the `.<kind>.` separator.
fn explicit_dep_table_crate(header: &str) -> Option<String> {
    let inner = header.strip_prefix('[')?.strip_suffix(']')?.trim();
    for kind in ["dependencies", "dev-dependencies", "build-dependencies"] {
        // Leading form: `dependencies.<name>`.
        if let Some(rest) = inner.strip_prefix(&format!("{kind}.")) {
            let name = strip_toml_key_quotes(rest.trim());
            return (!name.is_empty()).then(|| name.to_string());
        }
        // Target form: `target.<cfg>.<kind>.<name>`.
        if inner.starts_with("target.") {
            let needle = format!(".{kind}.");
            if let Some(pos) = inner.rfind(&needle) {
                let name = strip_toml_key_quotes(inner[pos + needle.len()..].trim());
                return (!name.is_empty()).then(|| name.to_string());
            }
        }
    }
    None
}

/// Match a line like `[dependencies]`, `[dev-dependencies]`,
/// `[build-dependencies]`, or any `[target.<cfg>.dependencies]` /
/// `[target.<cfg>.dev-dependencies]` /
/// `[target.<cfg>.build-dependencies]` head.
fn is_dependencies_table(trimmed: &str) -> bool {
    let inner = trimmed
        .strip_prefix('[')
        .and_then(|s| s.strip_suffix(']'))
        .unwrap_or(trimmed);
    let inner = inner.trim();
    matches!(
        inner,
        "dependencies" | "dev-dependencies" | "build-dependencies"
    ) || (inner.starts_with("target.")
        && (inner.ends_with(".dependencies")
            || inner.ends_with(".dev-dependencies")
            || inner.ends_with(".build-dependencies")))
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

fn render_patch_block(
    authority: &Path,
    build_root: &Path,
    pkgs: &[String],
    nano_ros_path: Option<&Path>,
    extra_runtime_crates: &[String],
) -> RenderedBlock {
    let authority_dir = authority.parent().unwrap();
    let mut managed_names: Vec<String> = Vec::new();
    let mut entries = String::new();

    // 1) Generated msg crates (path = generated/<pkg>).
    for pkg in pkgs {
        let crate_root = build_root.join(pkg);
        let rel = pathdiff::diff_paths(&crate_root, authority_dir).unwrap_or(crate_root);
        entries.push_str(&format!("{pkg} = {{ path = \"{}\" }}\n", rel.display()));
        managed_names.push(pkg.clone());
    }

    // 2) Minimum runtime patches the GENERATED msg crates depend on.
    //    Generated `<pkg>/Cargo.toml` carries `nros-core = "*"` +
    //    `nros-serdes = "*"` (registry-style), so even when the user's
    //    own Cargo.toml has direct path-deps for the larger nros-*
    //    runtime (canonical 212 shape), the generated crates need
    //    `[patch.crates-io]` entries for these two specific crates to
    //    resolve.
    //
    // 3) Phase 220.E — additionally, every `nros-*` (or `nros` /
    //    `cyclonedds-sys`) runtime crate the CONSUMER's own
    //    `[dependencies]` references in registry-style (`version = "*"`)
    //    needs a path-dep here too — otherwise cargo can't resolve them
    //    (they aren't on crates.io). Without this, examples like
    //    `examples/zephyr/rust/talker/` which declare
    //    `nros = { version = "*", ... }` + `nros-rmw-zenoh = { version =
    //    "*", ... }` would fail post-`ws sync` with `no matching package`.
    //
    //    The union of (220.E extras) + (the minimal `nros-core` +
    //    `nros-serdes` for generated msg crates) is emitted in a single
    //    deduped, alphabetical pass for deterministic diffs.
    if let Some(nrp) = nano_ros_path {
        // Always include the minimum runtime patches the generated msg
        // crates depend on, then union in the consumer-referenced extras.
        let mut wanted: Vec<String> = vec!["nros-core".to_string(), "nros-serdes".to_string()];
        // Phase 244 E3 — the GENERATED crates can themselves carry registry-style
        // runtime deps the consumer never names directly (an action crate deps
        // `nros-rmw-cyclonedds` for `RosAction::register_protocol_types`). Scan
        // each generated pkg's Cargo.toml so those get a `[patch.crates-io]` path
        // too — otherwise cargo can't resolve them post-sync.
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
                // Unknown nros-* / cyclonedds-sys variant — likely a
                // third-party extension. Log + skip rather than fail;
                // user can still hand-patch outside the managed block.
                eprintln!(
                    "ws sync: unknown runtime crate `{extra}` referenced as registry dep; \
                     no path mapping in the nros lookup table — skipping patch entry. \
                     Add a manual `[patch.crates-io]` row above the BEGIN marker."
                );
            }
        }
        // Deterministic order — alphabetical keeps the diff stable across
        // re-syncs and across consumers with different dep sets.
        wanted.sort();
        wanted.dedup();
        for cname in &wanted {
            let sub = nros_crate_subpath(cname).expect("cname is a managed crate; subpath exists");
            let crate_root = nrp.join(&sub);
            if !crate_root.join("Cargo.toml").is_file() {
                continue;
            }
            let rel = pathdiff::diff_paths(&crate_root, authority_dir).unwrap_or(crate_root);
            entries.push_str(&format!("{cname} = {{ path = \"{}\" }}\n", rel.display()));
            managed_names.push(cname.clone());
        }
    }

    let mut block = String::new();
    block.push_str(BEGIN);
    block.push('\n');
    // No timestamp — deterministic output keeps the committed Cargo.toml
    // diff-stable across re-syncs (only path entries change when the user
    // adds/removes a msg pkg). For "when did sync last run" debugging,
    // grep the generated/<pkg>/Cargo.toml mtime.
    block.push_str("# Auto-generated by `nros ws sync`. Do not edit between\n");
    block.push_str("# the BEGIN/END markers — re-run sync instead.\n");
    block.push_str(&entries);
    block.push_str(END);
    block.push('\n');

    RenderedBlock {
        managed_names,
        block,
    }
}

/// Splice the rendered BEGIN/END block into `body`, guaranteeing exactly one
/// `[patch.crates-io]` table in the output.
///
/// Phase 210.D.1 (the original writer) appended the block verbatim, which
/// produced a second `[patch.crates-io]` header whenever the consumer's
/// `Cargo.toml` already had a hand-authored one (e.g. for `builtin_interfaces`
/// + `example_interfaces` codegen output dir paths). cargo's TOML parser
/// rejects duplicate tables.
///
/// This splicer:
///   1. Drops any existing BEGIN/END region (idempotent re-runs).
///   2. Locates any existing `[patch.crates-io]` table; partitions its rows
///      into "user-preserved" (names NOT in `rendered.managed_names`) and
///      "managed-duplicate" (names in the managed set — evicted because the
///      BEGIN/END block carries the authoritative copy).
///   3. Rewrites the file with exactly one `[patch.crates-io]` header,
///      followed by the user-preserved rows, followed by the BEGIN/END
///      block. If no `[patch.crates-io]` table existed previously, a fresh
///      header is emitted just above the BEGIN marker.
fn splice_patch_block(body: &str, rendered: &RenderedBlock) -> String {
    // 1) Strip existing BEGIN/END region.
    let without_block = strip_managed_block(body);

    // 2) Find and excise any existing `[patch.crates-io]` table.
    let (without_table, preserved_entries, had_table) =
        extract_patch_table(&without_block, &rendered.managed_names);

    // 3) Assemble: head + new patch table + new block.
    let mut out = without_table.trim_end_matches('\n').to_string();
    if !out.is_empty() {
        out.push('\n');
    }
    // Blank line separator before the [patch.crates-io] header for
    // readability — only when the head wasn't already empty.
    if !out.is_empty() {
        out.push('\n');
    }
    out.push_str("[patch.crates-io]\n");
    for entry in &preserved_entries {
        out.push_str(entry);
        if !entry.ends_with('\n') {
            out.push('\n');
        }
    }
    // Blank line between preserved user rows and the BEGIN marker when
    // any preserved rows exist — diff-stable cosmetic.
    if !preserved_entries.is_empty() {
        out.push('\n');
    }
    out.push_str(&rendered.block);
    let _ = had_table; // silence unused — branching on it adds no clarity here.
    out
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

/// Find a `[patch.crates-io]` table in `body`, split its rows into
/// user-preserved (NOT in `managed_names`) vs evicted (in `managed_names`),
/// and return the body with the table removed alongside the preserved rows.
///
/// "Row" preservation is verbatim — including any comments inline among the
/// rows — because users may have annotated their entries. Comment lines are
/// kept attached to the next non-comment row when possible; standalone
/// trailing comments are preserved as-is.
fn extract_patch_table(body: &str, managed_names: &[String]) -> (String, Vec<String>, bool) {
    // Locate header line. Match any line whose trimmed text begins with
    // `[patch.crates-io]` — TOML allows whitespace before headers but we
    // emit them flush-left, so a leading-whitespace match is sufficient.
    let mut header_start: Option<usize> = None;
    let mut cursor = 0usize;
    for line in body.split_inclusive('\n') {
        if is_patch_crates_io_header(line) {
            header_start = Some(cursor);
            break;
        }
        cursor += line.len();
    }
    let header_start = match header_start {
        Some(i) => i,
        None => return (body.to_string(), Vec::new(), false),
    };

    // Locate the line break after the header.
    let header_line_end = header_start
        + body[header_start..]
            .find('\n')
            .map(|i| i + 1)
            .unwrap_or(body.len() - header_start);

    // Walk forward line-by-line until we hit the next `[section]` header,
    // a BEGIN marker (shouldn't be present after strip, but defensive),
    // or EOF. Collect rows.
    let mut body_cursor = header_line_end;
    let mut preserved: Vec<String> = Vec::new();
    let mut pending_comments: Vec<String> = Vec::new();
    let mut table_end = body.len();
    for line in body[header_line_end..].split_inclusive('\n') {
        let trimmed = line.trim_start();
        if trimmed.starts_with('[') && !is_patch_crates_io_header(line) {
            // Next section starts here. Stop.
            table_end = body_cursor;
            break;
        }
        if trimmed.starts_with(BEGIN) {
            table_end = body_cursor;
            break;
        }
        if trimmed.is_empty() {
            // Blank line — flush any pending comments as standalone
            // preserved (they're not attached to a managed row, so they
            // either belong to the user or are decorative).
            for c in pending_comments.drain(..) {
                preserved.push(c);
            }
            body_cursor += line.len();
            continue;
        }
        if trimmed.starts_with('#') {
            pending_comments.push(line.to_string());
            body_cursor += line.len();
            continue;
        }
        // Looks like an entry: parse the key (text before `=`).
        if let Some(eq) = trimmed.find('=') {
            let key = trimmed[..eq].trim();
            let key = strip_toml_key_quotes(key);
            let is_managed = managed_names.iter().any(|n| n == key);
            if is_managed {
                // Evict: drop attached comments + the row.
                pending_comments.clear();
            } else {
                // Preserve: flush comments + this row.
                for c in pending_comments.drain(..) {
                    preserved.push(c);
                }
                preserved.push(line.to_string());
            }
            body_cursor += line.len();
            continue;
        }
        // Unknown shape — preserve verbatim to be safe.
        for c in pending_comments.drain(..) {
            preserved.push(c);
        }
        preserved.push(line.to_string());
        body_cursor += line.len();
    }
    // Flush any trailing pending comments that didn't attach to a row.
    for c in pending_comments {
        preserved.push(c);
    }

    let mut out = String::new();
    out.push_str(&body[..header_start]);
    // Trim trailing blank line above the removed header so we don't leave
    // a double-blank gap.
    if out.ends_with("\n\n") {
        out.pop();
    }
    out.push_str(&body[table_end..]);
    (out, preserved, true)
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
        if p.is_rust_pkg && !p.is_msg_pkg {
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
    // Strip the auto-managed patch block from every Rust consumer's patch
    // authority Cargo.toml.
    let rust_consumers: Vec<&WsPkg> = scan.iter().filter(|p| p.is_rust_pkg).collect();
    let mut authorities: std::collections::HashSet<PathBuf> = std::collections::HashSet::new();
    for c in &rust_consumers {
        if let Ok(a) = find_patch_authority(&c.dir, &ws_root) {
            authorities.insert(a);
        }
    }
    for authority in authorities {
        let body = match std::fs::read_to_string(&authority) {
            Ok(b) => b,
            Err(_) => continue,
        };
        let (Some(b), Some(e)) = (body.find(BEGIN), body.find(END)) else {
            continue;
        };
        if args.dry_run {
            println!(
                "ws clean: WOULD strip patch block from {}",
                authority.display()
            );
            continue;
        }
        let end_line_end = e + END.len();
        let mut out = String::new();
        out.push_str(&body[..b]);
        // Drop the preceding blank line we added when writing the block.
        let trimmed = out.trim_end_matches('\n');
        out.truncate(trimmed.len());
        out.push('\n');
        out.push_str(&body[end_line_end..]);
        std::fs::write(&authority, out)
            .wrap_err_with(|| format!("ws clean: write {}", authority.display()))?;
        println!(
            "ws clean: stripped patch block from {}",
            authority.display()
        );
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
        // (c) rust consumer: is the patch authority Cargo.toml sane?
        if pkg.is_rust_pkg && !pkg.is_msg_pkg {
            match find_patch_authority(&pkg.dir, &ws_root) {
                Ok(a) => {
                    let body = std::fs::read_to_string(&a).unwrap_or_default();
                    if !body.contains(BEGIN) {
                        eprintln!(
                            "  ⚠ {}: no nros-managed [patch.crates-io] block in \
                             patch authority ({}). Run `nros ws sync`.",
                            pkg.name,
                            a.display()
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
mod patch_block_tests {
    use super::*;

    fn rendered(names: &[&str]) -> RenderedBlock {
        // Synthetic block — bypasses path resolution so tests stay
        // hermetic. Mirrors the shape `render_patch_block` emits.
        let mut block = String::new();
        block.push_str(BEGIN);
        block.push('\n');
        block.push_str("# Auto-generated by `nros ws sync`. Do not edit between\n");
        block.push_str("# the BEGIN/END markers — re-run sync instead.\n");
        for n in names {
            block.push_str(&format!("{n} = {{ path = \"managed/{n}\" }}\n"));
        }
        block.push_str(END);
        block.push('\n');
        RenderedBlock {
            managed_names: names.iter().map(|n| (*n).to_string()).collect(),
            block,
        }
    }

    fn count_patch_headers(body: &str) -> usize {
        body.lines()
            .filter(|l| l.trim_start().starts_with("[patch.crates-io]"))
            .count()
    }

    /// Bug repro: a pre-existing `[patch.crates-io]` table + a
    /// (legacy-shape) BEGIN/END block carrying its own `[patch.crates-io]`
    /// header used to produce two table headers. After the fix exactly
    /// one header survives, every user row is preserved, and the managed
    /// rows live under the BEGIN/END markers without a duplicate header.
    #[test]
    fn splice_dedups_preexisting_patch_table() {
        let body = "\
[package]
name = \"demo\"
version = \"0.1.0\"

[patch.crates-io]
builtin_interfaces = { path = \"generated/builtin_interfaces\" }
example_interfaces = { path = \"generated/example_interfaces\" }
nros-core = { path = \"../packages/core/nros-core\" }

# === BEGIN nros-managed [patch.crates-io] ===
# Auto-generated by `nros ws sync`. Do not edit between
# the BEGIN/END markers — re-run sync instead.
[patch.crates-io]
nros-core = { path = \"../packages/core/nros-core\" }
nros-serdes = { path = \"../packages/core/nros-serdes\" }
# === END nros-managed [patch.crates-io] ===
";
        let r = rendered(&["nros-core", "nros-serdes"]);
        let out = splice_patch_block(body, &r);
        assert_eq!(
            count_patch_headers(&out),
            1,
            "expected exactly one [patch.crates-io] header, got:\n{out}"
        );
        assert!(
            out.contains("builtin_interfaces = { path = \"generated/builtin_interfaces\" }"),
            "user-preserved entry missing: {out}"
        );
        assert!(
            out.contains("example_interfaces = { path = \"generated/example_interfaces\" }"),
            "user-preserved entry missing: {out}"
        );
        // Managed entry now only appears once, inside the block.
        let managed_occurrences = out
            .matches("nros-core = { path = \"managed/nros-core\" }")
            .count();
        assert_eq!(
            managed_occurrences, 1,
            "managed row should appear once: {out}"
        );
        // The user's hand-authored `nros-core` row above the block was
        // evicted (it's in the managed set), preventing TOML's
        // "duplicate key in [patch.crates-io]" error.
        assert!(
            !out.contains("nros-core = { path = \"../packages/core/nros-core\" }"),
            "managed-duplicate user row should have been evicted: {out}"
        );
    }

    /// Idempotence: running the writer twice on the same body produces
    /// the same body the second time. This guards against the historical
    /// behaviour where a re-sync would keep appending new blocks.
    #[test]
    fn splice_is_idempotent() {
        let body = "\
[package]
name = \"demo\"
version = \"0.1.0\"

[patch.crates-io]
builtin_interfaces = { path = \"generated/builtin_interfaces\" }
";
        let r = rendered(&["nros-core", "nros-serdes"]);
        let first = splice_patch_block(body, &r);
        let second = splice_patch_block(&first, &r);
        assert_eq!(
            first, second,
            "writer is not idempotent:\nfirst:\n{first}\nsecond:\n{second}"
        );
        assert_eq!(count_patch_headers(&first), 1);
    }

    /// First-time write into a file with no pre-existing `[patch.crates-io]`
    /// table: a fresh header is emitted, and the BEGIN/END block follows
    /// directly.
    #[test]
    fn splice_emits_header_when_absent() {
        let body = "\
[package]
name = \"demo\"
version = \"0.1.0\"

[dependencies]
serde = \"1\"
";
        let r = rendered(&["nros-core"]);
        let out = splice_patch_block(body, &r);
        assert_eq!(count_patch_headers(&out), 1, "{out}");
        assert!(out.contains(BEGIN));
        assert!(out.contains(END));
        assert!(out.contains("nros-core = { path = \"managed/nros-core\" }"));
    }

    /// User entries that look like nros-managed names but live OUTSIDE the
    /// managed set (e.g. a custom `nros-rmw-zenoh` path the user maintains
    /// manually when the sync run wasn't asked to manage it) must be
    /// preserved. Only names that the current render claims authority over
    /// are evicted.
    #[test]
    fn splice_preserves_unmanaged_nros_entries() {
        let body = "\
[patch.crates-io]
nros-rmw-zenoh = { path = \"../packages/zpico/nros-rmw-zenoh\" }
";
        // Only nros-core is claimed by this sync run.
        let r = rendered(&["nros-core"]);
        let out = splice_patch_block(body, &r);
        assert_eq!(count_patch_headers(&out), 1, "{out}");
        assert!(
            out.contains("nros-rmw-zenoh = { path = \"../packages/zpico/nros-rmw-zenoh\" }"),
            "unmanaged user row dropped: {out}"
        );
    }

    /// Exact repro of the four broken nano-ros `examples/threadx-linux/rust/
    /// *_entry/Cargo.toml` files described in the task brief — pre-existing
    /// `[patch.crates-io]` table holds codegen + nros-* path rows, the
    /// BEGIN/END block adds cyclonedds-sys. After splice: exactly one
    /// header, `builtin_interfaces` + `example_interfaces` preserved,
    /// `cyclonedds-sys` appears once under the block.
    #[test]
    fn splice_repro_nano_ros_threadx_linux_entry_pkg() {
        let body = "\
[package]
name = \"threadx_linux_rs_action_client_entry\"
version = \"0.1.0\"
edition = \"2024\"

[[bin]]
name = \"threadx_linux_rs_action_client_entry\"
path = \"src/main.rs\"

[dependencies]
nros-board-threadx-linux = { path = \"../../../../packages/boards/nros-board-threadx-linux\" }

[workspace]

[patch.crates-io]
builtin_interfaces = { path = \"../action-client/generated/builtin_interfaces\" }
example_interfaces = { path = \"../action-client/generated/example_interfaces\" }

nros-core = { path = \"../../../../packages/core/nros-core\" }
nros-serdes = { path = \"../../../../packages/core/nros-serdes\" }

# === BEGIN nros-managed [patch.crates-io] ===
# Auto-generated by `nros ws sync`. Do not edit between
# the BEGIN/END markers — re-run sync instead.
[patch.crates-io]
cyclonedds-sys = { path = \"../../../../packages/dds/cyclonedds-sys\" }
# === END nros-managed [patch.crates-io] ===
";
        let r = rendered(&["cyclonedds-sys"]);
        let out = splice_patch_block(body, &r);
        assert_eq!(
            count_patch_headers(&out),
            1,
            "expected one [patch.crates-io] header, got:\n{out}"
        );
        assert!(out.contains("builtin_interfaces"), "{out}");
        assert!(out.contains("example_interfaces"), "{out}");
        assert!(
            out.contains("cyclonedds-sys = { path = \"managed/cyclonedds-sys\" }"),
            "{out}"
        );
        // No duplicate header inside the BEGIN/END region. The BEGIN
        // marker comment itself contains `[patch.crates-io]` so we check
        // for a flush-left table header on its own line — the actual TOML
        // header that would have triggered the parse error.
        let begin_idx = out.find(BEGIN).unwrap();
        let end_idx = out.find(END).unwrap();
        let block_region = &out[begin_idx..end_idx];
        let inner_header_lines: Vec<&str> = block_region
            .lines()
            .filter(|l| l.trim_start() == "[patch.crates-io]")
            .collect();
        assert!(
            inner_header_lines.is_empty(),
            "BEGIN/END region carries its own header line(s): {inner_header_lines:?}"
        );
    }

    /// `strip_managed_block` is a no-op when no BEGIN marker is present.
    #[test]
    fn strip_managed_block_noop_without_markers() {
        let body = "[package]\nname = \"x\"\n";
        assert_eq!(strip_managed_block(body), body);
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

    /// `cyclonedds-sys` lives under `packages/dds/` and is intentionally
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
nros-rmw-zenoh = { path = "../../../packages/zpico/nros-rmw-zenoh" }
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

    /// TOML quoted keys (`"name" = ...`) are recognised as managed.
    #[test]
    fn extract_table_handles_quoted_keys() {
        let body = "[patch.crates-io]\n\"nros-core\" = { path = \"x\" }\nfoo = { path = \"y\" }\n";
        let (out, preserved, had) = extract_patch_table(body, &["nros-core".to_string()]);
        assert!(had);
        assert!(
            !out.contains("nros-core"),
            "managed quoted key not evicted: {out}"
        );
        assert_eq!(preserved.len(), 1, "{:?}", preserved);
        assert!(preserved[0].contains("foo"));
    }

    /// Issue #94 case A — the TOML-equivalent quoted header
    /// `[patch."crates-io"]` (emitted by toml_edit / hand-authored) must be
    /// recognised as the patch table, so the splicer reuses it instead of
    /// adding a second header.
    #[test]
    fn extract_table_handles_quoted_crates_io_header() {
        let body = "[patch.\"crates-io\"]\nnros-core = { path = \"x\" }\nfoo = { path = \"y\" }\n";
        let (out, preserved, had) = extract_patch_table(body, &["nros-core".to_string()]);
        assert!(
            had,
            "quoted [patch.\"crates-io\"] header not detected:\n{out}"
        );
        assert!(
            !out.contains("nros-core"),
            "managed row under quoted header not evicted:\n{out}"
        );
        assert_eq!(preserved.len(), 1, "{preserved:?}");
        assert!(preserved[0].contains("foo"));
    }

    /// Issue #94 case A — splicing into a manifest whose existing patch
    /// table uses the quoted header form must leave exactly ONE patch
    /// table (counting both bare + quoted forms). Otherwise cargo rejects
    /// the duplicate `[patch.crates-io]`.
    #[test]
    fn splice_dedups_quoted_preexisting_header() {
        let body = "\
[package]
name = \"d\"

[patch.\"crates-io\"]
builtin_interfaces = { path = \"generated/builtin_interfaces\" }
nros-core = { path = \"../core/nros-core\" }
";
        let r = rendered(&["nros-core"]);
        let out = splice_patch_block(body, &r);
        let total = out
            .lines()
            .filter(|l| {
                let t = l.trim_start();
                t.starts_with("[patch.crates-io]")
                    || t.starts_with("[patch.\"crates-io\"]")
                    || t.starts_with("[patch.'crates-io']")
            })
            .count();
        assert_eq!(total, 1, "duplicate patch table for quoted form:\n{out}");
        assert!(
            out.contains("builtin_interfaces"),
            "user row dropped:\n{out}"
        );
        assert!(
            !out.contains("nros-core = { path = \"../core/nros-core\" }"),
            "managed-duplicate user row not evicted:\n{out}"
        );
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
path = "../zpico/nros-rmw-zenoh"

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
}
