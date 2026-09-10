//! RFC-0097 D12, through the real binary: **`nros doctor` with no workspace
//! verifies the INSTALL** instead of refusing to run.
//!
//! `nros-cli-core/src/cmd/doctor.rs`'s `install_report_tests` cover what the
//! report SAYS. This file covers whether a user standing in a directory that is
//! not a nano-ros checkout can reach it at all — which is a different question,
//! and was answered "no" until D12. Measured on 2026-09-10 from `/tmp`:
//!
//! ```text
//! Error: could not auto-detect the nano-ros workspace root; pass --workspace <path> explicitly
//! ```
//!
//! A user who has just run `install.sh` has no workspace to pass. The one
//! person who needed the answer was the one person refused it.
//!
//! Every knob here is passed as CHILD-process environment, so nothing in this
//! test process is mutated and no other test can race it. `NROS_STORE` points
//! at a tempdir (the report must never read the developer's real store) and
//! `NROS_OFFLINE` keeps the RFC-0097 D5 index fetch off the network.

use std::{path::Path, process::Command};

fn doctor_in(cwd: &Path, store: &Path) -> (Option<i32>, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_nros"))
        .arg("doctor")
        .current_dir(cwd)
        .env("NROS_STORE", store)
        .env("NROS_OFFLINE", "1")
        // The launcher must not dispatch out of this test, and the workspace
        // walk must not find one above the tempdir.
        .env_remove("NROS_WORKSPACE_ROOT")
        .env_remove("NROS_WORKSPACE")
        .output()
        .expect("failed to exec nros");
    let mut text = String::from_utf8_lossy(&out.stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&out.stderr));
    (out.status.code(), text)
}

/// The acceptance: it runs, it succeeds, and it reports the install.
#[test]
fn doctor_outside_a_workspace_reports_the_install_and_succeeds() {
    let tmp = tempfile::tempdir().unwrap();
    let store = tmp.path().join("store");
    let cwd = tmp.path().join("fresh-user");
    std::fs::create_dir_all(&cwd).unwrap();

    let (code, text) = doctor_in(&cwd, &store);
    assert!(
        !text.contains("could not auto-detect"),
        "D12: no workspace is a different QUESTION, not a failure:\n{text}"
    );
    assert_eq!(code, Some(0), "a healthy install exits 0:\n{text}");
    for needle in ["store", "launcher", "pin"] {
        assert!(
            text.contains(needle),
            "the install report must cover `{needle}`:\n{text}"
        );
    }
    assert!(
        text.contains(&store.display().to_string()),
        "it must name the store it read — and that store is $NROS_STORE, never \
         a hardcoded ~/.nros:\n{text}"
    );
}

/// And it still FAILS when the install is broken: a project pinning a nano-ros
/// the store cannot produce cannot be built, so `doctor` must say so with a
/// non-zero exit. Without this the verb would be unfalsifiable — it would exit
/// 0 whatever it found.
#[test]
fn doctor_outside_a_workspace_still_fails_on_an_unresolvable_pin() {
    let tmp = tempfile::tempdir().unwrap();
    let store = tmp.path().join("store");
    // A store toolchain, so the launcher decision reaches the pin at all.
    let bin = store.join("sdk").join("nros").join("0.7.9").join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    std::fs::write(bin.join("nros"), b"#!/bin/sh\nexit 0\n").unwrap();

    let cwd = tmp.path().join("someone-elses-project");
    std::fs::create_dir_all(&cwd).unwrap();
    std::fs::write(
        cwd.join("nros-toolchain.toml"),
        "[toolchain]\nversion = \"0.9.9\"\n",
    )
    .unwrap();

    let (code, text) = doctor_in(&cwd, &store);
    assert_ne!(
        code,
        Some(0),
        "an unresolvable pin must not exit 0:\n{text}"
    );
    assert!(
        text.contains("0.9.9"),
        "it must name the version the project asked for:\n{text}"
    );
}
