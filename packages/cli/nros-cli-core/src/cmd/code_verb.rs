//! `nros code list` / `nros code show <code>` (RFC-0133, phase-569 W6).
//!
//! The diagnostic codes a refusal prints (`error[NROS-0001]`), read from the
//! compiled-in code table. `show` prints the same explanation `nros explain
//! <code>` does; `nros explain` stays, because refusals tell users to run it.

use clap::{Args as ClapArgs, Subcommand};
use eyre::Result;
use nros_diagnostic_codes::{ALL, Entry};
use serde::Serialize;

use crate::cmd::frame::{self, Table};

/// The `--json` schema names (RFC-0133 D1). Adding a key keeps the version;
/// removing or renaming one is a new version.
pub const LIST_SCHEMA: &str = "nros.code.list.v1";
pub const SHOW_SCHEMA: &str = "nros.code.show.v1";

#[derive(Debug, Subcommand)]
pub enum Args {
    /// List every diagnostic code with what it reports.
    List(ListArgs),
    /// Show one code: what it reports, where it is raised, and how to fix it.
    Show(ShowArgs),
}

#[derive(Debug, ClapArgs)]
pub struct ListArgs {
    /// List retired codes too.
    #[arg(long)]
    pub all: bool,
    /// Print one JSON object instead of text.
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, ClapArgs)]
pub struct ShowArgs {
    /// The code, as a refusal prints it (`NROS-0001`).
    pub code: String,
    /// Print one JSON object instead of text.
    #[arg(long)]
    pub json: bool,
}

/// One diagnostic code.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CodeItem {
    pub code: &'static str,
    pub name: &'static str,
    /// What it reports, in one line. Empty for a retired code.
    pub title: &'static str,
    pub retired: bool,
    /// Where it can be raised (`Cli`, `Cmake`, …).
    pub surfaces: Vec<&'static str>,
    /// The default remedy line.
    pub remedy: &'static str,
}

pub fn run(args: Args) -> Result<()> {
    match args {
        Args::List(a) => list(a),
        Args::Show(a) => show(a),
    }
}

pub fn item(e: &Entry) -> CodeItem {
    CodeItem {
        code: e.code,
        name: e.name,
        title: e.title,
        retired: e.title.is_empty(),
        surfaces: e.surfaces.iter().map(|s| s.name()).collect(),
        remedy: e.remedy,
    }
}

/// Every code, live ones only unless `all`.
pub fn items(all: bool) -> Vec<CodeItem> {
    ALL.iter().map(item).filter(|i| all || !i.retired).collect()
}

fn list(args: ListArgs) -> Result<()> {
    let items = items(args.all);
    if args.json {
        return frame::print_json(LIST_SCHEMA, "items", &items, &[]);
    }
    print!("{}", render_list(&items));
    Ok(())
}

/// The entry `code` names, or the refusal with the nearest codes.
pub fn find(code: &str) -> Result<&'static Entry> {
    nros_diagnostic_codes::lookup(code).ok_or_else(|| {
        frame::unknown(
            "diagnostic code",
            code,
            ALL.iter().filter(|e| !e.title.is_empty()).map(|e| e.code),
            "nros code list",
        )
    })
}

fn show(args: ShowArgs) -> Result<()> {
    let e = find(&args.code)?;
    if args.json {
        return frame::print_json(SHOW_SCHEMA, "item", &item(e), &[]);
    }
    print!("{}", render_show(e));
    Ok(())
}

pub fn render_list(items: &[CodeItem]) -> String {
    let mut t = Table::new(&["CODE", "WHAT IT REPORTS"], "No diagnostic codes.");
    for i in items {
        let what = if i.retired { "(retired)" } else { i.title };
        t.row(vec![i.code.to_string(), what.to_string()]);
    }
    t.footer("`nros code show <code>` explains one and how to fix it.");
    t.render()
}

/// The Book page of the code table, at `e`'s own section.
pub fn book_page(e: &Entry) -> String {
    format!(
        "reference/diagnostic-codes.html#{}",
        slug(&format!("{}: {}", e.code, e.title))
    )
}

/// mdBook's anchor for a heading: lower case, spaces to `-`, and everything
/// but letters, digits, `-` and `_` dropped.
fn slug(heading: &str) -> String {
    heading
        .trim()
        .to_lowercase()
        .chars()
        .filter_map(|c| match c {
            c if c.is_alphanumeric() || c == '-' || c == '_' => Some(c),
            c if c.is_whitespace() => Some('-'),
            _ => None,
        })
        .collect()
}

pub fn render_show(e: &Entry) -> String {
    let mut out = nros_diagnostic_codes::views::explain_text(e);
    if !out.ends_with('\n') {
        out.push('\n');
    }
    out.push_str(&frame::book_line(&book_page(e)));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_list_holds_every_live_code_and_retired_ones_on_request() {
        let live = ALL.iter().filter(|e| !e.title.is_empty()).count();
        assert_eq!(items(false).len(), live);
        assert_eq!(items(true).len(), ALL.len());
        assert!(render_list(&items(false)).contains("NROS-0001"));
    }

    #[test]
    fn an_unknown_code_is_refused_with_the_list_command() {
        let e = find("NROS-9998").unwrap_err().to_string();
        assert!(e.contains("no diagnostic code named `NROS-9998`"), "{e}");
        assert!(e.contains("`nros code list` names every one"), "{e}");
    }

    #[test]
    fn the_book_link_is_the_code_s_own_heading() {
        let e = nros_diagnostic_codes::lookup("NROS-0001").unwrap();
        let page = book_page(e);
        assert!(
            page.starts_with("reference/diagnostic-codes.html#nros-0001-"),
            "{page}"
        );
        assert!(!page.contains(':') && !page.contains(' '), "{page}");
        // The heading the anchor is made from is the one the Book prints.
        let book = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../../book/src/reference/diagnostic-codes.md"),
        )
        .unwrap();
        assert!(book.contains(&format!("## {}: {}", e.code, e.title)));
    }

    #[test]
    fn json_schema_names_are_pinned() {
        assert_eq!(LIST_SCHEMA, "nros.code.list.v1");
        assert_eq!(SHOW_SCHEMA, "nros.code.show.v1");
    }
}
