//! `nros board` — board crate introspection.
//!
//! * `list` — every board key in the catalog, with its crate, platform, and
//!   the support status and cadence of the board support registry (phase-522
//!   W5); `--json` (phase-569 W5b).
//! * `show <name>` — a board's evidence (status, where it runs, cadence,
//!   breadth) and every table of its `nros-board.toml` as written, then its
//!   Book line; `--json` (phase-569 W5b).
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
    /// List every board key, with its crate, platform, support status and
    /// cadence
    List(ListArgs),
    /// Show one board: its support evidence and every table of its
    /// descriptor
    // RFC-0114.
    Show(ShowArgs),
    /// Print a board's resolved descriptor as JSON.
    Info(InfoArgs),
    /// Scaffold a new board package.
    // phase-375 W2/W9.
    New(crate::cmd::board_new::NewArgs),
    /// Project a board descriptor into cmake variables.
    ///
    /// Written per configure into the build directory by
    /// `nano_ros_use_board()`; never committed. The descriptor is the single
    /// authored source, and this is the mechanical projection of it.
    // RFC-0064 R5 D4.
    #[command(hide = true)]
    CmakeVars(CmakeVarsArgs),
    /// Print a board's resolved network identity defaults.
    ///
    /// The board's `[board.net]` over the platform default — the one value the
    /// board crate's `Config::default()`, its cargo-road C `NROS_APP_CONFIG`
    /// and its cmake-road C `NROS_APP_CONFIG` all use.
    // phase-484 W4b.
    #[command(hide = true)]
    Net(NetArgs),
}

#[derive(Debug, ClapArgs)]
pub struct NetArgs {
    /// Board key.
    #[arg(add = clap_complete::ArgValueCandidates::new(crate::cmd::complete::boards))]
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
    /// An image's resolved network (`NROS_IMAGE_NET`): its primary interface
    /// replaces the board default.
    ///
    /// Empty or absent keeps the board default.
    // RFC-0103 D10f. It replaced `--ip-last` / `--mac-last`, which let a
    // build row restate one octet of an address the image's `system.toml`
    // now owns (phase-497 W2).
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
    /// Print one JSON object instead of a table
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, ClapArgs)]
pub struct ShowArgs {
    /// Board: a board key (`mps2-an385-freertos`), a package
    /// (`nros-board-zephyr`), a registry key (`nros-board-zephyr@Fvp`) or an
    /// evidence page name (`fvp`).
    #[arg(add = clap_complete::ArgValueCandidates::new(crate::cmd::complete::boards))]
    pub name: String,
    /// nano-ros SDK root holding the board catalog. Default:
    /// `$NROS_WORKSPACE_ROOT`, a checkout enclosing cwd, then this `nros`
    /// binary's own SDK root.
    #[arg(long)]
    pub workspace: Option<PathBuf>,
    /// Print one JSON object instead of text
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, ClapArgs)]
pub struct InfoArgs {
    /// Board key.
    ///
    /// Resolves to a board CRATE (`packages/boards/nros-board-<name>/`) or to
    /// a conf BUNDLE under a family crate
    /// (`packages/boards/nros-board-<family>/boards/<name>/`) — e.g.
    /// `fvp-aemv8r-smp` is `nros-board-zephyr/boards/fvp-aemv8r-smp/`.
    // Conf bundles resolve since phase-337 W9.a.
    #[arg(add = clap_complete::ArgValueCandidates::new(crate::cmd::complete::boards))]
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
    #[arg(add = clap_complete::ArgValueCandidates::new(crate::cmd::complete::boards))]
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
        Args::Show(args) => show(args),
        Args::Info(args) => info(args),
        Args::CmakeVars(args) => cmake_vars(args),
        Args::Net(args) => net(args),
        Args::New(args) => crate::cmd::board_new::run(args),
    }
}

/// The `--json` schema names (RFC-0133 D1). Adding a key keeps the version;
/// removing or renaming one is a new version.
pub const LIST_SCHEMA: &str = "nros.board.list.v1";
pub const SHOW_SCHEMA: &str = "nros.board.show.v1";

/// The page that lists every board; a `show` links the board's own section of
/// the support levels page when the registry names one.
pub const BOOK_PAGE: &str = "reference/supported-boards.html";

/// One board key, as `list` reports it.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct BoardItem {
    /// The key a project writes as `board = "…"`.
    pub key: String,
    /// The descriptor's other names.
    pub aliases: Vec<String>,
    /// The board package (crate) the descriptor lives in.
    #[serde(rename = "crate")]
    pub krate: String,
    /// The platform the board builds for, as the descriptor names it.
    pub platform: String,
    /// The support status word of each registry row for it, today.
    pub status: Vec<String>,
    /// The cadence of each registry row for it.
    pub cadence: Vec<String>,
}

/// The board package a descriptor lives in: the nearest directory at or
/// above its `nros-board.toml`, inside the checkout, that holds a
/// `Cargo.toml` or a `package.xml` (a conf bundle sits under its family
/// crate); else the
/// descriptor's `board_crate`; else `-`.
fn crate_of(d: &BoardDescriptor, root: &Path) -> String {
    let found = board_projection::descriptor_dir(d, root).and_then(|dir| {
        dir.ancestors()
            .take_while(|a| *a != root && !a.join(".git").exists())
            .find(|a| a.join("Cargo.toml").is_file() || a.join("package.xml").is_file())
            .and_then(|a| a.file_name())
            .and_then(|n| n.to_str())
            .map(str::to_string)
    });
    found
        .or_else(|| d.board_crate.clone())
        .unwrap_or_else(|| "-".into())
}

fn platform_of(d: &BoardDescriptor) -> String {
    d.platform_name
        .clone()
        .unwrap_or_else(|| d.platform.kebab().to_string())
}

/// Every board key in the catalog under `root`, sorted.
pub fn board_items(
    catalog: &BoardCatalog,
    root: &Path,
    status: &crate::cmd::board_support::Status,
    today: &str,
) -> Vec<BoardItem> {
    let mut items: Vec<BoardItem> = catalog
        .descriptors()
        .iter()
        .filter_map(|d| {
            let (key, aliases) = d.names.split_first()?;
            let krate = crate_of(d, root);
            // Issue 2995 -- the rows that CLAIM this key (`board_keys`), else
            // its package's, as `nros doctor` reads them. Matching by name
            // instead put every row of every package sharing the word in one
            // cell (`linux`: the host, the musl cross board and the descriptor
            // directory).
            let rows = status.rows_for_board(key, Some(&krate));
            Some(BoardItem {
                key: key.clone(),
                aliases: aliases.to_vec(),
                platform: platform_of(d),
                status: rows.iter().map(|r| r.status_on(today)).collect(),
                cadence: rows.iter().map(|r| r.cadence_text()).collect(),
                krate,
            })
        })
        .collect();
    items.sort_by(|a, b| a.key.cmp(&b.key));
    items.dedup_by(|a, b| a.key == b.key);
    items
}

/// The `list` table.
pub fn render_list(items: &[BoardItem]) -> String {
    let mut t = crate::cmd::frame::Table::new(
        &["BOARD", "CRATE", "PLATFORM", "STATUS", "CADENCE"],
        "No boards found.",
    );
    let words = |v: &[String]| {
        if v.is_empty() {
            "-".to_string()
        } else {
            v.join("; ")
        }
    };
    for i in items {
        t.row(vec![
            i.key.clone(),
            i.krate.clone(),
            i.platform.clone(),
            words(&i.status),
            words(&i.cadence),
        ]);
    }
    t.footer("`nros board show <board>` shows one board: its support evidence and its descriptor.");
    t.render()
}

/// Every board key `nros board list` reports for `workspace` (or the
/// directory the command runs in), with the notes it prints after the table.
pub(crate) fn collect(workspace: Option<PathBuf>) -> Result<(Vec<BoardItem>, Vec<String>)> {
    let root = catalog_root(workspace)?;
    let catalog = BoardCatalog::load(&root).map_err(|e| eyre!("load board catalog: {e}"))?;
    // phase-522 W5 — the status word and cadence of every row, read from the
    // registry this root ships.
    let status = crate::cmd::board_support::Status::load(&root)?;
    let (notes, _other) = crate::cmd::board_support::other_copy(&root, own_sdk_root().as_deref());
    let today = crate::cmd::board_support::today();
    Ok((board_items(&catalog, &root, &status, &today), notes))
}

fn list(args: ListArgs) -> Result<()> {
    let (items, notes) = collect(args.workspace)?;
    if args.json {
        return crate::cmd::frame::print_json(LIST_SCHEMA, "items", &items, &[]);
    }
    print!("{}", render_list(&items));
    for n in notes {
        println!("{n}");
    }
    Ok(())
}

/// The SDK root this `nros` binary would answer from with no checkout around:
/// the decision step of phase-522 W5 compares the registry it read with this
/// one, so a released `nros` run inside a newer checkout says which it read.
fn own_sdk_root() -> Option<PathBuf> {
    crate::orchestration::nano_ros_root::resolve_from(crate::orchestration::nano_ros_root::Rungs {
        explicit: None,
        workspace: None,
        exe: nros_launcher::checkout::canonical_current_exe(),
        built_in: Some(nros_launcher::checkout::built_in_dir().to_path_buf()),
    })
}

/// One registry row, as `show` reports it.
#[derive(Debug, Clone, Serialize)]
pub struct SupportItem {
    pub key: String,
    pub status: String,
    pub runs_on: String,
    pub cadence: String,
    pub breadth: String,
    /// The Book page with the board's evidence.
    pub book: Option<String>,
}

/// One board, as `show` reports it.
#[derive(Debug, Clone, Serialize)]
pub struct BoardShow {
    pub key: String,
    pub aliases: Vec<String>,
    #[serde(rename = "crate")]
    pub krate: Option<String>,
    pub platform: Option<String>,
    /// The descriptor file, and every table in it as written.
    pub descriptor: Option<PathBuf>,
    pub tables: Option<serde_json::Value>,
    pub support: Vec<SupportItem>,
}

/// The Book page of a registry row's evidence (`book/src/x.md#y` →
/// `x.html#y`).
fn evidence_page(page: &str) -> String {
    let p = page.trim_start_matches("book/src/");
    match p.split_once('#') {
        Some((f, frag)) => format!("{}#{frag}", f.replace(".md", ".html")),
        None => p.replace(".md", ".html"),
    }
}

/// The descriptor's tables as text: each table under its `[path]` header,
/// its keys as the file writes them. A table a new design adds is printed
/// like any other, with no edit here. A multi-line string prints as its line
/// count.
pub fn render_tables(v: &toml::Value) -> String {
    fn scalar(v: &toml::Value) -> String {
        match v {
            toml::Value::String(s) if s.contains('\n') => {
                format!("({} lines)", s.lines().count())
            }
            other => other.to_string(),
        }
    }
    /// `header` is how the table is opened: `[path]`, or `[[path]]` for an
    /// element of an array of tables (whose sub-tables are still `[path.k]`).
    fn walk(path: &str, header: &str, t: &toml::value::Table, out: &mut String) {
        let plain: Vec<(&String, &toml::Value)> =
            t.iter().filter(|(_, v)| !is_table_like(v)).collect();
        if !plain.is_empty() && !path.is_empty() {
            out.push_str(&format!("  {header}\n"));
            let w = plain
                .iter()
                .map(|(k, _)| k.chars().count())
                .max()
                .unwrap_or(0);
            for (k, v) in plain {
                out.push_str(&format!("    {k:<w$} = {}\n", scalar(v)));
            }
        }
        for (k, v) in t {
            let p = if path.is_empty() {
                k.clone()
            } else {
                format!("{path}.{k}")
            };
            match v {
                toml::Value::Table(sub) => walk(&p, &format!("[{p}]"), sub, out),
                toml::Value::Array(a) if a.iter().all(|x| x.is_table()) && !a.is_empty() => {
                    for x in a {
                        if let toml::Value::Table(sub) = x {
                            walk(&p, &format!("[[{p}]]"), sub, out);
                        }
                    }
                }
                _ => {}
            }
        }
    }
    fn is_table_like(v: &toml::Value) -> bool {
        match v {
            toml::Value::Table(_) => true,
            toml::Value::Array(a) => !a.is_empty() && a.iter().all(|x| x.is_table()),
            _ => false,
        }
    }
    let mut out = String::new();
    if let toml::Value::Table(t) = v {
        walk("", "", t, &mut out);
    }
    out
}

/// The `show` text.
pub fn render_show(b: &BoardShow) -> String {
    let mut out = crate::cmd::frame::show_head(&b.key, "");
    let mut f = crate::cmd::frame::Fields::new();
    if !b.aliases.is_empty() {
        f.field("also named", b.aliases.join(", "));
    }
    f.field("selected by", format!("board = \"{}\"", b.key));
    if let Some(k) = &b.krate {
        f.field("crate", k.clone());
    }
    if let Some(p) = &b.platform {
        f.field("platform", p.clone());
    }
    for s in &b.support {
        f.field("support", format!("{} ({})", s.status, s.key))
            .field("runs on", s.runs_on.clone())
            .field("cadence", s.cadence.clone())
            .field("breadth", s.breadth.clone());
    }
    out.push_str(&f.render());
    if let (Some(path), Some(tables)) = (&b.descriptor, &b.tables)
        && let Ok(v) = serde_json::from_value::<toml::Value>(tables.clone())
    {
        out.push_str(&format!("\n  Descriptor ({}):\n", path.display()));
        out.push_str(&render_tables(&v));
    }
    // A row's evidence page only when one row speaks for the board: a crate
    // with several rows has one section per row, not one for this key.
    let page = match b.support.as_slice() {
        [one] => one.book.clone(),
        _ => None,
    }
    .unwrap_or_else(|| BOOK_PAGE.to_string());
    out.push_str(&crate::cmd::frame::book_line(&page));
    out
}

/// `nros board show <name>` — a board's support evidence and its descriptor.
fn show(args: ShowArgs) -> Result<()> {
    let root = catalog_root(args.workspace)?;
    let status = crate::cmd::board_support::Status::load(&root)?;
    let runs = crate::cmd::board_support::LaneRuns::load(&root);
    let (notes, _other) = crate::cmd::board_support::other_copy(&root, own_sdk_root().as_deref());
    let catalog = BoardCatalog::load(&root).map_err(|e| eyre!("load board catalog: {e}"))?;
    let desc = catalog
        .descriptors()
        .iter()
        .find(|d| d.answers_to(&args.name));
    let krate = desc.map(|d| crate_of(d, &root));
    // A board key reads the rows that claim it, as `list` does (issue 2995);
    // a package or registry key reads the rows that answer to that name.
    let mut rows = match desc {
        Some(_) => status.rows_for_board(&args.name, krate.as_deref()),
        None => status.rows_named(&args.name),
    };
    if rows.is_empty()
        && let Some(k) = &krate
    {
        rows = status.rows_for_crate(k);
    }
    if desc.is_none() && rows.is_empty() {
        let known: Vec<&str> = catalog
            .descriptors()
            .iter()
            .flat_map(|d| d.names.iter().map(String::as_str))
            .chain(status.rows.iter().map(|r| r.key.as_str()))
            .collect();
        return Err(crate::cmd::frame::unknown(
            "board",
            &args.name,
            known,
            "nros board list",
        ));
    }
    let today = crate::cmd::board_support::today();
    let support = rows
        .iter()
        .map(|r| {
            let mut cadence = r.cadence_text();
            if let Some(t) = r.last_run_text(runs.as_ref()) {
                cadence = format!("{cadence}; last run {t}");
            }
            SupportItem {
                key: r.key.clone(),
                status: r.status_on(&today),
                runs_on: r.runs_on(),
                cadence,
                breadth: r.breadth.clone(),
                book: r.page.as_deref().map(evidence_page),
            }
        })
        .collect();
    let path = desc.and_then(|d| {
        board_projection::descriptor_dir(d, &root).map(|dir| dir.join("nros-board.toml"))
    });
    let tables = match &path {
        Some(p) => {
            let text = fs::read_to_string(p).wrap_err_with(|| format!("read {}", p.display()))?;
            let v: toml::Value =
                toml::from_str(&text).wrap_err_with(|| format!("parse {}", p.display()))?;
            Some(serde_json::to_value(v)?)
        }
        None => None,
    };
    let (key, aliases) = match desc.and_then(|d| d.names.split_first()) {
        Some((k, a)) => (k.clone(), a.to_vec()),
        None => (args.name.clone(), Vec::new()),
    };
    let item = BoardShow {
        key,
        aliases,
        krate: krate.or_else(|| rows.first().map(|r| r.krate.clone())),
        platform: desc.map(platform_of),
        descriptor: path,
        tables,
        support,
    };
    if args.json {
        return crate::cmd::frame::print_json(SHOW_SCHEMA, "item", &item, &[]);
    }
    print!("{}", render_show(&item));
    for n in notes {
        println!("{n}");
    }
    Ok(())
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
    /// The platform the board names: its `platform` as authored.
    platform: String,
    /// The built-in family the board builds as. Equal to `platform` for a
    /// built-in platform; for an out-of-tree one, the family its
    /// `inherits` chain reaches (issue 2310).
    platform_family: &'static str,
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

/// The board catalog a build in `ws` resolves against: `root`'s boards,
/// `$NROS_EXTRA_BOARD_PATH`, and the board packages the workspace carries.
///
/// # Errors
///
/// A descriptor that does not parse, or one with no `package.xml` announcing
/// it (`BoardCatalog::load_with_packages`).
pub(crate) fn workspace_catalog(root: &Path, ws: &Path) -> Result<BoardCatalog> {
    let members = crate::builder::discover::cargo_members_or_walk(ws);
    let pkg_dirs: Vec<PathBuf> = crate::builder::discover::discover(ws, &members)
        .map(|found| found.packages.iter().map(|p| p.dir.clone()).collect())
        .unwrap_or_default();
    BoardCatalog::load_with_packages(root, &pkg_dirs).map_err(|e| eyre!("board catalog: {e}"))
}

/// The board catalog for a directory `dir` in (or at the root of) a workspace:
/// the one [`workspace_catalog`] `nros build` resolves against, for the
/// workspace `dir` sits in. Outside any workspace, the checkout's boards and
/// `$NROS_EXTRA_BOARD_PATH`.
///
/// Issue 2637 -- every verb that resolves a board for a build asks THIS, so a
/// workspace's own board package resolves for all of them or for none.
/// `nros dev image board-facts` (run inside the west configure `nros build`
/// spawns) used to load the checkout's catalog alone, so a board `nros build`
/// found in the workspace failed there unless `$NROS_EXTRA_BOARD_PATH` named it.
///
/// # Errors
///
/// As [`workspace_catalog`].
pub(crate) fn catalog_for_dir(root: &Path, dir: &Path) -> Result<BoardCatalog> {
    match crate::cmd::image_facts::workspace_root_from(dir) {
        Some(ws) => workspace_catalog(root, &ws),
        None => BoardCatalog::load(root).map_err(|e| eyre!("board catalog: {e}")),
    }
}

/// `nros check <board>` (issue 2294): what a board package must get right
/// before an image is built on it.
///
/// # Errors
///
/// The first step that fails, named.
pub(crate) fn board_check(root: &Path, ws: &Path, name: &str) -> Result<()> {
    board_check_in(&workspace_catalog(root, ws)?, root, name)
}

/// [`board_check`] over a loaded catalog. Loading it already parsed every
/// descriptor and required the `package.xml` beside each to announce a board
/// (RFC-0064 R5 D5); this adds the rest of what a board package states:
///
/// - **resolve**: the name resolves through the catalog, as a build's
///   `board = "<name>"` does (`nros board info` reads the same answer);
/// - **announce**: the board names `package.xml` announces equal the names
///   its descriptor declares (`check-provider-announcements` A2, for one
///   package and outside the tree);
/// - **net**: `[board.net]` resolves over the platform default (what
///   `nros dev board net` prints and the image's network starts from);
/// - **project**: a `[board.zephyr]` board projects into the cmake variables
///   `nano_ros_use_board()` reads (what `nros board info` prints).
pub(crate) fn board_check_in(catalog: &BoardCatalog, root: &Path, name: &str) -> Result<()> {
    let say = |step: &str, what: String| println!("ok    {step:<10} {what}");
    let board = catalog
        .descriptors()
        .iter()
        .find(|d| d.answers_to(name))
        .ok_or_else(|| {
            eyre!(
                "no board named `{name}` in the board catalog. `nros board list` names the \
                 boards; an out-of-tree board is reached through $NROS_EXTRA_BOARD_PATH."
            )
        })?;
    let dir = board_projection::descriptor_dir(board, root)
        .ok_or_else(|| eyre!("board `{name}` has no recorded descriptor path"))?;
    let descriptor = dir.join("nros-board.toml");
    say(
        "resolve",
        format!("board `{name}` is {}", descriptor.display()),
    );

    // Every `[[board]]` entry of this one descriptor file.
    let mut declared: Vec<String> = catalog
        .descriptors()
        .iter()
        .filter(|d| board_projection::descriptor_dir(d, root).as_deref() == Some(dir.as_path()))
        .flat_map(|d| d.names.iter().cloned())
        .collect();
    let scan = cargo_nano_ros::provider_scan::scan_root(&dir, 0)
        .map_err(|e| eyre!("reading {}: {e}", dir.join("package.xml").display()))?;
    let mut announced: Vec<String> = scan
        .providers
        .iter()
        .filter(|p| p.dir == dir)
        .flat_map(|p| p.provides.iter())
        .filter(|pr| pr.kind == "board")
        .map(|pr| pr.name.clone())
        .collect();
    declared.sort();
    declared.dedup();
    announced.sort();
    announced.dedup();
    if declared != announced {
        eyre::bail!(
            "{} announces boards {announced:?}, but {} declares {declared:?}; the two must \
             name the same boards",
            dir.join("package.xml").display(),
            descriptor.display()
        );
    }
    say(
        "announce",
        format!("package.xml announces {announced:?}, as the descriptor declares"),
    );

    use nros_board_common::board_net::{ResolvedNet, resolve as resolve_net};
    let n = resolve_net(&descriptor, Some(name), ResolvedNet::platform_default())
        .map_err(|e| eyre!("{}: {e}", descriptor.display()))?;
    let dotted = |o: &[u8]| o.iter().map(u8::to_string).collect::<Vec<_>>().join(".");
    say(
        "net",
        format!("ip {} gateway {}", dotted(&n.ip), dotted(&n.gateway)),
    );

    if board.zephyr.is_some() {
        let module_dir = dir.join("<build-dir>/nros-board-rust-support");
        board_projection::project(board, &dir, &module_dir)?;
        say("project", "cmake variables for nano_ros_use_board()".into());
    } else {
        say(
            "project",
            "skipped: no [board.zephyr], so no cmake projection".into(),
        );
    }
    println!("board `{name}`: OK");
    Ok(())
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
    // RFC-0132 D4 (phase-560 W9.e) -- the image's CAN frame device, decoded
    // from the same text by the same codec the board crates use at boot, for
    // the cmake road's hand-off (`nros_entry_can_image`).
    let can_device = match args.image_net.as_deref().filter(|t| !t.is_empty()) {
        Some(t) => nros_board_common::can_image::device(t)
            .map_err(|e| eyre!("--image-net: {e}"))?
            .map(str::to_string),
        None => None,
    };
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
            // The router an image on this board dials when nothing states one:
            // the one rule (`board_net::default_locator_over`) the board crates
            // and `nros run` read too, over the image's network. "" where the
            // RTOS's own configuration owns it (Zephyr).
            let desc = dir.join("nros-board.toml");
            let locator =
                nros_board_common::board_net::default_locator_over(&desc, Some(&args.name), &n)
                    .map_err(|e| eyre!("{}: {e}", desc.display()))?;
            println!(
                "set(NROS_BOARD_NET_DEFAULT_LOCATOR \"{}\")",
                locator.unwrap_or_default()
            );
            // RFC-0132 D4 -- "" when the image names no device (or the mock).
            println!(
                "set(NROS_BOARD_NET_CAN_DEVICE \"{}\")",
                can_device.unwrap_or_default()
            );
            // RFC-0132 D4 -- the board's stack, which picks the backend the
            // device goes to (`socketcan` on "host").
            let stack = nros_board_common::board_net::stack(&desc, Some(&args.name))
                .map_err(|e| eyre!("{}: {e}", desc.display()))?;
            println!(
                "set(NROS_BOARD_NET_STACK \"{}\")",
                stack.unwrap_or_default()
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
        platform: board
            .platform_name
            .clone()
            .unwrap_or_else(|| board.platform.kebab().to_string()),
        platform_family: board.platform.kebab(),
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

#[cfg(test)]
mod view_tests {
    /// RFC-0133 D2: a section a new design adds is printed with no CLI edit.
    #[test]
    fn board_show_prints_every_descriptor_table_as_written() {
        let v: toml::Value = toml::from_str(
            r#"
names = ["my-board", "mb"]
platform = "zephyr"
[board.can]
bitrate = 500000
driver = "mcan"
[board.run.fvp]
cores = 2
[[board.flash.region]]
name = "boot"
[board.entry]
signature = """
#[unsafe(no_mangle)]
fn main() -> ! {"""
"#,
        )
        .unwrap();
        let text = super::render_tables(&v);
        for want in [
            "  [board.can]\n    bitrate = 500000\n    driver  = \"mcan\"\n",
            "  [board.run.fvp]\n    cores = 2\n",
            "  [[board.flash.region]]\n    name = \"boot\"\n",
            "    signature = (2 lines)\n",
        ] {
            assert!(text.contains(want), "missing {want:?} in\n{text}");
        }
    }

    #[test]
    fn evidence_page_is_a_book_url_path() {
        assert_eq!(
            super::evidence_page("book/src/reference/board-support-levels.md#fvp"),
            "reference/board-support-levels.html#fvp"
        );
    }

    /// Issue 2995: a board key's status is the registry rows that claim it
    /// (else its package's), never every row whose name shares a word.
    #[test]
    fn each_board_key_lists_only_its_own_registry_rows() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
        let catalog =
            crate::orchestration::board_descriptor::BoardCatalog::load_with_extra(&root, &[])
                .expect("catalog");
        let status = crate::cmd::board_support::Status::load(&root).expect("registry");
        let items = super::board_items(&catalog, &root, &status, "2026-11-27");
        let item = |k: &str| items.iter().find(|i| i.key == k).cloned().expect(k);
        let linux = item("linux");
        assert_eq!(linux.status.len(), 1, "one claiming row: {linux:?}");
        assert_eq!(linux.cadence.len(), 1, "{linux:?}");
        assert!(!linux.status[0].contains("Infrastructure"), "{linux:?}");
        let freertos = item("freertos");
        assert!(!freertos.status.is_empty(), "{freertos:?}");
        assert!(
            freertos
                .status
                .iter()
                .all(|s| !s.contains("Infrastructure")),
            "its package's evidence row, not a platform page's: {freertos:?}"
        );
    }

    /// RFC-0133 D1: renaming a key or the schema name is a new version.
    #[test]
    fn board_json_schemas_are_pinned() {
        assert_eq!(super::LIST_SCHEMA, "nros.board.list.v1");
        assert_eq!(super::SHOW_SCHEMA, "nros.board.show.v1");
        let keys = |v: serde_json::Value| {
            let mut k: Vec<String> = v.as_object().unwrap().keys().cloned().collect();
            k.sort_unstable();
            k
        };
        let item = super::BoardItem {
            key: "k".into(),
            aliases: vec![],
            krate: "c".into(),
            platform: "p".into(),
            status: vec![],
            cadence: vec![],
        };
        assert_eq!(
            keys(serde_json::to_value(&item).unwrap()),
            ["aliases", "cadence", "crate", "key", "platform", "status"]
        );
        let show = super::BoardShow {
            key: "k".into(),
            aliases: vec![],
            krate: None,
            platform: None,
            descriptor: None,
            tables: None,
            support: vec![super::SupportItem {
                key: "c@p".into(),
                status: "Runs".into(),
                runs_on: "qemu".into(),
                cadence: "daily".into(),
                breadth: "b".into(),
                book: None,
            }],
        };
        let v = serde_json::to_value(&show).unwrap();
        assert_eq!(
            keys(v.clone()),
            [
                "aliases",
                "crate",
                "descriptor",
                "key",
                "platform",
                "support",
                "tables"
            ]
        );
        assert_eq!(
            keys(v["support"][0].clone()),
            ["book", "breadth", "cadence", "key", "runs_on", "status"]
        );
    }
}
