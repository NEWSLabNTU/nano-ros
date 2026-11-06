//! `nros dev migrate-bringup <dir>` — the one-shot of RFC-0121 D8 (phase-531
//! W6).
//!
//! It turns a standalone package (a directory holding code AND a
//! `system.toml`) into a single-node workspace:
//!
//! ```text
//! <dir>/                       <dir>/
//! ├── Cargo.toml               ├── README.md, .gitignore   (stay)
//! ├── package.xml       ==>    └── src/
//! ├── system.toml                  ├── <pkg>/              the node package
//! ├── src/{lib,main}.rs            └── <pkg>_bringup/      package.xml + system.toml
//! └── README.md
//! ```
//!
//! What it does, and what it leaves to a person:
//!
//! * The bringup gets `system.toml` and every file that file names (an
//!   `os_config` fragment, a board-net table), plus `launch/`, `boards/` and
//!   the `*.contract.yaml` sidecars, which are read beside `system.toml`.
//! * A `main.rs` that only calls `nros::main!` is deleted: the generated entry
//!   writes that call. Any other `main` (a C `int main`, a C++ `rclcpp::init`,
//!   a Rust `main` with logic) is moved with the node and REPORTED, for a
//!   person to turn into a component (RFC-0122 D10).
//! * The selection-crate dependency, the `[[bin]]` that named the deleted
//!   `main` and the `[workspace]` table are removed from the manifest, and its
//!   relative `path =` dependencies are re-rooted for the new location.
//! * A Zephyr west application whose node is already its own package (a
//!   subdirectory with a `package.xml`) keeps that package as the node, and
//!   its west glue (`CMakeLists.txt`, the image crate, `sample.yaml`) is
//!   deleted: the generated west application replaces it.
//! * With `--fixtures`, every `[[fixture]]` row naming the directory becomes a
//!   `[[workspace_fixture]]` row on the image that builds the same
//!   coordinate. `platform`, `lang` and `rmw` are kept, so `row_coord()` and
//!   lane membership do not move. A row this cannot map (a cargo `features`
//!   list, an RMW no image builds) is left as it is and reported.
//!
//! RFC-0121 D8: this is kept in `scripts/oneshot/` once the tree is migrated.

use std::path::{Component, Path, PathBuf};

use clap::Args as ClapArgs;
use eyre::{Result, WrapErr, bail, eyre};

#[derive(Debug, Clone, ClapArgs)]
pub struct Args {
    /// The standalone package directory to migrate.
    pub dir: PathBuf,
    /// Print what would move and change, and touch nothing.
    #[arg(long)]
    pub dry_run: bool,
    /// A fixture manifest whose rows naming `<dir>` are rewritten.
    #[arg(long)]
    pub fixtures: Option<PathBuf>,
    /// The directory `--fixtures`' `dir =` values are relative to (default:
    /// the manifest's parent's parent, i.e. the repository root for
    /// `examples/fixtures.toml`).
    #[arg(long)]
    pub fixtures_root: Option<PathBuf>,
}

/// One planned file operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Op {
    Move { from: PathBuf, to: PathBuf },
    Delete(PathBuf),
    Write { path: PathBuf, text: String },
}

/// What a migration does, computed before anything is touched.
#[derive(Debug, Default)]
pub struct Plan {
    pub node: String,
    pub bringup: String,
    pub ops: Vec<Op>,
    /// Things a person has to finish.
    pub report: Vec<String>,
}

/// Top-level names that stay at the workspace root.
fn stays_at_root(name: &str) -> bool {
    name.starts_with("README") || name.starts_with("LICENSE") || name == ".gitignore"
}

/// Build output and per-host state: never moved, never deleted.
fn is_build_output(name: &str) -> bool {
    matches!(
        name,
        "build" | "generated" | "metadata" | "target" | ".cargo" | "Cargo.lock" | "nros-plan.json"
    ) || name.starts_with("target-")
        || name.starts_with("build-")
}

/// Bringup files read beside `system.toml` by name.
fn is_bringup_by_name(name: &str) -> bool {
    name == "system.toml"
        || name == "launch"
        || name == "boards"
        || name.ends_with(".contract.yaml")
        || (name.starts_with("board-net") && name.ends_with(".toml"))
}

fn package_name(xml: &str) -> Option<String> {
    let start = xml.find("<name>")? + "<name>".len();
    let end = xml[start..].find("</name>")? + start;
    Some(xml[start..end].trim().to_string())
}

/// Every string in a TOML value tree.
fn strings_of(v: &toml::Value, out: &mut Vec<String>) {
    match v {
        toml::Value::String(s) => out.push(s.clone()),
        toml::Value::Array(a) => a.iter().for_each(|x| strings_of(x, out)),
        toml::Value::Table(t) => t.values().for_each(|x| strings_of(x, out)),
        _ => {}
    }
}

/// The top-level entries of `dir` that `system.toml` names as a relative path.
fn named_by_system_toml(dir: &Path, system: &toml::Value) -> Vec<String> {
    let mut strings = Vec::new();
    strings_of(system, &mut strings);
    let mut out: Vec<String> = strings
        .iter()
        .filter(|s| !s.is_empty() && !s.starts_with('/') && !s.contains("://"))
        .filter_map(|s| {
            let first = Path::new(s).components().next()?;
            let Component::Normal(first) = first else {
                return None;
            };
            let first = first.to_string_lossy().into_owned();
            dir.join(&first).exists().then_some(first)
        })
        .collect();
    out.sort();
    out.dedup();
    out
}

/// A Rust `main.rs` that only calls `nros::main!`, with arguments the
/// generated entry states itself: `spin = "forever"` (a hosted entry spins
/// forever, `builder::entry`) and `panic = "<policy>"` (an image's `panic`,
/// RFC-0077). `#![no_std]` / `#![no_main]` are the entry's too.
///
/// `Some(panic)` when it is such a main (`Some(None)`: no panic argument),
/// `None` when it does more.
fn trivial_rust_main(text: &str) -> Option<Option<String>> {
    let code: String = text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with("//"))
        .filter(|l| *l != "#![no_std]" && *l != "#![no_main]")
        .collect::<Vec<_>>()
        .join(" ");
    let call = regex::Regex::new(r"^nros::main!\s*\((.*)\)\s*;?$").expect("static regex");
    let args = call.captures(&code)?.get(1)?.as_str().trim().to_string();
    let arg = regex::Regex::new(r#"^\s*(spin|panic)\s*=\s*"([^"]*)"\s*$"#).expect("static regex");
    let mut panic = None;
    for a in args.split(',').filter(|a| !a.trim().is_empty()) {
        let c = arg.captures(a)?;
        match (&c[1], &c[2]) {
            ("spin", "forever") => {}
            ("panic", p) => panic = Some(p.to_string()),
            _ => return None,
        }
    }
    Some(panic)
}

/// `system.toml` with `panic = "<policy>"` stated for every image that does
/// not state one, through `[image_defaults]`; `None` when every image already
/// states that policy.
fn with_image_panic(system_text: &str, policy: &str) -> Result<Option<String>> {
    let mut doc: toml_edit::DocumentMut = system_text.parse().wrap_err("parsing system.toml")?;
    let default = doc
        .get("image_defaults")
        .and_then(|d| d.get("panic"))
        .and_then(|p| p.as_str())
        .map(str::to_string);
    let images: Vec<Option<String>> = doc
        .get("image")
        .and_then(|i| i.as_table())
        .map(|t| {
            t.iter()
                .map(|(_, img)| {
                    img.get("panic")
                        .and_then(|p| p.as_str())
                        .map(str::to_string)
                        .or_else(|| default.clone())
                })
                .collect()
        })
        .unwrap_or_default();
    if images.iter().all(|p| p.as_deref() == Some(policy)) {
        return Ok(None);
    }
    if images
        .iter()
        .any(|p| p.as_deref().is_some_and(|p| p != policy))
    {
        bail!(
            "`nros::main!(panic = \"{policy}\")` disagrees with an image's `panic` in \
             system.toml; reconcile them before migrating"
        );
    }
    if doc.get("image_defaults").is_none() {
        doc["image_defaults"] = toml_edit::table();
    }
    doc["image_defaults"]["panic"] = toml_edit::value(policy);
    Ok(Some(doc.to_string()))
}

/// A C or C++ source that defines `main`.
fn defines_c_main(text: &str) -> bool {
    let re = regex::Regex::new(r"(?m)^\s*(int|void)\s+main\s*\(").expect("static regex");
    re.is_match(text)
}

/// Lexically normalised `base/rel`.
fn normalise(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in path.components() {
        match c {
            Component::ParentDir => {
                if !out.pop() {
                    out.push("..");
                }
            }
            Component::CurDir => {}
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// `target` relative to `from`, both lexical paths under one root.
fn relative(target: &Path, from: &Path) -> PathBuf {
    let t: Vec<_> = target.components().collect();
    let f: Vec<_> = from.components().collect();
    let common = t.iter().zip(&f).take_while(|(a, b)| a == b).count();
    let mut out = PathBuf::new();
    for _ in common..f.len() {
        out.push("..");
    }
    for c in &t[common..] {
        out.push(c.as_os_str());
    }
    out
}

/// Re-root one relative `path =` value written in `old_dir` for `new_dir`.
fn reroot(value: &str, old_dir: &Path, new_dir: &Path) -> String {
    if Path::new(value).is_absolute() {
        return value.to_string();
    }
    let target = normalise(&old_dir.join(value));
    relative(&target, &normalise(new_dir))
        .to_string_lossy()
        .into_owned()
}

/// The node manifest after the move: no selection crate, no `[[bin]]` for a
/// deleted `main`, no `[workspace]`, relative paths re-rooted.
///
/// The package is also renamed to its ROS name: a workspace's generated entry
/// depends on a node package by its `package.xml` name, so the two must agree
/// (`talker_pkg` names both in `examples/workspaces/rust`). The lib target
/// follows, and the old crate ident is returned so the caller can rewrite the
/// `[[component]] class` paths that name it.
fn rewrite_cargo_toml(
    text: &str,
    old_dir: &Path,
    new_dir: &Path,
    deleted_bin_path: Option<&str>,
    ros_name: &str,
    report: &mut Vec<String>,
) -> Result<(String, Option<String>)> {
    let mut doc: toml_edit::DocumentMut = text.parse().wrap_err("parsing Cargo.toml")?;
    doc.remove("workspace");
    let cargo_name = doc
        .get("package")
        .and_then(|p| p.get("name"))
        .and_then(|n| n.as_str())
        .map(str::to_string);
    let lib_name = doc
        .get("lib")
        .and_then(|l| l.get("name"))
        .and_then(|n| n.as_str())
        .map(str::to_string);
    let old_ident = lib_name
        .clone()
        .or_else(|| cargo_name.as_ref().map(|n| n.replace('-', "_")));
    let mut renamed = None;
    if let Some(name) = &cargo_name
        && name != ros_name
    {
        doc["package"]["name"] = toml_edit::value(ros_name);
        report.push(format!(
            "renamed the crate `{name}` to `{ros_name}`, its package.xml name: the \
             generated entry depends on a node package by that name"
        ));
    }
    if lib_name.as_deref().is_some_and(|l| l != ros_name) {
        doc["lib"]["name"] = toml_edit::value(ros_name);
    }
    if let Some(old) = old_ident
        && old != ros_name
    {
        renamed = Some(old);
    }
    if let Some(bin_path) = deleted_bin_path
        && let Some(bins) = doc.get_mut("bin").and_then(|b| b.as_array_of_tables_mut())
    {
        bins.retain(|t| t.get("path").and_then(|p| p.as_str()) != Some(bin_path));
        if bins.is_empty() {
            doc.remove("bin");
        }
    }
    let mut edit = |deps: &mut toml_edit::Table| {
        let selection: Vec<String> = deps
            .iter()
            .map(|(k, _)| k.to_string())
            .filter(|k| k.ends_with("_nros_selection") || k.ends_with("_nros_derived"))
            .collect();
        for k in selection {
            deps.remove(&k);
            // RFC-0071 D13: a node package names no selection or adapter crate.
            report.push(format!(
                "removed the `{k}` dependency: a node package names no selection crate"
            ));
        }
        for (_, spec) in deps.iter_mut() {
            let path = match spec {
                toml_edit::Item::Value(toml_edit::Value::InlineTable(t)) => t.get_mut("path"),
                toml_edit::Item::Table(t) => t.get_mut("path").and_then(|i| i.as_value_mut()),
                _ => None,
            };
            if let Some(p) = path
                && let Some(s) = p.as_str()
            {
                let new = reroot(s, old_dir, new_dir);
                if new != s {
                    let decor = p.decor().clone();
                    *p = toml_edit::Value::from(new);
                    *p.decor_mut() = decor;
                }
            }
        }
    };
    // Every dependency table, `[target.*]` included.
    const DEP_TABLES: [&str; 3] = ["dependencies", "build-dependencies", "dev-dependencies"];
    for key in DEP_TABLES {
        if let Some(t) = doc.get_mut(key).and_then(|t| t.as_table_mut()) {
            edit(t);
        }
    }
    if let Some(targets) = doc.get_mut("target").and_then(|t| t.as_table_mut()) {
        for (_, t) in targets.iter_mut() {
            for key in DEP_TABLES {
                if let Some(d) = t.get_mut(key).and_then(|d| d.as_table_mut()) {
                    edit(d);
                }
            }
        }
    }
    Ok((doc.to_string(), renamed))
}

/// `system.toml` with every `[[component]] class` path that starts with
/// `old::` restated under `new::`.
fn rename_class_paths(system_text: &str, old: &str, new: &str) -> Result<String> {
    let mut doc: toml_edit::DocumentMut = system_text.parse().wrap_err("parsing system.toml")?;
    if let Some(rows) = doc
        .get_mut("component")
        .and_then(|c| c.as_array_of_tables_mut())
    {
        for row in rows.iter_mut() {
            let renamed = row
                .get("class")
                .and_then(|c| c.as_str())
                .and_then(|c| c.strip_prefix(&format!("{old}::")))
                .map(|rest| format!("{new}::{rest}"));
            if let Some(class) = renamed {
                row["class"] = toml_edit::value(class);
            }
        }
    }
    Ok(doc.to_string())
}

/// Plan the migration of `dir`. Pure: reads, never writes.
pub fn plan(dir: &Path) -> Result<Plan> {
    let system_path = dir.join("system.toml");
    if !system_path.is_file() {
        bail!(
            "{} has no system.toml: there is nothing to migrate",
            dir.display()
        );
    }
    let has_cargo = dir.join("Cargo.toml").is_file();
    let has_cmake = dir.join("CMakeLists.txt").is_file();
    if !has_cargo && !has_cmake {
        bail!(
            "{} holds a system.toml but no Cargo.toml or CMakeLists.txt: it is \
             already a bringup",
            dir.display()
        );
    }
    let xml = std::fs::read_to_string(dir.join("package.xml"))
        .wrap_err_with(|| format!("reading {}/package.xml", dir.display()))?;
    let outer = package_name(&xml).ok_or_else(|| eyre!("package.xml names no <name>"))?;
    let system_text = std::fs::read_to_string(&system_path)?;
    let system: toml::Value = system_text
        .parse()
        .wrap_err_with(|| format!("parsing {}", system_path.display()))?;

    let cmake_text = std::fs::read_to_string(dir.join("CMakeLists.txt")).unwrap_or_default();
    let is_west = cmake_text.contains("find_package(Zephyr");

    // A west application whose node already has its own package.
    let nested_node: Option<(String, String)> = if is_west {
        std::fs::read_dir(dir)?
            .flatten()
            .filter(|e| e.path().is_dir())
            .filter_map(|e| {
                let name = e.file_name().to_string_lossy().into_owned();
                let xml = std::fs::read_to_string(e.path().join("package.xml")).ok()?;
                Some((name, package_name(&xml)?))
            })
            .find(|(sub, _)| sub != "src")
    } else {
        None
    };

    let node = nested_node
        .as_ref()
        .map_or_else(|| outer.clone(), |(_, n)| n.clone());
    let bringup = crate::cmd::new_workspace::bringup_name(&node);
    let src = dir.join("src");
    let node_dir = src.join(&node);
    let bringup_dir = src.join(&bringup);
    if node_dir.join("package.xml").is_file() || bringup_dir.join("system.toml").is_file() {
        bail!("{} is already migrated", dir.display());
    }

    let mut plan = Plan {
        node: node.clone(),
        bringup: bringup.clone(),
        ..Plan::default()
    };
    let named = named_by_system_toml(dir, &system);

    let mut entries: Vec<String> = std::fs::read_dir(dir)?
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    entries.sort();

    let mut moved_main: Option<String> = None;
    let mut system_edit: Option<String> = None;
    for name in &entries {
        let path = dir.join(name);
        if stays_at_root(name) {
            continue;
        }
        if is_build_output(name) {
            plan.report.push(format!(
                "left `{name}` in place: build output, regenerated by `nros build`"
            ));
            continue;
        }
        if is_bringup_by_name(name) || named.contains(name) {
            plan.ops.push(Op::Move {
                from: path.clone(),
                to: bringup_dir.join(name),
            });
            continue;
        }
        // Zephyr configuration nothing in `system.toml` names (the base
        // `prj.conf`, an overlay, a Kconfig fragment) is the image's, not the
        // node's: it follows the bringup, where a person decides whether the
        // generated west application should read it (`boards/<board>/`).
        if is_west
            && (name.ends_with(".conf")
                || name.ends_with(".overlay")
                || name.starts_with("Kconfig"))
        {
            plan.ops.push(Op::Move {
                from: path.clone(),
                to: bringup_dir.join(name),
            });
            plan.report.push(format!(
                "moved `{name}` into the bringup: system.toml does not name it, so the \
                 generated west application reads it only from `boards/<board>/` or an \
                 `os_config` entry"
            ));
            continue;
        }
        if let Some((sub, _)) = &nested_node {
            if name == sub {
                plan.ops.push(Op::Move {
                    from: path.clone(),
                    to: node_dir.clone(),
                });
            } else {
                // The west glue: the generated west application replaces it.
                plan.ops.push(Op::Delete(path.clone()));
                plan.report.push(format!(
                    "deleted the west glue `{name}`: the generated west application \
                     replaces it"
                ));
            }
            continue;
        }
        // Everything else is the node's: its manifest, package.xml, sources.
        if name == "src" {
            continue; // handled file by file below
        }
        plan.ops.push(Op::Move {
            from: path.clone(),
            to: node_dir.join(name),
        });
    }

    if nested_node.is_none() && src.is_dir() {
        for e in std::fs::read_dir(&src)?.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            let from = e.path();
            let rel = format!("src/{name}");
            if name == "main.rs" {
                let text = std::fs::read_to_string(&from).unwrap_or_default();
                if let Some(panic) = trivial_rust_main(&text) {
                    if let Some(policy) = panic
                        && let Some(edited) = with_image_panic(&system_text, &policy)?
                    {
                        system_edit = Some(edited);
                        plan.report.push(format!(
                            "stated `panic = \"{policy}\"` in `[image_defaults]`: the \
                             deleted main passed it to `nros::main!`"
                        ));
                    }
                    plan.ops.push(Op::Delete(from));
                    plan.report.push(
                        "deleted src/main.rs: it only called `nros::main!`, which the \
                         generated entry writes"
                            .to_string(),
                    );
                    moved_main = Some(rel);
                    continue;
                }
                plan.report.push(format!(
                    "src/{node}/src/main.rs does more than call `nros::main!`: move its \
                     logic into the node's component and delete it"
                ));
            } else if from.is_file()
                && matches!(
                    nros_lang::Language::of_source(&name),
                    Ok(nros_lang::SourceVerdict::Decides(
                        nros_lang::Language::C | nros_lang::Language::Cpp
                    ))
                )
                && defines_c_main(&std::fs::read_to_string(&from).unwrap_or_default())
            {
                plan.report.push(format!(
                    "src/{node}/src/{name} defines `main`: a node package is a component, \
                     so its logic moves into a registered component and the generated \
                     entry boots it"
                ));
            }
            plan.ops.push(Op::Move {
                from,
                to: node_dir.join("src").join(&name),
            });
        }
    }

    // The node manifest, re-rooted.
    let (manifest_from, manifest_old_dir) = match &nested_node {
        Some((sub, _)) => (dir.join(sub).join("Cargo.toml"), dir.join(sub)),
        None => (dir.join("Cargo.toml"), dir.to_path_buf()),
    };
    if manifest_from.is_file() && (nested_node.is_some() || has_cargo) {
        let text = std::fs::read_to_string(&manifest_from)?;
        let (rewritten, renamed) = rewrite_cargo_toml(
            &text,
            &manifest_old_dir,
            &node_dir,
            moved_main.as_deref(),
            &node,
            &mut plan.report,
        )?;
        if let Some(old) = renamed {
            let base = system_edit.clone().unwrap_or_else(|| system_text.clone());
            let edited = rename_class_paths(&base, &old, &node)?;
            if edited != base {
                plan.report.push(format!(
                    "restated `[[component]] class` paths from `{old}::` to `{node}::`"
                ));
                system_edit = Some(edited);
            }
        }
        plan.ops.push(Op::Write {
            path: node_dir.join("Cargo.toml"),
            text: rewritten,
        });
    }
    if has_cmake && !is_west && cmake_text.contains("../") {
        plan.report.push(format!(
            "src/{node}/CMakeLists.txt names a relative `../` path; check it from its \
             new location"
        ));
    }
    if is_west && nested_node.is_none() {
        // A C/C++ west application registers its component in the same
        // CMakeLists.txt that finds Zephyr. The node package keeps the
        // registration; the generated west application finds Zephyr.
        let kept: Vec<&str> = cmake_text
            .lines()
            .filter(|l| !l.trim_start().starts_with("find_package(Zephyr"))
            .collect();
        let mut text = kept.join("\n");
        text.push('\n');
        plan.ops.push(Op::Write {
            path: node_dir.join("CMakeLists.txt"),
            text,
        });
        plan.report.push(format!(
            "dropped `find_package(Zephyr)` from src/{node}/CMakeLists.txt: the generated \
             west application finds Zephyr, and a node package is a component library"
        ));
    }
    if node != outer && nested_node.is_none() {
        unreachable!("the node is the outer package unless a nested one was found");
    }

    if let Some(text) = system_edit {
        plan.ops.push(Op::Write {
            path: bringup_dir.join("system.toml"),
            text,
        });
    }
    plan.ops.push(Op::Write {
        path: bringup_dir.join("package.xml"),
        text: crate::cmd::new_workspace::bringup_package_xml(&bringup, &node),
    });
    Ok(plan)
}

/// Apply a plan. Deletes first (a west application's glue `src/` must be gone
/// before its node moves into `src/<node>`), then moves, then writes, so a
/// written manifest lands over its moved original.
pub fn apply(plan: &Plan) -> Result<()> {
    for op in &plan.ops {
        if let Op::Delete(p) = op {
            if p.is_dir() {
                std::fs::remove_dir_all(p)?;
            } else if p.exists() {
                std::fs::remove_file(p)?;
            }
        }
    }
    for op in &plan.ops {
        if let Op::Move { from, to } = op {
            if let Some(parent) = to.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::rename(from, to)
                .wrap_err_with(|| format!("moving {} to {}", from.display(), to.display()))?;
        }
    }
    for op in &plan.ops {
        if let Op::Write { path, text } = op {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(path, text)?;
        }
    }
    Ok(())
}

// ---- fixture rows -----------------------------------------------------------

/// The image of `system` that builds `rmw`, by the build's own ladder:
/// `[image.<id>] rmw`, else `[image_defaults] rmw`, else `[system] rmw`, else
/// zenoh. Among several, the one with no `ros_edition` of its own; then the
/// one `default_images` names.
pub fn image_for_rmw(system: &toml::Value, rmw: &str) -> std::result::Result<String, String> {
    let str_at = |v: Option<&toml::Value>, k: &str| {
        v.and_then(|t| t.get(k))
            .and_then(|s| s.as_str())
            .map(str::to_string)
    };
    let defaults = system.get("image_defaults");
    let sys = system.get("system");
    let fallback_rmw = str_at(defaults, "rmw")
        .or_else(|| str_at(sys, "rmw"))
        .unwrap_or_else(|| "zenoh".to_string());
    let fallback_edition = str_at(defaults, "ros_edition");
    let images = system
        .get("image")
        .and_then(|i| i.as_table())
        .cloned()
        .unwrap_or_default();
    let mut matching: Vec<(String, bool)> = images
        .iter()
        .filter(|(_, img)| str_at(Some(img), "rmw").unwrap_or_else(|| fallback_rmw.clone()) == rmw)
        .map(|(id, img)| {
            let edition = str_at(Some(img), "ros_edition").or_else(|| fallback_edition.clone());
            (id.clone(), edition.is_none())
        })
        .collect();
    if matching.is_empty() {
        return Err(format!("no `[image.*]` builds rmw `{rmw}`"));
    }
    if matching.iter().any(|(_, plain)| *plain) {
        matching.retain(|(_, plain)| *plain);
    }
    if matching.len() == 1 {
        return Ok(matching.remove(0).0);
    }
    let defaults: Vec<String> = sys
        .and_then(|s| s.get("default_images"))
        .and_then(|d| d.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    let named: Vec<&(String, bool)> = matching
        .iter()
        .filter(|(id, _)| defaults.contains(id))
        .collect();
    if named.len() == 1 {
        return Ok(named[0].0.clone());
    }
    Err(format!(
        "several images build rmw `{rmw}` ({}); name one in the row",
        matching
            .iter()
            .map(|(i, _)| i.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    ))
}

/// Keys a `[[workspace_fixture]]` row reads, which a migrated row may carry.
const CARRIED_KEYS: &[&str] = &[
    "id",
    "platform",
    "lang",
    "rmw",
    "dir",
    "board",
    "env",
    "serves",
    "target_dir",
    // A leaf row may already name the image it builds; it is kept as written.
    "image",
];

/// Rewrite the `[[fixture]]` rows of `manifest` whose `dir` is `rel_dir` into
/// `[[workspace_fixture]]` rows on `bringup`'s images. Text-level, so every
/// comment stays where it was. Returns the new text and what it could not do.
pub fn rewrite_fixture_rows(
    manifest: &str,
    rel_dir: &str,
    bringup: &str,
    system: &toml::Value,
) -> (String, Vec<String>) {
    let lines: Vec<&str> = manifest.lines().collect();
    let is_header = |l: &str| l.trim_start().starts_with('[');
    let headers: Vec<usize> = (0..lines.len()).filter(|&i| is_header(lines[i])).collect();
    let mut report = Vec::new();
    let mut out: Vec<String> = lines.iter().map(|l| (*l).to_string()).collect();
    let mut taken_ids: Vec<String> = Vec::new();
    // Every id already in the file, so a synthesised one cannot collide.
    if let Ok(doc) = manifest.parse::<toml::Value>() {
        for kind in ["fixture", "workspace_fixture"] {
            for row in doc
                .get(kind)
                .and_then(|r| r.as_array())
                .into_iter()
                .flatten()
            {
                if let Some(id) = row.get("id").and_then(|i| i.as_str()) {
                    taken_ids.push(id.to_string());
                }
            }
        }
    }
    // Edits are applied bottom-up so earlier indices stay valid.
    let mut edits: Vec<(usize, usize, Vec<String>)> = Vec::new();
    for (n, &start) in headers.iter().enumerate() {
        if lines[start].trim() != "[[fixture]]" {
            continue;
        }
        let mut end = headers.get(n + 1).copied().unwrap_or(lines.len());
        // Comment and blank lines just above the next header are that row's.
        while end > start + 1 {
            let l = lines[end - 1].trim();
            if l.is_empty() || l.starts_with('#') {
                end -= 1;
            } else {
                break;
            }
        }
        let body = lines[start + 1..end].join("\n");
        let Ok(row) = body.parse::<toml::Table>() else {
            continue;
        };
        if row.get("dir").and_then(|d| d.as_str()) != Some(rel_dir) {
            continue;
        }
        let where_ = format!("fixture row at line {}", start + 1);
        let extra: Vec<&String> = row
            .keys()
            .filter(|k| !CARRIED_KEYS.contains(&k.as_str()))
            .collect();
        if !extra.is_empty() {
            report.push(format!(
                "{where_} left as it is: a workspace row has no {} (move what it states \
                 into an image of the bringup, then rewrite it by hand)",
                extra
                    .iter()
                    .map(|k| format!("`{k}`"))
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
            continue;
        }
        let platform = row.get("platform").and_then(|v| v.as_str()).unwrap_or("");
        let lang = row.get("lang").and_then(|v| v.as_str()).unwrap_or("");
        let rmw = row
            .get("rmw")
            .and_then(|v| v.as_str())
            .unwrap_or("zenoh")
            .to_string();
        let stated = row
            .get("image")
            .and_then(|v| v.as_str())
            .map(str::to_string);
        let image = match stated.map_or_else(|| image_for_rmw(system, &rmw), Ok) {
            Ok(i) => i,
            Err(e) => {
                report.push(format!("{where_} left as it is: {e}"));
                continue;
            }
        };
        let mut block: Vec<String> = vec!["[[workspace_fixture]]".to_string()];
        if row.get("id").is_none() {
            let base = rel_dir
                .rsplit('/')
                .next()
                .unwrap_or("leaf")
                .replace('_', "-");
            let mut id = format!("{platform}-{lang}-{rmw}-{base}");
            let mut k = 2;
            while taken_ids.contains(&id) {
                id = format!("{platform}-{lang}-{rmw}-{base}-{k}");
                k += 1;
            }
            taken_ids.push(id.clone());
            block.push(format!("id = \"{id}\""));
        }
        let mut have_rmw = false;
        for l in &lines[start + 1..end] {
            block.push((*l).to_string());
            let key = l.split('=').next().unwrap_or("").trim();
            if key == "rmw" {
                have_rmw = true;
            }
            if key == "dir" {
                if !have_rmw && row.get("rmw").is_none() {
                    block.push(format!("rmw = \"{rmw}\""));
                    have_rmw = true;
                }
                block.push(format!("bringup = \"src/{bringup}\""));
                if row.get("image").is_none() {
                    block.push(format!("image = \"{image}\""));
                }
            }
        }
        edits.push((start, end, block));
    }
    for (start, end, block) in edits.into_iter().rev() {
        out.splice(start..end, block);
    }
    let mut text = out.join("\n");
    if manifest.ends_with('\n') {
        text.push('\n');
    }
    (text, report)
}

// ---- the verb -----------------------------------------------------------------

/// `nros dev migrate-bringup`.
///
/// # Errors
///
/// A directory that is not a standalone package, or an I/O failure.
pub fn run(args: Args) -> Result<()> {
    let dir = &args.dir;
    let plan = plan(dir)?;
    for op in &plan.ops {
        match op {
            Op::Move { from, to } => {
                eprintln!("  move   {} -> {}", from.display(), to.display());
            }
            Op::Delete(p) => eprintln!("  delete {}", p.display()),
            Op::Write { path, .. } => eprintln!("  write  {}", path.display()),
        }
    }
    let mut report = plan.report.clone();

    let mut fixture_edit: Option<(PathBuf, String)> = None;
    if let Some(manifest) = &args.fixtures {
        let root = match &args.fixtures_root {
            Some(r) => r.clone(),
            None => manifest
                .parent()
                .and_then(Path::parent)
                .map(Path::to_path_buf)
                .unwrap_or_default(),
        };
        let abs = |p: &Path| {
            let p = if p.as_os_str().is_empty() {
                Path::new(".")
            } else {
                p
            };
            std::path::absolute(p).unwrap_or_else(|_| p.to_path_buf())
        };
        let rel = relative(&normalise(&abs(dir)), &normalise(&abs(&root)));
        let rel = rel.to_string_lossy().replace('\\', "/");
        let text = std::fs::read_to_string(manifest)
            .wrap_err_with(|| format!("reading {}", manifest.display()))?;
        let system: toml::Value = std::fs::read_to_string(dir.join("system.toml"))?.parse()?;
        let (new_text, notes) = rewrite_fixture_rows(&text, &rel, &plan.bringup, &system);
        report.extend(notes);
        if new_text != text {
            eprintln!("  edit   {}", manifest.display());
            fixture_edit = Some((manifest.clone(), new_text));
        }
    }

    if !args.dry_run {
        apply(&plan)?;
        if let Some((path, text)) = fixture_edit {
            std::fs::write(&path, text)?;
        }
    }
    eprintln!(
        "nros dev migrate-bringup: {} -> src/{} + src/{}{}",
        dir.display(),
        plan.node,
        plan.bringup,
        if args.dry_run { " (dry run)" } else { "" }
    );
    for r in &report {
        eprintln!("  note: {r}");
    }
    Ok(())
}

/// The keys of a parsed manifest, for the tests.
#[cfg(test)]
fn rows(text: &str, kind: &str) -> Vec<std::collections::BTreeMap<String, toml::Value>> {
    text.parse::<toml::Value>()
        .unwrap()
        .get(kind)
        .and_then(|r| r.as_array().cloned())
        .unwrap_or_default()
        .into_iter()
        .map(|r| r.as_table().unwrap().clone().into_iter().collect())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static N: AtomicUsize = AtomicUsize::new(0);
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join(format!(
            "../../../tmp/migrate-bringup-{name}-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write(path: &Path, text: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    fn package_xml(name: &str, build_type: &str) -> String {
        format!(
            "<?xml version=\"1.0\"?>\n<package format=\"3\">\n  <name>{name}</name>\n  \
             <depend>std_msgs</depend>\n  <export>\n    <build_type>{build_type}</build_type>\n  \
             </export>\n</package>\n"
        )
    }

    const SYSTEM: &str = r#"
[system]
name = "t"
rmw = "zenoh"
default_images = ["linux"]

[[component]]
pkg = "native_talker"
class = "native_rs_talker::Talker"
name = "talker"

[image_defaults]
board = "linux"

[image.linux]

[image.linux_xrce]
rmw = "xrce"

[image.linux_iron]
ros_edition = "iron"
"#;

    /// A copy of the Rust shape (`examples/linux/rust/talker`).
    #[test]
    fn rust_leaf_becomes_a_node_and_a_bringup() {
        let d = scratch("rust");
        write(
            &d.join("Cargo.toml"),
            r#"[package]
name = "native-rs-talker"

[[bin]]
name = "talker"
path = "src/main.rs"

[lib]
name = "native_rs_talker"
path = "src/lib.rs"

[dependencies]
nros = { version = "*", features = ["macros"] }
native_rs_talker_nros_selection = { path = "build/nros/selection" }
std_msgs = { path = "generated/std_msgs", default-features = false }  # nros-managed

[workspace]
"#,
        );
        write(
            &d.join("package.xml"),
            &package_xml("native_talker", "nros_cargo"),
        );
        write(&d.join("system.toml"), SYSTEM);
        write(&d.join("README.md"), "# talker\n");
        write(&d.join("src/lib.rs"), "pub struct Talker;\n");
        write(
            &d.join("src/main.rs"),
            "//! The entry.\n\nnros::main!(spin = \"forever\");\n",
        );
        std::fs::create_dir_all(d.join("generated/std_msgs")).unwrap();

        let p = plan(&d).unwrap();
        apply(&p).unwrap();

        let node = d.join("src/native_talker");
        let bringup = d.join("src/native_talker_bringup");
        assert!(
            d.join("README.md").is_file(),
            "the README stays at the root"
        );
        assert!(node.join("src/lib.rs").is_file());
        assert!(
            !node.join("src/main.rs").exists(),
            "a trivial main is deleted"
        );
        assert!(node.join("package.xml").is_file());
        assert!(bringup.join("system.toml").is_file());
        let bxml = std::fs::read_to_string(bringup.join("package.xml")).unwrap();
        assert!(
            bxml.contains("<exec_depend>native_talker</exec_depend>"),
            "{bxml}"
        );
        assert!(d.join("generated").is_dir(), "build output stays in place");
        let manifest = std::fs::read_to_string(node.join("Cargo.toml")).unwrap();
        assert!(!manifest.contains("_nros_selection"), "{manifest}");
        assert!(!manifest.contains("[[bin]]"), "{manifest}");
        assert!(!manifest.contains("[workspace]"), "{manifest}");
        assert!(
            manifest.contains("path = \"../../generated/std_msgs\""),
            "the message dep is re-rooted: {manifest}"
        );
        assert!(manifest.contains("# nros-managed"), "comments survive");
        assert!(
            manifest.contains("name = \"native_talker\""),
            "the crate takes its package.xml name: {manifest}"
        );
        let sys = std::fs::read_to_string(bringup.join("system.toml")).unwrap();
        assert!(
            sys.contains("class = \"native_talker::Talker\""),
            "the class path follows the crate: {sys}"
        );
        assert!(
            plan(&d).is_err(),
            "a migrated directory holds no system.toml beside code"
        );
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn a_main_that_only_names_a_panic_policy_moves_it_into_the_bringup() {
        let main = "#![no_std]\n#![no_main]\n// glue\nnros::main!(panic = \"platform\");\n";
        assert_eq!(trivial_rust_main(main), Some(Some("platform".to_string())));
        assert_eq!(trivial_rust_main("nros::main!();"), Some(None));
        assert_eq!(trivial_rust_main("nros::main!(launch = \"x\");"), None);
        assert_eq!(trivial_rust_main("fn main() {}"), None);
        let edited = with_image_panic("[image.a]\nboard = \"b\"\n", "platform")
            .unwrap()
            .unwrap();
        assert!(
            edited.contains("[image_defaults]\npanic = \"platform\""),
            "{edited}"
        );
        assert!(
            with_image_panic(
                "[image_defaults]\npanic = \"platform\"\n[image.a]\n",
                "platform"
            )
            .unwrap()
            .is_none(),
            "already stated"
        );
        assert!(with_image_panic("[image.a]\npanic = \"own\"\n", "platform").is_err());
    }

    /// A Rust `main` with logic is moved and reported, never deleted.
    #[test]
    fn hand_written_main_is_reported_not_deleted() {
        let d = scratch("main");
        write(
            &d.join("Cargo.toml"),
            "[package]\nname = \"x\"\n\n[[bin]]\nname = \"x\"\npath = \"src/main.rs\"\n",
        );
        write(&d.join("package.xml"), &package_xml("x", "nros_cargo"));
        write(&d.join("system.toml"), SYSTEM);
        write(&d.join("src/main.rs"), "fn main() { println!(\"hi\"); }\n");
        let p = plan(&d).unwrap();
        assert!(
            p.report.iter().any(|r| r.contains("does more than call")),
            "{:?}",
            p.report
        );
        apply(&p).unwrap();
        assert!(d.join("src/x/src/main.rs").is_file());
        let m = std::fs::read_to_string(d.join("src/x/Cargo.toml")).unwrap();
        assert!(m.contains("[[bin]]"), "the bin of a kept main stays: {m}");
        let _ = std::fs::remove_dir_all(&d);
    }

    /// The C and C++ shapes: the code moves, and the `main` is reported.
    #[test]
    fn c_and_cpp_leaves_move_and_report_their_main() {
        for (lang, file, body) in [
            (
                "c",
                "main.c",
                "#include <stdio.h>\nint main(void) { return 0; }\n",
            ),
            (
                "cpp",
                "main.cpp",
                "int main(int argc, char **argv) { return 0; }\n",
            ),
        ] {
            let d = scratch(lang);
            write(
                &d.join("CMakeLists.txt"),
                "project(t)\nnano_ros_add_executable(t src/main.c)\n",
            );
            write(
                &d.join("package.xml"),
                &package_xml("native_c_talker", "nros_cmake"),
            );
            write(&d.join("system.toml"), SYSTEM);
            write(&d.join("src").join(file), body);
            let p = plan(&d).unwrap();
            assert!(
                p.report.iter().any(|r| r.contains("defines `main`")),
                "{lang}: {:?}",
                p.report
            );
            apply(&p).unwrap();
            let node = d.join("src/native_c_talker");
            assert!(node.join("CMakeLists.txt").is_file(), "{lang}");
            assert!(node.join("src").join(file).is_file(), "{lang}");
            assert!(
                d.join("src/native_c_talker_bringup/system.toml").is_file(),
                "{lang}"
            );
            let _ = std::fs::remove_dir_all(&d);
        }
    }

    /// A copy of the Zephyr west shape (`examples/zephyr/rust/talker`): the
    /// node package is kept, the west glue goes, and the Kconfig fragments
    /// `system.toml` names follow it into the bringup.
    #[test]
    fn zephyr_west_leaf_keeps_its_node_and_drops_the_glue() {
        let d = scratch("west");
        write(
            &d.join("CMakeLists.txt"),
            "find_package(Zephyr REQUIRED HINTS $ENV{ZEPHYR_BASE})\nrust_cargo_application()\n",
        );
        write(
            &d.join("Cargo.toml"),
            "[package]\nname = \"nros_zephyr_talker\"\n\n[dependencies]\n\
             zephyr_talker_node = { path = \"node\" }\n",
        );
        write(
            &d.join("package.xml"),
            &package_xml("zephyr_talker", "nros_cmake"),
        );
        write(&d.join("build.rs"), "fn main() {}\n");
        write(&d.join("sample.yaml"), "sample: {}\n");
        write(
            &d.join("src/app_main.rs"),
            "nros::zephyr_component_main!(crate::Talker);\n",
        );
        write(&d.join("src/lib.rs"), "mod app_main;\n");
        write(&d.join("prj.conf"), "CONFIG_X=y\n");
        write(&d.join("prj-zenoh.conf"), "CONFIG_Y=y\n");
        write(
            &d.join("system.toml"),
            "[system]\nname = \"zephyr_talker\"\nrmw = \"zenoh\"\n\n[[component]]\n\
             pkg = \"zephyr_talker_node\"\n\n[image.zephyr_zenoh]\nboard = \"zephyr\"\n\
             os_config = [\"prj-zenoh.conf\"]\n",
        );
        write(
            &d.join("node/Cargo.toml"),
            "[package]\nname = \"zephyr_talker_node\"\n\n[dependencies]\n\
             std_msgs = { path = \"../generated/std_msgs\" }\n",
        );
        write(
            &d.join("node/package.xml"),
            &package_xml("zephyr_talker_node", "nros_cargo"),
        );
        write(&d.join("node/src/lib.rs"), "pub struct Talker;\n");

        let p = plan(&d).unwrap();
        assert_eq!(p.node, "zephyr_talker_node");
        apply(&p).unwrap();
        let node = d.join("src/zephyr_talker_node");
        let bringup = d.join("src/zephyr_talker_node_bringup");
        assert!(node.join("src/lib.rs").is_file());
        assert!(
            bringup.join("prj-zenoh.conf").is_file(),
            "a named fragment follows"
        );
        assert!(!d.join("CMakeLists.txt").exists(), "the west glue goes");
        assert!(!d.join("src/app_main.rs").exists());
        let m = std::fs::read_to_string(node.join("Cargo.toml")).unwrap();
        assert!(m.contains("path = \"../../generated/std_msgs\""), "{m}");
        assert!(
            bringup.join("prj.conf").is_file(),
            "an unnamed fragment follows the bringup too"
        );
        assert!(
            p.report.iter().any(|r| r.contains("prj.conf")),
            "and is reported: {:?}",
            p.report
        );
        let _ = std::fs::remove_dir_all(&d);
    }

    /// A copy of the Zephyr C shape (`examples/zephyr/c/talker`): the
    /// component registration stays with the node, `find_package(Zephyr)`
    /// goes, and every Kconfig fragment follows the bringup.
    #[test]
    fn zephyr_c_leaf_keeps_its_component_and_drops_zephyr() {
        let d = scratch("west-c");
        write(
            &d.join("CMakeLists.txt"),
            "cmake_minimum_required(VERSION 3.20.0)\n\
             find_package(Zephyr REQUIRED HINTS $ENV{ZEPHYR_BASE})\nproject(t)\n\
             find_package(nano_ros REQUIRED)\n\
             nano_ros_auto_add_library(talker_lib STATIC src/Talker.c)\n",
        );
        write(
            &d.join("package.xml"),
            &package_xml("nros_zephyr_talker_c", "nros_cmake"),
        );
        write(
            &d.join("src/Talker.c"),
            "static int tick(void) { return 0; }\n",
        );
        write(&d.join("prj.conf"), "CONFIG_X=y\n");
        write(&d.join("prj-zenoh.conf"), "CONFIG_Y=y\n");
        write(&d.join("sample.yaml"), "sample: {}\n");
        write(
            &d.join("system.toml"),
            "[system]\nname = \"t\"\n\n[[component]]\npkg = \"nros_zephyr_talker_c\"\n\n\
             [image.zephyr_zenoh]\nboard = \"zephyr\"\nos_config = [\"prj-zenoh.conf\"]\n",
        );
        let p = plan(&d).unwrap();
        apply(&p).unwrap();
        let node = d.join("src/nros_zephyr_talker_c");
        let bringup = d.join("src/nros_zephyr_talker_c_bringup");
        let cmake = std::fs::read_to_string(node.join("CMakeLists.txt")).unwrap();
        assert!(!cmake.contains("find_package(Zephyr"), "{cmake}");
        assert!(cmake.contains("nano_ros_auto_add_library"), "{cmake}");
        assert!(node.join("src/Talker.c").is_file());
        assert!(bringup.join("prj-zenoh.conf").is_file());
        assert!(bringup.join("prj.conf").is_file());
        assert!(
            !p.report.iter().any(|r| r.contains("defines `main`")),
            "a component defines no main: {:?}",
            p.report
        );
        let _ = std::fs::remove_dir_all(&d);
    }

    /// A fixture row keeps its coordinate and lands on the image that builds
    /// it; a row stating a cargo feature is left for a person.
    #[test]
    fn fixture_rows_keep_their_coordinate() {
        let system: toml::Value = SYSTEM.parse().unwrap();
        let manifest = r#"# header comment

[[fixture]]
id = "native-rust-talker"
platform = "linux"
lang = "rust"
dir = "examples/linux/rust/talker"
serves = ["zephyr,c,zenoh"]

# TLS variant.
[[fixture]]
platform = "linux"
lang = "rust"
dir = "examples/linux/rust/talker"
features = ["link-tls"]

[[fixture]]
platform = "linux"
lang = "c"
rmw = "xrce"
dir = "examples/linux/rust/talker"

[[fixture]]
platform = "linux"
lang = "rust"
rmw = "xrce"
dir = "examples/linux/rust/talker"
image = "linux_xrce"

[[fixture]]
platform = "linux"
lang = "rust"
dir = "examples/linux/rust/listener"
"#;
        let (text, report) = rewrite_fixture_rows(
            manifest,
            "examples/linux/rust/talker",
            "native_talker_bringup",
            &system,
        );
        let ws = rows(&text, "workspace_fixture");
        assert_eq!(ws.len(), 3, "{text}");
        assert_eq!(
            ws[2]["image"].as_str(),
            Some("linux_xrce"),
            "a stated image is kept"
        );
        assert_eq!(ws[0]["id"].as_str(), Some("native-rust-talker"));
        assert_eq!(ws[0]["image"].as_str(), Some("linux"));
        assert_eq!(
            ws[0]["rmw"].as_str(),
            Some("zenoh"),
            "the default is written"
        );
        assert_eq!(ws[0]["bringup"].as_str(), Some("src/native_talker_bringup"));
        assert!(ws[0].contains_key("serves"));
        assert_eq!(ws[1]["image"].as_str(), Some("linux_xrce"));
        assert_eq!(ws[1]["id"].as_str(), Some("linux-c-xrce-talker"));
        let plain = rows(&text, "fixture");
        assert_eq!(plain.len(), 2, "the features row and the listener stay");
        assert!(
            report.iter().any(|r| r.contains("`features`")),
            "{report:?}"
        );
        assert!(text.contains("# TLS variant."), "comments stay: {text}");
    }

    #[test]
    fn image_choice_prefers_the_plain_edition_and_refuses_ambiguity() {
        let system: toml::Value = SYSTEM.parse().unwrap();
        assert_eq!(image_for_rmw(&system, "zenoh").unwrap(), "linux");
        assert!(image_for_rmw(&system, "cyclonedds").is_err());
        let two: toml::Value = "[image.a]\nrmw = \"xrce\"\n[image.b]\nrmw = \"xrce\"\n"
            .parse()
            .unwrap();
        assert!(image_for_rmw(&two, "xrce").is_err());
    }

    #[test]
    fn reroot_is_lexical_and_relative() {
        assert_eq!(
            reroot("generated/x", Path::new("w"), Path::new("w/src/n")),
            "../../generated/x"
        );
        assert_eq!(
            reroot("../generated/x", Path::new("w/node"), Path::new("w/src/n")),
            "../../generated/x"
        );
        assert_eq!(
            reroot(
                "../../../packages/core",
                Path::new("a/b/c"),
                Path::new("a/b/c/src/n")
            ),
            "../../../../../packages/core"
        );
    }
}
