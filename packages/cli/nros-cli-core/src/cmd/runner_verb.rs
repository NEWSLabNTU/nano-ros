//! `nros runner list` / `nros runner show <name>` (RFC-0133, phase-569 W6).
//!
//! The runners `nros run` and `nros flash` can start, and the extension
//! commands `nros <verb>` can dispatch to, as the workspace the current
//! directory is in reaches them. Both come from the provider search path,
//! resolved the way the verbs that use them resolve them
//! ([`extension::discover_runners`], [`extension::discover`]), so a package
//! that announces a runner or a command is listed with no change to this verb.

use std::path::Path;

use clap::{Args as ClapArgs, Subcommand};
use eyre::Result;
use serde::Serialize;

use crate::cmd::{
    extension::{self, Listed, ListedRunner},
    frame::{self, Fields, Table},
};

/// The `--json` schema names (RFC-0133 D1). Adding a key keeps the version;
/// removing or renaming one is a new version.
pub const LIST_SCHEMA: &str = "nros.runner.list.v1";
pub const SHOW_SCHEMA: &str = "nros.runner.show.v1";
/// The Book page on runner and command packages.
pub const BOOK_PAGE: &str = "extending/runner-and-command-packages.html";

#[derive(Debug, Subcommand)]
pub enum Args {
    /// List the runners and extension commands this workspace reaches.
    List(JsonArgs),
    /// Show one runner or extension command: its package, verbs and needs.
    Show(ShowArgs),
}

#[derive(Debug, ClapArgs)]
pub struct JsonArgs {
    /// Print one JSON object instead of text.
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, ClapArgs)]
pub struct ShowArgs {
    /// A runner (`qemu`), or a command as it is typed (`router`).
    pub name: String,
    /// Print one JSON object instead of text.
    #[arg(long)]
    pub json: bool,
}

/// What a listed item is.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Runner,
    Command,
}

impl Kind {
    fn as_str(self) -> &'static str {
        match self {
            Kind::Runner => "runner",
            Kind::Command => "command",
        }
    }
}

/// One runner or extension command.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct RunnerItem {
    pub kind: Kind,
    /// A runner's name (`qemu`), or a command's verb (`router`).
    pub name: String,
    /// How it is used, one line per verb: `[image.<id>.run] method =
    /// "qemu"` for a runner, `nros router` for a command.
    pub usage: Vec<String>,
    /// The providing package.
    pub package: String,
    /// A runner's verbs (`run`, `flash`, `console`); empty for a command.
    pub verbs: Vec<&'static str>,
    /// A runner's plan placeholders (`elf`); empty for a command.
    pub needs: Vec<String>,
    /// A command's summary; empty for a runner.
    pub summary: String,
    /// The package it replaces, when an override applies.
    pub overrides: Option<String>,
}

pub fn run(args: Args) -> Result<()> {
    match args {
        Args::List(a) => list(a),
        Args::Show(a) => show(a),
    }
}

/// How a runner is used for each verb it implements: `run` and `flash` are
/// selected by an image's `method`, `console` is opened by `nros monitor`
/// on a board whose console it is.
fn runner_usage(name: &str, verbs: &[&str]) -> Vec<String> {
    verbs
        .iter()
        .map(|v| match *v {
            "console" => "nros monitor, on a board whose console it is".to_string(),
            v => format!("[image.<id>.{v}] method = \"{name}\""),
        })
        .collect()
}

pub fn runner_item(r: ListedRunner) -> RunnerItem {
    RunnerItem {
        kind: Kind::Runner,
        usage: runner_usage(&r.name, &r.verbs),
        name: r.name,
        package: r.package,
        verbs: r.verbs,
        needs: r.needs,
        summary: String::new(),
        overrides: r.overrides,
    }
}

pub fn command_item(c: Listed) -> RunnerItem {
    let name = c
        .spelling
        .rsplit(' ')
        .next()
        .unwrap_or(&c.spelling)
        .to_string();
    RunnerItem {
        kind: Kind::Command,
        name,
        usage: vec![c.spelling],
        package: c.package,
        verbs: Vec::new(),
        needs: Vec::new(),
        summary: c.summary,
        overrides: None,
    }
}

/// Every runner, then every command, the workspace `ws` reaches, with the
/// refusals of the ones that do not resolve.
pub fn items(ws: &Path) -> (Vec<RunnerItem>, Vec<String>) {
    let (runners, mut refused) = extension::discover_runners(ws);
    let (commands, refused_commands) = extension::discover(ws);
    refused.extend(refused_commands);
    let items = runners
        .into_iter()
        .map(runner_item)
        .chain(commands.into_iter().map(command_item))
        .collect();
    (items, refused)
}

fn warn(refused: &[String]) {
    for r in refused {
        eprintln!("warning: {r}");
    }
}

fn list(args: JsonArgs) -> Result<()> {
    let (items, refused) = items(&extension::workspace());
    warn(&refused);
    if args.json {
        return frame::print_json(LIST_SCHEMA, "items", &items, &[]);
    }
    print!("{}", render_list(&items));
    Ok(())
}

/// The item `name` names, a runner first, or the refusal with the nearest
/// names.
pub fn find<'a>(items: &'a [RunnerItem], name: &str) -> Result<&'a RunnerItem> {
    let bare = name.strip_prefix("nros ").unwrap_or(name);
    let bare = bare.strip_prefix("dev ").unwrap_or(bare);
    items
        .iter()
        .find(|i| i.kind == Kind::Runner && i.name == bare)
        .or_else(|| items.iter().find(|i| i.name == bare))
        .ok_or_else(|| {
            frame::unknown(
                "runner or command",
                name,
                items.iter().map(|i| i.name.as_str()),
                "nros runner list",
            )
        })
}

fn show(args: ShowArgs) -> Result<()> {
    let (items, refused) = items(&extension::workspace());
    warn(&refused);
    let item = find(&items, &args.name)?;
    if args.json {
        return frame::print_json(SHOW_SCHEMA, "item", item, &[]);
    }
    print!("{}", render_show(item));
    Ok(())
}

fn joined(v: &[impl AsRef<str>]) -> String {
    if v.is_empty() {
        "-".to_string()
    } else {
        v.iter().map(AsRef::as_ref).collect::<Vec<_>>().join(", ")
    }
}

/// The last column of a row: a runner's verbs and needs, a command's summary.
fn what(i: &RunnerItem) -> String {
    match i.kind {
        Kind::Runner if i.needs.is_empty() => joined(&i.verbs),
        Kind::Runner => format!("{}; needs {}", joined(&i.verbs), joined(&i.needs)),
        Kind::Command => i.summary.clone(),
    }
}

pub fn render_list(items: &[RunnerItem]) -> String {
    let mut t = Table::new(
        &["NAME", "KIND", "PACKAGE", "WHAT IT DOES"],
        "No runners or extension commands found.",
    );
    for i in items {
        t.row(vec![
            i.name.clone(),
            i.kind.as_str().to_string(),
            i.package.clone(),
            what(i),
        ]);
    }
    t.footer("`nros runner show <name>` tells how one is used.");
    t.render()
}

pub fn render_show(i: &RunnerItem) -> String {
    let mut out = frame::show_head(&i.name, &i.summary);
    let mut f = Fields::new();
    f.field("kind", i.kind.as_str())
        .field("package", i.package.as_str());
    if i.kind == Kind::Runner {
        f.field("verbs", joined(&i.verbs))
            .field("needs", joined(&i.needs));
    }
    for (n, u) in i.usage.iter().enumerate() {
        f.field(if n == 0 { "used by" } else { "" }, u.as_str());
    }
    if let Some(o) = &i.overrides {
        f.field("overrides", o.as_str());
    }
    out.push_str(&f.render());
    out.push_str(&frame::book_line(BOOK_PAGE));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../..")
            .canonicalize()
            .unwrap()
    }

    fn copy_dir(from: &Path, to: &Path) {
        std::fs::create_dir_all(to).unwrap();
        for e in std::fs::read_dir(from).unwrap() {
            let e = e.unwrap();
            let dest = to.join(e.file_name());
            if e.file_type().unwrap().is_dir() {
                copy_dir(&e.path(), &dest);
            } else {
                std::fs::copy(e.path(), dest).unwrap();
            }
        }
    }

    /// A workspace holding the fixture runner and command packages. The
    /// fixture tree itself is `COLCON_IGNORE`d, so they are copied out.
    fn fixture_ws() -> tempfile::TempDir {
        let src =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/extension_packages/src");
        let ws = tempfile::tempdir().unwrap();
        for pkg in ["fixture_runner", "fixture_command"] {
            copy_dir(&src.join(pkg), &ws.path().join("src").join(pkg));
        }
        ws
    }

    /// RFC-0133 acceptance: a runner and a command a workspace package
    /// announces are listed, with no CLI edit.
    #[test]
    fn a_workspace_s_runner_and_command_are_listed() {
        let ws = fixture_ws();
        let (items, _) = items(&ws.path().canonicalize().unwrap());
        let runner = items
            .iter()
            .find(|i| i.kind == Kind::Runner && i.package == "fixture_runner")
            .unwrap_or_else(|| panic!("no fixture runner in {items:?}"));
        assert!(!runner.verbs.is_empty(), "{runner:?}");
        assert!(render_list(&items).contains(&runner.name));
        let command = items
            .iter()
            .find(|i| i.kind == Kind::Command && i.package == "fixture_command")
            .unwrap_or_else(|| panic!("no fixture command in {items:?}"));
        assert!(command.usage[0].starts_with("nros "), "{command:?}");
        assert_eq!(find(&items, &command.usage[0]).unwrap(), command);
    }

    #[test]
    fn the_in_tree_runners_are_listed() {
        let (items, refused) = items(&repo());
        assert!(refused.is_empty(), "{refused:?}");
        for r in ["qemu", "native", "west"] {
            let i = find(&items, r).unwrap();
            assert_eq!(i.kind, Kind::Runner);
            assert!(i.verbs.contains(&"run"), "{i:?}");
        }
        let qemu = find(&items, "qemu").unwrap();
        assert!(qemu.needs.iter().any(|n| n == "elf"), "{qemu:?}");
        assert!(
            render_show(qemu).contains("[image.<id>.run] method = \"qemu\""),
            "{}",
            render_show(qemu)
        );
        let west = find(&items, "west").unwrap();
        assert_eq!(west.usage.len(), 2, "{west:?}");
    }

    #[test]
    fn an_unknown_name_is_refused_with_the_nearest() {
        let (items, _) = items(&repo());
        let e = find(&items, "qem").unwrap_err().to_string();
        assert!(e.contains("`qemu`"), "{e}");
        assert!(e.contains("`nros runner list` names every one"), "{e}");
    }

    #[test]
    fn the_book_page_exists() {
        let md = repo()
            .join("book/src")
            .join(BOOK_PAGE.replace(".html", ".md"));
        assert!(md.is_file(), "{}", md.display());
    }

    #[test]
    fn json_schema_names_are_pinned() {
        assert_eq!(LIST_SCHEMA, "nros.runner.list.v1");
        assert_eq!(SHOW_SCHEMA, "nros.runner.show.v1");
    }
}
