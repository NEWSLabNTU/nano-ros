//! Issue 2802, through the real binary: the Arm FVP is USER-PROVIDED, and
//! `nros setup board fvp-aemv8r-smp` neither fails nor downloads without it.
//!
//! A consumer's bootstrap runs this command on hosts with and without the
//! model, so it must exit 0 either way, say the model is user-provided, and
//! name the setting that found it when one did. `--dry-run` keeps the other
//! steps (RMW fetch, patch set, rustup) off this test; the FVP step runs the
//! same with or without it.
//!
//! Every knob is CHILD-process environment, so nothing in this process is
//! mutated and no other test can race it.

use std::{
    path::{Path, PathBuf},
    process::Command,
};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .unwrap()
}

fn setup_board(zephyr_ws: &Path, armfvp_bin_path: Option<&Path>) -> (Option<i32>, String) {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_nros"));
    cmd.args([
        "setup",
        "board",
        "fvp-aemv8r-smp",
        "--dry-run",
        "--zephyr-workspace",
    ])
    .arg(zephyr_ws)
    .arg("--workspace")
    .arg(repo_root())
    .current_dir(zephyr_ws)
    .env("NROS_OFFLINE", "1")
    .env_remove("ARMFVP_BIN_PATH")
    .env_remove("ARM_FVP_DIR")
    // No FVP on this PATH.
    .env("PATH", zephyr_ws.join("empty-path"));
    if let Some(dir) = armfvp_bin_path {
        cmd.env("ARMFVP_BIN_PATH", dir);
    }
    let out = cmd.output().expect("failed to exec nros");
    let mut text = String::from_utf8_lossy(&out.stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&out.stderr));
    (out.status.code(), text)
}

#[test]
fn setup_board_fvp_succeeds_without_the_model_and_names_the_setting_with_it() {
    let tmp = tempfile::tempdir().unwrap();
    let ws = tmp.path().join("zws");
    std::fs::create_dir_all(ws.join("zephyr")).unwrap();

    let (code, text) = setup_board(&ws, None);
    assert_eq!(
        code,
        Some(0),
        "no FVP must not fail the board setup:\n{text}"
    );
    for needle in ["user-provided", "ARMFVP_BIN_PATH", "libatomic1"] {
        assert!(text.contains(needle), "`{needle}` missing:\n{text}");
    }
    assert!(
        !text.contains("nros setup --tool arm-fvp"),
        "nano-ros no longer installs the FVP:\n{text}"
    );

    let bin_dir = tmp.path().join("fvp/bin");
    std::fs::create_dir_all(&bin_dir).unwrap();
    let bin = bin_dir.join("FVP_BaseR_AEMv8R");
    nros_launcher::test_support::write_executable_stub(&bin, "#!/bin/sh\nexit 0\n");
    let (code, text) = setup_board(&ws, Some(&bin_dir));
    assert_eq!(code, Some(0), "{text}");
    assert!(text.contains("found via ARMFVP_BIN_PATH"), "{text}");
}
