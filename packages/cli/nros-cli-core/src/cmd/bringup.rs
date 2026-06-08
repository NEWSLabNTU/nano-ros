//! Phase 212.F — bringup-package discovery + lint helpers.
//!
//! Shared between:
//!
//! * `nros check --bringup <dir>` — pure-declarative lint (no `Cargo.toml`,
//!   no `CMakeLists.txt`, no `src/`, no `add_executable`).
//! * `cargo nros plan <dir>` discovery walk (Phase 212.F.3 — find sibling
//!   bringup pkgs not in `[workspace] members`).
//!
//! Design references:
//! * `docs/design/0024-multi-node-workspace-layout.md` §4 ("The orchestration
//!   package").
//! * `docs/design/0025-workspace-layout-by-case.md` Case 3 + Case 4.

use std::{
    fs,
    path::{Path, PathBuf},
};

use eyre::{Result, WrapErr, bail};

/// Files / dirs that must NOT live inside a bringup package. Path A.
/// The list comes from the Phase 212.F.2 task brief — every entry here is a
/// code-bearing surface that means "the bringup pkg has a build target",
/// which is the very thing Path A disallows. `[[bin]]` / `[lib]` text
/// patterns inside `Cargo.toml` are covered transitively (the `Cargo.toml`
/// file itself is rejected).
const FORBIDDEN_FILES: &[&str] = &["Cargo.toml", "CMakeLists.txt"];
const FORBIDDEN_DIRS: &[&str] = &["src", "include", "lib"];

/// Run the bringup lint against a single directory. Emits a clean error
/// containing every offence the directory carries.
///
/// Returns `Ok(())` when the directory is a pure-declarative bringup pkg
/// (has `package.xml` + `system.toml` + no forbidden surfaces). The
/// `package.xml` / `system.toml` presence is also enforced — a directory
/// missing either is not yet a complete bringup.
pub fn lint_bringup(bringup_dir: &Path) -> Result<()> {
    let pkg_name = bringup_dir
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("<unknown>");

    // 1. Presence checks: every bringup MUST carry these two declarative files.
    let mut missing: Vec<&str> = Vec::new();
    if !bringup_dir.join("package.xml").is_file() {
        missing.push("package.xml");
    }
    if !bringup_dir.join("system.toml").is_file() {
        missing.push("system.toml");
    }
    if !missing.is_empty() {
        bail!(
            "bringup pkg {pkg_name} is incomplete — missing {} (see \
             docs/design/0024-multi-node-workspace-layout.md §4)",
            missing.join(", ")
        );
    }

    // 2. Forbidden-surface checks (Phase 212.F.2). Factored out so the
    //    workspace-walk lint can reuse only this slice without also
    //    forcing the exec_depend drift / class-prefix lints on every dir.
    lint_bringup_forbidden_surfaces(bringup_dir)?;

    // 3. Cross-validate `package.xml` `<exec_depend>` rows against
    //    `[[component]].pkg` rows in `system.toml`. The bringup's
    //    `<exec_depend>` block IS a derived view of the system's component
    //    list; any drift means a stale package.xml after a component
    //    rename or add/remove. This replaces the retired
    //    `nros emit package-xml` auto-regeneration path (Phase 212.G).
    check_exec_depend_drift(bringup_dir, pkg_name)?;

    // 4. Phase 212.L.4 — `[[component]].class` must be `<pkg>::<Type>`.
    crate::cmd::check_workspace::lint_class_pkg_prefix(bringup_dir, pkg_name)?;

    Ok(())
}

/// Phase 212.F.2 — pure-declarative surface lint, without the
/// `<exec_depend>` drift / class-prefix checks. The workspace-walk path
/// calls this on every dir with `package.xml + system.toml`, even when the
/// dir also carries forbidden files (a misconfigured bringup): the goal of
/// F.2 is precisely to surface those code-bearing files.
pub fn lint_bringup_forbidden_surfaces(bringup_dir: &Path) -> Result<()> {
    let pkg_name = bringup_dir
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("<unknown>");

    let mut found: Vec<String> = Vec::new();
    for name in FORBIDDEN_FILES {
        if bringup_dir.join(name).exists() {
            found.push((*name).to_string());
        }
    }
    for name in FORBIDDEN_DIRS {
        let p = bringup_dir.join(name);
        if p.exists() && p.is_dir() {
            found.push(format!("{name}/"));
        }
    }
    // Walk any nested CMakeLists.txt and inspect for `add_executable`. The
    // top-level file presence is caught above; this scan handles files inside
    // `launch/` or future subdirs that someone might add.
    for stray_cmake in walk_cmakelists(bringup_dir) {
        let body = fs::read_to_string(&stray_cmake).unwrap_or_default();
        if body.contains("add_executable(") {
            found.push(format!(
                "{} (carries add_executable)",
                stray_cmake
                    .strip_prefix(bringup_dir)
                    .unwrap_or(&stray_cmake)
                    .display()
            ));
        }
    }
    if !found.is_empty() {
        return Err(forbidden_surface_error(pkg_name, &found));
    }
    Ok(())
}

/// Construct the canonical Phase 212.F.2 diagnostic for a bringup pkg
/// carrying code-bearing surfaces. Centralised so the wording stays in
/// sync between the single-dir `lint_bringup` path and the workspace-walk
/// `lint_bringup_forbidden_surfaces` path.
fn forbidden_surface_error(pkg_name: &str, found: &[String]) -> eyre::Report {
    eyre::eyre!(
        "bringup pkg {pkg_name} must be pure declarative — found {}; code \
         belongs in a sibling component pkg (see \
         docs/design/0024-multi-node-workspace-layout.md §4)",
        found.join(", ")
    )
}

/// Compare `<exec_depend>…</exec_depend>` rows in `<bringup>/package.xml`
/// against `[[component]].pkg` rows in `<bringup>/system.toml`. Surfaces
/// extras + missing as a single lint error. Empty `<exec_depend>` blocks
/// fine when `[[component]]` list is also empty.
fn check_exec_depend_drift(bringup_dir: &Path, pkg_name: &str) -> Result<()> {
    use std::collections::BTreeSet;

    use crate::orchestration::cargo_metadata_schema::SystemToml;

    let system_toml_raw = fs::read_to_string(bringup_dir.join("system.toml"))?;
    let system: SystemToml = toml::from_str(&system_toml_raw)?;
    let want: BTreeSet<String> = system.components.iter().map(|c| c.pkg.clone()).collect();

    let package_xml = fs::read_to_string(bringup_dir.join("package.xml"))?;
    let got = parse_exec_depend(&package_xml);

    let missing: Vec<&String> = want.difference(&got).collect();
    let extra: Vec<&String> = got.difference(&want).collect();
    if missing.is_empty() && extra.is_empty() {
        return Ok(());
    }
    let mut details: Vec<String> = Vec::new();
    if !missing.is_empty() {
        let names: Vec<String> = missing.iter().map(|s| s.to_string()).collect();
        details.push(format!(
            "missing <exec_depend>: {} (declared in system.toml [[component]])",
            names.join(", ")
        ));
    }
    if !extra.is_empty() {
        let names: Vec<String> = extra.iter().map(|s| s.to_string()).collect();
        details.push(format!(
            "stray <exec_depend>: {} (not in system.toml [[component]])",
            names.join(", ")
        ));
    }
    bail!(
        "bringup pkg {pkg_name}: package.xml drift vs system.toml — {}. \
         Hand-edit package.xml to add/remove the listed entries; the \
         `nros emit package-xml` auto-regen verb was retired in Phase 212.G.",
        details.join("; ")
    );
}

/// Pull every `<exec_depend>NAME</exec_depend>` body from a `package.xml`
/// blob. Minimal substring parser — `package.xml` is regular enough that
/// a full XML pass would be overkill. Whitespace is trimmed; duplicates
/// collapse into the BTreeSet.
fn parse_exec_depend(xml: &str) -> std::collections::BTreeSet<String> {
    use std::collections::BTreeSet;

    let mut out = BTreeSet::new();
    let mut rest = xml;
    while let Some(start) = rest.find("<exec_depend") {
        rest = &rest[start..];
        let Some(open_end) = rest.find('>') else {
            break;
        };
        rest = &rest[open_end + 1..];
        let Some(close) = rest.find("</exec_depend>") else {
            break;
        };
        let body = rest[..close].trim();
        if !body.is_empty() {
            out.insert(body.to_string());
        }
        rest = &rest[close + "</exec_depend>".len()..];
    }
    out
}

fn walk_cmakelists(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(entries) = fs::read_dir(&d) else {
            continue;
        };
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_dir() {
                stack.push(p);
            } else if p.file_name().and_then(|n| n.to_str()) == Some("CMakeLists.txt") {
                out.push(p);
            }
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Discovery walk — used by `cargo nros plan <dir>` (Phase 212.F.3).
// ---------------------------------------------------------------------------

/// One discovered bringup package.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveredBringup {
    pub pkg_name: String,
    pub dir: PathBuf,
}

/// Walk a workspace directory looking for bringup packages.
///
/// # Algorithm
///
/// 1. **Read the workspace `Cargo.toml`** at `workspace_root` (when present)
///    and collect `[workspace] members` + `[workspace] exclude`. These names
///    constrain the walk:
///    * Members are skipped — they are component crates, never bringup.
///    * Excluded entries are explicitly considered (Path A puts the bringup
///      pkg in `exclude`).
/// 2. **Enumerate immediate children of `workspace_root`** (single level —
///    bringup pkgs are siblings of component pkgs, never nested arbitrarily
///    deep). For each child directory:
///    * Skip if name is in the members set.
///    * Treat as a candidate bringup if it contains BOTH `package.xml` and
///      `system.toml`.
/// 3. **Return** the candidates in deterministic (sorted-by-name) order.
///
/// This walk is the dispatch surface for `cargo nros plan <dir>` when the
/// user has not supplied an explicit bringup path on the command line.
pub fn discover_bringups(workspace_root: &Path) -> Result<Vec<DiscoveredBringup>> {
    let members = read_workspace_members(workspace_root)
        .wrap_err("read [workspace] members")?
        .unwrap_or_default();

    let mut found: Vec<DiscoveredBringup> = Vec::new();
    let entries =
        fs::read_dir(workspace_root).wrap_err_with(|| format!("read {:?}", workspace_root))?;
    for entry in entries.flatten() {
        let p = entry.path();
        if !p.is_dir() {
            continue;
        }
        let name = match p.file_name().and_then(|n| n.to_str()) {
            Some(n) => n.to_string(),
            None => continue,
        };
        if members.iter().any(|m| m == &name) {
            continue;
        }
        if p.join("package.xml").is_file() && p.join("system.toml").is_file() {
            found.push(DiscoveredBringup {
                pkg_name: name,
                dir: p,
            });
        }
    }
    found.sort_by(|a, b| a.pkg_name.cmp(&b.pkg_name));
    Ok(found)
}

/// Returns `Some(members)` when a workspace `Cargo.toml` exists at the root,
/// `None` otherwise.
fn read_workspace_members(workspace_root: &Path) -> Result<Option<Vec<String>>> {
    let cargo_toml = workspace_root.join("Cargo.toml");
    if !cargo_toml.is_file() {
        return Ok(None);
    }
    let raw = fs::read_to_string(&cargo_toml)
        .wrap_err_with(|| format!("read {}", cargo_toml.display()))?;
    let doc: toml::Value =
        toml::from_str(&raw).wrap_err_with(|| format!("parse {}", cargo_toml.display()))?;
    let members = doc
        .get("workspace")
        .and_then(|w| w.get("members"))
        .and_then(|m| m.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    Ok(Some(members))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_root(tag: &str) -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir =
            std::env::temp_dir().join(format!("nros-bringup-{tag}-{}-{stamp}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write_pure_bringup(dir: &Path) {
        fs::create_dir_all(dir.join("launch")).unwrap();
        fs::write(
            dir.join("package.xml"),
            "<?xml version=\"1.0\"?><package format=\"3\"><name>demo_bringup</name>\
             <version>0.1.0</version></package>",
        )
        .unwrap();
        fs::write(
            dir.join("system.toml"),
            "[system]\nname=\"demo\"\nrmw=\"zenoh\"\ndomain_id=0\n",
        )
        .unwrap();
        fs::write(dir.join("launch/system.launch.xml"), "<launch/>").unwrap();
    }

    #[test]
    fn nros_check_accepts_pure_declarative_bringup() {
        let root = temp_root("accept_clean");
        let bringup = root.join("demo_bringup");
        write_pure_bringup(&bringup);
        lint_bringup(&bringup).expect("clean bringup passes lint");
    }

    #[test]
    fn nros_check_rejects_cargo_toml_in_bringup() {
        let root = temp_root("reject_cargo");
        let bringup = root.join("demo_bringup");
        write_pure_bringup(&bringup);
        fs::write(
            bringup.join("Cargo.toml"),
            "[package]\nname=\"demo_bringup\"\n",
        )
        .unwrap();
        let err = lint_bringup(&bringup).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("Cargo.toml"), "diagnostic: {msg}");
        assert!(msg.contains("pure declarative"), "diagnostic: {msg}");
        assert!(msg.contains("sibling component pkg"), "diagnostic: {msg}");
    }

    #[test]
    fn nros_check_rejects_src_dir_in_bringup() {
        let root = temp_root("reject_src");
        let bringup = root.join("demo_bringup");
        write_pure_bringup(&bringup);
        fs::create_dir_all(bringup.join("src")).unwrap();
        fs::write(bringup.join("src/lib.rs"), "// no").unwrap();
        let err = lint_bringup(&bringup).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("src/"), "diagnostic: {msg}");
    }

    #[test]
    fn nros_check_rejects_cmakelists_in_bringup() {
        let root = temp_root("reject_cmake");
        let bringup = root.join("demo_bringup");
        write_pure_bringup(&bringup);
        fs::write(
            bringup.join("CMakeLists.txt"),
            "add_executable(demo src/main.cpp)\n",
        )
        .unwrap();
        let err = lint_bringup(&bringup).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("CMakeLists.txt"), "diagnostic: {msg}");
    }

    #[test]
    fn nros_check_rejects_nested_cmakelists_with_add_executable() {
        let root = temp_root("reject_nested_addexec");
        let bringup = root.join("demo_bringup");
        write_pure_bringup(&bringup);
        // Sneak a CMakeLists into `launch/` (not the top-level forbidden path).
        fs::write(
            bringup.join("launch/CMakeLists.txt"),
            "add_executable(rogue rogue.cpp)\n",
        )
        .unwrap();
        let err = lint_bringup(&bringup).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("add_executable"), "diagnostic: {msg}");
    }

    // -----------------------------------------------------------------------
    // Phase 212.F.2 spec-named regression tests. The spec brief lists four
    // tests by exact name; three (`rejects_cargo_toml_in_bringup`,
    // `rejects_cmakelists_in_bringup`, `rejects_src_dir_in_bringup`) match
    // the pre-existing names above 1:1. The fourth one
    // (`accepts_clean_bringup`) maps onto `accepts_pure_declarative_bringup`,
    // which we keep, plus add the spec-named alias below + coverage for the
    // broadened forbidden-dir list (`include/`, `lib/`).
    // -----------------------------------------------------------------------

    #[test]
    fn nros_check_accepts_clean_bringup() {
        let root = temp_root("accepts_clean_bringup");
        let bringup = root.join("demo_bringup");
        write_pure_bringup(&bringup);
        lint_bringup(&bringup).expect("clean bringup passes F.2 lint");
    }

    #[test]
    fn nros_check_rejects_include_dir_in_bringup() {
        let root = temp_root("reject_include");
        let bringup = root.join("demo_bringup");
        write_pure_bringup(&bringup);
        fs::create_dir_all(bringup.join("include")).unwrap();
        fs::write(bringup.join("include/demo.h"), "// no").unwrap();
        let err = lint_bringup(&bringup).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("include/"), "diagnostic: {msg}");
        assert!(msg.contains("pure declarative"), "diagnostic: {msg}");
    }

    #[test]
    fn nros_check_rejects_lib_dir_in_bringup() {
        let root = temp_root("reject_lib");
        let bringup = root.join("demo_bringup");
        write_pure_bringup(&bringup);
        fs::create_dir_all(bringup.join("lib")).unwrap();
        fs::write(bringup.join("lib/x.a"), "// no").unwrap();
        let err = lint_bringup(&bringup).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("lib/"), "diagnostic: {msg}");
        assert!(msg.contains("pure declarative"), "diagnostic: {msg}");
    }

    #[test]
    fn nros_check_lint_bringup_forbidden_surfaces_only_skips_drift() {
        // The surface-only helper used by the workspace walk must NOT call
        // `check_exec_depend_drift` — otherwise pre-existing tests that
        // stamp a stub package.xml without `<exec_depend>` rows would
        // regress when `[[component]]` entries are present in system.toml.
        let root = temp_root("surface_only");
        let bringup = root.join("demo_bringup");
        write_pure_bringup(&bringup);
        // Append a [[component]] row that the stub package.xml lacks.
        let sys = format!(
            "{}\n\n[[component]]\npkg = \"talker_pkg\"\nclass = \"talker_pkg::T\"\nname = \"t\"\n",
            fs::read_to_string(bringup.join("system.toml")).unwrap()
        );
        fs::write(bringup.join("system.toml"), sys).unwrap();
        // Surface-only lint: no Cargo.toml etc. → must pass.
        lint_bringup_forbidden_surfaces(&bringup)
            .expect("surface-only lint ignores <exec_depend> drift");
        // Now add a forbidden surface; it MUST surface.
        fs::write(bringup.join("Cargo.toml"), "[package]\nname=\"x\"\n").unwrap();
        let err = lint_bringup_forbidden_surfaces(&bringup).unwrap_err();
        assert!(err.to_string().contains("Cargo.toml"));
    }

    #[test]
    fn nros_check_rejects_incomplete_bringup() {
        let root = temp_root("incomplete");
        let bringup = root.join("demo_bringup");
        fs::create_dir_all(&bringup).unwrap();
        // package.xml present but no system.toml.
        fs::write(bringup.join("package.xml"), "<package/>").unwrap();
        let err = lint_bringup(&bringup).unwrap_err();
        assert!(err.to_string().contains("system.toml"), "diagnostic: {err}");
    }

    #[test]
    fn cargo_nros_plan_discovers_bringup_via_dirwalk() {
        let root = temp_root("discover");
        // workspace Cargo.toml — bringup goes in [workspace] exclude (Path A).
        fs::write(
            root.join("Cargo.toml"),
            "[workspace]\nresolver = \"2\"\n\
             members = [\"talker_pkg\", \"listener_pkg\"]\n\
             exclude = [\"demo_bringup\"]\n",
        )
        .unwrap();
        // Pretend component pkgs.
        fs::create_dir_all(root.join("talker_pkg/src")).unwrap();
        fs::write(
            root.join("talker_pkg/Cargo.toml"),
            "[package]\nname=\"talker_pkg\"\nversion=\"0.1.0\"\n",
        )
        .unwrap();
        fs::create_dir_all(root.join("listener_pkg/src")).unwrap();
        fs::write(
            root.join("listener_pkg/Cargo.toml"),
            "[package]\nname=\"listener_pkg\"\nversion=\"0.1.0\"\n",
        )
        .unwrap();
        // Bringup sibling.
        let bringup = root.join("demo_bringup");
        write_pure_bringup(&bringup);

        let found = discover_bringups(&root).expect("discovery walk");
        assert_eq!(found.len(), 1, "expected exactly one bringup: {found:?}");
        assert_eq!(found[0].pkg_name, "demo_bringup");
        assert_eq!(found[0].dir, bringup);
    }

    #[test]
    fn cargo_nros_plan_skips_workspace_members_during_dirwalk() {
        // Even if a member pkg accidentally has package.xml + system.toml,
        // it must NOT be discovered as a bringup. Workspace members are
        // component crates by contract.
        let root = temp_root("skip_member");
        fs::write(
            root.join("Cargo.toml"),
            "[workspace]\nresolver = \"2\"\n\
             members = [\"odd_pkg\"]\n",
        )
        .unwrap();
        let odd = root.join("odd_pkg");
        write_pure_bringup(&odd);
        fs::write(
            odd.join("Cargo.toml"),
            "[package]\nname=\"odd_pkg\"\nversion=\"0.1.0\"\n",
        )
        .unwrap();

        let found = discover_bringups(&root).expect("discovery walk");
        assert!(found.is_empty(), "members must be skipped, got: {found:?}");
    }

    #[test]
    fn cargo_nros_plan_discovery_sorts_results_deterministically() {
        let root = temp_root("sorted");
        fs::write(root.join("Cargo.toml"), "[workspace]\nmembers = []\n").unwrap();
        write_pure_bringup(&root.join("z_bringup"));
        write_pure_bringup(&root.join("a_bringup"));
        write_pure_bringup(&root.join("m_bringup"));
        let found = discover_bringups(&root).unwrap();
        let names: Vec<&str> = found.iter().map(|f| f.pkg_name.as_str()).collect();
        assert_eq!(names, ["a_bringup", "m_bringup", "z_bringup"]);
    }
}
