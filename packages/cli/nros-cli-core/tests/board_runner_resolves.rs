//! `check-board-runner-resolves` (RFC-0119 D10, phase-528 W3): every in-tree
//! board whose registry row says it runs on the host or in an emulator
//! resolves, through the RunPlan resolver, to a runner PACKAGE that exists on
//! the provider search path, announces that runner, implements `run`, and
//! needs no placeholder the board's plan leaves empty.
//!
//! The rows come from the registry's generated status file
//! (`packages/boards/board-support-status.json`, RFC-0114 D10), the plans
//! from `run_plan::resolve` (the same call `nros build`'s next step and `nros
//! run` make), and the runner from the provider scan of the nano-ros tree.
//! Nothing here names a runner per board: a board whose plan names a runner
//! no package provides fails, and so does a provider that would not accept
//! the plan.
//!
//! The row-to-descriptor rule is `scripts/lib/board_registry.py`'s
//! `row_descriptor`: the row's first `board_keys` entry, else the single
//! `[[board]]` in `packages/boards/<crate>/nros-board.toml`.

use std::path::{Path, PathBuf};

use cargo_nano_ros::{
    extension_descriptor::{ExtensionResolveError, ResolvedRunner, resolve_runner_in},
    provider_scan::{ScanResult, scan_roots},
};
use nros_cli_core::{
    cmd::{
        build_next::BuildOutput,
        run_plan::{Bench, ImageTables, Inputs, Invocation, resolve},
    },
    orchestration::board_descriptor::{BoardCatalog, BoardDescriptor, PlatformKind},
};
use nros_extension::plan::Verb;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .unwrap()
        .to_path_buf()
}

/// The one runner lookup this gate makes. It is the provider resolution
/// every runner lookup goes through, so swapping it for the shared
/// precedence-aware resolver is this one call.
fn lookup_runner(scan: &ScanResult, name: &str) -> Result<ResolvedRunner, ExtensionResolveError> {
    resolve_runner_in(scan, name)
}

/// One registry row that runs: its key and the descriptor names it is for.
struct Row {
    key: String,
    names: Vec<String>,
}

fn runnable_rows(root: &Path) -> Vec<Row> {
    let path = root.join("packages/boards/board-support-status.json");
    let doc: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    let mut out = Vec::new();
    for r in doc["rows"].as_array().expect("rows") {
        let runs = r["runs"].as_str().unwrap_or("");
        if !matches!(runs, "host" | "emulator") || !r["external"].is_null() {
            continue;
        }
        let key = r["key"].as_str().unwrap().to_string();
        let board_keys: Vec<String> = r["board_keys"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|k| k.as_str().map(str::to_string))
            .collect();
        let names = if let Some(first) = board_keys.first() {
            vec![first.clone()]
        } else {
            let krate = r["crate"].as_str().unwrap();
            let file = root.join(format!("packages/boards/{krate}/nros-board.toml"));
            let text = std::fs::read_to_string(&file)
                .unwrap_or_else(|e| panic!("{key}: {}: {e}", file.display()));
            let doc: toml::Table = text.parse().unwrap();
            let boards = doc
                .get("board")
                .and_then(toml::Value::as_array)
                .unwrap_or_else(|| panic!("{key}: {} has no [[board]]", file.display()));
            assert_eq!(
                boards.len(),
                1,
                "{key}: {} holds {} [[board]] entries and the row names no board_keys",
                file.display(),
                boards.len()
            );
            boards[0]["names"]
                .as_array()
                .unwrap()
                .iter()
                .map(|n| n.as_str().unwrap().to_string())
                .collect()
        };
        out.push(Row { key, names });
    }
    out
}

fn descriptor_for<'a>(cat: &'a BoardCatalog, row: &Row) -> &'a BoardDescriptor {
    let hits: Vec<&BoardDescriptor> = cat
        .descriptors()
        .iter()
        .filter(|d| {
            if row.names.len() == 1 {
                d.names.contains(&row.names[0])
            } else {
                d.names == row.names
            }
        })
        .collect();
    assert_eq!(
        hits.len(),
        1,
        "{}: the names {:?} pick {} board descriptors",
        row.key,
        row.names,
        hits.len()
    );
    hits[0]
}

/// The build road's output for a board, as the planner would record it: a
/// Zephyr board builds with west, every other one with cargo (its triple, or
/// the host's).
fn output_for(d: &BoardDescriptor) -> BuildOutput {
    if d.platform == PlatformKind::Zephyr {
        BuildOutput::West {
            build_dir: Some("/ws/build/zephyr/img".into()),
        }
    } else {
        BuildOutput::Cargo {
            target_dir: "/ws/target".into(),
            triple: d.target.clone(),
            profile_dir: "release".into(),
            bin: Some("app".into()),
        }
    }
}

#[test]
fn every_runnable_board_resolves_to_a_runner_package() {
    let root = repo_root();
    let cat = BoardCatalog::load(&root).unwrap();
    let scan = scan_roots(std::slice::from_ref(&root)).unwrap();
    let rows = runnable_rows(&root);
    assert!(
        rows.len() >= 10,
        "the registry lists {} host/emulator rows; the status file is not the one this gate \
         was written against",
        rows.len()
    );
    // A malformed `package.xml` is skipped by the scan and reported only as a
    // scan error, so a runner package that cannot be READ would show up below
    // as "no provider" — name the real cause first.
    let runners_dir = root.join("packages/runners");
    let mut problems: Vec<String> = scan
        .errors
        .iter()
        .filter(|e| e.path.starts_with(&runners_dir))
        .map(|e| format!("{}: {}", e.path.display(), e.message))
        .collect();
    for row in &rows {
        let d = descriptor_for(&cat, row);
        let out = output_for(d);
        let plan = match resolve(
            Verb::Run,
            Inputs {
                image: "gate:img",
                board: &row.names[0],
                descriptor: d,
                output: Some(&out),
                tables: &ImageTables::default(),
                bench: &Bench::default(),
                invocation: &Invocation::default(),
            },
        ) {
            Ok(p) => p,
            Err(e) => {
                problems.push(format!("{}: no run plan resolves: {e}", row.key));
                continue;
            }
        };
        let runner = match lookup_runner(&scan, &plan.runner.name) {
            Ok(r) => r,
            Err(e) => {
                problems.push(format!(
                    "{}: its plan names runner `{}`, which resolves to no package: {e}",
                    row.key, plan.runner.name
                ));
                continue;
            }
        };
        if runner.package != plan.runner.package {
            problems.push(format!(
                "{}: the plan names package `{}` for runner `{}`, the provider is `{}`",
                row.key, plan.runner.package, plan.runner.name, runner.package
            ));
        }
        if !runner.descriptor.verbs.contains(&Verb::Run) {
            problems.push(format!(
                "{}: runner `{}` ({}) does not implement `run`",
                row.key,
                runner.name,
                runner.dir.display()
            ));
        }
        for need in &runner.descriptor.needs {
            if plan.placeholder(need).is_none() {
                problems.push(format!(
                    "{}: runner `{}` needs `{{{need}}}`, which the board's plan leaves empty",
                    row.key, runner.name
                ));
            }
        }
        if runner.name == "qemu" && nros_qemu_ladder::plan_line(&plan.args).is_err() {
            problems.push(format!(
                "{}: its QEMU args render no line: {:?}",
                row.key,
                nros_qemu_ladder::plan_line(&plan.args).unwrap_err()
            ));
        }
    }
    assert!(
        problems.is_empty(),
        "boards that do not resolve to a runner package:\n  {}",
        problems.join("\n  ")
    );
}

/// The negative control: a board whose plan names a runner nothing provides
/// is caught, so the gate can fail.
#[test]
fn a_runner_no_package_provides_is_refused() {
    let root = repo_root();
    let scan = scan_roots(&[root]).unwrap();
    assert!(lookup_runner(&scan, "no-such-runner").is_err());
    assert!(lookup_runner(&scan, "qemu").is_ok());
}
