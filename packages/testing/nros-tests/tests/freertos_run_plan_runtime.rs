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
        QemuProcess, ZenohRouter,
        freertos::{is_arm_gcc_available, is_freertos_available, is_lwip_available},
        is_qemu_available, require_zenohd,
    },
    project_root,
};

/// The `talker_entry` deploy overlay points the firmware at
/// `tcp/10.0.2.2:7451` (the slirp host alias). The host router must listen on
/// this port so `Executor::open` connects instead of hanging.
const ENTRY_ZENOHD_PORT: u16 = 7451;

/// FreeRTOS Entry pkg fixture dir for `<entry>` (e.g. `talker_entry`).
fn entry_dir(entry: &str) -> PathBuf {
    project_root()
        .join("examples/qemu-arm-freertos/rust")
        .join(entry)
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
fn build_or_locate_entry_binary(dir: &Path, bin_name: &str) -> Result<PathBuf, String> {
    // Release, not debug: the connected run opens a real zenoh-pico session over
    // slirp on the emulated Cortex-M3, and a debug-profile zenoh-pico is far too
    // slow to complete the session handshake within the test's wait budget (it
    // boots to `Network ready.` but never reaches `Executor::open` success in
    // 90s). The release fixture connects in a few seconds (#48).
    let bin = dir.join(format!("target/thumbv7m-none-eabi/release/{bin_name}"));
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
        .args(["build", "--release", "--bin", bin_name])
        .current_dir(dir)
        .env(
            "NROS_PLATFORM_FREERTOS_SRC",
            root.join("packages/core/nros-platform-freertos/src"),
        )
        .env(
            // phase-241 B.2 — canonical platform headers moved to nros-platform-api.
            "NROS_PLATFORM_CFFI_INCLUDE",
            root.join("packages/core/nros-platform-api/include"),
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

/// Shared boot+connected-run gate for any qemu-arm-freertos Entry pkg. All six
/// share the `Mps2An385` board, `nros::main!()` self-bringup, the
/// `tcp/10.0.2.2:7451` deploy locator + `10.0.2.15` deploy ip/gateway, and the
/// release-profile build, so the #45 (panic/crate-type/linker) + #46 (stack/heap)
/// + #48 (deploy-thread + zenoh-backend-link) fixes that unblocked the talker
/// unblock the siblings; these prove it per pkg. Serialized via the
/// `qemu-freertos-entry` nextest group (shared port 7451 + QEMU slirp).
fn boot_and_connect(entry: &str, bin_name: &str) {
    if let Some(reason) = require_freertos_qemu_prereqs() {
        nros_tests::skip!("{reason}");
    }
    if !require_zenohd() {
        nros_tests::skip!("zenohd not found");
    }

    let dir = entry_dir(entry);
    if !dir.is_dir() {
        nros_tests::skip!("FreeRTOS Entry pkg fixture missing at {}", dir.display());
    }
    // The Entry pkg's `[patch.crates-io]` points its msg deps at `generated/`,
    // which `nros ws sync` (run by `just freertos build-examples`) produces. Skip
    // — rather than fail the in-place build — when that build step hasn't run.
    if !dir.join("generated").is_dir() {
        nros_tests::skip!(
            "{} has no `generated/` msg crates — run `just freertos build-examples` first",
            dir.display()
        );
    }

    let bin = match build_or_locate_entry_binary(&dir, bin_name) {
        Ok(b) => b,
        Err(why) => panic!("{why}"),
    };

    // Host router on 0.0.0.0:7451 — the deploy overlay points the firmware at
    // `tcp/10.0.2.2:7451` (slirp host alias), so `Executor::open` connects here
    // instead of hanging on an unreachable locator (#46/#48).
    let _router = ZenohRouter::start_slirp(ENTRY_ZENOHD_PORT)
        .expect("start slirp zenohd router for the FreeRTOS Entry pkg");

    // Spawn under QEMU MPS2-AN385 with slirp LAN9118 networking — same
    // shape `QemuProcess::start_mps2_an385_networked` uses for the
    // running FreeRTOS e2e tests in `freertos_qemu.rs`.
    let mut qemu = QemuProcess::start_mps2_an385_networked(&bin)
        .expect("spawn FreeRTOS Entry pkg under QEMU MPS2-AN385");

    // Boot to network bringup (the deterministic part); #46's memory fix lets
    // the `nros_app` task reach this instead of stack-overflowing at Executor
    // creation.
    let output = qemu
        .wait_for_output_pattern("Network ready.", Duration::from_secs(15))
        .unwrap_or_default();
    // The post-network connected run goes through `Executor::open`. #48 (both
    // causes) is fixed, so it now establishes: the zenoh RMW backend is linked +
    // registered (cause 2) AND the deploy overlay threads the reachable slirp
    // locator/ip (`tcp/10.0.2.2:7451` on guest `10.0.2.15`) into the firmware
    // (cause 1), so the firmware connects to the host zenohd above and the
    // run-plan closure returns `Ok` ("Application setup complete").
    let extra = qemu
        .wait_for_output_pattern("Application", Duration::from_secs(25))
        .unwrap_or_default();
    let combined = format!("{output}{extra}");
    qemu.kill();

    eprintln!("FreeRTOS Entry pkg lifecycle output:\n{combined}");

    // (a) Board::run lifecycle proof — the family driver's pre-network
    // banner + the LAN9118 init line, both printed by
    // `nros_board_freertos::run_entry` before the app task spawns. With #46's
    // stack/heap sizing the app task now boots through this cleanly.
    let saw_banner = combined.contains("nros FreeRTOS Platform");
    let saw_lan9118 = combined.contains("Initializing LAN9118 + lwIP");
    let saw_network = combined.contains("Network ready.");
    assert!(
        saw_banner && saw_lan9118 && saw_network,
        "did not reach BoardEntry::run lifecycle — banner ({saw_banner}), \
         LAN9118 init ({saw_lan9118}), network ready ({saw_network}).\n\
         output:\n{combined}",
    );

    // (b) Connected run — ASSERTED (no longer best-effort) now that #48 is fixed.
    // `Executor::open` connects to the host zenohd and the run-plan closure
    // returns `Ok`, so the firmware must NOT print the open-failure marker and
    // MUST print a run-plan success marker.
    let saw_executor_open_fail = combined.contains("Executor::open failed");
    let saw_app_setup_complete = combined.contains("Application setup complete");
    let saw_publish = combined.contains("Published:");
    let saw_receive = combined.contains("Received:");
    assert!(
        !saw_executor_open_fail,
        "Executor::open failed on the connected run — #48 regression (zenoh RMW \
         backend unlinked/unregistered, or the deploy locator/ip is inert again).\n\
         output:\n{combined}",
    );
    assert!(
        saw_app_setup_complete || saw_publish || saw_receive,
        "connected run reached neither a setup-complete nor a pub/sub marker \
         (expected `Application setup complete` once `Executor::open` connects to \
         the slirp zenohd).\noutput:\n{combined}",
    );
}

// One boot+connected gate per qemu-arm-freertos Entry pkg. Serialized via the
// `qemu-freertos-entry` nextest group (shared port 7451 + QEMU slirp).

#[test]
fn freertos_board_run_executes_run_plan() {
    boot_and_connect("talker_entry", "freertos_rs_talker_entry");
}

#[test]
fn freertos_listener_entry_boots_and_connects() {
    boot_and_connect("listener_entry", "freertos_rs_listener_entry");
}

#[test]
fn freertos_service_server_entry_boots_and_connects() {
    boot_and_connect("service-server_entry", "freertos_rs_service_server_entry");
}

#[test]
fn freertos_service_client_entry_boots_and_connects() {
    boot_and_connect("service-client_entry", "freertos_rs_service_client_entry");
}

#[test]
fn freertos_action_server_entry_boots_and_connects() {
    boot_and_connect("action-server_entry", "freertos_rs_action_server_entry");
}

#[test]
fn freertos_action_client_entry_boots_and_connects() {
    boot_and_connect("action-client_entry", "freertos_rs_action_client_entry");
}
