//! Issue 2350 -- `nros run <image> --dry-run --why` after `system.toml` moved
//! since the last configure.
//!
//! A dry run never configures (Ruling R43), so the SystemModel the last
//! configure resolved is stale when the dry run plans. A real `nros run`
//! re-resolves it first, so the dry run must say it WOULD re-resolve it and
//! go on to print the plan, never refuse with "is stale".
//!
//! Drives the real binary on a copy of a tracked leaf under the repo's
//! gitignored `tmp/`, because the plan goes to stdout and a refusal is a
//! process exit.
//!
//! Run with:
//! `cargo nextest run --manifest-path packages/cli/Cargo.toml -p nros-cli --test run_dry_run_stale_model`

use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
    time::{SystemTime, UNIX_EPOCH},
};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .expect("repo root")
        .to_path_buf()
}

// Issue 1659 -- every git the CLI tree spawns clears the inherited git env.
include!("../../../tooling/nros-build-paths/src/git_env.rs");

/// The TRACKED files of `src` (repo-relative), copied under `tmp/`.
fn scratch_copy(src: &str, tag: &str) -> PathBuf {
    let root = repo_root();
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let src = Path::new(src);
    let dest_root = root
        .join("tmp")
        .join(format!("run-dry-{tag}-{}-{stamp}", std::process::id()))
        .join(src.file_name().unwrap());
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

/// The in-tree resolver, by absolute path (issue 0285); asserted, not skipped.
fn launch_resolver() -> PathBuf {
    let r = nros_cli_core::orchestration::model_location::launch_resolver_in_checkout(&repo_root());
    assert!(
        r.is_file(),
        "nros-launch-resolve not built at {} -- run `just setup-launch-resolve`",
        r.display()
    );
    r
}

fn nros(leaf: &Path, argv: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_nros"))
        .args(argv)
        .current_dir(leaf)
        .env("NROS_LAUNCH_RESOLVE", launch_resolver())
        .env("NROS_SKIP_STALE_CHECK", "1")
        .output()
        .expect("spawn nros")
}

fn text(o: &Output) -> String {
    format!(
        "status: {}\n--- stdout\n{}\n--- stderr\n{}",
        o.status,
        String::from_utf8_lossy(&o.stdout),
        String::from_utf8_lossy(&o.stderr)
    )
}

#[test]
fn run_dry_run_why_after_an_edit_reports_would_re_resolve_and_prints_the_plan() {
    let leaf = scratch_copy("examples/linux/rust/listener", "stale-model");
    let configured = nros(&leaf, &["configure"]);
    assert!(
        configured.status.success(),
        "configure: {}",
        text(&configured)
    );
    let model = leaf.join("build/nros/models/listener/system_model.yaml");
    assert!(model.is_file(), "configure resolved {}", model.display());
    let before = fs::read(&model).unwrap();

    // Tutorial 9's step: a second image added after the configure.
    let sys = leaf.join("system.toml");
    let mut body = fs::read_to_string(&sys).unwrap();
    body.push_str("\n[image.linux_second]\n");
    fs::write(&sys, body).unwrap();

    let o = nros(&leaf, &["run", "linux", "--dry-run", "--why"]);
    let all = text(&o);
    assert!(o.status.success(), "the dry run refused: {all}");
    let stdout = String::from_utf8_lossy(&o.stdout);
    assert!(
        stdout.lines().any(|l| l.starts_with("image = ")),
        "the plan names its image: {all}"
    );
    assert!(
        stdout
            .lines()
            .any(|l| l.starts_with("image = ") && l.contains("rung")),
        "--why names the rung that resolved the image: {all}"
    );
    let stderr = String::from_utf8_lossy(&o.stderr);
    assert!(
        stderr.contains("nros run: --dry-run: would re-resolve"),
        "the stale model is reported as one a run would re-resolve: {all}"
    );
    // Ruling R43: still nothing re-resolved.
    assert_eq!(
        fs::read(&model).unwrap(),
        before,
        "a dry run re-resolved the model"
    );

    let _ = fs::remove_dir_all(leaf.parent().unwrap());
}
