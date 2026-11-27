//! phase-578 W2 (RFC-0118 D9) — a fault on FreeRTOS + MPS2-AN385 is named by
//! function and source line.
//!
//! `bins/fault-report-freertos-mps2` executes an undefined instruction in its
//! `inject_fault` function. The board's HardFault handler prints the
//! `nros-fault:` line with the stacked `pc`, and `nros run` (which every QEMU
//! image here boots through, with the image's ELF in its plan) looks `pc` up
//! in the ELF and prints the function and `file:line` under it. This reads
//! that output and checks the line named is the `udf` line in the image's
//! source.
//!
//! Fixture must be prebuilt (`just build freertos`).

use nros_tests::{
    fixtures::{QemuProcess, RequireFixture, build_fault_report_freertos_mps2, is_qemu_available},
    matrix::PlatformId,
    output::{
        FAULT_REPORT_FAIL, FAULT_REPORT_INJECTING, NROS_FAULT_HARDFAULT, NROS_FAULT_PC_LOOKUP,
    },
};
use std::time::Duration;

/// The image's source, and the 1-based line holding its `udf`.
fn faulting_line() -> u32 {
    let src = nros_tests::project_root().join(
        "packages/testing/nros-tests/bins/fault-report-freertos-mps2/src/\
         fault-report-freertos-mps2/src/main.rs",
    );
    let text =
        std::fs::read_to_string(&src).unwrap_or_else(|e| panic!("reading {}: {e}", src.display()));
    let at = text
        .lines()
        .position(|l| l.contains("asm!(\"udf #0\")"))
        .unwrap_or_else(|| panic!("{} has no `udf` line", src.display()));
    u32::try_from(at + 1).expect("a source line number fits u32")
}

#[test]
fn a_fault_on_freertos_mps2_is_named_by_function_and_line() {
    nros_tests::fixtures::lane::require_platform_in_lane(
        &[PlatformId::FreertosMps2],
        "the FreeRTOS fault report image",
    );
    if !is_qemu_available() {
        nros_tests::unmet!("qemu-system-arm not available");
    }

    let binary = build_fault_report_freertos_mps2()
        .require("fault-report-freertos-mps2 — run `just build freertos`");
    let mut qemu = QemuProcess::start_mps2_an385_networked(binary)
        .expect("start QEMU for the FreeRTOS fault report image");
    let output = qemu
        .wait_for_output(Duration::from_secs(60))
        .expect("the FreeRTOS fault report image produced no output in time");

    assert!(
        output.contains(FAULT_REPORT_INJECTING),
        "the image never reached the fault:\n{output}"
    );
    assert!(
        !output.contains(FAULT_REPORT_FAIL),
        "the fault was not taken:\n{output}"
    );
    let fault = output
        .lines()
        .find(|l| l.contains(NROS_FAULT_HARDFAULT))
        .unwrap_or_else(|| panic!("the board printed no `nros-fault:` line:\n{output}"));
    assert!(
        fault.contains(" pc=0x") && fault.contains(" lr=0x"),
        "the fault line has no stacked pc and lr: {fault}"
    );
    let lookup = output
        .lines()
        .find(|l| l.starts_with(NROS_FAULT_PC_LOOKUP))
        .unwrap_or_else(|| panic!("the CLI looked no `pc` up:\n{output}"));
    let line = faulting_line();
    assert!(
        lookup.contains(": inject_fault at ") && lookup.ends_with(&format!("main.rs:{line}")),
        "the CLI did not name `inject_fault` at main.rs:{line}: {lookup}\n{output}"
    );
}
