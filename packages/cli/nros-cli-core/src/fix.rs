// RFC-0129 D7 and D8.
//! Refusals that point at a line, and the mechanical ones `nros fix` applies.
//!
//! A [`Finding`] is a refusal about a configuration file a person wrote, with
//! the place it is about and, when the remedy is mechanical, the [`Fix`] that
//! makes it. Two consumers share one collector:
//!
//! - `nros build` and `nros configure` run [`preflight`] before reading the
//!   workspace, so a refused shape stops the build at its line, and the
//!   refusal names `nros fix` when it has a fix;
//! - `nros fix` runs the same collector, applies every fix to the file's
//!   text, prints a unified diff, and writes only with `--apply`.
//!
//! A span edit ([`Edit::RenameKey`], [`Edit::ReplaceValue`]) replaces a span
//! of the file's own text. A structural edit ([`Edit::MoveKey`],
//! [`Edit::SetKey`]) goes through `toml_edit`, which keeps comments and
//! layout. Either way the lines no fix is about are untouched.

use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::Write as _,
    path::{Path, PathBuf},
};

use eyre::{Result, eyre};
use nros_orchestration_ir::leaf_system::KnobValue;

use crate::{
    diag::{Diag, DiagExt, Refusal, RefusalGroup},
    source_file::{Located, SourceFile},
};

/// One mechanical edit to a configuration file (RFC-0129 D8, with
/// RFC-0140 D4's `scale`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Edit {
    /// Rename the key at the finding's location to `to`.
    RenameKey {
        /// The new key.
        to: String,
    },
    /// Delete the row at the finding's key path and state it as `to_key =
    /// value` in `to_table`. `value` is the old value times `scale`,
    /// converted to the new row's type and already checked against its
    /// range: an edit whose result the row refuses is never made.
    MoveKey {
        /// The table the row moves to (`["image", "talker", "knobs", "zenoh"]`).
        to_table: Vec<String>,
        /// The key it is stated under there.
        to_key: String,
        /// The factor the old value was multiplied by; 1 for a pure move.
        scale: u32,
        /// The new value.
        value: KnobValue,
    },
    /// Replace the value at the finding's location: `from` and `to` are the
    /// value's TOML text (`"native"` to `"linux"`, quotes included).
    ReplaceValue {
        /// The text the value has now.
        from: String,
        /// The text it gets.
        to: String,
    },
    /// State a key that is absent: `key = value` in `table`.
    SetKey {
        /// The table (`["image", "talker", "network"]`).
        table: Vec<String>,
        /// The key.
        key: String,
        /// Its value.
        value: KnobValue,
    },
}

/// A mechanical remedy: the file, the key path it is about, and the edit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fix {
    /// The file to edit.
    pub path: PathBuf,
    /// The key the edit is about, as a dotted path for a refusal's words.
    pub key_path: Vec<String>,
    /// Where the key (or, for [`Edit::ReplaceValue`], the value) is, and its
    /// byte range.
    pub at: Located,
    /// What to do.
    pub edit: Edit,
}

/// A refusal about a configuration file, with its location and, when the
/// remedy is mechanical, its fix.
#[derive(Debug, Clone)]
pub struct Finding {
    /// The refusal, already carrying its location.
    pub refusal: Refusal,
    /// The mechanical remedy, if there is one.
    pub fix: Option<Fix>,
    /// What the refusal is about (`image `talker``): several findings about
    /// one subject render as one group.
    pub subject: String,
}

/// Every `system.toml` a workspace states: `<ws>/system.toml`, else
/// `<ws>/src/*/system.toml`, sorted. The same rule `nros board facts` reads.
#[must_use]
pub fn system_toml_paths(ws: &Path) -> Vec<PathBuf> {
    let direct = ws.join("system.toml");
    if direct.is_file() {
        return vec![direct];
    }
    let mut found: Vec<PathBuf> = std::fs::read_dir(ws.join("src"))
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path().join("system.toml"))
        .filter(|p| p.is_file())
        .collect();
    found.sort();
    found
}

/// Every configuration file the workspace's `system.toml`s reach: each one,
/// then each fragment it includes. A file that cannot be expanded is left to
/// the reader that refuses it with its own words.
#[must_use]
pub fn workspace_files(ws: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    for p in system_toml_paths(ws) {
        match nros_system_include::read(&p) {
            Ok(e) => files.extend(e.files),
            Err(_) => files.push(p),
        }
    }
    files.dedup();
    files
}

/// Every finding in one file.
#[must_use]
pub fn findings_in(file: &SourceFile, ws: &Path) -> Vec<Finding> {
    let editable = is_under(&file.path, ws);
    let Ok(table) = file.text.parse::<toml::Table>() else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for (prefix, t) in image_tables(&table) {
        out.extend(env_rows(file, editable, &prefix, t));
        out.extend(retired_board_key(file, editable, &prefix, t));
    }
    out
}

/// Every image table a `system.toml` states, by key path: each
/// `[image.<id>]`, then `[image_defaults]`.
fn image_tables(table: &toml::Table) -> Vec<(Vec<String>, &toml::Table)> {
    let mut tables: Vec<(Vec<String>, &toml::Table)> = Vec::new();
    if let Some(images) = table.get("image").and_then(toml::Value::as_table) {
        for (id, t) in images {
            if let Some(t) = t.as_table() {
                tables.push((vec!["image".into(), id.clone()], t));
            }
        }
    }
    if let Some(t) = table.get("image_defaults").and_then(toml::Value::as_table) {
        tables.push((vec!["image_defaults".into()], t));
    }
    tables
}

/// The subject a refusal about `prefix`'s table is grouped under.
fn subject(prefix: &[String]) -> String {
    match prefix {
        [_, id] => format!("image `{id}`"),
        _ => "`[image_defaults]`".to_string(),
    }
}

/// Every finding in the workspace's configuration files.
#[must_use]
pub fn workspace_findings(ws: &Path) -> Vec<Finding> {
    workspace_files(ws)
        .iter()
        .filter_map(|p| SourceFile::read(p).ok())
        .flat_map(|f| findings_in(&f, ws))
        .collect()
}

/// Refuse the build when the workspace's configuration holds a refused shape,
/// each at its line. Runs before anything reads the workspace, so the user
/// sees the line rather than a refusal that names only the table.
///
/// # Errors
///
/// One or more findings, rendered one after another.
pub fn preflight(ws: &Path) -> Result<()> {
    let found = workspace_findings(ws);
    if found.is_empty() {
        return Ok(());
    }
    Err(eyre!("{}", render_all(&found)))
}

/// Several findings, each a full diagnostic, separated by a blank line.
/// Findings about one subject render as one [`RefusalGroup`] when there is
/// more than one of them (RFC-0129 D7), in the order first met.
#[must_use]
pub fn render_all(found: &[Finding]) -> String {
    let mut order: Vec<&str> = Vec::new();
    let mut by: BTreeMap<&str, Vec<&Refusal>> = BTreeMap::new();
    for f in found {
        if !by.contains_key(f.subject.as_str()) {
            order.push(&f.subject);
        }
        by.entry(&f.subject).or_default().push(&f.refusal);
    }
    order
        .iter()
        .map(|s| match by[s].as_slice() {
            [one] => one.to_string(),
            many => many
                .iter()
                .fold(
                    RefusalGroup::new(format_args!("{s} cannot be configured")),
                    |g, r| g.child((*r).clone()),
                )
                .to_string(),
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// `env` rows of one image table that name a registry knob, under its
/// current spelling (NROS-0012) or a retired one (NROS-0011). Either way the
/// row belongs in the image's knobs table, so the fix is a
/// [`Edit::MoveKey`] there, with a retired spelling's unit converted. A knob
/// retired outright has no current row, so its refusal has no fix.
fn env_rows(file: &SourceFile, editable: bool, prefix: &[String], t: &toml::Table) -> Vec<Finding> {
    let Some(env) = t.get("env").and_then(toml::Value::as_table) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for (key, raw) in env {
        let retired = nros_zephyr_build::retired_spelling_refusal(key);
        let row = match &retired {
            Some(_) => nros_zephyr_build::knob_row(key).filter(|r| r.retired.is_none()),
            None => match crate::cmd::leaf_settings::registry_knob_of_env_row(key) {
                Some(row) => Some(row),
                None => continue,
            },
        };
        let mut key_path = prefix.to_vec();
        key_path.extend(["env".to_string(), key.clone()]);
        let path_refs: Vec<&str> = key_path.iter().map(String::as_str).collect();
        let Some(at) = file.locate(&path_refs) else {
            continue;
        };
        let (diag, detail) = match &retired {
            Some(_) => (
                Diag::RetiredKnobSpelling,
                format!("`{key}` is a retired knob spelling"),
            ),
            None => (
                Diag::ImageEnvNamesKnob,
                format!("the env row `{key}` sets a registry knob"),
            ),
        };
        let mut refusal = diag.refusal(detail).at(at.clone());
        let mut fix = None;
        match row {
            None => refusal = refusal.help(retired.clone().unwrap_or_default()),
            Some(row) => {
                let factor = row
                    .retired_spellings
                    .iter()
                    .find(|s| s.road == nros_zephyr_build::RetiredRoad::Env && s.spelling == key)
                    .map_or(1, |s| s.factor);
                let (group, name) = row.image_group_name();
                let mut to_table = prefix.to_vec();
                to_table.push("knobs".into());
                to_table.extend(group.split('.').filter(|g| !g.is_empty()).map(String::from));
                let shown = format!("[{}] {name}", to_table.join("."));
                match moved_value(row, prefix, raw, factor) {
                    Ok(value) => {
                        let scaled = if factor == 1 {
                            String::new()
                        } else {
                            format!(" (the value x {factor})")
                        };
                        refusal =
                            refusal.help(format_args!("state it as `{shown} = {value}`{scaled}"));
                        fix = editable.then(|| Fix {
                            path: file.path.clone(),
                            key_path: key_path.clone(),
                            at,
                            edit: Edit::MoveKey {
                                to_table,
                                to_key: name.to_string(),
                                scale: factor,
                                value,
                            },
                        });
                    }
                    Err(why) => {
                        refusal = refusal.help(format_args!(
                            "state it as `{shown}`; the value cannot move as written: {why}"
                        ));
                    }
                }
            }
        }
        out.push(Finding {
            refusal: refusal.fixable(fix.is_some()),
            fix,
            subject: subject(prefix),
        });
    }
    out
}

/// An env row's text as the knobs-table value of `row`: times `factor`,
/// typed as the row is, and checked by the same rule `app_rung` reads the
/// knobs table with.
fn moved_value(
    row: &nros_zephyr_build::KnobRow,
    prefix: &[String],
    raw: &toml::Value,
    factor: u32,
) -> std::result::Result<KnobValue, String> {
    let text = match raw {
        toml::Value::String(s) => s.trim().to_string(),
        other => other.to_string(),
    };
    let int = || -> Option<i64> {
        match text.strip_prefix("0x").or_else(|| text.strip_prefix("0X")) {
            Some(hex) => i64::from_str_radix(hex, 16).ok(),
            None => text.parse().ok(),
        }
    };
    let value = match row.ty {
        Some("bool") => match text.as_str() {
            "1" | "true" | "y" => KnobValue::Bool(true),
            "0" | "false" | "n" => KnobValue::Bool(false),
            _ => return Err(format!("`{text}` is not a bool")),
        },
        Some("int" | "hex") | None => match int() {
            Some(n) => KnobValue::Int(
                n.checked_mul(i64::from(factor))
                    .ok_or_else(|| format!("`{text}` x {factor} does not fit in an integer"))?,
            ),
            None if factor != 1 => return Err(format!("`{text}` is not an integer")),
            None if row.ty.is_none() => KnobValue::Str(text.clone()),
            None => KnobValue::Str(text.clone()),
        },
        Some(_) => KnobValue::Str(text.clone()),
    };
    let table = format!("{}.knobs", prefix.join("."));
    crate::cmd::leaf_settings::knob_env_row(&table, row.image_key, &value)?;
    Ok(value)
}

/// `board = "native"` or `"posix"`: the host board's retired aliases
/// (NROS-0013). The fix replaces the value with the board's one name. A
/// board key removed outright has nothing to write instead, so no fix.
fn retired_board_key(
    file: &SourceFile,
    editable: bool,
    prefix: &[String],
    t: &toml::Table,
) -> Option<Finding> {
    let board = t.get("board")?.as_str()?;
    let hint = nros_entry_lower::retired_board_key_hint(board)?;
    let mut key_path = prefix.to_vec();
    key_path.push("board".into());
    let path_refs: Vec<&str> = key_path.iter().map(String::as_str).collect();
    let at = file.locate_value(&path_refs)?;
    let now = nros_entry_lower::RETIRED_BOARD_KEYS
        .iter()
        .find(|(old, _, _)| *old == board)
        .map(|(_, now, _)| *now);
    let fix = now.filter(|_| editable).map(|now| Fix {
        path: file.path.clone(),
        key_path: key_path.clone(),
        at: at.clone(),
        edit: Edit::ReplaceValue {
            from: file.text[at.bytes.clone()].to_string(),
            to: format!("\"{now}\""),
        },
    });
    let refusal = Diag::RetiredBoardKey
        .refusal(format_args!("board `{board}` is a retired board key"))
        .at(at)
        .help(hint)
        .fixable(fix.is_some());
    Some(Finding {
        refusal,
        fix,
        subject: subject(prefix),
    })
}

// RFC-0129 D7, RFC-0111.
/// An image's `framework` refused against its board, pointing at both lines:
/// the image's `framework` (in `files`, the image's own table first, then
/// `[image_defaults]`), and the board definition's `frameworks` (or, for a
/// board whose platform decides its entry shape, its `platform`). `why` is
/// the board descriptor's own refusal, which stays the authority; this only
/// locates it. A line that cannot be found is left out, never guessed.
#[must_use]
pub fn framework_refusal(
    files: &[PathBuf],
    image_id: &str,
    board: &str,
    board_file: Option<&Path>,
    why: &str,
) -> Refusal {
    let mut refusal =
        Diag::ImageFrameworkRefused.refusal(format_args!("image `{image_id}`: {why}"));
    let here = files
        .iter()
        .filter_map(|p| SourceFile::read(p).ok())
        .find_map(|f| {
            f.locate_value(&["image", image_id, "framework"])
                .or_else(|| f.locate_value(&["image_defaults", "framework"]))
        });
    if let Some(at) = here {
        refusal = refusal.at(at);
    }
    let there = board_file
        .and_then(|p| SourceFile::read(p).ok())
        .and_then(|f| {
            f.locate_board_value(board, &["frameworks"])
                .or_else(|| f.locate_board_value(board, &["platform"]))
        });
    if let Some(at) = there {
        refusal = refusal.note_at(at, format_args!("board `{board}` states this"));
    }
    refusal
}

/// Whether `path` is inside the workspace: `nros fix` edits nothing else.
fn is_under(path: &Path, ws: &Path) -> bool {
    let canon = |p: &Path| std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
    canon(path).starts_with(canon(ws))
}

/// The fixes in `found`, applied to each file's text, as `(path, old, new)`.
///
/// # Errors
///
/// Nothing is applied, and the conflict is named, when two fixes touch one
/// span, a value is no longer what its fix read, a rename's target key is
/// already stated in its table, or two moves (or a move and a key already
/// there) would state one key twice.
pub fn apply(found: &[Finding]) -> Result<Vec<(PathBuf, String, String)>> {
    let mut by_file: BTreeMap<PathBuf, Vec<&Fix>> = BTreeMap::new();
    for f in found.iter().filter_map(|f| f.fix.as_ref()) {
        by_file.entry(f.path.clone()).or_default().push(f);
    }
    let mut out = Vec::new();
    for (path, fixes) in by_file {
        let old = nros_system_include::read_own_text(&path)
            .map_err(|e| eyre!("read {}: {e}", path.display()))?;
        let file = SourceFile {
            path: path.clone(),
            text: old.clone(),
        };
        let (mut spans, structural): (Vec<&Fix>, Vec<&Fix>) = fixes
            .into_iter()
            .partition(|f| matches!(f.edit, Edit::RenameKey { .. } | Edit::ReplaceValue { .. }));
        let mut new = apply_spans(&file, &mut spans)?;
        if !structural.is_empty() {
            new = apply_structural(&path, &new, &structural)?;
        }
        out.push((path, old, new));
    }
    Ok(out)
}

/// The span edits, checked against each other and the file, applied from
/// the end so every earlier span stays valid.
fn apply_spans(file: &SourceFile, fixes: &mut [&Fix]) -> Result<String> {
    let path = &file.path;
    fixes.sort_by_key(|f| f.at.bytes.start);
    for pair in fixes.windows(2) {
        if pair[0].at.bytes.end > pair[1].at.bytes.start {
            return Err(eyre!(
                "two fixes touch one place in {} (lines {} and {}); fix them by hand",
                path.display(),
                pair[0].at.line,
                pair[1].at.line
            ));
        }
    }
    for f in fixes.iter() {
        match &f.edit {
            Edit::RenameKey { to } => {
                let mut target: Vec<&str> = f.key_path.iter().map(String::as_str).collect();
                *target.last_mut().expect("a key path names a key") = to;
                if let Some(there) = file.locate(&target) {
                    return Err(eyre!(
                        "{}:{}: renaming `{}` to `{to}` would state `{to}` twice; it is already at line {}. \
                         Delete one of the two rows by hand",
                        path.display(),
                        f.at.line,
                        f.key_path.join("."),
                        there.line
                    ));
                }
            }
            Edit::ReplaceValue { from, .. } => {
                if file.text.get(f.at.bytes.clone()) != Some(from.as_str()) {
                    return Err(eyre!(
                        "{}:{}: the value of `{}` is no longer {from}; run `nros fix` again",
                        path.display(),
                        f.at.line,
                        f.key_path.join(".")
                    ));
                }
            }
            Edit::MoveKey { .. } | Edit::SetKey { .. } => {}
        }
    }
    let mut new = file.text.clone();
    for f in fixes.iter().rev() {
        let to = match &f.edit {
            Edit::RenameKey { to } | Edit::ReplaceValue { to, .. } => to,
            Edit::MoveKey { .. } | Edit::SetKey { .. } => continue,
        };
        new.replace_range(f.at.bytes.clone(), to);
    }
    Ok(new)
}

/// The structural edits, through `toml_edit`: every target checked first
/// (no key stated twice, by the file or by two fixes), then applied.
fn apply_structural(path: &Path, text: &str, fixes: &[&Fix]) -> Result<String> {
    let mut doc: toml_edit::DocumentMut =
        text.parse().map_err(|e| eyre!("{}: {e}", path.display()))?;
    let mut targets: BTreeSet<(Vec<String>, String)> = BTreeSet::new();
    for f in fixes {
        let (table, key) = target_of(&f.edit);
        let mut full: Vec<&str> = table.iter().map(String::as_str).collect();
        full.push(key);
        let here = SourceFile {
            path: path.to_path_buf(),
            text: text.to_string(),
        };
        if let Some(there) = here.locate(&full) {
            return Err(eyre!(
                "{}:{}: `{}` would be stated twice; `{}` is already at line {}. \
                 Delete one of the two rows by hand",
                path.display(),
                f.at.line,
                f.key_path.join("."),
                full.join("."),
                there.line
            ));
        }
        if !targets.insert((table.to_vec(), key.to_string())) {
            return Err(eyre!(
                "{}:{}: two rows would both become `{}`; keep one by hand",
                path.display(),
                f.at.line,
                full.join(".")
            ));
        }
    }
    for f in fixes {
        if let Edit::MoveKey { .. } = f.edit {
            remove_key(&mut doc, &f.key_path).ok_or_else(|| {
                eyre!(
                    "{}: `{}` is gone; run `nros fix` again",
                    path.display(),
                    f.key_path.join(".")
                )
            })?;
        }
        let (table, key) = target_of(&f.edit);
        let value = match &f.edit {
            Edit::MoveKey { value, .. } | Edit::SetKey { value, .. } => value,
            _ => unreachable!("only structural edits reach here"),
        };
        let t = table_mut(&mut doc, table).ok_or_else(|| {
            eyre!(
                "{}: `{}` is not a table `nros fix` can add to; state `{key}` there by hand",
                path.display(),
                table.join(".")
            )
        })?;
        t.insert(key, toml_edit::value(toml_value(value)));
    }
    Ok(doc.to_string())
}

/// Where a structural edit states its key.
fn target_of(edit: &Edit) -> (&[String], &str) {
    match edit {
        Edit::MoveKey {
            to_table, to_key, ..
        } => (to_table, to_key),
        Edit::SetKey { table, key, .. } => (table, key),
        Edit::RenameKey { .. } | Edit::ReplaceValue { .. } => (&[], ""),
    }
}

fn toml_value(v: &KnobValue) -> toml_edit::Value {
    match v {
        KnobValue::Int(n) => (*n).into(),
        KnobValue::Bool(b) => (*b).into(),
        KnobValue::Str(s) => s.as_str().into(),
    }
}

/// Remove the key at `key_path`, and its table too when that leaves the
/// table empty (an `env = { X = "1" }` whose one row moved).
fn remove_key(doc: &mut toml_edit::DocumentMut, key_path: &[String]) -> Option<()> {
    let (last, parents) = key_path.split_last()?;
    let (parent_key, grand) = parents.split_last()?;
    let mut t: &mut dyn toml_edit::TableLike = doc.as_table_mut();
    for k in grand {
        t = t.get_mut(k)?.as_table_like_mut()?;
    }
    let parent = t.get_mut(parent_key)?.as_table_like_mut()?;
    parent.remove(last)?;
    if parent.is_empty() {
        t.remove(parent_key);
    }
    Some(())
}

/// The table at `path`, created (as an implicit chain of standard tables,
/// so only the last header is written) where absent. `None` when something
/// on the way is a value, or an array of tables.
fn table_mut<'a>(
    doc: &'a mut toml_edit::DocumentMut,
    path: &[String],
) -> Option<&'a mut dyn toml_edit::TableLike> {
    let mut t: &mut dyn toml_edit::TableLike = doc.as_table_mut();
    for (i, k) in path.iter().enumerate() {
        if t.get(k).is_none() {
            let mut new = toml_edit::Table::new();
            new.set_implicit(i + 1 < path.len());
            t.insert(k, toml_edit::Item::Table(new));
        }
        t = t.get_mut(k)?.as_table_like_mut()?;
    }
    Some(t)
}

/// A unified diff of one file's edit, with paths relative to `ws`.
#[must_use]
pub fn unified_diff(ws: &Path, path: &Path, old: &str, new: &str) -> String {
    let shown = path.strip_prefix(ws).unwrap_or(path).display().to_string();
    let mut s = String::new();
    let _ = write!(
        s,
        "{}",
        similar::TextDiff::from_lines(old, new)
            .unified_diff()
            .context_radius(2)
            .header(&format!("a/{shown}"), &format!("b/{shown}"))
    );
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    /// A retired spelling, the subject of these tests (the gate exempts this line).
    const OLD: &str = "ZPICO_MAX_QUERYABLES"; // retired spelling under test
    /// A retired spelling in KiB, whose current knob is in bytes (the gate exempts this line).
    const OLD_KB: &str = "NROS_FREERTOS_HEAP_KB"; // retired spelling under test

    fn ws_with(system: &str) -> tempfile::TempDir {
        let td = tempfile::tempdir().unwrap();
        let b = td.path().join("src/app_bringup");
        std::fs::create_dir_all(&b).unwrap();
        std::fs::write(b.join("system.toml"), system).unwrap();
        td
    }

    fn retired_text() -> String {
        format!(
            "[system]\nname = \"s\"\n\n[image.talker]\nboard = \"linux\"\n\
             env = {{ {OLD} = \"4\", MY_OWN = \"1\" }}   # kept\n"
        )
    }

    /// What `nros build` reads for image `id`: the leaf reader, then the APP
    /// rung that refuses an env row naming a registry knob. The bringup gets
    /// a `CMakeLists.txt` so the reader takes it as a leaf.
    fn reads_back(
        td: &tempfile::TempDir,
        id: &str,
    ) -> std::result::Result<BTreeMap<String, String>, String> {
        let dir = td.path().join("src/app_bringup");
        std::fs::write(dir.join("CMakeLists.txt"), "").unwrap();
        let ls = nros_orchestration_ir::leaf_system::read_image(&dir, id)?.ok_or("no image")?;
        crate::cmd::leaf_settings::app_rung(Some(id), &ls.knobs, &ls.env, None)
            .map_err(|e| e.to_string())
    }

    /// Apply every fix and write the result back; the new text.
    fn fixed(td: &tempfile::TempDir) -> String {
        let edits = apply(&workspace_findings(td.path())).unwrap();
        assert_eq!(edits.len(), 1);
        std::fs::write(&edits[0].0, &edits[0].2).unwrap();
        edits[0].2.clone()
    }

    #[test]
    fn a_retired_spelling_is_refused_at_its_line_and_names_nros_fix() {
        let td = ws_with(&retired_text());
        let found = workspace_findings(td.path());
        assert_eq!(found.len(), 1);
        let f = &found[0];
        let at = &f.fix.as_ref().unwrap().at;
        assert_eq!((at.line, at.column), (6, 9));
        let text = f.refusal.to_string();
        assert!(text.starts_with("error[NROS-0011]: "), "{text}");
        assert!(text.contains("system.toml:6:9"), "{text}");
        assert!(text.contains("^^^^^^^^^^^^^^^^^^^^"), "{text}");
        assert!(
            text.contains("help: state it as `[image.talker.knobs.zenoh] max_queryables = 4`"),
            "{text}"
        );
        assert!(text.contains("fix: nros fix"), "{text}");
        assert!(preflight(td.path()).is_err());
    }

    #[test]
    fn nros_fix_moves_a_retired_spelling_to_the_knobs_table_and_keeps_the_rest() {
        let td = ws_with(&retired_text());
        assert!(
            reads_back(&td, "talker").is_err(),
            "the reader refuses it too"
        );
        let new = fixed(&td);
        assert!(!new.contains(OLD), "{new}");
        assert!(new.contains("MY_OWN = \"1\""), "{new}");
        assert!(new.contains("# kept"), "{new}");
        assert!(
            new.contains("[image.talker.knobs.zenoh]\nmax_queryables = 4\n"),
            "{new}"
        );
        // What it wrote is what `nros build` reads: nothing left to refuse,
        // and the knobs table resolves through the reader's own rule.
        assert!(workspace_findings(td.path()).is_empty());
        preflight(td.path()).unwrap();
        let t: toml::Table = new.parse().unwrap();
        let v = t["image"]["talker"]["knobs"]["zenoh"]["max_queryables"].clone();
        assert_eq!(v.as_integer(), Some(4));
        let rows = reads_back(&td, "talker").unwrap();
        assert_eq!(rows["NROS_ZENOH_MAX_QUERYABLES"], "4");
        assert_eq!(rows["MY_OWN"], "1");
    }

    #[test]
    fn a_retired_unit_is_converted_when_it_moves() {
        let td = ws_with(&format!(
            "[image.talker]\nboard = \"linux\"\n[image.talker.env]\n{OLD_KB} = \"64\"\n"
        ));
        let found = workspace_findings(td.path());
        let text = found[0].refusal.to_string();
        assert!(text.contains("= 65536` (the value x 1024)"), "{text}");
        let new = fixed(&td);
        assert!(
            !new.contains("[image.talker.env]"),
            "an emptied env table goes: {new}"
        );
        assert!(new.contains("heap_bytes = 65536"), "{new}");
        assert!(workspace_findings(td.path()).is_empty());
        let env = nros_zephyr_build::knob_by_image_key("memory.heap_bytes")
            .unwrap()
            .env;
        assert_eq!(reads_back(&td, "talker").unwrap()[env], "65536");
    }

    #[test]
    fn a_current_env_spelling_of_a_registry_knob_moves_too() {
        let td = ws_with(
            "[image.talker]\nboard = \"linux\"\nenv = { NROS_ZENOH_MAX_QUERYABLES = \"6\" }\n",
        );
        let found = workspace_findings(td.path());
        assert_eq!(found.len(), 1);
        let text = found[0].refusal.to_string();
        assert!(text.starts_with("error[NROS-0012]: "), "{text}");
        assert!(text.contains("system.toml:3:9"), "{text}");
        let new = fixed(&td);
        assert!(new.contains("max_queryables = 6"), "{new}");
        assert!(!new.contains("env ="), "{new}");
        assert!(workspace_findings(td.path()).is_empty());
        assert_eq!(
            reads_back(&td, "talker").unwrap()["NROS_ZENOH_MAX_QUERYABLES"],
            "6"
        );
    }

    #[test]
    fn image_defaults_rows_move_to_image_defaults_knobs() {
        let td = ws_with(
            "[image_defaults]\nenv = { NROS_ZENOH_MAX_QUERYABLES = \"6\" }\n\n[image.t]\nboard = \"linux\"\n",
        );
        let new = fixed(&td);
        let t: toml::Table = new.parse().unwrap();
        assert_eq!(
            t["image_defaults"]["knobs"]["zenoh"]["max_queryables"].as_integer(),
            Some(6),
            "{new}"
        );
    }

    #[test]
    fn a_value_the_knob_refuses_is_not_moved_and_says_why() {
        let td = ws_with(
            "[image.talker]\nboard = \"linux\"\nenv = { NROS_ZENOH_MAX_QUERYABLES = \"many\" }\n",
        );
        let found = workspace_findings(td.path());
        assert!(found[0].fix.is_none());
        let text = found[0].refusal.to_string();
        assert!(text.contains("the value cannot move as written"), "{text}");
        assert!(!text.contains("fix: nros fix"), "{text}");
    }

    #[test]
    fn a_move_onto_a_knob_already_stated_is_refused_and_nothing_is_applied() {
        let td = ws_with(&format!(
            "[image.talker]\nboard = \"linux\"\nenv = {{ {OLD} = \"4\" }}\n\n\
             [image.talker.knobs.zenoh]\nmax_queryables = 8\n"
        ));
        let e = apply(&workspace_findings(td.path()))
            .unwrap_err()
            .to_string();
        assert!(e.contains("would be stated twice"), "{e}");
        assert!(e.contains("line 6"), "{e}");
    }

    #[test]
    fn two_rows_that_become_one_knob_are_refused() {
        let td = ws_with(&format!(
            "[image.talker]\nboard = \"linux\"\n\
             env = {{ {OLD} = \"4\", NROS_ZENOH_MAX_QUERYABLES = \"8\" }}\n"
        ));
        let e = apply(&workspace_findings(td.path()))
            .unwrap_err()
            .to_string();
        assert!(e.contains("two rows would both become"), "{e}");
    }

    #[test]
    fn a_retired_host_board_key_is_replaced_with_linux() {
        let td = ws_with("[image.talker]\nboard = \"native\"   # host\n");
        let found = workspace_findings(td.path());
        assert_eq!(found.len(), 1);
        let text = found[0].refusal.to_string();
        assert!(text.starts_with("error[NROS-0013]: "), "{text}");
        assert!(text.contains("system.toml:2:9"), "{text}");
        assert!(text.contains("^^^^^^^^"), "{text}");
        let new = fixed(&td);
        assert_eq!(new, "[image.talker]\nboard = \"linux\"   # host\n");
        assert!(workspace_findings(td.path()).is_empty());
        std::fs::write(td.path().join("src/app_bringup/CMakeLists.txt"), "").unwrap();
        let ls = nros_orchestration_ir::leaf_system::read_image(
            &td.path().join("src/app_bringup"),
            "talker",
        )
        .unwrap()
        .unwrap();
        assert_eq!(ls.board.as_deref(), Some("linux"));
    }

    #[test]
    fn findings_about_one_image_render_as_one_group() {
        let td = ws_with(&format!(
            "[image.talker]\nboard = \"posix\"\nenv = {{ {OLD} = \"4\" }}\n\n\
             [image.other]\nboard = \"linux\"\n"
        ));
        let found = workspace_findings(td.path());
        assert_eq!(found.len(), 2);
        let text = render_all(&found);
        assert!(
            text.starts_with(
                "error: image `talker` cannot be configured (2 problems)\n\nerror[NROS-"
            ),
            "{text}"
        );
        assert!(text.contains("error[NROS-0011]"), "{text}");
        assert!(text.contains("error[NROS-0013]"), "{text}");
        // Every child's fix is applied, as a lone refusal's would be.
        let new = fixed(&td);
        assert!(
            new.contains("board = \"linux\"") && new.contains("max_queryables = 4"),
            "{new}"
        );
    }

    #[test]
    fn a_lone_finding_renders_with_no_group_header() {
        let td = ws_with(&retired_text());
        let text = render_all(&workspace_findings(td.path()));
        assert!(text.starts_with("error[NROS-0011]"), "{text}");
    }

    #[test]
    fn set_key_states_an_absent_key_in_its_table() {
        let td = ws_with("[image.talker]\nboard = \"linux\"\n");
        let path = td.path().join("src/app_bringup/system.toml");
        let file = SourceFile::read(&path).unwrap();
        let at = file.locate(&["image", "talker", "board"]).unwrap();
        let finding = Finding {
            refusal: Diag::ImageNoBoard.refusal("test"),
            fix: Some(Fix {
                path: path.clone(),
                key_path: vec![
                    "image".into(),
                    "talker".into(),
                    "network".into(),
                    "transport".into(),
                ],
                at,
                edit: Edit::SetKey {
                    table: vec!["image".into(), "talker".into(), "network".into()],
                    key: "transport".into(),
                    value: KnobValue::Str("serial".into()),
                },
            }),
            subject: "image `talker`".into(),
        };
        let edits = apply(std::slice::from_ref(&finding)).unwrap();
        assert_eq!(
            edits[0].2,
            "[image.talker]\nboard = \"linux\"\n\n[image.talker.network]\ntransport = \"serial\"\n"
        );
        // A key already stated is never overwritten.
        std::fs::write(&path, &edits[0].2).unwrap();
        let e = apply(std::slice::from_ref(&finding))
            .unwrap_err()
            .to_string();
        assert!(e.contains("would be stated twice"), "{e}");
    }

    #[test]
    fn a_framework_refusal_points_at_the_image_and_the_board() {
        let td = ws_with("[image.talker]\nboard = \"acme\"\nframework = \"rtic\"\n");
        let board = td.path().join("nros-board.toml");
        std::fs::write(
            &board,
            "[[board]]\nnames = [\"acme\"]\nplatform = \"bare-metal\"\nframeworks = [\"owned-spin\"]\n",
        )
        .unwrap();
        let r = framework_refusal(
            &system_toml_paths(td.path()),
            "talker",
            "acme",
            Some(&board),
            "`framework = \"rtic\"`: this board does not boot it. It boots: owned-spin",
        );
        let text = r.to_string();
        assert!(
            text.starts_with("error[NROS-0014]: image `talker`: "),
            "{text}"
        );
        assert!(text.contains("system.toml:3:13"), "{text}");
        assert!(text.contains("note: board `acme` states this"), "{text}");
        assert!(text.contains("nros-board.toml:4:14"), "{text}");
        assert!(!text.contains("fix: nros fix"), "{text}");
    }

    #[test]
    fn a_current_spelling_is_not_a_finding() {
        let td = ws_with(
            "[image.talker]\nboard = \"linux\"\nenv = { MY_OWN = \"1\" }\n\
             [image.talker.knobs.zenoh]\nmax_queryables = 4\n",
        );
        assert!(workspace_findings(td.path()).is_empty());
    }

    #[test]
    fn a_file_outside_the_workspace_gets_no_fix() {
        let outside = tempfile::tempdir().unwrap();
        let p = outside.path().join("system.toml");
        std::fs::write(&p, retired_text()).unwrap();
        let ws = tempfile::tempdir().unwrap();
        let f = SourceFile::read(&p).unwrap();
        let found = findings_in(&f, ws.path());
        assert_eq!(found.len(), 1);
        assert!(found[0].fix.is_none());
        assert!(!found[0].refusal.to_string().contains("fix: nros fix"));
    }
}
