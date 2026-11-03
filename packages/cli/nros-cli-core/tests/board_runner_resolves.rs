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
//! Flash (phase-529 W2): every in-tree board stating `[board.flash]` resolves
//! a `flash` plan through the same resolver to a package that implements
//! `flash`, and a board whose console is `serial` finds the `serial` runner
//! implementing `console` (the default console of `nros flash --monitor` and
//! `nros monitor`).
//!
//! Known gaps: an emulator kind in `run_plan::RUNNERLESS_EMULATORS` is
//! refused by the resolver, naming an issue. Such a row passes only while the
//! issue is open and no package provides the runner; once one does, the
//! exemption is stale and the gate fails.
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
        run_plan::{Bench, ImageTables, Inputs, Invocation, RUNNERLESS_EMULATORS, resolve},
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
    let mut gaps: Vec<String> = Vec::new();
    for row in &rows {
        let d = descriptor_for(&cat, row);
        let out = output_for(d);
        let kind = d
            .run
            .as_ref()
            .and_then(|r| r.emulator_parts())
            .map(|(k, _)| k.to_string());
        if let Some((kind, issue)) = kind
            .as_deref()
            .and_then(|k| RUNNERLESS_EMULATORS.iter().find(|(r, _)| *r == k))
        {
            match known_gap(&root, &scan, row, d, &out, kind, issue) {
                Ok(note) => gaps.push(note),
                Err(p) => problems.push(p),
            }
            continue;
        }
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
    for g in &gaps {
        eprintln!("known gap: {g}");
    }
    assert!(
        problems.is_empty(),
        "boards that do not resolve to a runner package:\n  {}",
        problems.join("\n  ")
    );
}

/// A row on a runnerless emulator kind: the resolver must refuse it naming
/// the issue, the issue must be open, and no package may provide the runner
/// (else the exemption is stale and the board should resolve normally).
fn known_gap(
    root: &Path,
    scan: &ScanResult,
    row: &Row,
    d: &BoardDescriptor,
    out: &BuildOutput,
    kind: &str,
    issue: &str,
) -> Result<String, String> {
    if let Ok(r) = lookup_runner(scan, kind) {
        return Err(format!(
            "{}: runner `{kind}` is now provided by {} ({}), so remove `{kind}` from \
             run_plan::RUNNERLESS_EMULATORS and close issue {issue}",
            row.key,
            r.package,
            r.dir.display()
        ));
    }
    let open = std::fs::read_dir(root.join("docs/issues"))
        .unwrap()
        .filter_map(Result::ok)
        .any(|e| {
            e.file_name()
                .to_string_lossy()
                .starts_with(&format!("{issue}-"))
        });
    if !open {
        return Err(format!(
            "{}: `{kind}` is exempt for issue {issue}, which is not open in docs/issues/",
            row.key
        ));
    }
    match resolve(
        Verb::Run,
        Inputs {
            image: "gate:img",
            board: &row.names[0],
            descriptor: d,
            output: Some(out),
            tables: &ImageTables::default(),
            bench: &Bench::default(),
            invocation: &Invocation::default(),
        },
    ) {
        Err(e) if e.to_string().contains(&format!("issue {issue}")) => Ok(format!(
            "{}: `{kind}` has no runner package (issue {issue})",
            row.key
        )),
        Err(e) => Err(format!(
            "{}: refused, but not for the `{kind}` gap (issue {issue}): {e}",
            row.key
        )),
        Ok(p) => Err(format!(
            "{}: `{kind}` is exempt for issue {issue}, yet a plan resolved to runner `{}`",
            row.key, p.runner.name
        )),
    }
}

/// phase-529 W2: every in-tree board with `[board.flash]` resolves a flash
/// plan to a runner package implementing `flash`, and its serial console to
/// the `serial` runner implementing `console`. `port` and `probe` are the
/// machine's (the bench, RFC-0118 D6), so a runner needing them is not held
/// to a board-only plan.
#[test]
fn every_flashable_board_resolves_a_flash_runner() {
    let root = repo_root();
    let cat = BoardCatalog::load(&root).unwrap();
    let scan = scan_roots(std::slice::from_ref(&root)).unwrap();
    let boards: Vec<&BoardDescriptor> = cat
        .descriptors()
        .iter()
        .filter(|d| d.flash.is_some())
        .collect();
    assert!(
        boards.len() >= 2,
        "{} in-tree boards state `[board.flash]`; the gate was written against the Nucleo \
         and the Pico W",
        boards.len()
    );
    let mut problems = Vec::new();
    for d in boards {
        let name = &d.names[0];
        let out = output_for(d);
        let plan = match resolve(
            Verb::Flash,
            Inputs {
                image: "gate:img",
                board: name,
                descriptor: d,
                output: Some(&out),
                tables: &ImageTables::default(),
                bench: &Bench::default(),
                invocation: &Invocation::default(),
            },
        ) {
            Ok(p) => p,
            Err(e) => {
                problems.push(format!("{name}: no flash plan resolves: {e}"));
                continue;
            }
        };
        match lookup_runner(&scan, &plan.runner.name) {
            Ok(r) => {
                if r.package != plan.runner.package {
                    problems.push(format!(
                        "{name}: the plan names package `{}` for flash runner `{}`, the \
                         provider is `{}`",
                        plan.runner.package, plan.runner.name, r.package
                    ));
                }
                if !r.descriptor.verbs.contains(&Verb::Flash) {
                    problems.push(format!(
                        "{name}: runner `{}` ({}) does not implement `flash`",
                        r.name,
                        r.dir.display()
                    ));
                }
                for need in &r.descriptor.needs {
                    if !MACHINE_FACTS.contains(&need.as_str()) && plan.placeholder(need).is_none() {
                        problems.push(format!(
                            "{name}: flash runner `{}` needs `{{{need}}}`, which the board's \
                             plan leaves empty",
                            r.name
                        ));
                    }
                }
            }
            Err(e) => problems.push(format!(
                "{name}: its flash plan names runner `{}`, which resolves to no package: {e}",
                plan.runner.name
            )),
        }
        if plan.console.kind == nros_extension::plan::ConsoleKind::Serial {
            match lookup_runner(&scan, "serial") {
                Ok(r) if r.descriptor.verbs.contains(&Verb::Console) => {}
                Ok(r) => problems.push(format!(
                    "{name}: the `serial` runner ({}) does not implement `console`",
                    r.dir.display()
                )),
                Err(e) => problems.push(format!(
                    "{name}: its console is serial and no package provides the `serial` \
                     runner: {e}"
                )),
            }
        }
    }
    assert!(
        problems.is_empty(),
        "boards with `[board.flash]` that do not resolve a flash runner:\n  {}",
        problems.join("\n  ")
    );
}

/// phase-529 W3: what `nros flash --dry-run --why` and `nros monitor
/// --dry-run --why` print resolves on every in-tree board with
/// `[board.flash]`. The flash plan renders with its provenance, and the
/// console plan (`--monitor`, and `nros monitor`) resolves to a package
/// implementing `console`, on the flash plan's port.
#[test]
fn every_flashable_board_resolves_flash_and_monitor_dry_runs() {
    use nros_cli_core::cmd::{flash::monitor_plan, run_plan::render_plan_text};
    let root = repo_root();
    let cat = BoardCatalog::load(&root).unwrap();
    let scan = scan_roots(std::slice::from_ref(&root)).unwrap();
    let invocation = Invocation {
        port: Some("/dev/ttyACM9".into()),
        ..Invocation::default()
    };
    let mut problems = Vec::new();
    let mut seen = 0;
    for d in cat.descriptors().iter().filter(|d| d.flash.is_some()) {
        seen += 1;
        let name = &d.names[0];
        let out = output_for(d);
        let inputs = Inputs {
            image: "gate:img",
            board: name,
            descriptor: d,
            output: Some(&out),
            tables: &ImageTables::default(),
            bench: &Bench::default(),
            invocation: &invocation,
        };
        let flash = match resolve(Verb::Flash, inputs) {
            Ok(p) => p,
            Err(e) => {
                problems.push(format!(
                    "{name}: `nros flash --dry-run` resolves nothing: {e}"
                ));
                continue;
            }
        };
        let why = render_plan_text(&flash, true, false);
        if !why.contains("[provenance]") || !why.contains("\"console.port\" = \"invocation\"") {
            problems.push(format!(
                "{name}: `--why` names no rung for the port:\n{why}"
            ));
        }
        let console = match resolve(Verb::Console, inputs) {
            Ok(p) => p,
            Err(e) => {
                problems.push(format!(
                    "{name}: `nros monitor --dry-run` resolves nothing: {e}"
                ));
                continue;
            }
        };
        if console != monitor_plan(&flash) {
            problems.push(format!(
                "{name}: `nros monitor`'s plan is not `nros flash --monitor`'s console plan"
            ));
        }
        if console.console.port.as_deref() != Some("/dev/ttyACM9") {
            problems.push(format!("{name}: the console lost the flashed port"));
        }
        match lookup_runner(&scan, &console.runner.name) {
            Ok(r) if r.descriptor.verbs.contains(&Verb::Console) => {}
            Ok(r) => problems.push(format!(
                "{name}: console runner `{}` does not implement `console`",
                r.name
            )),
            Err(e) => problems.push(format!(
                "{name}: console runner `{}` resolves to no package: {e}",
                console.runner.name
            )),
        }
    }
    assert!(seen >= 2, "{seen} boards state `[board.flash]`");
    assert!(
        problems.is_empty(),
        "flash/monitor dry runs that do not resolve:\n  {}",
        problems.join("\n  ")
    );
}

/// `nros monitor` on an emulated board is refused, naming `nros run`.
#[test]
fn monitor_on_an_emulated_board_names_nros_run() {
    let root = repo_root();
    let cat = BoardCatalog::load(&root).unwrap();
    let d = cat
        .descriptors()
        .iter()
        .find(|d| {
            d.flash.is_none()
                && d.platform != PlatformKind::Zephyr
                && d.run.as_ref().is_some_and(|r| r.emulator.is_some())
        })
        .expect("an emulated, non-Zephyr board");
    let out = output_for(d);
    let e = resolve(
        Verb::Console,
        Inputs {
            image: "gate:img",
            board: &d.names[0],
            descriptor: d,
            output: Some(&out),
            tables: &ImageTables::default(),
            bench: &Bench::default(),
            invocation: &Invocation::default(),
        },
    )
    .unwrap_err();
    assert!(e.to_string().contains("nros run"), "{e}");
}

/// Placeholders the machine supplies at flash time (`--port`, `--probe`, the
/// bench file), never the board descriptor.
const MACHINE_FACTS: &[&str] = &["port", "probe"];

/// The negative control: a board whose plan names a runner nothing provides
/// is caught, so the gate can fail.
#[test]
fn a_runner_no_package_provides_is_refused() {
    let root = repo_root();
    let scan = scan_roots(&[root]).unwrap();
    assert!(lookup_runner(&scan, "no-such-runner").is_err());
    assert!(lookup_runner(&scan, "qemu").is_ok());
}
