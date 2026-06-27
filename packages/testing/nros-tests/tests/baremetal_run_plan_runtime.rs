//! Phase 244.D1 — `baremetal_board_run_executes_run_plan` runtime gate.
//!
//! Bare-metal (no-RTOS) Cortex-M analog of the FreeRTOS sibling
//! `freertos_run_plan_runtime::freertos_board_run_executes_run_plan` and the
//! posix `entry_poc_boots_through_board_entry_run`. Proves the Phase 244.D1
//! enabler: an Entry pkg with `[package.metadata.nros.entry] deploy =
//! "qemu-mps2-an385"` + a bare `nros::main!()` boots under QEMU MPS2-AN385
//! through the macro-emitted `#[cortex_m_rt::entry]` reset → the board's
//! `nros_platform::BoardEntry::run_with_deploy` → `init_hardware` → network →
//! `Executor::open`.
//!
//! ## Fixture
//!
//! `examples/qemu-arm-baremetal/rust/qemu-baremetal-main-e2e/` — a single
//! self-contained crate (issue 0100): `main.rs` is the 5-line `nros::main!()`
//! Entry (`#![no_std]/#![no_main]/panic` + the macro), `lib.rs` is the node
//! (`nros::node!(E2eNode)`); the macro owns the whole boot scaffold.
//!
//! ## Proof (no peer required — mirrors the posix/freertos two-arm gate)
//!
//! The board prints `init_hardware`'s `nros QEMU Platform` banner BEFORE opening
//! the executor, then reaches `Executor::open`. With no zenohd/DDS peer in this
//! bare QEMU run, open returns `Transport(ConnectionFailed)` and the board's
//! `Executor::open failed:` banner fires — which IS the boot-reached proof (the
//! reset → `BoardEntry::run` → init → open chain all executed). The alternate
//! arm (`Application setup complete`) appears only when a peer is present.
//!
//! Per issue 0041 ("No compilation inside tests") the fixture is built in the
//! build stage by `just qemu-baremetal build-fixtures` (auto-discovered by the
//! `examples/qemu-arm-baremetal` Cargo.toml walk); this test only locates the
//! prebuilt ELF and runs it. Absent fixture / QEMU / target → skip.

use std::{process::Command, time::Duration};

use nros_tests::fixtures::{QemuProcess, is_qemu_available, qemu_baremetal_main_e2e_binary};

/// `thumbv7m-none-eabi` Rust target installed?
fn thumbv7m_target_installed() -> bool {
    let out = match Command::new("rustup")
        .args(["target", "list", "--installed"])
        .output()
    {
        Ok(o) => o,
        Err(_) => return false,
    };
    out.status.success()
        && String::from_utf8_lossy(&out.stdout)
            .lines()
            .any(|l| l.trim() == "thumbv7m-none-eabi")
}

#[test]
fn baremetal_board_run_executes_run_plan() {
    if !thumbv7m_target_installed() {
        nros_tests::skip!("thumbv7m-none-eabi target not installed");
    }
    if !is_qemu_available() {
        nros_tests::skip!("qemu-system-arm not found");
    }
    let bin = match qemu_baremetal_main_e2e_binary() {
        Ok(b) => b,
        Err(_) => nros_tests::skip!(
            "qemu-baremetal-main-e2e fixture not prebuilt — run `just qemu-baremetal build-fixtures`"
        ),
    };

    let mut qemu =
        QemuProcess::start_mps2_an385_networked(&bin).expect("failed to start QEMU MPS2-AN385");

    // Boot reaches `Executor::open` (its failure banner without a peer is the
    // lifecycle proof). The accumulated output carries the earlier boot banner.
    let output = qemu
        .wait_for_output_pattern("Executor::open", Duration::from_secs(25))
        .unwrap_or_default();
    qemu.kill();

    println!("=== qemu-baremetal-main-e2e boot output ===\n{output}");

    assert!(
        output.contains("nros QEMU Platform"),
        "boot must reach the board `init_hardware` banner (reset → BoardEntry::run). Output:\n{output}"
    );
    assert!(
        output.contains("Executor::open"),
        "boot must reach `Executor::open` (full BoardEntry boot scaffold ran). Output:\n{output}"
    );
}
