//! FVP runtime lane for the Rust cyclonedds talker on
//! `fvp_baser_aemv8r/fvp_aemv8r_aarch64/smp` (Phase 217.D.3; deepened for
//! issue #232).
//!
//! Sibling of `fvp_runtime.rs` (cpp/cyclonedds). Drives the recipe
//! `just zephyr run-fvp-aemv8r-cyclonedds-rust` and asserts the talker
//! reaches its `Publishing:` line — i.e. cyclone actually created a
//! participant + writer and delivered a sample on the FVP, not merely that
//! the ELF booted. The recipe owns env wiring (workspace `cd`, pinned
//! `make`/`ninja` on PATH, `ZEPHYR_SDK_INSTALL_DIR`, `west fvp run`
//! resolver); the `build-fvp-aemv8r-cyclonedds-rust` recipe bakes
//! `cache_state_modelled=0` so the busy DDS path runs fast-functional.
//!
//! Issue #232 — the pre-existing assertion was boot-banner-only, which
//! passes even when cyclone dies at participant creation: walls
//! #4/#5/#8/#9 (snippet conf, loopback getifaddrs, missing descriptor
//! codegen, mutex-pool exhaustion) all shipped invisible behind it and were
//! found only when the ASI consumer RAN on the model (phase-292 W2). Phase-
//! 292 W2 also proved the SMP `system_main.c` shim boots + runs cyclone on
//! the real FVP, unblocking this bump (and Phase 217.C.3 Rust↔C parity).
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
fn fvp_rust_cyclonedds_talker_publishes() {
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

    // FVP cold-boots in ~30 s; cyclone discovery + first publish is the
    // busy path — but the FVP build bakes `cache_state_modelled=0`
    // (issue #232, `build-fvp-*` recipes) so it runs fast-functional, not
    // the ~1000x-slower cache-modelled default. 120 s budget to the first
    // `Publishing:` line covers cold start + host load. ManagedProcess::Drop
    // kills the FVP group on hit / timeout / panic / SIGKILL.
    //
    // Issue #232 — assert the DEEP signal (cyclone actually published a
    // sample), not just the boot banner. The boot banner passes even when
    // cyclone dies at participant creation (walls #4/#5/#8/#9 shipped
    // invisible behind a boot-banner-only gate); `Publishing:` only appears
    // after `dds_create_participant` + writer create + a live send. Parity
    // with the C/cpp lane (`fvp_runtime.rs`); unblocked by phase-292 W2,
    // which proved the SMP `system_main.c` shim boots + runs cyclone on the
    // real FVP.
    let timeout = Duration::from_secs(120);
    let output = match proc.wait_for_output_pattern(nros_tests::output::TALKER_LOG_PREFIX, timeout)
    {
        Ok(o) => o,
        Err(e) => panic!(
            "FVP Rust talker did not reach `Publishing:` within {:?}: {}\n\
             (boot may have reached the banner but cyclone failed at \
             participant/writer create — the exact class this lane guards)",
            timeout, e
        ),
    };

    // Sanity: the boot banner must precede the publish (a `Publishing:` with
    // no banner would mean stale/leaked output, not a real run).
    assert!(
        output.contains("Booting Zephyr OS"),
        "missing Zephyr boot banner in FVP UART output:\n{output}"
    );
    assert!(
        output.contains(nros_tests::output::TALKER_LOG_PREFIX),
        "missing talker `Publishing:` line — cyclone did not deliver on the FVP:\n{output}"
    );

    drop(proc);
}
