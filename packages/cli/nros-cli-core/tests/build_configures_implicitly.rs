//! phase-530 W1 (RFC-0120 D3) -- `nros build` configures implicitly, like
//! cargo: a stale (or missing) configure stamp runs `nros configure` before
//! the build plans, so a fresh clone builds with no `nros sync` first.
//!
//! Each test drives the binary's own dispatch in-process on a COPY of a
//! tracked example under the repo's gitignored `tmp/`, so the copy starts as a
//! fresh clone would.
//!
//! Run with:
//! `cargo nextest run --manifest-path packages/cli/Cargo.toml -p nros-cli-core --test build_configures_implicitly`

mod common;

use std::{
    fs,
    path::{Path, PathBuf},
};

// Issue 1659 -- every git the CLI tree spawns clears the inherited git env.
include!("../../../tooling/nros-build-paths/src/git_env.rs");

use clap::Parser;
use nros_cli_core::cmd::Cmd;

#[derive(Parser)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

/// The full dispatch, exactly as the binary runs it (lowering included).
fn nros(argv: &[&str]) -> Result<(), String> {
    let mut full = vec!["nros"];
    full.extend_from_slice(argv);
    let cmd = Cli::try_parse_from(&full)
        .unwrap_or_else(|e| panic!("`{}` does not parse: {e}", argv.join(" ")))
        .cmd;
    nros_cli_core::run(cmd).map_err(|e| format!("{e:?}"))
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .expect("repo root")
        .to_path_buf()
}

/// Process-wide setup every test needs, once.
fn setup() {
    common::isolate_model_discovery();
    let resolver = common::pinned_launch_resolver();
    // SAFETY: the same values every time, set before any verb in this test
    // runs; the readers are this process's own verbs. The stale guard compares
    // the running BINARY's checkout with the workspace's, and this binary is a
    // test, not the `nros` it guards.
    unsafe {
        std::env::set_var("NROS_LAUNCH_RESOLVE", &resolver);
        std::env::set_var("NROS_SKIP_STALE_CHECK", "1");
    }
}

/// The TRACKED files of `src` (repo-relative), copied to `<tmp>/<tag>/<name>`.
fn scratch_copy(src: &str, tag: &str) -> PathBuf {
    let root = repo_root();
    let src = Path::new(src);
    let dir = root.join("tmp").join(format!(
        "build-implicit-{tag}-{}-{}",
        std::process::id(),
        common::unique_stamp()
    ));
    let dest_root = dir.join(src.file_name().unwrap());
    let out = nros_git_command("git")
        .arg("-C")
        .arg(&root)
        .args(["ls-files", "-z", "--"])
        .arg(src)
        .output()
        .expect("git ls-files");
    assert!(out.status.success(), "git ls-files failed");
    let files: Vec<&[u8]> = out
        .stdout
        .split(|b| *b == 0)
        .filter(|f| !f.is_empty())
        .collect();
    assert!(!files.is_empty(), "{} is tracked", src.display());
    for f in files {
        let rel = Path::new(std::str::from_utf8(f).unwrap());
        let dest = dest_root.join(rel.strip_prefix(src).unwrap());
        fs::create_dir_all(dest.parent().unwrap()).unwrap();
        fs::copy(root.join(rel), &dest).unwrap();
    }
    dest_root
}

fn stamp_file(project: &Path, key: &str) -> PathBuf {
    project
        .join("build/nros/configured")
        .join(key.replace(':', "__"))
        .join("stamp.toml")
}

fn cleanup(project: &Path) {
    let _ = fs::remove_dir_all(project.parent().unwrap());
}

/// A fresh copy has never been synced; `nros build` configures it itself.
#[test]
fn build_on_a_fresh_copy_needs_no_sync() {
    setup();
    let leaf = scratch_copy("examples/linux/rust/listener", "fresh");
    assert!(
        !stamp_file(&leaf, "linux").exists(),
        "a fresh copy has no stamp"
    );
    nros(&["build", "--dry-run", "--workspace", leaf.to_str().unwrap()])
        .unwrap_or_else(|e| panic!("`nros build --dry-run` on a fresh copy: {e}"));
    assert!(
        stamp_file(&leaf, "linux").is_file(),
        "the implicit configure recorded {}",
        stamp_file(&leaf, "linux").display()
    );
    cleanup(&leaf);
}

/// `--dry-run` reaches the implicit configure: it configures (and records the
/// stamp) but runs no native program, so a C/C++ leaf whose components read
/// `entities = "census"` takes no census.
#[test]
fn build_dry_run_configures_without_census() {
    setup();
    let leaf = scratch_copy("examples/qemu-armv7a-nuttx/c/talker", "census");
    let sys = fs::read_to_string(leaf.join("system.toml")).unwrap();
    assert!(
        sys.contains("entities = \"census\""),
        "the fixture states a census"
    );
    nros(&["build", "--dry-run", "--workspace", leaf.to_str().unwrap()])
        .unwrap_or_else(|e| panic!("`nros build --dry-run` on a census leaf: {e}"));
    assert!(
        stamp_file(&leaf, "qemu-armv7a-nuttx").is_file(),
        "the implicit configure recorded {}",
        stamp_file(&leaf, "qemu-armv7a-nuttx").display()
    );
    assert!(
        !leaf.join("build/census-host").exists(),
        "a dry run took a census: {} exists",
        leaf.join("build/census-host").display()
    );
    cleanup(&leaf);
}
