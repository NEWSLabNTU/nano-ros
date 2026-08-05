//! `nros board` — board crate introspection.
//!
//! * `list` — Phase 111.A.8: enumerate every `nros-board-*` crate under
//!   `<workspace>/packages/boards/`.
//! * `info <name>` — Phase 215.C.3: print the side-by-side `Cargo.toml` +
//!   `board.cmake` views of a board crate's manifest, optionally erroring
//!   when the two faces drift (the Phase 215.F audit hook).

use clap::{Args as ClapArgs, Subcommand};
use eyre::{Result, WrapErr, eyre};
use serde::Serialize;
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use crate::orchestration::board_metadata::{
    BoardMetadata, DriftEntry, compute_drift, parse_board_cmake, parse_board_metadata,
};

#[derive(Debug, Subcommand)]
pub enum Args {
    /// List every supported board crate
    List(ListArgs),
    /// Print a board crate's Cargo.toml + board.cmake views side-by-side
    /// (Phase 215.C.3).
    Info(InfoArgs),
}

#[derive(Debug, ClapArgs)]
pub struct ListArgs {
    /// Path to the nano-ros workspace root (auto-detected by walking
    /// upward from cwd if omitted)
    #[arg(long)]
    pub workspace: Option<PathBuf>,
}

#[derive(Debug, ClapArgs)]
pub struct InfoArgs {
    /// Board key. Resolves to a board CRATE
    /// (`packages/boards/nros-board-<name>/`) or, since phase-337 W9.a, to a
    /// conf BUNDLE under a family crate
    /// (`packages/boards/nros-board-<family>/boards/<name>/`) — e.g.
    /// `fvp-aemv8r-smp` is `nros-board-zephyr/boards/fvp-aemv8r-smp/`.
    pub name: String,
    /// Path to the nano-ros workspace root (auto-detected by walking
    /// upward from cwd if omitted). May also be set via
    /// `NROS_WORKSPACE_ROOT`.
    #[arg(long)]
    pub workspace: Option<PathBuf>,
    /// Exit with status 1 if drift is detected between the Cargo.toml
    /// view and the board.cmake view. When only one source is present
    /// (e.g. a bare board with no board.cmake), exits 0 — there is
    /// nothing to drift against.
    #[arg(long)]
    pub check_drift: bool,
}

pub fn run(args: Args) -> Result<()> {
    match args {
        Args::List(args) => list(args),
        Args::Info(args) => info(args),
    }
}

fn list(args: ListArgs) -> Result<()> {
    let root = match args.workspace {
        Some(p) => p,
        None => find_workspace_root()?,
    };
    let boards_dir = root.join("packages").join("boards");
    if !boards_dir.is_dir() {
        return Err(eyre!(
            "no `packages/boards/` directory under {}",
            root.display()
        ));
    }

    let mut entries: Vec<BoardEntry> = Vec::new();
    for entry in fs::read_dir(&boards_dir)
        .wrap_err_with(|| format!("failed to read {}", boards_dir.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        let cargo_toml = path.join("Cargo.toml");
        if !cargo_toml.is_file() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if !name.starts_with("nros-board-") {
            continue;
        }
        match read_board(&cargo_toml) {
            Ok(b) => entries.push(b),
            Err(e) => eprintln!("warning: skipping {}: {e}", name),
        }
    }
    entries.sort_by(|a, b| a.name.cmp(&b.name));

    if entries.is_empty() {
        println!("No board crates found under {}", boards_dir.display());
        return Ok(());
    }

    let name_w = entries
        .iter()
        .map(|e| e.name.len())
        .max()
        .unwrap_or(4)
        .max(4);
    println!("{:<name_w$}  description", "name", name_w = name_w);
    println!(
        "{:<name_w$}  {}",
        "-".repeat(name_w),
        "-".repeat(60),
        name_w = name_w
    );
    for b in entries {
        println!("{:<name_w$}  {}", b.name, b.description, name_w = name_w);
    }
    Ok(())
}

struct BoardEntry {
    name: String,
    description: String,
}

fn read_board(cargo_toml: &Path) -> Result<BoardEntry> {
    let raw = fs::read_to_string(cargo_toml)?;
    let doc: toml_edit::DocumentMut = raw.parse()?;
    let pkg = doc
        .get("package")
        .and_then(|p| p.as_table())
        .ok_or_else(|| eyre!("no [package] table in {}", cargo_toml.display()))?;
    let name = pkg
        .get("name")
        .and_then(|n| n.as_str())
        .ok_or_else(|| eyre!("no [package].name in {}", cargo_toml.display()))?
        .to_string();
    let description = pkg
        .get("description")
        .and_then(|d| d.as_str())
        .unwrap_or("")
        .to_string();
    Ok(BoardEntry { name, description })
}

// -----------------------------------------------------------------------
// Phase 215.C.3 — `nros board info <name>`
// -----------------------------------------------------------------------

/// JSON envelope produced by `nros board info`.
#[derive(Debug, Serialize)]
struct BoardInfo {
    name: String,
    crate_dir: PathBuf,
    cargo_metadata: Option<BoardMetadata>,
    board_cmake: Option<BTreeMap<String, String>>,
    drift: Vec<DriftEntry>,
}

fn info(args: InfoArgs) -> Result<()> {
    let root = match args.workspace {
        Some(p) => p,
        None => find_workspace_root()?,
    };
    let crate_dir = locate_board_crate(&root, &args.name)?;
    let cargo_toml = crate_dir.join("Cargo.toml");
    let board_cmake_path = crate_dir.join("board.cmake");

    let cargo_metadata = if cargo_toml.is_file() {
        match parse_board_metadata(&cargo_toml) {
            Ok(m) => Some(m),
            Err(e) => {
                // Surface the diagnostic on stderr so users see WHY the
                // Cargo.toml face was skipped, but keep the info dump
                // useful when only board.cmake is authored yet.
                eprintln!("warning: {e}");
                None
            }
        }
    } else {
        None
    };

    let board_cmake = if board_cmake_path.is_file() {
        let raw = fs::read_to_string(&board_cmake_path)
            .wrap_err_with(|| format!("read {}", board_cmake_path.display()))?;
        Some(parse_board_cmake(&raw))
    } else {
        None
    };

    let drift = match (&cargo_metadata, &board_cmake) {
        (Some(c), Some(k)) => compute_drift(c, k),
        _ => Vec::new(),
    };

    let info = BoardInfo {
        name: args.name.clone(),
        crate_dir: crate_dir.clone(),
        cargo_metadata,
        board_cmake,
        drift,
    };
    let json = serde_json::to_string_pretty(&info).wrap_err("serialise BoardInfo as JSON")?;
    println!("{json}");

    if args.check_drift && !info.drift.is_empty() {
        return Err(eyre!(
            "drift detected between Cargo.toml and board.cmake for `{}` \
             ({} field(s))",
            args.name,
            info.drift.len()
        ));
    }
    Ok(())
}

/// Resolve a board key to the directory holding its `board.cmake`.
///
/// Two shapes, tried in this order — phase-337 W9.a added the second, and
/// RFC-0064 expects it to become the common one:
///
/// 1. a board CRATE — `packages/boards/nros-board-<name>/`
/// 2. a CONF BUNDLE under a family crate —
///    `packages/boards/nros-board-<family>/boards/<name>/`
///
/// A per-board crate for a Zephyr board is duplication: Zephyr owns boot, MMU,
/// net stack and drivers, so the "crate" held a `prj.conf`, a DTS overlay and a
/// manifest — a config bundle wearing a `Cargo.toml`. Shape 1 stays first and
/// stays supported for boards that carry real Rust.
///
/// This MUST match `nano_ros_use_board()`'s lookup order
/// (`zephyr/cmake/nano_ros_use_board.cmake`). Two languages, so no shared
/// implementation is possible — the pair is kept honest by the fact that
/// `nros board info <name>` and a Zephyr build of the same board must agree.
fn locate_board_crate(workspace_root: &Path, name: &str) -> Result<PathBuf> {
    let boards = workspace_root.join("packages").join("boards");

    let dir_name = format!("nros-board-{name}");
    let crate_dir = boards.join(&dir_name);
    if crate_dir.is_dir() {
        return Ok(crate_dir);
    }

    // Board keys are global, so more than one match is a naming collision
    // rather than something to disambiguate by search order.
    let mut bundles: Vec<PathBuf> = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&boards) {
        for entry in entries.flatten() {
            let candidate = entry.path().join("boards").join(name);
            if candidate.join("board.cmake").is_file() {
                bundles.push(candidate);
            }
        }
    }
    bundles.sort();
    match bundles.len() {
        1 => Ok(bundles.remove(0)),
        0 => Err(eyre!(
            "no board `{name}` under `{}/packages/boards/`: looked for a board \
             crate `{dir_name}/` and for a conf bundle `*/boards/{name}/`",
            workspace_root.display(),
        )),
        _ => Err(eyre!(
            "board `{name}` is ambiguous — it is a conf bundle under more than \
             one family crate: {bundles:?}. Board keys are global; rename one."
        )),
    }
}

/// Walk upward from cwd until a directory containing `packages/boards/`
/// is found. The `NROS_WORKSPACE_ROOT` env var, when set, short-
/// circuits the walk (matches `nros_build::pkg_index::detect_workspace_root`).
pub(crate) fn find_workspace_root() -> Result<PathBuf> {
    if let Some(override_) = std::env::var_os("NROS_WORKSPACE_ROOT") {
        let p = PathBuf::from(override_);
        if !p.exists() {
            return Err(eyre!(
                "NROS_WORKSPACE_ROOT=`{}` does not exist on disk",
                p.display()
            ));
        }
        return Ok(p);
    }
    let cwd = std::env::current_dir()?;
    let mut cur: &Path = &cwd;
    loop {
        if cur.join("packages").join("boards").is_dir() {
            return Ok(cur.to_path_buf());
        }
        match cur.parent() {
            Some(p) => cur = p,
            None => {
                return Err(eyre!(
                    "could not auto-detect nano-ros workspace root from {}; \
                     pass --workspace <path> explicitly",
                    cwd.display()
                ));
            }
        }
    }
}
