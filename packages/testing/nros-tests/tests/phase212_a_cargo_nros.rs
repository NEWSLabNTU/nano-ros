//! Phase 212.A — `cargo nros` subcommand dispatch.
//!
//! Two coverage points:
//! 1. `cargo_nros_plan_matches_nros_plan` — `cargo nros plan <bringup> …`
//!    produces a `nros-plan.json` byte-identical to what `nros plan …`
//!    writes on the same fixture inputs.
//! 2. `cargo_nros_plan_explain_dispatch_dry_run` — `cargo nros plan
//!    --explain …` prints the underlying `nros plan …` invocation and
//!    exits 0 without writing any plan artifact.
//!
//! Skips cleanly via `nros_tests::skip!` when `cargo`, `nros`, or
//! `cargo-nros` aren't on the host. Mirrors the stage_fixture pattern in
//! `phase212_d_workspace_metadata.rs`.

use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

fn workspace_root() -> PathBuf {
    nros_tests::project_root()
}

fn fixture(name: &str) -> PathBuf {
    workspace_root()
        .join("packages/testing/nros-tests/fixtures")
        .join(name)
}

fn copy_tree(src: &Path, dst: &Path) -> std::io::Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let from = entry.path();
        let to = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_tree(&from, &to)?;
        } else if ty.is_file() {
            fs::copy(&from, &to)?;
        }
    }
    Ok(())
}

fn stage_fixture(name: &str) -> (tempfile::TempDir, PathBuf) {
    let src = fixture(name);
    let dst = tempfile::tempdir().expect("tempdir");
    copy_tree(&src, dst.path()).expect("copy fixture");
    // Drop pre-baked build/ — we re-generate the plan in test.
    let _ = fs::remove_dir_all(dst.path().join("build"));
    let root = dst.path().to_path_buf();
    (dst, root)
}

fn nros_bin_dir() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    let dir = PathBuf::from(home).join(".nros/bin");
    if dir.is_dir() { Some(dir) } else { None }
}

fn cargo_on_path() -> bool {
    Command::new("cargo")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Prepend the `~/.nros/bin` dir to PATH on a `Command`, so a bare
/// `cargo nros …` finds the installed `cargo-nros` shim.
fn with_nros_path(cmd: &mut Command, nros_dir: &Path) {
    let existing = std::env::var_os("PATH").unwrap_or_default();
    let mut paths: Vec<PathBuf> = std::env::split_paths(&existing).collect();
    paths.insert(0, nros_dir.to_path_buf());
    let joined = std::env::join_paths(paths).expect("join PATH");
    cmd.env("PATH", joined);
}

fn require_prereqs() -> Option<PathBuf> {
    let dir = nros_bin_dir()?;
    if !dir.join("nros").is_file() {
        return None;
    }
    if !dir.join("cargo-nros").is_file() {
        return None;
    }
    if !cargo_on_path() {
        return None;
    }
    Some(dir)
}

const BRINGUP: &str = "demo_bringup";
const LAUNCH: &str = "src/demo_bringup/launch/system.launch.xml";

#[test]
fn cargo_nros_plan_matches_nros_plan() {
    let Some(nros_dir) = require_prereqs() else {
        nros_tests::skip!("prereqs missing (cargo / ~/.nros/bin/nros / ~/.nros/bin/cargo-nros)");
    };

    // Two stagings: one for the `nros plan` ref, one for the
    // `cargo nros plan` candidate. The plan JSON embeds the relative
    // `--out-dir` path (`launch_record` field) so we MUST use the
    // same out-dir name in both invocations to get a byte match.
    let (_g_ref, root_ref) = stage_fixture("multi_pkg_workspace_cpp");
    let (_g_cand, root_cand) = stage_fixture("multi_pkg_workspace_cpp");
    let out = "out";

    // Reference: direct `nros plan`.
    let nros = nros_dir.join("nros");
    let ref_status = Command::new(&nros)
        .args(["plan", "--workspace", ".", "--out-dir", out, BRINGUP, LAUNCH])
        .current_dir(&root_ref)
        .output()
        .expect("spawn nros plan");
    assert!(
        ref_status.status.success(),
        "nros plan exit={} stderr={}",
        ref_status.status,
        String::from_utf8_lossy(&ref_status.stderr)
    );

    // Candidate: `cargo nros plan` via the cargo subcommand shim.
    let mut cand = Command::new("cargo");
    with_nros_path(&mut cand, &nros_dir);
    let cand_status = cand
        .args(["nros", "plan", "--workspace", ".", "--out-dir", out, BRINGUP, LAUNCH])
        .current_dir(&root_cand)
        .output()
        .expect("spawn cargo nros plan");
    assert!(
        cand_status.status.success(),
        "cargo nros plan exit={} stderr={}",
        cand_status.status,
        String::from_utf8_lossy(&cand_status.stderr)
    );

    let ref_plan = root_ref.join(out).join("nros-plan.json");
    let cand_plan = root_cand.join(out).join("nros-plan.json");
    let ref_bytes = fs::read(&ref_plan).expect("read ref nros-plan.json");
    let cand_bytes = fs::read(&cand_plan).expect("read cand nros-plan.json");
    assert_eq!(
        ref_bytes,
        cand_bytes,
        "`cargo nros plan` and `nros plan` produced divergent nros-plan.json\nref={}\ncand={}",
        ref_plan.display(),
        cand_plan.display()
    );
}

#[test]
fn cargo_nros_plan_explain_dispatch_dry_run() {
    let Some(nros_dir) = require_prereqs() else {
        nros_tests::skip!("prereqs missing (cargo / ~/.nros/bin/nros / ~/.nros/bin/cargo-nros)");
    };

    let (_g, root) = stage_fixture("multi_pkg_workspace_cpp");
    let out = "out";

    let mut cmd = Command::new("cargo");
    with_nros_path(&mut cmd, &nros_dir);
    let res = cmd
        .args([
            "nros", "plan", "--explain", "--workspace", ".", "--out-dir", out, BRINGUP, LAUNCH,
        ])
        .current_dir(&root)
        .output()
        .expect("spawn cargo nros plan --explain");
    assert!(
        res.status.success(),
        "cargo nros plan --explain exit={} stderr={}",
        res.status,
        String::from_utf8_lossy(&res.stderr)
    );

    let stdout = String::from_utf8_lossy(&res.stdout);
    // Explain output must surface the underlying `nros plan …` invocation.
    assert!(
        stdout.contains("nros plan"),
        "explain output missing dispatched `nros plan` invocation:\nstdout: {stdout}\nstderr: {}",
        String::from_utf8_lossy(&res.stderr)
    );
    // Each plan-positional must show up so the dispatch is verifiable.
    for needle in [BRINGUP, LAUNCH, "--out-dir", out, "--workspace"] {
        assert!(
            stdout.contains(needle),
            "explain output missing `{needle}`:\nstdout: {stdout}"
        );
    }

    // Dry run: no nros-plan.json on disk.
    let plan = root.join(out).join("nros-plan.json");
    assert!(
        !plan.exists(),
        "`--explain` should not write nros-plan.json, found {}",
        plan.display()
    );
}
