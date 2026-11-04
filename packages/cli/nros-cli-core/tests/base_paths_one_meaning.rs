//! `--base-paths` means one thing on every verb (RFC-0117 D4, RFC-0119 D4,
//! phase-527 W2d).
//!
//! colcon's model: the flag names the workspace's SOURCE roots. It replaces
//! the workspace and its `nros.toml` `package_paths`, and nothing else — the
//! nano-ros underlay and `NROS_PACKAGE_PATH` stay on the provider search path.
//! `nros build`, `nros list`, `nros sync`, `nros dev ws providers` and the
//! build inside `nros run`/`flash`/`monitor` all read it through one flag
//! struct and one root-list function (`cmd::ws::source_roots`).
//!
//! PLAN tests, like `host_build_role.rs`: nothing is compiled.

use std::path::{Path, PathBuf};

use cargo_nano_ros::{
    provider_overrides::{OverridePolicy, resolve_provider},
    provider_scan::{RootOrigin, scan_search_path},
};
use clap::Parser;
use nros_cli_core::cmd::{self, build, ws};

fn write(path: &Path, body: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, body).unwrap();
}

fn pkg_xml(name: &str, export: &str) -> String {
    format!(
        "<?xml version=\"1.0\"?>\n<package format=\"3\">\n<name>{name}</name>\n\
         <version>0.0.0</version>\n<description>t</description>\n\
         <maintainer email=\"a@b.c\">m</maintainer>\n<license>Apache-2.0</license>\n\
         <export>{export}</export>\n</package>\n"
    )
}

fn repo_root() -> PathBuf {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .expect("repo root")
        .to_path_buf();
    assert!(root.join("packages/boards").is_dir(), "{}", root.display());
    root.canonicalize().unwrap()
}

/// A verb's own argument struct, parsed the way the CLI parses it.
#[derive(Parser)]
struct Verb<T: clap::Args> {
    #[command(flatten)]
    args: T,
}

fn parse<T: clap::Args>(verb: &str, rest: &[&str]) -> T {
    let mut argv = vec![verb];
    argv.extend_from_slice(rest);
    Verb::<T>::try_parse_from(argv)
        .unwrap_or_else(|e| panic!("`{verb}`: {e}"))
        .args
}

/// A workspace whose packages live in `src/` and in a `vendor/` tree OUTSIDE
/// it, beside an unrelated `tools/` directory.
fn fixture(tmp: &Path) -> PathBuf {
    let ws = tmp.join("ws");
    write(
        &ws.join("src/talker_pkg/package.xml"),
        &pkg_xml("talker_pkg", ""),
    );
    write(
        &ws.join("tools/helper_pkg/package.xml"),
        &pkg_xml("helper_pkg", ""),
    );
    write(
        &tmp.join("vendor/vendored_pkg/package.xml"),
        &pkg_xml("vendored_pkg", ""),
    );
    ws.canonicalize().unwrap()
}

const BASE: &[&str] = &["--base-paths", "src", "../vendor"];

fn expected() -> Vec<PathBuf> {
    vec![PathBuf::from("src"), PathBuf::from("../vendor")]
}

/// Every verb that takes the flag parses it to the same list, and the build
/// inside `nros run`/`flash`/`monitor` is handed exactly that list.
#[test]
fn every_verb_reads_the_same_base_paths() {
    let b: build::Args = parse("build", BASE);
    assert_eq!(b.selection.sources.base_paths, expected(), "build");
    let l: ws::ListArgs = parse("list", BASE);
    assert_eq!(l.selection.sources.base_paths, expected(), "list");
    let s: ws::SyncArgs = parse("sync", BASE);
    assert_eq!(s.sources.base_paths, expected(), "sync");
    let p: ws::ProvidersArgs = parse("providers", BASE);
    assert_eq!(p.sources.base_paths, expected(), "providers");
    let f: cmd::flash::Args = parse("flash", BASE);
    assert_eq!(f.sources.base_paths, expected(), "flash");
    let m: cmd::monitor::Args = parse("monitor", BASE);
    assert_eq!(m.sources.base_paths, expected(), "monitor");

    // `nros run`: the selection it plans and builds with.
    let r: cmd::run_plan::Args = parse("run", BASE);
    let sel = cmd::run_plan::Selection::from(&r);
    assert_eq!(sel.base_paths, expected(), "run");
    let mut argv = vec!["build".to_string()];
    argv.extend(sel.build_args());
    let implicit = build::Args::try_parse_from(argv).expect("the implicit build parses");
    assert_eq!(
        implicit.selection.sources.base_paths,
        expected(),
        "the build inside `nros run` gets the same list"
    );
}

/// The one root list: build discovery walks what the provider search path
/// holds between the underlay and the environment overlay.
#[test]
fn discovery_and_the_provider_search_path_share_the_root_list() {
    let tmp = tempfile::tempdir().unwrap();
    let ws_root = fixture(tmp.path());
    let sources = ws::source_roots(&ws_root, &expected()).expect("roots");
    assert_eq!(
        sources.paths(),
        vec![
            ws_root.join("src"),
            tmp.path().join("vendor").canonicalize().unwrap()
        ]
    );
    let search = ws::provider_search_path_for(&ws_root, &expected()).expect("search path");
    let source_tier: Vec<PathBuf> = search
        .roots
        .iter()
        .filter(|r| r.origin == RootOrigin::BasePaths)
        .map(|r| r.path.clone())
        .collect();
    assert_eq!(source_tier, sources.paths());

    // `nros build` and `nros list` discover over that list: `tools/` is not
    // in it, the out-of-workspace `vendor/` is.
    let found = nros_cli_core::builder::discover::discover_in(&ws_root, &sources, &[], None)
        .expect("discovers");
    let mut names: Vec<&str> = found.packages.iter().map(|p| p.name.as_str()).collect();
    names.sort_unstable();
    assert_eq!(names, vec!["talker_pkg", "vendored_pkg"]);
}

/// The underlay stays: under `--base-paths` the nano-ros tree is still root
/// 0, and a runner it provides still resolves.
#[test]
fn the_underlay_stays_visible_under_base_paths() {
    let tmp = tempfile::tempdir().unwrap();
    let ws_root = fixture(tmp.path());
    let search = ws::provider_search_path_for(&ws_root, &[PathBuf::from("src")]).unwrap();
    assert_eq!(search.roots[0].origin, RootOrigin::NanoRosTree);
    assert_eq!(search.roots[0].path, repo_root());
    let scan = scan_search_path(&search).unwrap();
    let policy = OverridePolicy::new(&ws_root, &Default::default(), &[]);
    let r = resolve_provider(&scan, "runner", "native", &policy).expect("native resolves");
    assert!(
        r.resolution
            .winner
            .dir
            .ends_with("packages/runners/nros_runner_native"),
        "{}",
        r.resolution.winner.dir.display()
    );
}

/// A root outside the workspace is allowed and said: in the providers
/// listing's text, and in `nros run --why`.
#[test]
fn an_out_of_workspace_root_is_shown() {
    let tmp = tempfile::tempdir().unwrap();
    let ws_root = fixture(tmp.path());
    let vendor = tmp.path().join("vendor").canonicalize().unwrap();
    let search = ws::provider_search_path_for(&ws_root, &expected()).unwrap();
    let listing = ws::render_outside_roots(&ws_root, &search);
    assert!(
        listing.contains(&format!(
            "source root {} (--base-paths) is outside the workspace",
            vendor.display()
        )),
        "{listing}"
    );
    assert!(
        !listing.contains(&ws_root.join("src").display().to_string()),
        "{listing}"
    );

    let why = cmd::run_exec::why_source_roots(&ws_root, &expected());
    assert!(why.contains("# source roots:"), "{why}");
    assert!(
        why.contains(&format!(
            "{} (--base-paths, outside the workspace)",
            vendor.display()
        )),
        "{why}"
    );
    // Nothing to say for a workspace whose only root is itself.
    assert_eq!(cmd::run_exec::why_source_roots(&ws_root, &[]), "");
}

/// A missing base path is refused by every verb's root list; an existing
/// empty one warns from discovery.
#[test]
fn a_missing_base_path_errors_and_an_empty_one_warns() {
    let tmp = tempfile::tempdir().unwrap();
    let ws_root = fixture(tmp.path());
    let e = ws::source_roots(&ws_root, &[PathBuf::from("nope")]).unwrap_err();
    assert!(format!("{e:#}").contains("`nope`"), "{e:#}");
    let e = ws::provider_search_path_for(&ws_root, &[PathBuf::from("nope")]).unwrap_err();
    assert!(format!("{e:#}").contains("not a directory"), "{e:#}");

    std::fs::create_dir_all(ws_root.join("empty")).unwrap();
    let sources =
        ws::source_roots(&ws_root, &[PathBuf::from("src"), PathBuf::from("empty")]).unwrap();
    let found =
        nros_cli_core::builder::discover::discover_in(&ws_root, &sources, &[], None).unwrap();
    assert!(
        found
            .warnings
            .iter()
            .any(|w| w.contains("holds no package")),
        "{:?}",
        found.warnings
    );
}

/// `nros build --base-paths` plans over the same roots: a duplicate package
/// across two of them is refused, naming both.
#[test]
fn build_refuses_a_duplicate_across_two_base_paths() {
    let tmp = tempfile::tempdir().unwrap();
    let ws_root = fixture(tmp.path());
    write(
        &tmp.path().join("vendor/talker_pkg/package.xml"),
        &pkg_xml("talker_pkg", ""),
    );
    let mut args: build::Args =
        build::Args::try_parse_from(["build", "--dry-run", "--base-paths", "src", "../vendor"])
            .unwrap();
    args.workspace = Some(ws_root.clone());
    args.nano_ros_path = Some(repo_root());
    let e = format!("{:#}", build::plan_builds(&args).expect_err("a duplicate"));
    assert!(e.contains("package `talker_pkg` is in two places"), "{e}");
    assert!(
        e.contains(&ws_root.join("src/talker_pkg").display().to_string()),
        "{e}"
    );
    assert!(e.contains("vendor/talker_pkg"), "{e}");
}
