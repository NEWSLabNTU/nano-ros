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

/// A fresh copy has never been synced; `nros build` configures it itself,
/// through `configure::ensure_current` (the call build makes before it plans;
/// driven directly so no test compiles an image).
#[test]
fn build_on_a_fresh_copy_needs_no_sync() {
    setup();
    let leaf = scratch_copy("examples/linux/rust/listener", "fresh");
    assert!(!leaf.join("build/nros/models").exists(), "a fresh copy");
    nros_cli_core::cmd::configure::ensure_current(&leaf, &[], false, false)
        .unwrap_or_else(|e| panic!("build's implicit configure on a fresh copy: {e:?}"));
    assert!(
        leaf.join("build/nros/models").is_dir() && leaf.join("generated/std_msgs").is_dir(),
        "the implicit configure synced: the model and the message crate exist"
    );
    assert!(
        stamp_file(&leaf, "linux").is_file(),
        "it recorded its stamp"
    );
    cleanup(&leaf);
}

/// Ruling R43 — a dry run never configures, so it writes nothing: no stamp,
/// no model, no message crate. A never-configured copy has no plan to show,
/// and the refusal names the step that makes one.
#[test]
fn dry_run_records_no_stamp() {
    setup();
    let leaf = scratch_copy("examples/linux/rust/listener", "dry-stamp");
    let e = nros(&["build", "--dry-run", "--workspace", leaf.to_str().unwrap()])
        .expect_err("a never-configured copy has no plan, and a dry run makes none");
    assert!(
        e.contains("nros configure"),
        "the refusal names the step: {e}"
    );
    assert!(
        !stamp_file(&leaf, "linux").exists(),
        "a dry run recorded {}",
        stamp_file(&leaf, "linux").display()
    );
    assert!(
        !leaf.join("build/nros/models").exists() && !leaf.join("generated").exists(),
        "a dry run configured"
    );
    cleanup(&leaf);
}

/// A dry run runs no native program: a C/C++ leaf whose components read
/// `entities = "census"` takes no census (Ruling R43: it does not configure
/// at all, so it writes nothing either).
#[test]
fn build_dry_run_takes_no_census() {
    setup();
    let leaf = scratch_copy("examples/qemu-armv7a-nuttx/c/talker", "census");
    let sys = fs::read_to_string(leaf.join("system.toml")).unwrap();
    assert!(
        sys.contains("entities = \"census\""),
        "the fixture states a census"
    );
    let _ = nros(&["build", "--dry-run", "--workspace", leaf.to_str().unwrap()]);
    assert!(
        !leaf.join("build/census-host").exists(),
        "a dry run took a census: {} exists",
        leaf.join("build/census-host").display()
    );
    assert!(
        !leaf.join("build/nros/models").exists(),
        "a dry run configured"
    );
    cleanup(&leaf);
}
