//! Phase 217.C.1 — FVP runtime smoke for the cpp/cyclonedds talker on
//! `fvp_baser_aemv8r/fvp_aemv8r_aarch64/smp`.
//!
//! Drives the existing 217.A recipe (`just zephyr run-fvp-aemv8r-cyclonedds`)
//! end-to-end and asserts the talker reached its publish loop. The recipe
//! already owns the env wiring (workspace `cd`, pinned `make`/`ninja` on
//! PATH, `ZEPHYR_SDK_INSTALL_DIR`, `west fvp run` resolver). Re-implementing
//! that surface here would drift; the test is a thin invoker + UART scraper.
//!
//! Skip preconditions (each `nros_tests::skip!` — keeps `nros_tests` skip
//! semantics, i.e. `[SKIPPED]` panic that nextest treats as expected):
//!   1. ARM FVP not resolvable via `scripts/zephyr/resolve-fvp-bin.sh`
//!      (gated install, license-walled — `[gated.arm-fvp]` in
//!      `nros-sdk-index.toml`).
//!   2. `west` not on PATH (Zephyr SDK absent).
//!   3. Zephyr workspace not set up (mirrors the recipe's own skip).
//!   4. The cpp/cyclonedds talker ELF missing — hint at the build recipe.
//!
//! The FVP boots slowly (~30 s wall-clock cold). The test budgets 120 s
//! to first `Published:` line; on a hit it kills the FVP process group
//! (Drop closes the loop on Ctrl-C / panic / nextest SIGKILL).

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
/// Returns `Some(dir)` on a hit, `None` if the resolver exited non-zero
/// (no `ARMFVP_BIN_PATH` / `ARM_FVP_DIR` / `PATH` hit).
fn resolve_fvp_dir(root: &Path) -> Option<String> {
    let script = root.join("scripts/zephyr/resolve-fvp-bin.sh");
    let out = Command::new("bash").arg(&script).output().ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (!s.is_empty()).then_some(s)
}

/// Resolve the Zephyr workspace path the same way `just/zephyr.just` does:
///   `$NROS_ZEPHYR_WORKSPACE` → in-tree `zephyr-workspace/` →
///   sibling `../nano-ros-workspace/`.
/// Returns `Some(canonical)` only when the candidate carries a `zephyr/`
/// subdir (matches the recipe's `! -d "$workspace/zephyr"` skip).
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
fn fvp_cpp_cyclonedds_talker_publishes() {
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

    // 4. Talker ELF prebuilt? `build-aemv8r-cyclonedds-talker/zephyr/zephyr.elf`
    // matches `just zephyr build-fvp-aemv8r-cyclonedds` (217.A.3 build dir).
    let elf = workspace
        .join("build-aemv8r-cyclonedds-talker")
        .join("zephyr")
        .join("zephyr.elf");
    if !elf.is_file() {
        skip!(
            "FVP cpp/cyclonedds talker ELF missing at {}; \
             run `just zephyr build-fvp-aemv8r-cyclonedds` first",
            elf.display()
        );
    }

    // Drive the canonical recipe — it owns the env wiring + west fvp run
    // delegation (Phase 215.D.4). The recipe `cd`s into the workspace,
    // pins make/ninja, exports ZEPHYR_SDK_INSTALL_DIR, then execs
    // `west fvp run -d build-aemv8r-cyclonedds-talker`.
    let mut cmd = Command::new("just");
    cmd.current_dir(&root)
        .args(["zephyr", "run-fvp-aemv8r-cyclonedds"]);

    let mut proc = ManagedProcess::spawn_command(cmd, "fvp-cyclonedds-talker")
        .expect("spawn just zephyr run-fvp-aemv8r-cyclonedds");

    // The FVP cold-boots in ~30 s (Cortex-A SMP + Zephyr 3.7 + Cyclone DDS
    // discovery). Budget 120 s to first `Published:` line — generous enough
    // to absorb host load while still bounding a hung run. Drop kills the
    // FVP process group if we exceed the budget or the harness aborts.
    let timeout = Duration::from_secs(120);
    let output = match proc.wait_for_output_pattern(nros_tests::output::TALKER_LOG_PREFIX, timeout)
    {
        Ok(o) => o,
        Err(e) => {
            // ManagedProcess::Drop will kill the FVP; surface the captured
            // stream so the user can see boot progress.
            panic!(
                "FVP talker did not reach `Published:` within {:?}: {}\n--- captured ---\n{}",
                timeout,
                e,
                // best-effort: grab whatever buffered when we timed out
                ""
            );
        }
    };

    // Assert: Zephyr booted AND the talker printed its publish line.
    // The boot banner string is stable across Zephyr 3.7..4.x
    // (`*** Booting Zephyr OS build ...`).
    assert!(
        output.contains("Booting Zephyr OS"),
        "missing Zephyr boot banner in FVP UART output:\n{output}"
    );
    // `examples/zephyr/cpp/cyclonedds/talker-aemv8r/src/main.cpp` logs
    // `Published: <count>` via `LOG_INF` after every successful pub.
    assert!(
        output.contains(nros_tests::output::TALKER_LOG_PREFIX),
        "missing talker `Published:` line in FVP UART output:\n{output}"
    );

    // Explicit drop kills the FVP process group before returning. Drop
    // would catch this anyway, but being explicit makes the lifecycle
    // visible at the call site.
    drop(proc);
}
