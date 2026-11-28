//! Issue 2293 -- `nros config explain --platform <p>` finds a platform package
//! in the user's workspace, including one `nros new platform` just wrote.
//!
//! The scaffold's own `nros-platform.toml` tells the porter to run this
//! command. It reported the platform unknown for two reasons: the search path
//! never held the workspace, and the scaffold wrote no `names`, so the package
//! answered to its directory name (`nros-platform-acmeos`) and not `acmeos`.

use std::{
    path::{Path, PathBuf},
    process::{Command, Output},
};

fn nros() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_nros"))
}

fn scaffold_platform(ws: &Path, name: &str) {
    let src = ws.join("src");
    std::fs::create_dir_all(&src).unwrap();
    let out = Command::new(nros())
        .args(["new", "platform", name, "--into"])
        .arg(&src)
        .current_dir(ws)
        .output()
        .expect("run nros new platform");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

fn explain(cwd: &Path, extra: &[&str]) -> Output {
    Command::new(nros())
        .args(["config", "explain", "--platform", "acmeos"])
        .args(extra)
        .current_dir(cwd)
        .env_remove("NROS_PLATFORMS_DIR")
        .output()
        .expect("run nros config explain")
}

/// From the workspace, with no `$NROS_PLATFORMS_DIR`: the scaffolded platform
/// resolves by the name it was scaffolded under.
#[test]
fn a_scaffolded_platform_explains_from_its_workspace() {
    let ws = tempfile::tempdir().unwrap();
    scaffold_platform(ws.path(), "acmeos");
    let out = explain(ws.path(), &[]);
    assert!(
        out.status.success(),
        "config explain must find the workspace's platform: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    // `config explain --platform` is `knob list --platform` (RFC-0133 D3):
    // the platform resolved, so its knob table printed.
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.starts_with("KNOB"), "{stdout}");
}

/// `--workspace` names the workspace from anywhere else.
#[test]
fn workspace_flag_names_the_workspace() {
    let ws = tempfile::tempdir().unwrap();
    let elsewhere = tempfile::tempdir().unwrap();
    scaffold_platform(ws.path(), "acmeos");
    let out = explain(
        elsewhere.path(),
        &["--workspace", ws.path().to_str().unwrap()],
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// The negative control: the same command from a directory that holds no such
/// package still refuses, so the passes above come from the workspace.
#[test]
fn without_the_workspace_the_platform_stays_unknown() {
    let elsewhere = tempfile::tempdir().unwrap();
    let out = explain(elsewhere.path(), &[]);
    assert!(!out.status.success());
}

/// Issue 3002 -- from a directory that is no workspace, the search reads none
/// of the platform packages left below it: a broken `nros-platform.toml` two
/// levels down (as test runs leave in `/tmp`) neither stops the command nor
/// appears in its output, and the SDK's own platforms still resolve.
#[test]
fn outside_a_workspace_nothing_below_the_directory_is_read() {
    let junk = tempfile::tempdir().unwrap();
    let pkg = junk.path().join("leftover/nros-platform-broken");
    std::fs::create_dir_all(&pkg).unwrap();
    std::fs::write(pkg.join("nros-platform.toml"), "[[ not toml").unwrap();
    let out = Command::new(nros())
        .args(["config", "explain", "--platform", "posix"])
        .current_dir(junk.path())
        .env_remove("NROS_PLATFORMS_DIR")
        .output()
        .expect("run nros config explain");
    let all = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(out.status.success(), "{all}");
    assert!(!all.contains("nros-platform-broken"), "{all}");
}
