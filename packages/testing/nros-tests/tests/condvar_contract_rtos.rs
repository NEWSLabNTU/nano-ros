//! Issue 1906 — the platform condvar contract, checked at RUNTIME on ThreadX
//! and FreeRTOS.
//!
//! The POSIX port is checked on the host by `nros-platform-cffi`'s
//! `c_port_posix.rs`. ThreadX and FreeRTOS have no host harness, so each runs
//! `nros-condvar-contract` inside an image of its own
//! (`bins/condvar-contract-*`) and prints one line per check. This reads those
//! lines:
//!
//! - a timed wait nobody signals returns `1`, never before its deadline;
//! - a broadcast wakes all three waiters (ThreadX woke one);
//! - ThreadX only: a wait under a mutex held twice is refused (issue 1907).
//!
//! Fixtures must be prebuilt (`just build threadx-linux`, `just build freertos`).

use nros_tests::{
    fixtures::{
        ManagedProcess, QemuProcess, RequireFixture, build_condvar_contract_freertos_mps2,
        build_condvar_contract_threadx_linux, is_qemu_available, threadx_linux,
    },
    matrix::PlatformId,
    output::{CONDVAR_CONTRACT_ALL_PASSED, CONDVAR_CONTRACT_FAIL, CONDVAR_CONTRACT_PASS},
};
use std::time::Duration;

/// Asserts every named check printed its PASS line, none printed a FAIL line,
/// and the image reported that all checks passed.
fn assert_contract(output: &str, checks: &[&str]) {
    assert!(
        !output.contains(CONDVAR_CONTRACT_FAIL),
        "a condvar check failed:\n{output}"
    );
    for check in checks {
        let line = format!("{CONDVAR_CONTRACT_PASS}{check}:");
        assert!(
            output.contains(&line),
            "the image never reported `{line}`:\n{output}"
        );
    }
    assert!(
        output.contains(CONDVAR_CONTRACT_ALL_PASSED),
        "the image did not report that every check passed:\n{output}"
    );
}

#[test]
fn condvar_contract_holds_on_threadx_linux() {
    nros_tests::fixtures::lane::require_platform_in_lane(
        &[PlatformId::ThreadxLinux],
        "the ThreadX Linux condvar contract image",
    );
    if !threadx_linux::is_threadx_available() {
        nros_tests::unmet!("no ThreadX kernel tree provisioned");
    }

    let binary = build_condvar_contract_threadx_linux()
        .require("condvar-contract-threadx-linux — run `just build threadx-linux`");
    let mut proc = ManagedProcess::spawn(binary, &[], "condvar-contract-threadx-linux")
        .expect("spawn the ThreadX Linux condvar contract image");
    let output = proc
        .wait_for_all_output(Duration::from_secs(30))
        .expect("the ThreadX Linux condvar contract image produced no output in time");

    assert_contract(&output, &["timed-wait", "broadcast", "held-twice"]);
}

#[test]
fn condvar_contract_holds_on_freertos_mps2() {
    nros_tests::fixtures::lane::require_platform_in_lane(
        &[PlatformId::FreertosMps2],
        "the FreeRTOS condvar contract image",
    );
    if !is_qemu_available() {
        nros_tests::unmet!("qemu-system-arm not available");
    }

    let binary = build_condvar_contract_freertos_mps2()
        .require("condvar-contract-freertos-mps2 — run `just build freertos`");
    let mut qemu = QemuProcess::start_mps2_an385_networked(binary)
        .expect("start QEMU for the FreeRTOS condvar contract image");
    let output = qemu
        .wait_for_output(Duration::from_secs(60))
        .expect("the FreeRTOS condvar contract image produced no output in time");

    assert_contract(&output, &["timed-wait", "broadcast"]);
}
