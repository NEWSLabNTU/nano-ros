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
//! An edit replaces a span of the file's own text, so comments, layout and
//! every other line are untouched.

use std::{
    collections::BTreeMap,
    fmt::Write as _,
    path::{Path, PathBuf},
};

use eyre::{Result, eyre};

use crate::{
    diag::{Diag, DiagExt, Refusal},
    source_file::{Located, SourceFile},
};

/// One mechanical edit to a configuration file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Edit {
    /// Rename the key at the finding's location to `to`.
    RenameKey {
        /// The new key.
        to: String,
    },
}

/// A mechanical remedy: the file, the key path it is about, and the edit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fix {
    /// The file to edit.
    pub path: PathBuf,
    /// The key the edit is about, as a dotted path for a refusal's words.
    pub key_path: Vec<String>,
    /// Where the key is, and its byte range.
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
    retired_knob_spellings(file, ws)
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
#[must_use]
pub fn render_all(found: &[Finding]) -> String {
    found
        .iter()
        .map(|f| f.refusal.to_string())
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// `[image.<id>] env` and `[image_defaults] env` rows spelled with a knob's
/// retired name. A renamed knob gets a [`Edit::RenameKey`] fix; a knob
/// retired outright has no current spelling, so its refusal has no fix.
fn retired_knob_spellings(file: &SourceFile, ws: &Path) -> Vec<Finding> {
    let Ok(table) = file.text.parse::<toml::Table>() else {
        return Vec::new();
    };
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
    let editable = is_under(&file.path, ws);
    let mut out = Vec::new();
    for (prefix, t) in tables {
        let Some(env) = t.get("env").and_then(toml::Value::as_table) else {
            continue;
        };
        for key in env.keys() {
            let Some(words) = nros_zephyr_build::retired_spelling_refusal(key) else {
                continue;
            };
            let mut key_path = prefix.clone();
            key_path.extend(["env".to_string(), key.clone()]);
            let path_refs: Vec<&str> = key_path.iter().map(String::as_str).collect();
            let Some(at) = file.locate(&path_refs) else {
                continue;
            };
            let current = nros_zephyr_build::knob_row(key)
                .filter(|row| row.retired.is_none())
                .map(|row| row.env.to_string());
            let fix = current.as_ref().filter(|_| editable).map(|to| Fix {
                path: file.path.clone(),
                key_path: key_path.clone(),
                at: at.clone(),
                edit: Edit::RenameKey { to: to.clone() },
            });
            let mut refusal = Diag::RetiredKnobSpelling
                .refusal(format_args!("`{key}` is a retired knob spelling"))
                .at(at);
            refusal = match &current {
                Some(to) => refusal.help(format_args!("set {to} instead")),
                None => refusal.help(words),
            };
            out.push(Finding {
                refusal: refusal.fixable(fix.is_some()),
                fix,
            });
        }
    }
    out
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
/// Two fixes touch one span, or a rename's target key is already stated in
/// the same table: nothing is applied, and both are named.
pub fn apply(found: &[Finding]) -> Result<Vec<(PathBuf, String, String)>> {
    let mut by_file: BTreeMap<PathBuf, Vec<&Fix>> = BTreeMap::new();
    for f in found.iter().filter_map(|f| f.fix.as_ref()) {
        by_file.entry(f.path.clone()).or_default().push(f);
    }
    let mut out = Vec::new();
    for (path, mut fixes) in by_file {
        let old = nros_system_include::read_own_text(&path)
            .map_err(|e| eyre!("read {}: {e}", path.display()))?;
        let file = SourceFile {
            path: path.clone(),
            text: old.clone(),
        };
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
        for f in &fixes {
            let Edit::RenameKey { to } = &f.edit;
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
        let mut new = old.clone();
        for f in fixes.iter().rev() {
            let Edit::RenameKey { to } = &f.edit;
            new.replace_range(f.at.bytes.clone(), to);
        }
        out.push((path, old, new));
    }
    Ok(out)
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
             env = {{ {OLD} = \"4\" }}   # kept\n"
        )
    }

    #[test]
    fn a_retired_spelling_is_refused_at_its_line_and_names_nros_fix() {
        let td = ws_with(&retired_text());
        let found = workspace_findings(td.path());
        assert_eq!(found.len(), 1);
        let f = &found[0];
        assert_eq!(
            (
                f.fix.as_ref().unwrap().at.line,
                f.fix.as_ref().unwrap().at.column
            ),
            (6, 9)
        );
        let text = f.refusal.to_string();
        assert!(text.starts_with("error[NROS-0011]: "), "{text}");
        assert!(text.contains("system.toml:6:9"), "{text}");
        assert!(text.contains("^^^^^^^^^^^^^^^^^^^^"), "{text}");
        assert!(
            text.contains("help: set NROS_ZENOH_MAX_QUERYABLES instead"),
            "{text}"
        );
        assert!(text.contains("fix: nros fix"), "{text}");
        assert!(preflight(td.path()).is_err());
    }

    #[test]
    fn nros_fix_renames_the_key_and_keeps_the_rest_of_the_line() {
        let td = ws_with(&retired_text());
        let found = workspace_findings(td.path());
        let edits = apply(&found).unwrap();
        assert_eq!(edits.len(), 1);
        let (_, old, new) = &edits[0];
        assert_eq!(new, &old.replace(OLD, "NROS_ZENOH_MAX_QUERYABLES"));
        assert!(new.contains("# kept"));
        let diff = unified_diff(td.path(), &edits[0].0, old, new);
        assert!(diff.contains(&format!("-env = {{ {OLD}")), "{diff}");
        assert!(
            diff.contains("+env = { NROS_ZENOH_MAX_QUERYABLES"),
            "{diff}"
        );
        // The rewritten file has nothing left to refuse.
        std::fs::write(&edits[0].0, new).unwrap();
        assert!(workspace_findings(td.path()).is_empty());
        preflight(td.path()).unwrap();
    }

    #[test]
    fn a_rename_onto_a_key_already_stated_is_refused_and_nothing_is_applied() {
        let td = ws_with(&format!(
            "[image.talker]\nboard = \"linux\"\n[image.talker.env]\n\
             {OLD} = \"4\"\nNROS_ZENOH_MAX_QUERYABLES = \"8\"\n"
        ));
        let e = apply(&workspace_findings(td.path()))
            .unwrap_err()
            .to_string();
        assert!(
            e.contains("would state `NROS_ZENOH_MAX_QUERYABLES` twice"),
            "{e}"
        );
        assert!(e.contains("line 5"), "{e}");
    }

    #[test]
    fn a_current_spelling_is_not_a_finding() {
        let td = ws_with(
            "[image.talker]\nboard = \"linux\"\nenv = { NROS_ZENOH_MAX_QUERYABLES = \"4\", MY_OWN = \"1\" }\n",
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
