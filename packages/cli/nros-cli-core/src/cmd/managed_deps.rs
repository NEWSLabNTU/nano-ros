//! RFC-0098 amendment (2026-10-25, ACCEPTED 2026-10-26; phase-517 W8) — a
//! message `<depend>` is ONE edit for a Rust package too.
//!
//! `package.xml` already names every message package a node uses, and the C/C++
//! road reads it from there (`nros_generate_interfaces()`). The Rust road asked
//! the user to say it a second time, as a path row into the workspace's
//! `generated/` tree. Cargo offers no way to ADD a dependency from outside the
//! manifest — `--config` cannot (`[dependencies]` is not a config table, and
//! `[patch]` only replaces a dependency that already exists), a manifest has no
//! `include`, and a wrapper crate changes every `use` path — so `nros sync`
//! edits the package's own `[dependencies]`, the `cargo add` shape, with
//! `package.xml` as the command line.
//!
//! Every row it owns carries the same `# nros-managed` decor marker as the
//! sync-written `[patch.crates-io]` rows (issue 0457), read through the same
//! predicate ([`crate::cmd::ws::item_is_nros_managed`]). The rules:
//!
//! * a declared message dependency with no row gets one, tagged;
//! * a tagged row whose `<depend>` is gone is removed;
//! * an UNTAGGED row is never touched — that is the user's override (delete
//!   the tag to take a row over), and a user row anywhere in the manifest for
//!   the same crate suppresses the managed one;
//! * a tagged row whose dependency is still declared but did not resolve to a
//!   message package on THIS host is left as it is: resolution is
//!   host-dependent (an ament install, the bundled set), and removing a row
//!   because one host could not see the package would churn a tracked file;
//! * no byte is written when nothing changed, so a second sync leaves every
//!   manifest's mtime alone (fixtures key on mtimes).
//!
//! The `nros` row stays AUTHORED (maintainer ruling 2026-10-26): it carries a
//! feature choice a user can reasonably make.

use std::collections::{BTreeMap, BTreeSet};

use eyre::{Result, WrapErr};
use toml_edit::{DocumentMut, InlineTable, Item, Table, Value, value};

use crate::cmd::ws::{NROS_MANAGED_TAG, item_is_nros_managed};

/// The dependency tables a crate can be named in. A user row in ANY of them
/// (or under a `[target.'cfg(..)'.*]` table) means the user spelled this
/// dependency themselves, so no managed row is added beside it.
const DEP_TABLES: [&str; 3] = ["dependencies", "dev-dependencies", "build-dependencies"];

/// The `package.xml` tags whose packages a Rust package compiles against.
pub(crate) const MANAGED_DEPEND_TAGS: [&str; 3] = ["<depend>", "<build_depend>", "<exec_depend>"];

/// Crate names an untagged row in `tbl` provides: its key, a `package = ".."`
/// rename's target, and the last component of a `path` into `generated/`.
fn user_provided_in(tbl: &Table, out: &mut BTreeSet<String>) {
    for (k, item) in tbl.iter() {
        if item_is_nros_managed(item) {
            continue;
        }
        out.insert(k.to_string());
        let get = |key: &str| -> Option<String> {
            match item {
                Item::Value(Value::InlineTable(t)) => {
                    t.get(key).and_then(|v| v.as_str()).map(str::to_string)
                }
                Item::Table(t) => t.get(key).and_then(|v| v.as_str()).map(str::to_string),
                _ => None,
            }
        };
        if let Some(p) = get("package") {
            out.insert(p);
        }
        if let Some(p) = get("path") {
            let mut parts = p.trim_end_matches('/').rsplit('/');
            if let (Some(last), Some("generated")) = (parts.next(), parts.next()) {
                out.insert(last.to_string());
            }
        }
    }
}

fn user_provided(doc: &DocumentMut) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for t in DEP_TABLES {
        if let Some(tbl) = doc.get(t).and_then(Item::as_table) {
            user_provided_in(tbl, &mut out);
        }
    }
    if let Some(targets) = doc.get("target").and_then(Item::as_table) {
        for (_, cfg) in targets.iter() {
            let Some(cfg) = cfg.as_table() else { continue };
            for t in DEP_TABLES {
                if let Some(tbl) = cfg.get(t).and_then(Item::as_table) {
                    user_provided_in(tbl, &mut out);
                }
            }
        }
    }
    out
}

/// The one spelling of a managed row's value.
fn managed_value(rel: &str) -> Item {
    let mut it = InlineTable::new();
    it.insert("path", Value::from(rel));
    it.insert("default-features", Value::from(false));
    let mut item = value(Value::InlineTable(it));
    if let Some(v) = item.as_value_mut() {
        v.decor_mut().set_suffix(format!("  # {NROS_MANAGED_TAG}"));
    }
    item
}

/// Does an existing tagged row already say exactly what [`managed_value`] would?
fn is_current(item: &Item, rel: &str) -> bool {
    let Some(t) = item.as_inline_table() else {
        return false;
    };
    t.len() == 2
        && t.get("path").and_then(|v| v.as_str()) == Some(rel)
        && t.get("default-features").and_then(|v| v.as_bool()) == Some(false)
}

/// Pure DOM transform: rewrite the managed message rows of one `Cargo.toml`.
///
/// `declared` is every name the package's `package.xml` declares through
/// [`MANAGED_DEPEND_TAGS`]; `desired` maps the subset that resolved to a
/// message package onto its path relative to the package directory. Returns
/// the input unchanged (byte for byte) when nothing differs.
pub(crate) fn render(
    existing: &str,
    declared: &BTreeSet<String>,
    desired: &BTreeMap<String, String>,
) -> Result<String> {
    let mut doc: DocumentMut = existing.parse().wrap_err("parse Cargo.toml")?;
    let provided = user_provided(&doc);

    let mut changed = false;
    let has_deps = doc.get("dependencies").and_then(Item::as_table).is_some();
    if has_deps {
        let deps = doc["dependencies"].as_table_mut().expect("checked above");
        let tagged: Vec<String> = deps
            .iter()
            .filter(|(_, v)| item_is_nros_managed(v))
            .map(|(k, _)| k.to_string())
            .collect();
        for name in tagged {
            if !declared.contains(&name) {
                deps.remove(&name);
                changed = true;
            } else if let Some(rel) = desired.get(&name)
                && !is_current(&deps[name.as_str()], rel)
            {
                // Replace the VALUE only, so the key keeps its position and
                // any comment above it.
                deps[name.as_str()] = managed_value(rel);
                changed = true;
            }
        }
    }

    let missing: Vec<(&String, &String)> = desired
        .iter()
        .filter(|(n, _)| !provided.contains(*n))
        .filter(|(n, _)| {
            !doc.get("dependencies")
                .and_then(Item::as_table)
                .is_some_and(|t| t.contains_key(n))
        })
        .collect();
    if !missing.is_empty() {
        let deps = doc
            .as_table_mut()
            .entry("dependencies")
            .or_insert_with(|| Item::Table(Table::new()))
            .as_table_mut()
            .ok_or_else(|| eyre::eyre!("[dependencies] is not a table"))?;
        for (name, rel) in missing {
            deps.insert(name, managed_value(rel));
        }
        changed = true;
    }

    Ok(if changed {
        doc.to_string()
    } else {
        existing.to_string()
    })
}

/// Read, render and — only when the text differs — atomically rewrite one
/// manifest. Returns whether the file was written.
pub(crate) fn write(
    manifest: &std::path::Path,
    declared: &BTreeSet<String>,
    desired: &BTreeMap<String, String>,
    dry_run: bool,
) -> Result<bool> {
    let text = std::fs::read_to_string(manifest)
        .wrap_err_with(|| format!("managed deps: read {}", manifest.display()))?;
    let out = render(&text, declared, desired)
        .wrap_err_with(|| format!("managed deps: edit {}", manifest.display()))?;
    if out == text {
        return Ok(false);
    }
    if !dry_run {
        crate::atomic_file::atomic_write(manifest, &out)?;
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set(xs: &[&str]) -> BTreeSet<String> {
        xs.iter().map(|s| s.to_string()).collect()
    }
    fn map(xs: &[(&str, &str)]) -> BTreeMap<String, String> {
        xs.iter()
            .map(|(a, b)| (a.to_string(), b.to_string()))
            .collect()
    }

    const BASE: &str = "[package]\nname = \"p\"\n\n# authored\n[dependencies]\n\
nros = { path = \"../nros\", features = [\"alloc\"] }\n\
heapless = \"0.8\" # mine\n";

    #[test]
    fn a_declared_message_dep_gains_one_tagged_row() {
        let out = render(
            BASE,
            &set(&["std_msgs"]),
            &map(&[("std_msgs", "../../generated/std_msgs")]),
        )
        .unwrap();
        assert!(
            out.ends_with(
                "heapless = \"0.8\" # mine\n\
std_msgs = { path = \"../../generated/std_msgs\", default-features = false }  # nros-managed\n"
            ),
            "{out}"
        );
        assert!(out.starts_with("[package]\nname = \"p\"\n\n# authored\n[dependencies]\n"));
    }

    #[test]
    fn a_removed_depend_removes_its_tagged_row_only() {
        let with = render(
            BASE,
            &set(&["std_msgs"]),
            &map(&[("std_msgs", "../../generated/std_msgs")]),
        )
        .unwrap();
        let without = render(&with, &set(&[]), &map(&[])).unwrap();
        assert_eq!(without, BASE);
    }

    #[test]
    fn an_untagged_row_is_never_touched() {
        let user = format!("{BASE}std_msgs = {{ path = \"elsewhere/std_msgs\" }}\n");
        // Declared AND resolved, but the user spells it: no managed row.
        let out = render(
            &user,
            &set(&["std_msgs"]),
            &map(&[("std_msgs", "../../generated/std_msgs")]),
        )
        .unwrap();
        assert_eq!(out, user);
        // Undeclared: still the user's, so still not removed.
        assert_eq!(render(&user, &set(&[]), &map(&[])).unwrap(), user);
    }

    #[test]
    fn a_user_row_in_another_table_or_renamed_suppresses_the_managed_row() {
        let renamed = format!("{BASE}msgs = {{ package = \"std_msgs\", path = \"x\" }}\n");
        let d = map(&[("std_msgs", "../generated/std_msgs")]);
        assert_eq!(render(&renamed, &set(&["std_msgs"]), &d).unwrap(), renamed);
        let target = format!(
            "{BASE}\n[target.'cfg(unix)'.dependencies]\nstd_msgs = {{ path = \"../generated/std_msgs\" }}\n"
        );
        assert_eq!(render(&target, &set(&["std_msgs"]), &d).unwrap(), target);
    }

    #[test]
    fn a_second_render_is_byte_identical() {
        let d = map(&[
            ("std_msgs", "../../generated/std_msgs"),
            ("example_interfaces", "../../generated/example_interfaces"),
        ]);
        let once = render(BASE, &set(&["std_msgs", "example_interfaces"]), &d).unwrap();
        let twice = render(&once, &set(&["std_msgs", "example_interfaces"]), &d).unwrap();
        assert_eq!(once, twice);
    }

    #[test]
    fn a_stale_tagged_path_is_rewritten_in_place() {
        let stale = format!(
            "{BASE}std_msgs = {{ path = \"../generated/std_msgs\" }}  # nros-managed\nz = \"1\"\n"
        );
        let out = render(
            &stale,
            &set(&["std_msgs"]),
            &map(&[("std_msgs", "../../generated/std_msgs")]),
        )
        .unwrap();
        assert!(
            out.ends_with(
                "std_msgs = { path = \"../../generated/std_msgs\", default-features = false }  # nros-managed\nz = \"1\"\n"
            ),
            "{out}"
        );
    }

    #[test]
    fn a_declared_but_unresolved_tagged_row_is_kept() {
        let tagged = format!(
            "{BASE}px4_msgs = {{ path = \"generated/px4_msgs\", default-features = false }}  # nros-managed\n"
        );
        assert_eq!(
            render(&tagged, &set(&["px4_msgs"]), &map(&[])).unwrap(),
            tagged
        );
    }

    #[test]
    fn a_manifest_without_dependencies_gains_the_table() {
        let bare = "[package]\nname = \"p\"\n";
        let out = render(
            bare,
            &set(&["std_msgs"]),
            &map(&[("std_msgs", "generated/std_msgs")]),
        )
        .unwrap();
        assert_eq!(
            out,
            "[package]\nname = \"p\"\n\n[dependencies]\nstd_msgs = { path = \"generated/std_msgs\", default-features = false }  # nros-managed\n"
        );
        // …and nothing is added when nothing is desired.
        assert_eq!(render(bare, &set(&["x"]), &map(&[])).unwrap(), bare);
    }
}
