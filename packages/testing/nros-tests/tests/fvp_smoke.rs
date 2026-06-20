//! Phase 215.G — end-to-end FVP smoke through the board-crate IMPORT
//! surface.
//!
//! This is the canonical "is the board crate import surface healthy?"
//! gate (215.G.2). Unlike `phase217_c_fvp_runtime` (which drives an
//! example carve-out talker), this test exercises the minimal
//! ASI-shaped fixture at
//! `packages/testing/nros-tests/fixtures/board_import_fvp/` — whose
//! `CMakeLists.txt` carries ONLY `nano_ros_use_board(fvp-aemv8r-smp)`
//! with `find_package(Zephyr)` and a trivial `printk` app. A green run
//! proves the single-call import path (`board.cmake` → `BOARD` /
//! `EXTRA_CONF_FILE` / `DTC_OVERLAY_FILE` / `NANO_ROS_RMW` /
//! `NROS_BOARD_RUNNER`) boots to the application on real FVP hardware.
//!
//! The test is a thin invoker + UART scraper over the
//! `just zephyr build-fvp-board-import` / `run-fvp-board-import`
//! recipes (Phase 215.G.1), which own the env wiring (workspace `cd`,
//! pinned make/ninja, `ZEPHYR_SDK_INSTALL_DIR`, `NROS_REPO_DIR`,
//! `west fvp run` resolver). Re-implementing that surface here would
//! drift.
//!
//! Skip preconditions (each `nros_tests::skip!` — keeps the
//! `[SKIPPED]`-panic semantics nextest treats as expected; NEVER a
//! bare `eprintln!`+`return`, which would report a false PASS):
//!   1. ARM FVP not resolvable via `scripts/zephyr/resolve-fvp-bin.sh`
//!      (gated, license-walled — `[gated.arm-fvp]` in
//!      `nros-sdk-index.toml`).
//!   2. `west` not on PATH (Zephyr SDK absent).
//!   3. Zephyr workspace not set up.
//!   4. The fixture ELF missing — hint at `build-fvp-board-import`.
//!
//! On a hit the FVP process group is killed (`ManagedProcess::Drop`
//! closes the loop on timeout / panic / nextest SIGKILL).

use std::{
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};

use nros_tests::{process::ManagedProcess, project_root, skip};

fn have(cmd: &str) -> bool {
    Command::new("which")
        .arg(cmd)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Resolve the FVP binary directory via `scripts/zephyr/resolve-fvp-bin.sh`.
/// `Some(dir)` on a hit, `None` if the resolver exited non-zero (no
/// `ARMFVP_BIN_PATH` / `ARM_FVP_DIR` / `PATH` hit).
fn resolve_fvp_dir(root: &Path) -> Option<String> {
    let script = root.join("scripts/zephyr/resolve-fvp-bin.sh");
    let out = Command::new("bash").arg(&script).output().ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (!s.is_empty()).then_some(s)
}

/// Resolve the Zephyr workspace path the same way `just/zephyr.just`
/// does: `$NROS_ZEPHYR_WORKSPACE` → in-tree `zephyr-workspace/` →
/// sibling `../nano-ros-workspace/`. `Some(canonical)` only when the
/// candidate carries a `zephyr/` subdir.
fn resolve_zephyr_workspace(root: &Path) -> Option<PathBuf> {
    let candidates: Vec<PathBuf> = if let Ok(env) = std::env::var("NROS_ZEPHYR_WORKSPACE") {
        vec![PathBuf::from(env)]
    } else {
        vec![
            root.join("zephyr-workspace"),
            root.parent()
                .map(|p| p.join("nano-ros-workspace"))
                .unwrap_or_default(),
        ]
    };
    for cand in candidates {
        if let Ok(canon) = cand.canonicalize()
            && canon.join("zephyr").is_dir()
        {
            return Some(canon);
        }
    }
    None
}

#[test]
fn fvp_board_import_fixture_boots() {
    let root = project_root();

    // 1. FVP installed?
    if resolve_fvp_dir(&root).is_none() {
        skip!(
            "ARM FVP not resolvable (set ARMFVP_BIN_PATH or ARM_FVP_DIR; \
             gated install — accept Arm EULA at \
             https://developer.arm.com/downloads/-/arm-ecosystem-fvps)"
        );
    }

    // 2. west on PATH?
    if !have("west") {
        skip!("west not on PATH (Zephyr SDK not provisioned)");
    }

    // 3. Zephyr workspace set up?
    let workspace = match resolve_zephyr_workspace(&root) {
        Some(w) => w,
        None => skip!(
            "Zephyr workspace not set up (run `just zephyr setup` or set NROS_ZEPHYR_WORKSPACE)"
        ),
    };

    // 4. Fixture ELF prebuilt? `build-fvp-board-import/zephyr/zephyr.elf`
    // matches `just zephyr build-fvp-board-import` (215.G.1 build dir).
    let elf = workspace
        .join("build-fvp-board-import")
        .join("zephyr")
        .join("zephyr.elf");
    if !elf.is_file() {
        skip!(
            "FVP board-import fixture ELF missing at {}; \
             run `just zephyr build-fvp-board-import` first",
            elf.display()
        );
    }

    // Drive the canonical recipe — it owns the env wiring + `west fvp
    // run` delegation (Phase 215.D / 215.G.1).
    let mut cmd = Command::new("just");
    cmd.current_dir(&root)
        .args(["zephyr", "run-fvp-board-import"]);

    let mut proc = ManagedProcess::spawn_command(cmd, "fvp-board-import")
        .expect("spawn just zephyr run-fvp-board-import");

    // FVP cold-boots in ~30 s (Cortex-A SMP + Zephyr 3.7). Budget 120 s
    // to the fixture's `nros: smoke ok` line. Drop kills the FVP group
    // if we exceed the budget or the harness aborts.
    let timeout = Duration::from_secs(120);
    let output = match proc.wait_for_output_pattern("nros: smoke ok", timeout) {
        Ok(o) => o,
        Err(e) => {
            panic!(
                "FVP board-import fixture did not reach `nros: smoke ok` within {:?}: {}",
                timeout, e,
            );
        }
    };

    // Assert: Zephyr booted (banner stable across 3.7..4.x) AND the
    // fixture's `main()` ran through the import surface.
    assert!(
        output.contains("Booting Zephyr OS"),
        "missing Zephyr boot banner in FVP UART output:\n{output}"
    );
    assert!(
        output.contains("nros: smoke ok"),
        "missing fixture `nros: smoke ok` line in FVP UART output:\n{output}"
    );

    drop(proc);
}
