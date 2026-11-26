// RFC-0129 D7.
//! A configuration file's text kept beside what was parsed from it, so a
//! refusal can point at the line it is about.
//!
//! The serde structures the CLI reads configuration into carry no positions.
//! Rather than thread spans through every one of them, a refusal site that
//! knows a key's path asks [`SourceFile::locate`], which re-reads the text
//! with `toml_edit` and returns the key's span. The cost is paid only on the
//! refusal path.

use std::{
    ops::Range,
    path::{Path, PathBuf},
};

use toml_edit::{ImDocument, TableLike};

/// A configuration file: the path as the user names it, and its own text
/// (unexpanded: an included fragment is its own `SourceFile`).
#[derive(Debug, Clone)]
pub struct SourceFile {
    /// The file.
    pub path: PathBuf,
    /// Its text, as read.
    pub text: String,
}

/// A span in a [`SourceFile`], owned, ready to render as a
/// [`nros_diagnostic_codes::Location`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Located {
    /// The file, as the refusal prints it.
    pub path: String,
    /// The 1-based line.
    pub line: u32,
    /// The 1-based column, in characters.
    pub column: u32,
    /// The span's length, in characters.
    pub len: u32,
    /// The line's text, without its line break.
    pub text: String,
    /// The span's byte range in the file, for an edit.
    pub bytes: Range<usize>,
}

impl Located {
    /// As the renderer's borrowed location.
    #[must_use]
    pub fn as_location(&self) -> nros_diagnostic_codes::Location<'_> {
        nros_diagnostic_codes::Location {
            path: &self.path,
            line: self.line,
            column: self.column,
            len: self.len,
            text: &self.text,
        }
    }
}

impl SourceFile {
    /// Read `path`'s own text.
    ///
    /// # Errors
    ///
    /// The file cannot be read.
    pub fn read(path: &Path) -> std::io::Result<Self> {
        Ok(Self {
            path: path.to_path_buf(),
            text: nros_system_include::read_own_text(path)?,
        })
    }

    /// The KEY at `key_path` (`["image", "talker", "env", "ZPICO_X"]`), as a
    /// span. `None` when the text does not parse or holds no such key.
    #[must_use]
    pub fn locate(&self, key_path: &[&str]) -> Option<Located> {
        let doc = ImDocument::parse(self.text.as_str()).ok()?;
        let (key, _) = find(doc.as_table(), key_path)?;
        Some(self.span(key))
    }

    /// The VALUE at `key_path` (`"native"` in `board = "native"`), as a span,
    /// quotes included: what a [`crate::fix::Edit::ReplaceValue`] replaces.
    #[must_use]
    pub fn locate_value(&self, key_path: &[&str]) -> Option<Located> {
        let doc = ImDocument::parse(self.text.as_str()).ok()?;
        let (_, value) = find(doc.as_table(), key_path)?;
        Some(self.span(value?))
    }

    /// The VALUE at `key_path` inside the `[[board]]` entry that describes
    /// `board` in a board definition (`nros-board.toml`): the entry whose
    /// `names` lists it, else the file's only entry. A board file may hold
    /// several boards, so a plain key path cannot reach one.
    #[must_use]
    pub fn locate_board_value(&self, board: &str, key_path: &[&str]) -> Option<Located> {
        let doc = ImDocument::parse(self.text.as_str()).ok()?;
        let boards = doc.get("board")?.as_array_of_tables()?;
        let names = |t: &toml_edit::Table| {
            t.get("names")
                .and_then(toml_edit::Item::as_array)
                .is_some_and(|a| a.iter().any(|v| v.as_str() == Some(board)))
        };
        let entry = match boards.iter().find(|t| names(t)) {
            Some(t) => t,
            None if boards.len() == 1 => boards.get(0)?,
            None => return None,
        };
        let (_, value) = find(entry, key_path)?;
        Some(self.span(value?))
    }

    /// A byte range of this file as a [`Located`].
    #[must_use]
    pub fn span(&self, bytes: Range<usize>) -> Located {
        let before = &self.text[..bytes.start];
        let line_start = before.rfind('\n').map_or(0, |i| i + 1);
        let line_end = self.text[bytes.start..]
            .find('\n')
            .map_or(self.text.len(), |i| bytes.start + i);
        let to_u32 = |n: usize| u32::try_from(n).unwrap_or(u32::MAX);
        Located {
            path: self.path.display().to_string(),
            line: to_u32(before.matches('\n').count() + 1),
            column: to_u32(self.text[line_start..bytes.start].chars().count() + 1),
            len: to_u32(self.text[bytes.clone()].chars().count()),
            text: self.text[line_start..line_end]
                .trim_end_matches('\r')
                .to_string(),
            bytes,
        }
    }
}

/// The key's span and, when it has one, the value's, at `key_path` under
/// `table`. Descends standard, inline and dotted tables alike.
fn find(table: &dyn TableLike, key_path: &[&str]) -> Option<(Range<usize>, Option<Range<usize>>)> {
    let (last, tables) = key_path.split_last()?;
    let mut table = table;
    for k in tables {
        table = table.get(k)?.as_table_like()?;
    }
    let (key, item) = table.get_key_value(last)?;
    Some((
        key.span()?,
        item.as_value().and_then(toml_edit::Value::span),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(text: &str) -> SourceFile {
        SourceFile {
            path: PathBuf::from("src/app_bringup/system.toml"),
            text: text.to_string(),
        }
    }

    #[test]
    fn a_key_in_an_inline_table_is_located_at_its_line_and_column() {
        let f = file(
            "[system]\nname = \"s\"\n\n[image.talker]\nboard = \"linux\"\nenv = { NROS_APP_QUEUE_DEPTH = \"4\" }\n",
        );
        let at = f
            .locate(&["image", "talker", "env", "NROS_APP_QUEUE_DEPTH"])
            .expect("the key is there");
        assert_eq!((at.line, at.column, at.len), (6, 9, 20));
        assert_eq!(at.text, "env = { NROS_APP_QUEUE_DEPTH = \"4\" }");
        assert_eq!(&f.text[at.bytes], "NROS_APP_QUEUE_DEPTH");
    }

    #[test]
    fn a_key_in_a_standard_table_is_located() {
        let f = file("[image.talker.env]\n  ZPICO_X = \"1\"\n");
        let at = f.locate(&["image", "talker", "env", "ZPICO_X"]).unwrap();
        assert_eq!((at.line, at.column, at.len), (2, 3, 7));
    }

    #[test]
    fn a_missing_key_has_no_location() {
        let f = file("[image.talker]\nboard = \"linux\"\n");
        assert_eq!(f.locate(&["image", "talker", "env", "X"]), None);
    }

    #[test]
    fn a_value_is_located_with_its_quotes() {
        let f = file("[image.talker]\nboard = \"native\"   # host\n");
        let at = f.locate_value(&["image", "talker", "board"]).unwrap();
        assert_eq!((at.line, at.column, at.len), (2, 9, 8));
        assert_eq!(&f.text[at.bytes], "\"native\"");
    }

    #[test]
    fn a_board_entry_is_picked_by_its_names() {
        let f = file(
            "[[board]]\nnames = [\"a\"]\nframeworks = [\"owned-spin\"]\n\n\
             [[board]]\nnames = [\"b\", \"b2\"]\nframeworks = [\"owned-spin\", \"rtic\"]\n",
        );
        let at = f.locate_board_value("b2", &["frameworks"]).unwrap();
        assert_eq!(at.line, 7);
        assert_eq!(&f.text[at.bytes], "[\"owned-spin\", \"rtic\"]");
        assert_eq!(f.locate_board_value("zz", &["frameworks"]), None);
    }

    #[test]
    fn a_single_board_entry_answers_for_any_name() {
        let f = file("[[board]]\nnames = [\"a\"]\nframeworks = [\"owned-spin\"]\n");
        assert_eq!(
            f.locate_board_value("alias", &["frameworks"]).unwrap().line,
            3
        );
    }
}
