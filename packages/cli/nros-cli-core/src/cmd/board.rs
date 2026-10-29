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
    fs,
    path::{Path, PathBuf},
};

use crate::orchestration::{
    board_descriptor::{BoardCatalog, BoardDescriptor},
    board_projection,
};

#[derive(Debug, Subcommand)]
pub enum Args {
    /// List every supported board crate
    List(ListArgs),
    /// Print a board's resolved descriptor as JSON.
    Info(InfoArgs),
    /// Scaffold a new board package (phase-375 W2/W9).
    New(crate::cmd::board_new::NewArgs),
    /// Project a board descriptor into cmake variables (RFC-0064 R5 D4).
    ///
    /// Written per configure into the build directory by
    /// `nano_ros_use_board()`; never committed. The descriptor is the single
    /// authored source, and this is the mechanical projection of it.
    CmakeVars(CmakeVarsArgs),
    /// Print a board's resolved network identity defaults (phase-484 W4b).
    ///
    /// The board's `[board.net]` over the platform default — the one value the
    /// board crate's `Config::default()`, its cargo-road C `NROS_APP_CONFIG`
    /// and its cmake-road C `NROS_APP_CONFIG` all use.
    Net(NetArgs),
}

#[derive(Debug, ClapArgs)]
pub struct NetArgs {
    /// Board key.
    pub name: String,
    /// nano-ros SDK root holding the board catalog. Default: a checkout
    /// enclosing cwd, then this `nros` binary's own SDK root.
    #[arg(long)]
    pub workspace: Option<PathBuf>,
    /// `c-network`: the `.network = { … }` body of a C `nros_app_config_t`
    /// initializer. `toml`: the four resolved fields. `cmake`: `set()` lines
    /// for `nros_board_net_c()` — the initializer AND the descriptor it came
    /// from, so the configure can depend on that file without modelling where a
    /// board key's directory is.
    #[arg(long, default_value = "toml")]
    pub format: String,
    /// An image's resolved network (`NROS_IMAGE_NET`, RFC-0103 D10f): its
    /// primary interface replaces the board default. Empty or absent keeps the
    /// board default. It replaced `--ip-last` / `--mac-last`, which let a build
    /// row restate one octet of an address the image's `system.toml` now owns
    /// (phase-497 W2).
    #[arg(long, allow_hyphen_values = true)]
    pub image_net: Option<String>,
}

#[derive(Debug, ClapArgs)]
pub struct ListArgs {
    /// nano-ros SDK root holding the board catalog. Default:
    /// `$NROS_WORKSPACE_ROOT`, a checkout enclosing cwd, then this `nros`
    /// binary's own SDK root (the installed track needs no flag).
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
    /// nano-ros SDK root holding the board catalog. Default:
    /// `$NROS_WORKSPACE_ROOT`, a checkout enclosing cwd, then this `nros`
    /// binary's own SDK root.
    #[arg(long)]
    pub workspace: Option<PathBuf>,
}

#[derive(Debug, ClapArgs)]
pub struct CmakeVarsArgs {
    /// Board key, as `nano_ros_use_board(<key>)` spells it.
    pub name: String,
    /// nano-ros SDK root holding the board catalog. Default: a checkout
    /// enclosing cwd, then this `nros` binary's own SDK root.
    #[arg(long)]
    pub workspace: Option<PathBuf>,
    /// Where to write the projection. Its directory also receives the
    /// generated Rust-support Kconfig module when the board needs one.
    #[arg(long)]
    pub out: PathBuf,
}

pub fn run(args: Args) -> Result<()> {
    match args {
        Args::List(args) => list(args),
        Args::Info(args) => info(args),
        Args::CmakeVars(args) => cmake_vars(args),
        Args::Net(args) => net(args),
        Args::New(args) => crate::cmd::board_new::run(args),
    }
}

fn list(args: ListArgs) -> Result<()> {
    let root = catalog_root(args.workspace)?;
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
///
/// RFC-0064 R5 D4 removed the second face. This used to print a Cargo.toml
/// view beside a `board.cmake` view with a computed `drift` list; there is now
/// one authored file, so there is nothing to print twice and nothing to drift.
#[derive(Debug, Serialize)]
struct BoardInfo {
    name: String,
    board_dir: PathBuf,
    descriptor: PathBuf,
    /// The projection this board would produce for a Zephyr build, or `null`
    /// for a board with no `[board.zephyr]` block. Printed because "what does
    /// cmake actually see?" is the question `board info` was invented to
    /// answer, and it is now derivable rather than authored.
    cmake_vars: Option<String>,
}

/// Resolve `name` to its descriptor through the board CATALOG.
///
/// Not by a path convention. `resolve` below still exists for the
/// consumers that need a directory, but resolution by NAME must go through the
/// catalog, or `nros board info <alias>` and a build of the same alias answer
/// differently — which is what a board resolving only as an alias on another
/// board's descriptor used to guarantee.
fn resolve(root: &Path, name: &str) -> Result<(BoardDescriptor, PathBuf)> {
    let catalog = BoardCatalog::load(root).map_err(|e| eyre!("load board catalog: {e}"))?;
    let board = catalog
        .descriptors()
        .iter()
        .find(|d| d.answers_to(name))
        .ok_or_else(|| {
            let mut known: Vec<&str> = catalog
                .descriptors()
                .iter()
                .flat_map(|d| d.names.iter().map(String::as_str))
                .collect();
            known.sort_unstable();
            known.dedup();
            eyre!(
                "no board named `{name}`.\n  Known: {}\n  \
                 For an out-of-tree board, put its package directory on \
                 $NROS_EXTRA_BOARD_PATH.",
                known.join(", ")
            )
        })?
        .clone();
    let dir = board_projection::descriptor_dir(&board, root)
        .ok_or_else(|| eyre!("board `{name}` has no recorded descriptor path"))?;
    Ok((board, dir))
}

fn net(args: NetArgs) -> Result<()> {
    use nros_board_common::board_net::{ResolvedNet, resolve as resolve_net};
    let root = catalog_root(args.workspace)?;
    let (_board, dir) = resolve(&root, &args.name)?;
    let n = resolve_net(
        &dir.join("nros-board.toml"),
        Some(&args.name),
        ResolvedNet::platform_default(),
    )
    .map_err(|e| eyre!("{}: {e}", dir.join("nros-board.toml").display()))?;
    let n = match args.image_net.as_deref().filter(|t| !t.is_empty()) {
        Some(t) => match nros_board_common::image_net::primary(t) {
            Ok(Some(i)) => n.with_interface(&i),
            Ok(None) => n,
            Err(e) => eyre::bail!("--image-net: {e}"),
        },
        None => n,
    };
    match args.format.as_str() {
        "c-network" => println!("{}", n.c_network_initializer()),
        "cmake" => {
            println!("set(NROS_BOARD_NET_C \"{}\")", n.c_network_initializer());
            println!(
                "set(NROS_BOARD_NET_DESCRIPTOR \"{}\")",
                dir.join("nros-board.toml").display()
            );
            // RFC-0103 D10 — the board's router default is its gateway; the
            // entry-locator resolver builds `tcp/<gateway>:<port>` from it.
            println!(
                "set(NROS_BOARD_NET_GATEWAY \"{}\")",
                n.gateway
                    .iter()
                    .map(u8::to_string)
                    .collect::<Vec<_>>()
                    .join(".")
            );
        }
        "toml" => {
            let dotted = |o: &[u8]| o.iter().map(u8::to_string).collect::<Vec<_>>().join(".");
            let mac = n
                .mac
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<Vec<_>>()
                .join(":");
            println!(
                "ip = \"{}\"\nnetmask = \"{}\"\ngateway = \"{}\"\nmac = \"{mac}\"",
                dotted(&n.ip),
                dotted(&n.netmask),
                dotted(&n.gateway)
            );
        }
        other => {
            return Err(eyre!(
                "--format {other}: expected `toml`, `c-network` or `cmake`"
            ));
        }
    }
    Ok(())
}

fn info(args: InfoArgs) -> Result<()> {
    let root = catalog_root(args.workspace)?;
    let (board, board_dir) = resolve(&root, &args.name)?;
    // A module dir is needed to project, but `info` writes nothing — name the
    // path the build WOULD use rather than inventing a second spelling.
    let module_dir = board_dir.join("<build-dir>/nros-board-rust-support");
    let cmake_vars = board_projection::project(&board, &board_dir, &module_dir).ok();

    let info = BoardInfo {
        name: args.name.clone(),
        descriptor: board_dir.join("nros-board.toml"),
        board_dir,
        cmake_vars,
    };
    let json = serde_json::to_string_pretty(&info).wrap_err("serialise BoardInfo as JSON")?;
    println!("{json}");
    Ok(())
}

fn cmake_vars(args: CmakeVarsArgs) -> Result<()> {
    let root = catalog_root(args.workspace)?;
    let (board, board_dir) = resolve(&root, &args.name)?;

    let out_dir = args
        .out
        .parent()
        .ok_or_else(|| eyre!("--out {} has no parent directory", args.out.display()))?;
    fs::create_dir_all(out_dir).wrap_err_with(|| format!("create {}", out_dir.display()))?;

    let module_dir = out_dir.join(format!("nros-board-{}-rust-support", args.name));
    let text = board_projection::project(&board, &board_dir, &module_dir)?;

    // Generate the Rust-support module only for a board that asked for Rust —
    // the projection references it under exactly the same condition, so the two
    // read the same field rather than agreeing by accident.
    let wants_rust = board
        .provisioning
        .as_ref()
        .is_some_and(|p| !p.rust_targets.is_empty());
    if wants_rust && let Some(z) = board.zephyr.as_ref() {
        board_projection::write_rust_support_module(&module_dir, &args.name, &z.west_board)?;
    }

    fs::write(&args.out, &text).wrap_err_with(|| format!("write {}", args.out.display()))?;
    Ok(())
}

/// The SDK root whose board catalog the read-only `nros board` verbs (and
/// `nros setup board`) answer from — phase-517 W2.
///
/// `--workspace`, then `$NROS_WORKSPACE_ROOT`, then the SDK-root ladder every
/// other verb asks ([`crate::orchestration::nano_ros_root`]): a checkout
/// enclosing cwd, then this `nros` binary's own SDK root. The checkout walk-up
/// used to be the ONLY rung, so on the installed track (no checkout anywhere)
/// `nros board list` failed unless the user typed `--workspace "$(nros
/// sdk-root)"` — the answer the binary already had.
pub(crate) fn catalog_root(workspace: Option<PathBuf>) -> Result<PathBuf> {
    let cwd = std::env::current_dir()?;
    resolve_catalog_root(
        workspace,
        std::env::var_os("NROS_WORKSPACE_ROOT").map(PathBuf::from),
        crate::orchestration::nano_ros_root::Rungs {
            explicit: None,
            workspace: Some(&cwd),
            exe: nros_launcher::checkout::canonical_current_exe(),
            built_in: Some(nros_launcher::checkout::built_in_dir().to_path_buf()),
        },
    )
}

/// [`catalog_root`] over its inputs, so the order is testable without
/// touching cwd, the environment or `current_exe`.
fn resolve_catalog_root(
    workspace: Option<PathBuf>,
    env: Option<PathBuf>,
    rungs: crate::orchestration::nano_ros_root::Rungs<'_>,
) -> Result<PathBuf> {
    if let Some(p) = workspace {
        return Ok(p);
    }
    if let Some(p) = env {
        if !p.exists() {
            return Err(eyre!(
                "NROS_WORKSPACE_ROOT=`{}` does not exist on disk",
                p.display()
            ));
        }
        return Ok(p);
    }
    crate::orchestration::nano_ros_root::resolve_from(rungs).ok_or_else(|| {
        eyre!(
            "no nano-ros SDK root found, so there is no board catalog to read. Resolved \
             in order: --workspace, $NROS_WORKSPACE_ROOT, a nano-ros checkout enclosing \
             the current directory, then this `nros` binary's own SDK root (share/nano-ros \
             beside a released `nros`, or the checkout an in-tree `nros` was built in). \
             Pass --workspace <root> to name one."
        )
    })
}

/// Walk upward from cwd until a directory containing `packages/boards/`
/// is found. The `NROS_WORKSPACE_ROOT` env var, when set, short-
/// circuits the walk (matches `nros_build::pkg_index::detect_workspace_root`).
///
/// A CHECKOUT question, kept for the verbs that need one: `nros board new`
/// writes into `packages/boards/`, and `nros doctor` runs the checkout's
/// `just doctor`. A verb that only READS the board catalog asks
/// [`catalog_root`], which also answers on the installed track.
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

#[cfg(test)]
mod catalog_root_tests {
    use super::*;
    use crate::orchestration::nano_ros_root::{Rungs, SHIPPED_SUBDIR};

    /// A prefix laid out like an installed release: `<p>/bin/nros` beside
    /// `<p>/share/nano-ros/<marker>`, with a board directory under it.
    fn installed_prefix(dir: &Path) -> (PathBuf, PathBuf) {
        let root = dir.join(SHIPPED_SUBDIR);
        std::fs::create_dir_all(root.join("packages/core/nros-core")).unwrap();
        std::fs::create_dir_all(root.join("packages/boards")).unwrap();
        std::fs::write(
            root.join(nros_launcher::checkout::MONOREPO_MARKER),
            "[package]\nname = \"nros-core\"\n",
        )
        .unwrap();
        std::fs::create_dir_all(dir.join("bin")).unwrap();
        let exe = dir.join("bin/nros");
        std::fs::write(&exe, "").unwrap();
        (exe, root)
    }

    /// phase-517 W2 — from a directory in NO checkout, the read-only board
    /// verbs answer from the binary's own SDK root. The checkout walk-up was
    /// the only rung, so `nros board list` failed on the installed track
    /// unless the user passed `--workspace "$(nros sdk-root)"`.
    #[test]
    fn outside_every_checkout_the_installed_sdk_root_answers() {
        let tmp = tempfile::tempdir().unwrap();
        let (exe, root) = installed_prefix(&tmp.path().join("prefix"));
        let project = tmp.path().join("users/project");
        std::fs::create_dir_all(&project).unwrap();
        // The old rung, asked from the same place, finds nothing: no ancestor
        // holds `packages/boards/` (the negative control).
        assert!(
            project
                .ancestors()
                .all(|a| !a.join("packages").join("boards").is_dir()),
            "the temp project must sit outside every checkout"
        );
        let got = resolve_catalog_root(
            None,
            None,
            Rungs {
                workspace: Some(&project),
                exe: Some(exe),
                ..Rungs::default()
            },
        )
        .expect("the installed SDK root answers");
        assert_eq!(got, root);
    }

    /// The explicit rungs still win, in order: `--workspace`, then
    /// `$NROS_WORKSPACE_ROOT` (which must exist).
    #[test]
    fn explicit_rungs_win_over_the_ladder() {
        let tmp = tempfile::tempdir().unwrap();
        let (exe, _) = installed_prefix(&tmp.path().join("prefix"));
        let rungs = || Rungs {
            workspace: Some(tmp.path()),
            exe: Some(exe.clone()),
            ..Rungs::default()
        };
        let flag = tmp.path().join("flag");
        assert_eq!(
            resolve_catalog_root(Some(flag.clone()), Some(tmp.path().into()), rungs()).unwrap(),
            flag
        );
        assert_eq!(
            resolve_catalog_root(None, Some(tmp.path().into()), rungs()).unwrap(),
            tmp.path()
        );
        let err = resolve_catalog_root(None, Some(tmp.path().join("gone")), rungs())
            .unwrap_err()
            .to_string();
        assert!(err.contains("does not exist"), "{err}");
    }

    /// No rung answers: the error names the ladder, not a bare "pass
    /// --workspace".
    #[test]
    fn no_rung_names_the_ladder() {
        let tmp = tempfile::tempdir().unwrap();
        let err = resolve_catalog_root(
            None,
            None,
            Rungs {
                workspace: Some(tmp.path()),
                ..Rungs::default()
            },
        )
        .unwrap_err()
        .to_string();
        assert!(
            err.contains("--workspace") && err.contains("own SDK root"),
            "{err}"
        );
    }
}
