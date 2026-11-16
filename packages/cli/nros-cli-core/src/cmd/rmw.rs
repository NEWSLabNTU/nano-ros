//! `nros rmw list|show` — which RMW backends exist, what each needs on the
//! host, and which boards it is verified on (RFC-0133 D1/D2, phase-569 W1).
//!
//! Every fact comes from a source the build already reads, never from a table
//! here:
//!
//! - the backends and their names: the RMW index (each provider's
//!   `package.xml` announcement plus its `nros-rmw.toml` descriptor), the one
//!   the build resolves `rmw = "…"` against;
//! - the summary: the first paragraph of the provider's `package.xml`
//!   `<description>` (later paragraphs are contributor notes);
//! - the host side: the descriptor's `[rmw.host] peer` and the SDK index's
//!   `[rmw.<name>] packages`;
//! - the boards: the board support status (`rmws`, generated from the test
//!   matrix), with each board's platform from its descriptor.

use std::path::{Path, PathBuf};

use clap::{Args as ClapArgs, Subcommand};
use eyre::{Result, WrapErr, eyre};
use serde::Serialize;

use crate::{
    cmd::board_support::{Row, Status},
    orchestration::{board_descriptor::BoardCatalog, rmw_index, sdk_index::SdkIndex},
};

/// The `--json` schema names (RFC-0133 D1). Adding a key keeps the version;
/// removing or renaming one is a new version.
pub const LIST_SCHEMA: &str = "nros.rmw.list.v1";
pub const SHOW_SCHEMA: &str = "nros.rmw.show.v1";

#[derive(Debug, Subcommand)]
pub enum Args {
    /// List every RMW backend and the boards it is verified on
    List(ListArgs),
    /// Show one RMW backend: its names, what it needs on the host, and its
    /// boards
    Show(ShowArgs),
}

#[derive(Debug, ClapArgs)]
pub struct ListArgs {
    /// Print one JSON object instead of a table
    #[arg(long)]
    pub json: bool,
    /// Workspace whose own RMW providers are listed too (default: the one the
    /// current directory is in, else the SDK's backends alone)
    #[arg(long, value_name = "DIR")]
    pub workspace: Option<PathBuf>,
}

#[derive(Debug, ClapArgs)]
pub struct ShowArgs {
    /// The RMW name, as `rmw = "…"` spells it (e.g. `zenoh`)
    pub name: String,
    /// Print one JSON object instead of text
    #[arg(long)]
    pub json: bool,
    /// Workspace whose own RMW providers are searched too
    #[arg(long, value_name = "DIR")]
    pub workspace: Option<PathBuf>,
}

/// One RMW backend, as `list` and `show` report it.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct RmwItem {
    /// The canonical name (`rmw = "…"`).
    pub name: String,
    /// Other names the provider answers to.
    pub aliases: Vec<String>,
    /// One sentence from the provider's `package.xml`.
    pub summary: String,
    /// What `nros run` starts on the host for it: `zenoh-router`,
    /// `xrce-agent`, or `none`.
    pub host_peer: String,
    /// The SDK packages `nros setup` installs on the host for it.
    pub host_packages: Vec<String>,
    /// The providing package's name.
    pub package: String,
    /// Boards with a Runtime test over this RMW.
    pub boards: Vec<BoardRef>,
    /// Boards where it is built but not run.
    pub build_only_boards: Vec<BoardRef>,
}

/// A board, as an RMW's row names it.
#[derive(Debug, Clone, Serialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct BoardRef {
    /// The name `nros setup` / `board = "…"` takes.
    pub board: String,
    /// Its platform (`zephyr`, `freertos`, …), when its descriptor is found.
    pub platform: Option<String>,
}

pub fn run(args: Args) -> Result<()> {
    match args {
        Args::List(a) => list(a),
        Args::Show(a) => show(a),
    }
}

fn list(args: ListArgs) -> Result<()> {
    let items = collect(args.workspace)?;
    if args.json {
        let doc = serde_json::json!({ "schema": LIST_SCHEMA, "items": items });
        println!("{}", serde_json::to_string_pretty(&doc)?);
        return Ok(());
    }
    print!("{}", render_list(&items));
    Ok(())
}

fn show(args: ShowArgs) -> Result<()> {
    let items = collect(args.workspace)?;
    let item = find(&items, &args.name)?;
    if args.json {
        let doc = serde_json::json!({ "schema": SHOW_SCHEMA, "item": item });
        println!("{}", serde_json::to_string_pretty(&doc)?);
        return Ok(());
    }
    print!("{}", render_show(item));
    Ok(())
}

/// The item `name` names (canonical or alias), or a refusal listing them all.
pub fn find<'a>(items: &'a [RmwItem], name: &str) -> Result<&'a RmwItem> {
    items
        .iter()
        .find(|i| i.name == name || i.aliases.iter().any(|a| a == name))
        .ok_or_else(|| {
            let known: Vec<&str> = items.iter().map(|i| i.name.as_str()).collect();
            eyre!(
                "no RMW named `{name}`; the known RMWs are: {}\n  \
                 `nros rmw list` shows each one",
                known.join(", ")
            )
        })
}

/// Every backend the index knows, sorted by name.
fn collect(workspace: Option<PathBuf>) -> Result<Vec<RmwItem>> {
    let cwd = std::env::current_dir()?;
    let index = match &workspace {
        Some(ws) => rmw_index::for_workspace(ws)?,
        None => rmw_index::for_dir(&cwd)?,
    };
    let root = crate::cmd::board::catalog_root(workspace.clone())?;
    // The board facts are optional: an out-of-tree SDK root may ship no
    // status file or index, and the backends are still worth listing.
    let status = Status::load(&root).ok();
    let sdk = SdkIndex::load(&root.join("nros-sdk-index.toml")).ok();
    let catalog = BoardCatalog::load(&root).ok();

    let mut items = Vec::new();
    for name in index.known() {
        let resolved = index
            .resolve(&name)
            .map_err(|e| eyre!("resolve RMW `{name}`: {e}"))?;
        let summary = summary_of(&resolved.package_dir.join("package.xml"))?;
        let aliases = resolved
            .names
            .iter()
            .filter(|n| **n != resolved.declared)
            .cloned()
            .collect();
        let host_packages = sdk
            .as_ref()
            .and_then(|s| s.rmw.get(&resolved.declared))
            .map(|e| e.packages.clone())
            .unwrap_or_default();
        let (boards, build_only_boards) = match &status {
            Some(s) => boards_for(&board_rows(s, catalog.as_ref(), &root), &resolved.declared),
            None => (Vec::new(), Vec::new()),
        };
        let host_peer = match resolved.descriptor.host_peer.as_str() {
            "" => "none".to_string(),
            p => p.to_string(),
        };
        items.push(RmwItem {
            name: resolved.declared.clone(),
            aliases,
            summary,
            host_peer,
            host_packages,
            package: resolved.package.clone(),
            boards,
            build_only_boards,
        });
    }
    items.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(items)
}

/// One board and the status rows that report it.
pub struct BoardRows<'a> {
    pub board: BoardRef,
    pub rows: Vec<&'a Row>,
}

/// Every board the catalog knows, each with the status rows `board show` and
/// `nros doctor` read for it ([`Status::rows_for_board`]: the rows that claim
/// its key, else its package's rows). A row no catalog board reports (a board
/// whose descriptor this root does not ship) stands for the boards it names
/// itself, with no platform.
pub fn board_rows<'a>(
    status: &'a Status,
    catalog: Option<&BoardCatalog>,
    root: &Path,
) -> Vec<BoardRows<'a>> {
    let mut out: Vec<BoardRows<'a>> = match catalog {
        Some(c) => catalog_boards(status, c, root),
        None => Vec::new(),
    };
    for row in &status.rows {
        if out
            .iter()
            .any(|b| b.rows.iter().any(|r| std::ptr::eq(*r, row)))
        {
            continue;
        }
        for board in row_names(row) {
            out.push(BoardRows {
                board: BoardRef {
                    board,
                    platform: None,
                },
                rows: vec![row],
            });
        }
    }
    out
}

/// The names a row gives itself: its `board_keys`, else its package's name
/// without the `nros-board-` prefix.
fn row_names(row: &Row) -> Vec<String> {
    if !row.board_keys.is_empty() {
        return row.board_keys.clone();
    }
    vec![
        row.krate
            .strip_prefix("nros-board-")
            .unwrap_or(&row.krate)
            .to_string(),
    ]
}

fn catalog_boards<'a>(
    status: &'a Status,
    catalog: &BoardCatalog,
    root: &Path,
) -> Vec<BoardRows<'a>> {
    catalog
        .descriptors()
        .iter()
        .filter_map(|d| {
            let key = d.names.first()?.clone();
            let krate = crate::orchestration::board_projection::descriptor_dir(d, root)
                .and_then(|dir| dir.file_name().and_then(|n| n.to_str()).map(str::to_string));
            Some(BoardRows {
                board: BoardRef {
                    board: key.clone(),
                    platform: Some(d.platform.kebab().to_string()),
                },
                rows: status.rows_for_board(&key, krate.as_deref()),
            })
        })
        .collect()
}

/// The boards with a Runtime test over `rmw`, then those where it is only
/// built. Infrastructure rows count for neither.
pub fn boards_for(boards: &[BoardRows<'_>], rmw: &str) -> (Vec<BoardRef>, Vec<BoardRef>) {
    let mut runtime = Vec::new();
    let mut built = Vec::new();
    for b in boards {
        let rows = || b.rows.iter().filter(|r| !r.infra);
        let target = if rows().any(|r| r.rmws.runtime.iter().any(|x| x == rmw)) {
            &mut runtime
        } else if rows().any(|r| r.rmws.build_only.iter().any(|x| x == rmw)) {
            &mut built
        } else {
            continue;
        };
        if !target.contains(&b.board) {
            target.push(b.board.clone());
        }
    }
    runtime.sort();
    built.sort();
    (runtime, built)
}

/// The first paragraph of `package.xml`'s `<description>`, on one line.
fn summary_of(package_xml: &Path) -> Result<String> {
    let text = std::fs::read_to_string(package_xml)
        .wrap_err_with(|| format!("read {}", package_xml.display()))?;
    Ok(first_paragraph(&description_text(&text)?))
}

fn description_text(xml: &str) -> Result<String> {
    use quick_xml::{Reader, events::Event};
    let mut reader = Reader::from_str(xml);
    let mut inside = false;
    let mut text = nros_pkg_index::xml::ElementText::default();
    loop {
        match reader.read_event().map_err(|e| eyre!("package.xml: {e}"))? {
            Event::Start(e) if e.name().as_ref() == b"description" => {
                inside = true;
                text.clear();
            }
            Event::Text(t) if inside => text.text(&t)?,
            Event::GeneralRef(r) if inside => text.reference(&r)?,
            Event::End(e) if e.name().as_ref() == b"description" => {
                return Ok(text.take().unwrap_or_default());
            }
            Event::Eof => return Ok(String::new()),
            _ => {}
        }
    }
}

/// Text up to the first blank line, with whitespace collapsed.
pub fn first_paragraph(text: &str) -> String {
    let para: Vec<&str> = text
        .trim()
        .lines()
        .map(str::trim)
        .take_while(|l| !l.is_empty())
        .collect();
    para.join(" ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn board_names(boards: &[BoardRef]) -> String {
    if boards.is_empty() {
        return "-".into();
    }
    boards
        .iter()
        .map(|b| b.board.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}

/// The `list` table: name, host peer, boards, summary.
pub fn render_list(items: &[RmwItem]) -> String {
    if items.is_empty() {
        return "No RMW backends found.\n".into();
    }
    let w = items.iter().map(|i| i.name.len()).max().unwrap_or(4).max(4);
    let pw = items
        .iter()
        .map(|i| i.host_peer.len())
        .max()
        .unwrap_or(9)
        .max(9);
    let mut out = format!("{:<w$}  {:<pw$}  {}\n", "NAME", "HOST PEER", "SUMMARY");
    for i in items {
        out.push_str(&format!(
            "{:<w$}  {:<pw$}  {}\n",
            i.name, i.host_peer, i.summary
        ));
        out.push_str(&format!(
            "{:<w$}  {:<pw$}  verified on: {}\n",
            "",
            "",
            board_names(&i.boards)
        ));
    }
    out.push_str("\n`nros rmw show <name>` shows one backend in full.\n");
    out
}

/// The `show` text.
pub fn render_show(i: &RmwItem) -> String {
    let mut out = format!("{}\n  {}\n\n", i.name, i.summary);
    let line = |k: &str, v: String| format!("  {k:<18} {v}\n");
    if !i.aliases.is_empty() {
        out.push_str(&line("also named", i.aliases.join(", ")));
    }
    out.push_str(&line("selected by", format!("rmw = \"{}\"", i.name)));
    out.push_str(&line("host peer", i.host_peer.clone()));
    out.push_str(&line(
        "host packages",
        if i.host_packages.is_empty() {
            "-".into()
        } else {
            i.host_packages.join(", ")
        },
    ));
    out.push_str(&line("package", i.package.clone()));
    out.push('\n');
    out.push_str("  Verified on (a test runs over it):\n");
    out.push_str(&boards_block(&i.boards));
    if !i.build_only_boards.is_empty() {
        out.push_str("  Built, not run:\n");
        out.push_str(&boards_block(&i.build_only_boards));
    }
    out
}

fn boards_block(boards: &[BoardRef]) -> String {
    if boards.is_empty() {
        return "    -\n".into();
    }
    let w = boards.iter().map(|b| b.board.len()).max().unwrap_or(0);
    boards
        .iter()
        .map(|b| {
            format!(
                "    {:<w$}  {}\n",
                b.board,
                b.platform.as_deref().unwrap_or("-")
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cmd::board_support::RowRmws;

    fn row(krate: &str, keys: &[&str], runtime: &[&str], build_only: &[&str]) -> Row {
        let json = serde_json::json!({
            "key": format!("{krate}@P"), "crate": krate, "matrix_platform": "P",
            "status": "Verified", "built": true, "infra": false, "runs": null,
            "emulator": null, "hardware": null, "cadence": "daily",
            "last_run": null, "stale_from": null, "breadth": "witness",
            "external": null, "board_keys": keys, "book_run": null,
            "book_flash": null, "page": null,
        });
        let mut r: Row = serde_json::from_value(json).unwrap();
        r.rmws = RowRmws {
            runtime: runtime.iter().map(|s| s.to_string()).collect(),
            build_only: build_only.iter().map(|s| s.to_string()).collect(),
        };
        r
    }

    fn b(name: &str) -> BoardRef {
        BoardRef {
            board: name.into(),
            platform: None,
        }
    }

    #[test]
    fn boards_come_from_the_status_rows() {
        let linux = row("nros-board-linux", &[], &["zenoh", "xrce"], &[]);
        let zephyr = row("nros-board-zephyr", &[], &["zenoh"], &["xrce"]);
        let nuttx = row("nros-board-nuttx-qemu", &[], &[], &["zenoh"]);
        let boards = vec![
            BoardRows {
                board: b("linux"),
                rows: vec![&linux],
            },
            // Two boards of one package report its rows.
            BoardRows {
                board: b("native-sim"),
                rows: vec![&zephyr],
            },
            BoardRows {
                board: b("fvp"),
                rows: vec![&zephyr],
            },
            BoardRows {
                board: b("nuttx"),
                rows: vec![&nuttx],
            },
            BoardRows {
                board: b("no-row"),
                rows: vec![],
            },
        ];
        let (rt, bo) = boards_for(&boards, "zenoh");
        assert_eq!(rt, vec![b("fvp"), b("linux"), b("native-sim")]);
        assert_eq!(bo, vec![b("nuttx")]);
        let (rt, bo) = boards_for(&boards, "xrce");
        assert_eq!(rt, vec![b("linux")]);
        assert_eq!(bo, vec![b("fvp"), b("native-sim")]);
        assert_eq!(boards_for(&boards, "uorb"), (vec![], vec![]));
    }

    #[test]
    fn summary_is_the_first_paragraph_only() {
        let xml = "<package><description>\n  The zenoh RMW backend\n  for nano-ros.\n\n  \
                   Contributor notes that must not reach a user.\n</description></package>";
        assert_eq!(
            first_paragraph(&description_text(xml).unwrap()),
            "The zenoh RMW backend for nano-ros."
        );
        assert_eq!(
            first_paragraph(&description_text("<package/>").unwrap()),
            ""
        );
    }

    #[test]
    fn find_takes_an_alias_and_refuses_with_the_known_names() {
        let item = RmwItem {
            name: "xrce".into(),
            aliases: vec!["xrce-dds".into()],
            summary: String::new(),
            host_peer: "xrce-agent".into(),
            host_packages: vec![],
            package: "nros_rmw_xrce".into(),
            boards: vec![],
            build_only_boards: vec![],
        };
        let items = vec![item];
        assert_eq!(find(&items, "xrce-dds").unwrap().name, "xrce");
        let err = find(&items, "zenho").unwrap_err().to_string();
        assert!(
            err.contains("no RMW named `zenho`") && err.contains("xrce"),
            "{err}"
        );
    }

    #[test]
    fn json_schemas_are_pinned() {
        // Renaming a key or the schema name is a new version (RFC-0133 D1).
        assert_eq!(LIST_SCHEMA, "nros.rmw.list.v1");
        assert_eq!(SHOW_SCHEMA, "nros.rmw.show.v1");
        let item = RmwItem {
            name: "zenoh".into(),
            aliases: vec![],
            summary: "s".into(),
            host_peer: "zenoh-router".into(),
            host_packages: vec!["zenohd".into()],
            package: "nros_rmw_zenoh".into(),
            boards: vec![BoardRef {
                board: "linux".into(),
                platform: Some("posix".into()),
            }],
            build_only_boards: vec![],
        };
        let v = serde_json::to_value(&item).unwrap();
        let mut keys: Vec<&str> = v.as_object().unwrap().keys().map(String::as_str).collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            [
                "aliases",
                "boards",
                "build_only_boards",
                "host_packages",
                "host_peer",
                "name",
                "package",
                "summary"
            ]
        );
        assert_eq!(
            v["boards"][0],
            serde_json::json!({"board": "linux", "platform": "posix"})
        );
    }
}
