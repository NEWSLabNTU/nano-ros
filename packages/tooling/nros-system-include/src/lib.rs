//! The one reader of a bringup's `system.toml` (RFC-0024 §5, issue 2587).
//!
//! A `system.toml` may name fragments it shares with other bringups:
//!
//! ```toml
//! include = ["../shared/topology.toml", "../shared/knobs.toml"]
//! ```
//!
//! [`read`] returns the file with every include expanded, which is what every
//! reader of a system's MEANING must see. The rules, which RFC-0024 §5 states
//! for users:
//!
//! 1. `include` is a top-level array of paths, each relative to the file that
//!    names it. An included file may include further files.
//! 2. Tables merge key by key, at every depth.
//! 3. A key that is not a table, stated by two files, is refused, naming
//!    both. There is no override: every value has exactly one source, so a
//!    reader can always say where a value came from.
//! 4. An array of tables (`[[domain]]`, `[[component]]`) concatenates: each
//!    include's entries in the listed order, then the including file's own.
//! 5. A file that reaches itself (a cycle), or the same file reached twice
//!    (a diamond, whose array-of-table entries would be counted twice), is
//!    refused.
//! 6. An included file may not set `[system] name`: the system is named by
//!    the bringup's own file. Nor `[sources]` or `[tools]`: those are this
//!    machine's paths for this project, read by build scripts from the
//!    project's own file (`nros_build_paths::locate::project_entry`).
//!
//! A tool that EDITS a `system.toml` reads that file's own text with
//! [`read_own_text`], never the expansion, so it writes back only what the
//! file itself states.

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

/// The key that names the fragments.
pub const INCLUDE_KEY: &str = "include";

/// A `system.toml` with its includes expanded, and where each top-level
/// fragment came from.
#[derive(Debug, Clone)]
pub struct Expanded {
    /// The merged table; it holds no `include` key.
    pub table: toml::Table,
    /// Every file read, the including file first, then each include in the
    /// order it was expanded. A caller that watches inputs (a build script's
    /// `rerun-if-changed`, a configure's dependency list) watches all of them.
    pub files: Vec<PathBuf>,
}

impl Expanded {
    /// The merged table as TOML text, for a reader that parses text.
    #[must_use]
    pub fn text(&self) -> String {
        toml::to_string(&self.table).expect("a toml::Table serializes")
    }
}

/// Read `path` and expand its includes. See the crate docs for the rules.
///
/// # Errors
///
/// The file or an include cannot be read or parsed, an `include` is not an
/// array of strings, a key is stated twice, a file is reached twice or
/// through itself, or an include sets `[system] name`. Each message names the
/// file it is about.
pub fn read(path: &Path) -> Result<Expanded, String> {
    let mut files = Vec::new();
    let mut origin = BTreeMap::new();
    let table = expand(path, &mut Vec::new(), &mut files, &mut origin, true)?;
    Ok(Expanded { table, files })
}

/// [`read`], or `Ok(None)` when `path` does not exist.
///
/// # Errors
///
/// As [`read`], for a file that exists.
pub fn read_if_exists(path: &Path) -> Result<Option<Expanded>, String> {
    if path.is_file() {
        read(path).map(Some)
    } else {
        Ok(None)
    }
}

/// [`read`] as TOML text, with `std::fs::read_to_string`'s signature, so a
/// reader migrates by changing one path: a missing file is still
/// `ErrorKind::NotFound` (with std's own error), and an expansion error is
/// `ErrorKind::InvalidData` carrying the message.
///
/// # Errors
///
/// As above.
pub fn read_to_string(path: impl AsRef<Path>) -> std::io::Result<String> {
    let path = path.as_ref();
    if !path.exists() {
        return std::fs::read_to_string(path);
    }
    read(path)
        .map(|e| e.text())
        .map_err(|m| std::io::Error::new(std::io::ErrorKind::InvalidData, m))
}

/// The text of `path` itself, unexpanded, with `std::fs::read_to_string`'s
/// signature: for a tool that EDITS the file and must write back only what
/// this file states. Never for reading what the system means.
///
/// # Errors
///
/// The file cannot be read.
pub fn read_own_text(path: impl AsRef<Path>) -> std::io::Result<String> {
    std::fs::read_to_string(path)
}

/// Does the file at `path` name any include? A road that cannot expand
/// includes asks this and refuses such a file, rather than reading it raw.
#[must_use]
pub fn has_include(path: &Path) -> bool {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|t| t.parse::<toml::Table>().ok())
        .is_some_and(|t| t.contains_key(INCLUDE_KEY))
}

fn expand(
    path: &Path,
    stack: &mut Vec<PathBuf>,
    files: &mut Vec<PathBuf>,
    origin: &mut BTreeMap<String, PathBuf>,
    is_root: bool,
) -> Result<toml::Table, String> {
    let canon = path
        .canonicalize()
        .map_err(|e| format!("{}: {e}", path.display()))?;
    if let Some(i) = stack.iter().position(|p| *p == canon) {
        let chain: Vec<String> = stack[i..]
            .iter()
            .chain(std::iter::once(&canon))
            .map(|p| p.display().to_string())
            .collect();
        return Err(format!(
            "{}: `include` reaches this file again through itself ({}); remove one of the \
             includes in that chain",
            path.display(),
            chain.join(" -> ")
        ));
    }
    if files.contains(&canon) {
        return Err(format!(
            "{}: this file is included twice (once directly or through another include); \
             include it from one place only, or its `[[...]]` entries would be counted twice",
            path.display()
        ));
    }
    files.push(canon.clone());
    let text = std::fs::read_to_string(&canon).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut own: toml::Table = text
        .parse()
        .map_err(|e| format!("{}: not valid TOML: {e}", path.display()))?;
    if !is_root {
        if own
            .get("system")
            .and_then(|s| s.as_table())
            .is_some_and(|s| s.contains_key("name"))
        {
            return Err(format!(
                "{}: an included file may not set `[system] name`; the system is named by the \
                 bringup's own system.toml, so move `name` there",
                path.display()
            ));
        }
        for own_only in ["sources", "tools"] {
            if own.contains_key(own_only) {
                return Err(format!(
                    "{}: an included file may not set `[{own_only}]`; it holds this machine's \
                     paths for the project, not shared topology, and build scripts read it from \
                     the bringup's own system.toml only. Move `[{own_only}]` into the bringup's \
                     own system.toml.",
                    path.display()
                ));
            }
        }
    }
    let includes = match own.remove(INCLUDE_KEY) {
        None => Vec::new(),
        Some(toml::Value::Array(a)) => a
            .into_iter()
            .map(|v| match v {
                toml::Value::String(s) => Ok(s),
                other => Err(format!(
                    "{}: `include` holds paths (strings), not `{other}`",
                    path.display()
                )),
            })
            .collect::<Result<Vec<_>, _>>()?,
        Some(other) => {
            return Err(format!(
                "{}: `include` is a list of paths, e.g. `include = [\"../shared.toml\"]`, \
                 not `{other}`",
                path.display()
            ));
        }
    };
    let dir = canon.parent().unwrap_or(Path::new("."));
    stack.push(canon.clone());
    let mut merged = toml::Table::new();
    for inc in &includes {
        let child = dir.join(inc);
        if !child.is_file() {
            stack.pop();
            return Err(format!(
                "{}: `include` names `{inc}`, but {} is not a file (an include is relative to \
                 the file that names it)",
                path.display(),
                child.display()
            ));
        }
        let t = expand(&child, stack, files, origin, false)?;
        let child_canon = child.canonicalize().unwrap_or(child);
        merge_into(&mut merged, t, "", &child_canon, origin).inspect_err(|_| {
            stack.pop();
        })?;
    }
    stack.pop();
    merge_into(&mut merged, own, "", &canon, origin)?;
    Ok(merged)
}

fn is_table_array(v: &toml::Value) -> bool {
    matches!(v, toml::Value::Array(a) if !a.is_empty() && a.iter().all(toml::Value::is_table))
}

/// Merge `src` (from `file`) into `dst`, recording where each leaf key came
/// from so a second statement can name both files.
fn merge_into(
    dst: &mut toml::Table,
    src: toml::Table,
    prefix: &str,
    file: &Path,
    origin: &mut BTreeMap<String, PathBuf>,
) -> Result<(), String> {
    for (k, v) in src {
        let key = if prefix.is_empty() {
            k.clone()
        } else {
            format!("{prefix}.{k}")
        };
        match (dst.get_mut(&k), v) {
            (None, v) => {
                record(&v, &key, file, origin);
                dst.insert(k, v);
            }
            (Some(toml::Value::Table(d)), toml::Value::Table(s)) => {
                merge_into(d, s, &key, file, origin)?;
            }
            (Some(d @ toml::Value::Array(_)), v) if is_table_array(d) && is_table_array(&v) => {
                if let (toml::Value::Array(d), toml::Value::Array(s)) = (d, v) {
                    d.extend(s);
                }
            }
            (Some(_), _) => {
                let first = origin
                    .get(&key)
                    .map_or_else(|| "another file".to_string(), |p| p.display().to_string());
                return Err(format!(
                    "`{key}` is stated in both {first} and {}; a key may be stated in one \
                     file only (an include does not override, and is not overridden), so \
                     remove one of them",
                    file.display()
                ));
            }
        }
    }
    Ok(())
}

fn record(v: &toml::Value, key: &str, file: &Path, origin: &mut BTreeMap<String, PathBuf>) {
    if let toml::Value::Table(t) = v {
        for (k, v) in t {
            record(v, &format!("{key}.{k}"), file, origin);
        }
    } else {
        origin.insert(key.to_string(), file.to_path_buf());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree(files: &[(&str, &str)]) -> tempfile::TempDir {
        let td = tempfile::tempdir().unwrap();
        for (p, t) in files {
            let p = td.path().join(p);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, t).unwrap();
        }
        td
    }

    fn read_in(td: &tempfile::TempDir, p: &str) -> Result<Expanded, String> {
        read(&td.path().join(p))
    }

    #[test]
    fn a_file_with_no_include_reads_as_itself() {
        let td = tree(&[("a/system.toml", "[system]\nname = \"a\"\n")]);
        let e = read_in(&td, "a/system.toml").unwrap();
        assert_eq!(e.table["system"]["name"].as_str(), Some("a"));
        assert_eq!(e.files.len(), 1);
    }

    #[test]
    fn includes_merge_tables_and_concatenate_table_arrays_in_order() {
        let td = tree(&[
            (
                "shared/topo.toml",
                "[[domain]]\nid = 1\n[knobs.zenoh]\nmax_subscribers = 4\n",
            ),
            ("shared/more.toml", "[[domain]]\nid = 2\n"),
            (
                "a/system.toml",
                "include = [\"../shared/topo.toml\", \"../shared/more.toml\"]\n\
                 [system]\nname = \"a\"\n[knobs.zenoh]\nmax_publishers = 2\n[[domain]]\nid = 3\n",
            ),
        ]);
        let e = read_in(&td, "a/system.toml").unwrap();
        let ids: Vec<i64> = e.table["domain"]
            .as_array()
            .unwrap()
            .iter()
            .map(|d| d["id"].as_integer().unwrap())
            .collect();
        assert_eq!(ids, [1, 2, 3]);
        assert_eq!(
            e.table["knobs"]["zenoh"]["max_subscribers"].as_integer(),
            Some(4)
        );
        assert_eq!(
            e.table["knobs"]["zenoh"]["max_publishers"].as_integer(),
            Some(2)
        );
        assert!(!e.table.contains_key(INCLUDE_KEY));
        assert_eq!(e.files.len(), 3);
    }

    #[test]
    fn a_key_stated_twice_is_refused_naming_both_files() {
        let td = tree(&[
            ("shared.toml", "[knobs.zenoh]\nmax_subscribers = 4\n"),
            (
                "a/system.toml",
                "include = [\"../shared.toml\"]\n[knobs.zenoh]\nmax_subscribers = 8\n",
            ),
        ]);
        let e = read_in(&td, "a/system.toml").unwrap_err();
        assert!(
            e.contains("`knobs.zenoh.max_subscribers` is stated in both")
                && e.contains("shared.toml")
                && e.contains("system.toml"),
            "{e}"
        );
    }

    #[test]
    fn nested_includes_resolve_against_their_own_file() {
        let td = tree(&[
            ("lib/deep/leaf.toml", "[knobs.log]\nmax_level = 2\n"),
            ("lib/mid.toml", "include = [\"deep/leaf.toml\"]\n"),
            ("a/system.toml", "include = [\"../lib/mid.toml\"]\n"),
        ]);
        let e = read_in(&td, "a/system.toml").unwrap();
        assert_eq!(e.table["knobs"]["log"]["max_level"].as_integer(), Some(2));
    }

    #[test]
    fn a_cycle_and_a_diamond_are_refused() {
        let td = tree(&[
            ("x.toml", "include = [\"y.toml\"]\n"),
            ("y.toml", "include = [\"x.toml\"]\n"),
            ("a/system.toml", "include = [\"../x.toml\"]\n"),
        ]);
        let e = read_in(&td, "a/system.toml").unwrap_err();
        assert!(e.contains("through itself"), "{e}");
        let td = tree(&[
            ("base.toml", "[[domain]]\nid = 1\n"),
            ("p.toml", "include = [\"base.toml\"]\n"),
            ("q.toml", "include = [\"base.toml\"]\n"),
            (
                "a/system.toml",
                "include = [\"../p.toml\", \"../q.toml\"]\n",
            ),
        ]);
        let e = read_in(&td, "a/system.toml").unwrap_err();
        assert!(e.contains("included twice"), "{e}");
    }

    #[test]
    fn an_include_may_not_name_the_system() {
        let td = tree(&[
            ("shared.toml", "[system]\nname = \"other\"\n"),
            ("a/system.toml", "include = [\"../shared.toml\"]\n"),
        ]);
        let e = read_in(&td, "a/system.toml").unwrap_err();
        assert!(e.contains("may not set `[system] name`"), "{e}");
    }

    #[test]
    fn an_include_may_not_set_this_machines_sources_or_tools() {
        for table in ["sources", "tools"] {
            let td = tree(&[
                ("shared.toml", &*format!("[{table}]\nx = \"/opt/x\"\n")),
                ("a/system.toml", "include = [\"../shared.toml\"]\n"),
            ]);
            let e = read_in(&td, "a/system.toml").unwrap_err();
            assert!(e.contains(&format!("may not set `[{table}]`")), "{e}");
        }
    }

    #[test]
    fn a_bad_include_value_or_missing_file_is_refused() {
        let td = tree(&[("a/system.toml", "include = \"../x.toml\"\n")]);
        assert!(
            read_in(&td, "a/system.toml")
                .unwrap_err()
                .contains("is a list of paths")
        );
        let td = tree(&[("a/system.toml", "include = [\"../missing.toml\"]\n")]);
        let e = read_in(&td, "a/system.toml").unwrap_err();
        assert!(e.contains("is not a file"), "{e}");
    }

    #[test]
    fn read_to_string_keeps_not_found_and_reports_bad_data() {
        let td = tree(&[("a/system.toml", "include = [\"../nope.toml\"]\n")]);
        let missing = read_to_string(td.path().join("none.toml")).unwrap_err();
        assert_eq!(missing.kind(), std::io::ErrorKind::NotFound);
        let bad = read_to_string(td.path().join("a/system.toml")).unwrap_err();
        assert_eq!(bad.kind(), std::io::ErrorKind::InvalidData);
        assert!(bad.to_string().contains("is not a file"), "{bad}");
    }

    #[test]
    fn has_include_and_own_text() {
        let td = tree(&[
            ("s.toml", "[x]\ny = 1\n"),
            ("a/system.toml", "include = [\"../s.toml\"]\n"),
        ]);
        let p = td.path().join("a/system.toml");
        assert!(has_include(&p));
        assert!(!has_include(&td.path().join("s.toml")));
        assert_eq!(read_own_text(&p).unwrap(), "include = [\"../s.toml\"]\n");
    }
}
