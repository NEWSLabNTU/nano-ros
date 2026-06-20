//! Phase 217.D.3 — FVP runtime smoke for the Rust cyclonedds talker on
//! `fvp_baser_aemv8r/fvp_aemv8r_aarch64/smp`.
//!
//! Sibling of `phase217_c_fvp_runtime.rs` (cpp/cyclonedds smoke). Drives
//! the Phase 217.D.2 recipe `just zephyr run-fvp-aemv8r-cyclonedds-rust`
//! and asserts the FVP boots the Rust ELF (Zephyr boot banner reaches
//! UART). The recipe owns env wiring (workspace `cd`, pinned
//! `make`/`ninja` on PATH, `ZEPHYR_SDK_INSTALL_DIR`, `west fvp run`
//! resolver).
//!
//! Scope (intentional, 2026-06-04):
//!   - Asserts the Zephyr 3.7 boot banner reaches UART 0. This proves
//!     the Rust artifact links + executes on Cortex-A SMP under the
//!     FVP — the gating signal for Phase 215 board-crate-import +
//!     Phase 217.A run recipes against the Rust path.
//!   - Does NOT yet assert the talker's `Published:` line. The example
//!     is Component-pkg shape (`src/lib.rs` exports `nros::node!(Talker)`
//!     → `register(runtime)`); the generated `rust_main` driver that
//!     opens the executor + runs the spin loop is emitted by
//!     `nros codegen-system` via the H.1 Zephyr adapter shim. Both
//!     prereqs (Phase 212.L.7 self-bringup planner + Phase 212.M-F.3
//!     Zephyr self-pkg shim case) are LANDED; the example's
//!     `CMakeLists.txt` was wired to call
//!     `nros_system_generate(${CMAKE_CURRENT_SOURCE_DIR})` on
//!     2026-06-04. The remaining gap is a real FVP run on a host with
//!     the Arm FVP installed + the `aarch64-zephyr-elf` toolchain — at
//!     which point the assertion below bumps from boot-banner-only to
//!     `Published:`, unblocking Phase 217.C.3 parity.
//!
//! Skip preconditions (`nros_tests::skip!`):
//!   1. ARM FVP not resolvable via `scripts/zephyr/resolve-fvp-bin.sh`
//!      (gated install, license-walled).
//!   2. `west` not on PATH (Zephyr SDK absent).
//!   3. Zephyr workspace not set up.
//!   4. Rust talker ELF missing at
//!      `<workspace>/build-fvp-aemv8r-cyclonedds-rust-talker/zephyr/zephyr.elf`
//!      (hint at `just zephyr build-fvp-aemv8r-cyclonedds-rust`).

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
fn resolve_fvp_dir(root: &Path) -> Option<String> {
    let script = root.join("scripts/zephyr/resolve-fvp-bin.sh");
    let out = Command::new("bash").arg(&script).output().ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (!s.is_empty()).then_some(s)
}

/// Same workspace resolution rules as `just/zephyr.just`:
/// `$NROS_ZEPHYR_WORKSPACE` → in-tree `zephyr-workspace/` → sibling
/// `../nano-ros-workspace/`. Returns `Some(canonical)` only when the
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
fn fvp_rust_cyclonedds_talker_boots() {
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

    // 4. Rust talker ELF prebuilt? Build dir matches the 217.D.2 recipe.
    let elf = workspace
        .join("build-fvp-aemv8r-cyclonedds-rust-talker")
        .join("zephyr")
        .join("zephyr.elf");
    if !elf.is_file() {
        skip!(
            "FVP Rust cyclonedds talker ELF missing at {}; \
             run `just zephyr build-fvp-aemv8r-cyclonedds-rust` first",
            elf.display()
        );
    }

    let mut cmd = Command::new("just");
    cmd.current_dir(&root)
        .args(["zephyr", "run-fvp-aemv8r-cyclonedds-rust"]);

    let mut proc = ManagedProcess::spawn_command(cmd, "fvp-cyclonedds-rust-talker")
        .expect("spawn just zephyr run-fvp-aemv8r-cyclonedds-rust");

    // FVP cold-boots in ~30 s. 120 s budget to the boot banner is
    // comfortable for cold start + host load. ManagedProcess::Drop kills
    // the FVP group on timeout / panic / SIGKILL.
    let timeout = Duration::from_secs(120);
    let output = match proc.wait_for_output_pattern("Booting Zephyr OS", timeout) {
        Ok(o) => o,
        Err(e) => panic!(
            "FVP Rust talker did not print Zephyr boot banner within {:?}: {}",
            timeout, e
        ),
    };

    assert!(
        output.contains("Booting Zephyr OS"),
        "missing Zephyr boot banner in FVP UART output:\n{output}"
    );

    // TODO: once a real FVP run on a host with the Arm FVP installed
    // confirms the H.1 shim's `system_main.c` boots cleanly on
    // Cortex-A SMP, raise this assertion to also require the
    // `Published:` line (parity with `phase217_c_fvp_runtime.rs`). At
    // that point Phase 217.C.3 (Rust↔C wire-parity check) is unblocked.
    // The upstream prereqs (Phase 212.L.7 + 212.M-F.3) and the
    // `nros_system_generate(${CMAKE_CURRENT_SOURCE_DIR})` wire-up in
    // `examples/zephyr/rust/cyclonedds/talker-aemv8r/CMakeLists.txt`
    // are LANDED (2026-06-04).

    drop(proc);
}
