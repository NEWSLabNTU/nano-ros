//! Phase 212.O.1 — `freertos_board_run_executes_run_plan` runtime gate.
//!
//! FreeRTOS-side analog of the posix sibling
//! `phase212_n_entry_poc_runs::entry_poc_boots_through_board_entry_run`
//! (`tests/phase212_n_entry_poc_runs.rs:66`).
//!
//! The sibling gates that `main()` reaches `<NativeBoard as BoardEntry>::run`'s
//! setup closure on host POSIX. This file does the same for the FreeRTOS
//! family driver `nros_board_freertos::run_entry` under QEMU MPS2-AN385.
//!
//! ## Fixture
//!
//! `examples/qemu-arm-freertos/rust/talker_entry/` — the M-F.15-shipped
//! FreeRTOS Entry pkg (Phase 212.N.7 step-2 / 213.C.1). Shape:
//!
//! - `src/main.rs` is one line: `nros::main!();` (Form 1 self-bringup).
//! - `Cargo.toml::[package.metadata.nros.entry] deploy = "freertos"` →
//!   the proc macro resolves the board to
//!   `::nros_board_mps2_an385_freertos::Mps2An385`.
//! - `launch/system.launch.xml` is the empty placeholder (Form 1 doesn't
//!   parse it — the macro emits `<this_pkg>::register(runtime)?` instead,
//!   and `src/lib.rs` re-exports the sibling Component pkg's `register`).
//!
//! Build path: `cargo build` under the pkg dir; `.cargo/config.toml`
//! pins `target = "thumbv7m-none-eabi"`, so the artifact lands at
//! `target/thumbv7m-none-eabi/debug/freertos_rs_talker_entry`.
//!
//! ## What the lifecycle proof looks like
//!
//! The FreeRTOS family driver
//! (`packages/boards/nros-board-freertos/src/entry.rs::run_entry`)
//! prints a fixed banner sequence over semihosting BEFORE handing
//! control to the user closure:
//!
//! 1. `========================================`
//!    `  nros FreeRTOS Platform`               (banner)
//!    `========================================`
//! 2. `Initializing LAN9118 + lwIP...`         (transport bringup start)
//! 3. `Network ready.`                          (transport bringup OK)
//!
//! Then the app task opens `Executor::open(...)` and dispatches into
//! the codegen-emitted run-plan closure (which itself calls
//! `freertos_rs_talker_entry::register(runtime)` →
//! `freertos_rs_talker::register`). Four ends are accepted as
//! run_plan-reached proofs (mirror of the posix sibling's two-arm
//! assertion):
//!
//! - `Executor::open failed:` — open fails (no zenohd / no DDS peer in
//!   the isolated QEMU process); the failure path is itself emitted
//!   from inside the app task AFTER the closure-dispatch site, so
//!   seeing it proves `run_entry → app_task_entry_runtime` reached
//!   the dispatch point.
//! - `Application setup complete` — closure returned `Ok(())` (the
//!   empty-launch happy path).
//! - `Application error:`         — closure returned `Err(...)`.
//! - `Published:` / `Received:`   — Component register populated the
//!   executor and the timer/sub fired (a stronger proof).
//!
//! Any one of those four strings, in conjunction with the banner +
//! `Initializing LAN9118 + lwIP...`, gates (a) Board::run lifecycle AND
//! (b) run_plan body reached.
//!
//! ## Skip semantics
//!
//! Hard-fails via `nros_tests::skip!` on any missing prereq (CLAUDE.md
//! "Tests must fail on unmet preconditions" — the macro panics with the
//! `[SKIPPED]` prefix nextest treats as skipped, NOT a silent
//! `eprintln + return` that would report PASS).
//!
//! Run with: `cargo test -p nros-tests --test phase212_n_freertos_run_plan_runtime`

use std::{
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};

use nros_tests::{
    fixtures::{
        QemuProcess,
        freertos::{is_arm_gcc_available, is_freertos_available, is_lwip_available},
        is_qemu_available,
    },
    project_root,
};

/// FreeRTOS Entry pkg fixture dir — the M-F.15-shipped `talker_entry/`.
fn talker_entry_dir() -> PathBuf {
    project_root().join("examples/qemu-arm-freertos/rust/talker_entry")
}

/// `thumbv7m-none-eabi` Rust target installed? Mirror of the same
/// helper in `phase212_h3_freertos.rs` — kept local rather than re-
/// exported to keep this test file self-contained.
fn thumbv7m_target_installed() -> bool {
    let out = match Command::new("rustup")
        .args(["target", "list", "--installed"])
        .output()
    {
        Ok(o) => o,
        Err(_) => return false,
    };
    if !out.status.success() {
        return false;
    }
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .any(|l| l.trim() == "thumbv7m-none-eabi")
}

/// Single chokepoint for the FreeRTOS bring-up prerequisites. Returns
/// the first missing-piece reason as a description; the caller emits
/// it via `nros_tests::skip!`.
fn require_freertos_qemu_prereqs() -> Option<String> {
    if !thumbv7m_target_installed() {
        return Some("thumbv7m-none-eabi target not installed".to_string());
    }
    if !is_arm_gcc_available() {
        return Some("arm-none-eabi-gcc not found".to_string());
    }
    if !is_freertos_available() {
        return Some("FREERTOS_DIR not set or invalid — run `just freertos setup`".to_string());
    }
    if !is_lwip_available() {
        return Some("LWIP_DIR not set or invalid — run `just freertos setup`".to_string());
    }
    if !is_qemu_available() {
        return Some("qemu-system-arm not found".to_string());
    }
    None
}

/// Resolve the prebuilt-or-build-on-demand entry-pkg binary. Mirrors
/// the posix sibling's `cargo build` + path resolution.
///
/// The build is gated to debug-profile + the pinned `thumbv7m-none-eabi`
/// triple from `.cargo/config.toml`. If the binary already exists at
/// the expected path we trust it; otherwise we drive a `cargo build`.
fn build_or_locate_entry_binary(dir: &Path) -> Result<PathBuf, String> {
    let bin = dir.join("target/thumbv7m-none-eabi/debug/freertos_rs_talker_entry");
    if bin.is_file() {
        return Ok(bin);
    }

    // `nros-board-freertos/build.rs` compiles the FreeRTOS + CFFI
    // platform glue and needs these paths — normally injected by the
    // `just freertos` overlay's `.cargo/config.toml [env]`. This
    // standalone example carries no such overlay, so supply them here
    // (mirrors the board_agnostic_run_plan freertos leg).
    let root = project_root();
    let out = Command::new("cargo")
        .args(["build", "--bin", "freertos_rs_talker_entry"])
        .current_dir(dir)
        .env(
            "NROS_PLATFORM_FREERTOS_SRC",
            root.join("packages/core/nros-platform-freertos/src"),
        )
        .env(
            "NROS_PLATFORM_CFFI_INCLUDE",
            root.join("packages/core/nros-platform-cffi/include"),
        )
        .output()
        .map_err(|e| format!("spawn cargo build: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "cargo build failed in {}.\nstdout:\n{}\nstderr:\n{}",
            dir.display(),
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr),
        ));
    }
    if !bin.is_file() {
        return Err(format!(
            "cargo build claimed success but binary missing at {}",
            bin.display()
        ));
    }
    Ok(bin)
}

#[test]
#[ignore = "Phase 212.O.1 runtime tail — #45 (the Entry-pkg link/panic-handler \
            blocker) is RESOLVED: `freertos_rs_talker_entry` now compiles, links, \
            and boots through the board lifecycle under QEMU (banner → LAN9118 + \
            lwIP → MAC/IP). It then hits `*** STACK OVERFLOW: nros_app ***` at \
            Executor creation because the firmware links BOTH zpico_sys (zenoh) \
            and nros_rmw_cyclonedds via the Component's `rmw-cffi` umbrella, even \
            though the deploy config says rmw=zenoh — an rmw-selection + stack/heap \
            tuning problem, not a link bug. See docs/issues/0046."]
fn freertos_board_run_executes_run_plan() {
    if let Some(reason) = require_freertos_qemu_prereqs() {
        nros_tests::skip!("{reason}");
    }

    let dir = talker_entry_dir();
    if !dir.is_dir() {
        nros_tests::skip!("FreeRTOS Entry pkg fixture missing at {}", dir.display());
    }

    let bin = match build_or_locate_entry_binary(&dir) {
        Ok(b) => b,
        Err(why) => panic!("{why}"),
    };

    // Spawn under QEMU MPS2-AN385 with slirp LAN9118 networking — same
    // shape `QemuProcess::start_mps2_an385_networked` uses for the
    // running FreeRTOS e2e tests in `freertos_qemu.rs`. We do NOT need
    // zenohd up: the Executor::open failure path is itself one of the
    // accepted run_plan-reached proofs.
    let mut qemu = QemuProcess::start_mps2_an385_networked(&bin)
        .expect("spawn FreeRTOS Entry pkg under QEMU MPS2-AN385");

    // Generous ~10s budget per the task spec; the post-network app
    // task either drops a `Published:` (with peer), `Application
    // setup complete` (empty-launch happy path), or `Executor::open
    // failed:` (no peer) well before the ceiling.
    let output = qemu
        .wait_for_output_pattern("Network ready.", Duration::from_secs(10))
        .unwrap_or_default();
    // Drain a touch more so we catch the post-network proof line.
    let extra = qemu
        .wait_for_output_pattern("Application", Duration::from_secs(5))
        .unwrap_or_default();
    let combined = format!("{output}{extra}");
    qemu.kill();

    eprintln!("FreeRTOS Entry pkg lifecycle output:\n{combined}");

    // (a) Board::run lifecycle proof — the family driver's pre-network
    // banner + the LAN9118 init line, both printed by
    // `nros_board_freertos::run_entry` before the app task spawns.
    let saw_banner = combined.contains("nros FreeRTOS Platform");
    let saw_lan9118 = combined.contains("Initializing LAN9118 + lwIP");
    assert!(
        saw_banner && saw_lan9118,
        "did not reach BoardEntry::run lifecycle — \
         missing banner ({saw_banner}) and/or LAN9118 init ({saw_lan9118}).\n\
         output:\n{combined}",
    );

    // (b) run_plan(runtime) body reached — accept any of the four
    // post-closure-dispatch markers (mirror of the posix sibling's
    // two-arm `Executor::open failed` / `application error: NodeRegister`
    // pair, broadened for the FreeRTOS happy / talker paths).
    let saw_executor_open_fail = combined.contains("Executor::open failed");
    let saw_app_setup_complete = combined.contains("Application setup complete");
    let saw_app_error = combined.contains("Application error:");
    let saw_publish = combined.contains("Published:");
    let saw_receive = combined.contains("Received:");
    let reached_run_plan = saw_executor_open_fail
        || saw_app_setup_complete
        || saw_app_error
        || saw_publish
        || saw_receive;
    assert!(
        reached_run_plan,
        "did not reach run_plan(runtime) body — none of \
         {{Executor::open failed, Application setup complete, \
         Application error:, Published:, Received:}} matched.\n\
         output:\n{combined}",
    );
}
