//! Phase 166.5 — guard board-overlay build scripts from re-emitting
//! static archives already compiled by transitive board dependencies.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

#[derive(Debug)]
struct BoardCrate {
    deps: BTreeSet<String>,
    compiled_archives: BTreeSet<String>,
    linked_static_archives: BTreeSet<String>,
}

#[test]
fn board_overlays_do_not_reemit_transitive_static_archives() {
    let repo = repo_root();
    let boards_dir = repo.join("packages/boards");
    let crates = discover_board_crates(&boards_dir);

    let mut failures = Vec::new();
    for (name, krate) in &crates {
        let transitive_compiles = transitive_board_deps(name, &crates)
            .into_iter()
            .filter_map(|dep| crates.get(&dep))
            .flat_map(|dep| dep.compiled_archives.iter().cloned())
            .collect::<BTreeSet<_>>();

        let duplicates = krate
            .linked_static_archives
            .intersection(&transitive_compiles)
            .cloned()
            .collect::<Vec<_>>();

        if !duplicates.is_empty() {
            failures.push(format!(
                "{} re-emits static archive(s) compiled by transitive board deps: {}",
                name,
                duplicates.join(", ")
            ));
        }
    }

    assert!(
        failures.is_empty(),
        "board build.rs static archive re-emission would duplicate bundled objects:\n{}",
        failures.join("\n")
    );
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .expect("canonical repo root")
}

fn discover_board_crates(boards_dir: &Path) -> BTreeMap<String, BoardCrate> {
    let mut crates = BTreeMap::new();
    for entry in fs::read_dir(boards_dir).expect("read packages/boards") {
        let entry = entry.expect("read board crate entry");
        let dir = entry.path();
        if !dir.join("Cargo.toml").exists() {
            continue;
        }

        let cargo_toml = fs::read_to_string(dir.join("Cargo.toml")).expect("read Cargo.toml");
        let Some(name) = package_name(&cargo_toml) else {
            continue;
        };
        let build_rs = fs::read_to_string(dir.join("build.rs")).unwrap_or_default();

        crates.insert(
            name,
            BoardCrate {
                deps: board_path_deps(&cargo_toml),
                compiled_archives: compiled_archives(&build_rs),
                linked_static_archives: linked_static_archives(&build_rs),
            },
        );
    }
    crates
}

fn package_name(cargo_toml: &str) -> Option<String> {
    let mut in_package = false;
    for line in cargo_toml.lines() {
        let trimmed = line.trim();
        if trimmed == "[package]" {
            in_package = true;
            continue;
        }
        if in_package && trimmed.starts_with('[') {
            return None;
        }
        if in_package && let Some(value) = quoted_value_after_key(trimmed, "name") {
            return Some(value.to_string());
        }
    }
    None
}

fn board_path_deps(cargo_toml: &str) -> BTreeSet<String> {
    let mut deps = BTreeSet::new();
    let mut in_dependency_section = false;

    for line in cargo_toml.lines() {
        let trimmed = line.split('#').next().unwrap_or("").trim();
        if trimmed.starts_with('[') {
            in_dependency_section = matches!(
                trimmed,
                "[dependencies]" | "[dev-dependencies]" | "[build-dependencies]"
            );
            continue;
        }
        if !in_dependency_section || trimmed.starts_with('#') {
            continue;
        }
        let points_at_board =
            trimmed.contains("packages/boards") || trimmed.contains("../nros-board");
        if !trimmed.contains("path") || !points_at_board {
            continue;
        }
        if trimmed.contains("optional = true") {
            continue;
        }
        let Some((dep_name, _)) = trimmed.split_once('=') else {
            continue;
        };
        let dep_name = dep_name.trim();
        if dep_name.starts_with("nros-board-") {
            deps.insert(dep_name.to_string());
        }
    }

    deps
}

fn compiled_archives(build_rs: &str) -> BTreeSet<String> {
    build_rs
        .lines()
        .filter_map(|line| quoted_after(line, "compile("))
        .map(str::to_string)
        .collect()
}

fn linked_static_archives(build_rs: &str) -> BTreeSet<String> {
    build_rs
        .lines()
        .filter_map(|line| line.split("cargo:rustc-link-lib=static=").nth(1))
        .filter_map(|rest| rest.split('"').next())
        .filter(|name| !name.is_empty() && !name.contains('{') && !name.contains('}'))
        .map(str::to_string)
        .collect()
}

fn transitive_board_deps(name: &str, crates: &BTreeMap<String, BoardCrate>) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let mut stack = crates
        .get(name)
        .map(|krate| krate.deps.iter().cloned().collect::<Vec<_>>())
        .unwrap_or_default();

    while let Some(dep) = stack.pop() {
        if !out.insert(dep.clone()) {
            continue;
        }
        if let Some(dep_crate) = crates.get(&dep) {
            stack.extend(dep_crate.deps.iter().cloned());
        }
    }

    out
}

fn quoted_value_after_key<'a>(line: &'a str, key: &str) -> Option<&'a str> {
    let (lhs, rhs) = line.split_once('=')?;
    if lhs.trim() != key {
        return None;
    }
    rhs.trim().trim_matches('"').split('"').next()
}

fn quoted_after<'a>(line: &'a str, marker: &str) -> Option<&'a str> {
    let rest = line.split(marker).nth(1)?;
    rest.split('"').nth(1)
}
