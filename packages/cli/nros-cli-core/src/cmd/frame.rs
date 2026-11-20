//! The shared frame of the discovery verbs (RFC-0133 D1, phase-569 W5).
//!
//! Every `<noun> list` / `<noun> show` renders through this module, so they
//! read alike:
//!
//! - `list` prints a table (`Table`), or with `--json` one object
//!   `{"schema": "nros.<noun>.list.v1", "items": [...]}` (`print_json`);
//! - `show` prints the item's name, its summary, its fields (`Fields`) and
//!   last a Book line that links the page explaining it (`book_line`), or
//!   with `--json` `{"schema": "nros.<noun>.show.v1", "item": {...}}`;
//! - a name that matches nothing is refused with the nearest known names and
//!   the `list` command that names them all (`unknown`).
//!
//! A schema keeps its version while keys are only added; removing or renaming
//! a key is a new version.

use eyre::Report;
use serde::Serialize;

/// The Book's base URL. It is the CLI workspace's `documentation` field
/// (`packages/cli/Cargo.toml`), which `build.rs` compiles in; a test checks
/// `book/book.toml`'s `site-url` against its path. Empty when the crate was
/// built with no workspace manifest to read.
pub const BOOK_BASE: &str = env!("NROS_BOOK_URL");

/// The Book URL of `page` (a path under the Book's root, such as
/// `reference/supported-boards.html`, optionally with a `#fragment`).
pub fn book_url(page: &str) -> String {
    format!("{}{}", BOOK_BASE, page.trim_start_matches('/'))
}

/// The last line of a `show`: the page that explains the item. Nothing when
/// the build had no Book URL.
pub fn book_line(page: &str) -> String {
    if BOOK_BASE.is_empty() {
        return String::new();
    }
    format!("\n  Book: {}\n", book_url(page))
}

/// The `--json` document of a `list` (`key` = `items`) or a `show`
/// (`key` = `item`), with any extra top-level keys the noun adds.
pub fn json_doc<T: Serialize>(
    schema: &str,
    key: &str,
    value: &T,
    extra: &[(&str, serde_json::Value)],
) -> eyre::Result<serde_json::Value> {
    let mut doc = serde_json::Map::new();
    doc.insert("schema".into(), schema.into());
    for (k, v) in extra {
        doc.insert((*k).into(), v.clone());
    }
    doc.insert(key.into(), serde_json::to_value(value)?);
    Ok(serde_json::Value::Object(doc))
}

/// Print `json_doc` to stdout.
pub fn print_json<T: Serialize>(
    schema: &str,
    key: &str,
    value: &T,
    extra: &[(&str, serde_json::Value)],
) -> eyre::Result<()> {
    println!(
        "{}",
        serde_json::to_string_pretty(&json_doc(schema, key, value, extra)?)?
    );
    Ok(())
}

/// A `list` table: a header row, then one row per item. Every column but the
/// last is padded to its widest cell; the last runs free. A row may carry
/// notes, each printed on its own line under the last column.
pub struct Table {
    header: Vec<String>,
    rows: Vec<(Vec<String>, Vec<String>)>,
    empty: String,
    footer: Option<String>,
}

impl Table {
    /// A table with these column headers. `empty` is printed instead when the
    /// table has no rows (for example "No RMW backends found.").
    pub fn new(header: &[&str], empty: &str) -> Self {
        Table {
            header: header.iter().map(|h| h.to_string()).collect(),
            rows: Vec::new(),
            empty: empty.into(),
            footer: None,
        }
    }

    /// Add a row; it must have one cell per column.
    pub fn row(&mut self, cells: Vec<String>) -> &mut Self {
        self.row_with_notes(cells, Vec::new())
    }

    /// Add a row with notes under its last column.
    pub fn row_with_notes(&mut self, cells: Vec<String>, notes: Vec<String>) -> &mut Self {
        assert_eq!(
            cells.len(),
            self.header.len(),
            "a table row has one cell per column"
        );
        self.rows.push((cells, notes));
        self
    }

    /// A line printed after a blank line under the table, usually the `show`
    /// command that tells more about one row.
    pub fn footer(&mut self, line: &str) -> &mut Self {
        self.footer = Some(line.into());
        self
    }

    pub fn render(&self) -> String {
        if self.rows.is_empty() {
            return format!("{}\n", self.empty);
        }
        let last = self.header.len() - 1;
        let widths: Vec<usize> = (0..last)
            .map(|c| {
                self.rows
                    .iter()
                    .map(|(r, _)| r[c].chars().count())
                    .chain(std::iter::once(self.header[c].chars().count()))
                    .max()
                    .unwrap_or(0)
            })
            .collect();
        let line = |cells: &[String]| {
            let mut s = String::new();
            for (c, w) in widths.iter().enumerate() {
                s.push_str(&cells[c]);
                s.push_str(&" ".repeat(w - cells[c].chars().count() + 2));
            }
            s.push_str(&cells[last]);
            s.trim_end().to_string() + "\n"
        };
        let indent: String = widths.iter().map(|w| " ".repeat(w + 2)).collect();
        let mut out = line(&self.header);
        for (cells, notes) in &self.rows {
            out.push_str(&line(cells));
            for n in notes {
                out.push_str(&indent);
                out.push_str(n);
                out.push('\n');
            }
        }
        if let Some(f) = &self.footer {
            out.push('\n');
            out.push_str(f);
            out.push('\n');
        }
        out
    }
}

/// The fields of a `show`: one `label  value` line each, labels padded to
/// one width, indented under the item's name.
#[derive(Default)]
pub struct Fields {
    lines: Vec<(String, String)>,
}

impl Fields {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a field. An empty value prints as `-`.
    pub fn field(&mut self, label: &str, value: impl Into<String>) -> &mut Self {
        let v: String = value.into();
        self.lines
            .push((label.into(), if v.is_empty() { "-".into() } else { v }));
        self
    }

    pub fn render(&self) -> String {
        let w = self
            .lines
            .iter()
            .map(|(l, _)| l.chars().count())
            .max()
            .unwrap_or(0)
            .max(16);
        self.lines
            .iter()
            .map(|(l, v)| format!("  {l:<w$}  {v}\n"))
            .collect()
    }
}

/// The head of a `show`: the item's name, then its summary indented under it.
pub fn show_head(name: &str, summary: &str) -> String {
    if summary.is_empty() {
        format!("{name}\n\n")
    } else {
        format!("{name}\n  {summary}\n\n")
    }
}

/// The known names nearest to `name`: the ones it is a prefix or a part of,
/// and the ones a few edits away, closest first, at most `max`.
pub fn nearest<'a>(
    name: &str,
    known: impl IntoIterator<Item = &'a str>,
    max: usize,
) -> Vec<&'a str> {
    let lower = name.to_lowercase();
    let limit = (name.chars().count() / 3).max(2);
    let mut scored: Vec<(usize, &str)> = known
        .into_iter()
        .filter_map(|k| {
            let kl = k.to_lowercase();
            let d = strsim::levenshtein(&lower, &kl);
            if d <= limit {
                Some((d, k))
            } else if kl.contains(&lower) || (lower.len() >= 3 && lower.contains(&kl)) {
                Some((limit + 1, k))
            } else {
                None
            }
        })
        .collect();
    scored.sort_unstable();
    scored.dedup_by(|a, b| a.1 == b.1);
    scored.into_iter().take(max).map(|(_, k)| k).collect()
}

/// The refusal for a name that matches nothing: the nearest known names, and
/// the `list` command that names every one. `noun` is the word the user knows
/// (`RMW`, `board`, `knob`).
pub fn unknown<'a>(
    noun: &str,
    name: &str,
    known: impl IntoIterator<Item = &'a str>,
    list_cmd: &str,
) -> Report {
    let near = nearest(name, known, 5);
    let hint = match near.as_slice() {
        [] => String::new(),
        [one] => format!("; did you mean `{one}`?"),
        many => format!(
            "; the nearest are: {}",
            many.iter()
                .map(|n| format!("`{n}`"))
                .collect::<Vec<_>>()
                .join(", ")
        ),
    };
    eyre::eyre!("no {noun} named `{name}`{hint}\n  `{list_cmd}` names every one")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_pads_every_column_but_the_last() {
        let mut t = Table::new(&["NAME", "PEER", "SUMMARY"], "none");
        t.row(vec!["zenoh".into(), "zenoh-router".into(), "Zenoh.".into()])
            .row_with_notes(
                vec!["xrce".into(), "xrce-agent".into(), "XRCE-DDS.".into()],
                vec!["verified on: qemu".into()],
            )
            .footer("`nros rmw show <name>` shows one.");
        assert_eq!(
            t.render(),
            "NAME   PEER          SUMMARY\n\
             zenoh  zenoh-router  Zenoh.\n\
             xrce   xrce-agent    XRCE-DDS.\n\
             \x20                    verified on: qemu\n\
             \n\
             `nros rmw show <name>` shows one.\n"
        );
        assert_eq!(Table::new(&["A"], "No rows.").render(), "No rows.\n");
    }

    #[test]
    fn fields_align_and_mark_empty_values() {
        let mut f = Fields::new();
        f.field("host peer", "zenoh-router").field("aliases", "");
        assert_eq!(
            f.render(),
            "  host peer         zenoh-router\n  aliases           -\n"
        );
    }

    #[test]
    fn nearest_names_a_typo_and_a_part() {
        let known = ["zenoh", "xrce", "cyclonedds", "dds"];
        assert_eq!(nearest("zenho", known, 5), vec!["zenoh"]);
        assert_eq!(nearest("cyclone", known, 5), vec!["cyclonedds"]);
        assert!(nearest("wifi", known, 5).is_empty());
    }

    #[test]
    fn unknown_names_the_nearest_and_the_list_command() {
        let e = unknown("RMW", "zenho", ["zenoh", "xrce"], "nros rmw list").to_string();
        assert_eq!(
            e,
            "no RMW named `zenho`; did you mean `zenoh`?\n  `nros rmw list` names every one"
        );
        let e = unknown("RMW", "wifi", ["zenoh"], "nros rmw list").to_string();
        assert_eq!(e, "no RMW named `wifi`\n  `nros rmw list` names every one");
    }

    #[test]
    fn json_doc_carries_the_schema_the_value_and_the_extras() {
        let d = json_doc(
            "nros.x.list.v1",
            "items",
            &vec![1, 2],
            &[("platform", serde_json::json!("posix"))],
        )
        .unwrap();
        assert_eq!(
            d,
            serde_json::json!({"schema": "nros.x.list.v1", "platform": "posix", "items": [1, 2]})
        );
    }

    /// The Book is published at one URL; `book/book.toml`'s `site-url` is its
    /// path, so the two cannot drift apart.
    #[test]
    fn book_base_matches_the_book_site_url() {
        assert!(BOOK_BASE.starts_with("https://") && BOOK_BASE.ends_with('/'));
        let book = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../book/book.toml");
        let text = std::fs::read_to_string(&book)
            .unwrap_or_else(|e| panic!("read {}: {e}", book.display()));
        let doc: toml::Value = toml::from_str(&text).expect("book.toml parses");
        let site = doc["output"]["html"]["site-url"]
            .as_str()
            .expect("book.toml sets [output.html] site-url");
        let path = BOOK_BASE.splitn(4, '/').nth(3).map(|p| format!("/{p}"));
        assert_eq!(
            path.as_deref(),
            Some(site),
            "book/book.toml's site-url must be the path of the CLI's Book URL \
             (packages/cli/Cargo.toml, [workspace.package] documentation)"
        );
    }
}
