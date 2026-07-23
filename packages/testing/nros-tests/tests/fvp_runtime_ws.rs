//! phase-298 W3.2 (issue 0232) — FVP runtime gate over the ws-realtime-cpp-fvp
//! workspace Entry, the in-tree mirror of the ASI consumption shape
//! (`nano_ros_use_board(fvp-aemv8r-smp)` + `find_package(nano_ros)` +
//! `nano_ros_add_executable(BOARD zephyr MODEL … TYPED DEPLOY zephyr)` driving
//! `ZephyrBoard::run_tiers` — two tiers over one shared Cyclone DDS session).
//!
//! This is the maintainer pre-release gate (`just zephyr verify-fvp-runtime`
//! builds + runs it): the ARM FVP is license-walled, so the test `skip!`s
//! cleanly on hosts without the model and is run by a maintainer before an
//! ASI pin bump or release. It replaces the retired false-green legacy talker
//! runtime tests (phase-298 W4) — those targeted build-only images with no
//! ethernet device, so they could never publish and always skipped.
//!
//! Assertion: SELF-CONTAINED publish. On the FVP alone (SLIRP userNetworking,
//! no host peer, no tap0/root), both tier components must reach their publish
//! loops. `Ctrl.cpp` / `Telem.cpp` print `[ctrl] tick=N` / `[telem] tick=N`
//! ONLY on `publish().ok()`, so the two markers prove participant creation +
//! descriptor registration + typed publish on BOTH tiers — the exact chain
//! the phase-292 walls (#4/#5/#8/#9) broke invisibly.
//!
//! Skip preconditions (same ladder as the legacy fvp_runtime tests):
//!   1. ARM FVP not resolvable via `scripts/zephyr/resolve-fvp-bin.sh`.
//!   2. `west` not on PATH.
//!   3. Zephyr workspace not set up.
//!   4. `build-fvp-ws-entry/zephyr/zephyr.elf` missing (build recipe hint).

use std::{
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};

use nros_tests::{process::ManagedProcess, project_root, skip};

/// Publish-success markers — printed by the tier components ONLY when
/// `publish().ok()` (Ctrl.cpp:19 / Telem.cpp:18). NOT banner strings; do not
/// swap for `Publishing:`-style prose (issue 0157/0164 class).
const CTRL_MARKER: &str = "[ctrl] tick=";
const TELEM_MARKER: &str = "[telem] tick=";

fn have(cmd: &str) -> bool {
    Command::new("which")
        .arg(cmd)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn resolve_fvp_dir(root: &Path) -> Option<String> {
    let script = root.join("scripts/zephyr/resolve-fvp-bin.sh");
    let out = Command::new("bash").arg(&script).output().ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (!s.is_empty()).then_some(s)
}

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
fn fvp_ws_entry_two_tier_publishes() {
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

    // 4. ws-entry ELF prebuilt?
    let elf = workspace
        .join("build-fvp-ws-entry")
        .join("zephyr")
        .join("zephyr.elf");
    if !elf.is_file() {
        skip!(
            "FVP ws-entry ELF missing at {}; run `just zephyr build-fvp-ws-entry` first",
            elf.display()
        );
    }

    // Drive the canonical recipe — it owns the env wiring (workspace cd,
    // pinned make/ninja, ZEPHYR_SDK_INSTALL_DIR, NROS_REPO_DIR for the
    // possible reconfigure, ARMFVP_EXTRA_FLAGS fast-sim) and delegates to
    // `west fvp run -d build-fvp-ws-entry`.
    let mut cmd = Command::new("just");
    cmd.current_dir(&root).args(["zephyr", "run-fvp-ws-entry"]);

    let mut proc = ManagedProcess::spawn_command(cmd, "fvp-ws-entry")
        .expect("spawn just zephyr run-fvp-ws-entry");

    // Budget 180 s to the first `[telem] tick=` — the SLOWER marker (100 ms
    // tier), so by the time it prints the 10 ms ctrl tier has ticked ~10x.
    // Covers a `west build -t run` reconfigure (~60 s warm) + FVP boot +
    // Cyclone participant/thread setup under fast-sim (publishes start
    // <1 s sim-time after boot). Drop kills the FVP process group on
    // timeout/panic/nextest SIGKILL.
    let timeout = Duration::from_secs(180);
    let output = match proc.wait_for_output_pattern(TELEM_MARKER, timeout) {
        Ok(o) => o,
        Err(e) => {
            panic!("FVP ws-entry did not print `{TELEM_MARKER}` within {timeout:?}: {e}");
        }
    };

    // Both tiers must have published: telem gated the wait; ctrl runs 10x
    // faster, so its marker must already be in the accumulated output.
    assert!(
        output.contains(CTRL_MARKER),
        "telem published but ctrl marker `{CTRL_MARKER}` missing — high tier \
         not publishing (run_tiers/priority regression?):\n{output}"
    );

    drop(proc);
}
