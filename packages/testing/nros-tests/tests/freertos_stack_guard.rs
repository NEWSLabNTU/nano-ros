//! Issue 2507 — a FreeRTOS task stack overflow on MPS2-AN385 is caught by the
//! board's MPU stack guard and named by task.
//!
//! Before the guard, a task that ran off its stack wrote over the heap_4 block
//! below it and the image reported `*** MALLOC FAILED ***`, or hung with no
//! message. `bins/stack-guard-freertos-mps2` runs a correctly sized app task
//! (whose stack high-water mark is read under the guard), then starts a task
//! with a 2 KiB stack that recurses without bound. This reads its output:
//!
//! - the app task's high-water mark is reported, so reading the guard's bytes
//!   does not fault (the kernel's own pattern check reads them on every
//!   switch, so getting this far also shows that check still runs);
//! - the overflow is reported as a stack overflow of the task `overflow`;
//! - nothing reports `MALLOC FAILED`, and the image never woke up to say the
//!   overflow went uncaught.
//!
//! Fixture must be prebuilt (`just build freertos`).

use nros_tests::{
    fixtures::{QemuProcess, RequireFixture, build_stack_guard_freertos_mps2, is_qemu_available},
    matrix::PlatformId,
    output::{
        FREERTOS_MALLOC_FAILED, STACK_GUARD_CAUGHT, STACK_GUARD_FAIL, STACK_GUARD_HIGH_WATER,
        STACK_GUARD_OVERFLOWING,
    },
};
use std::time::Duration;

#[test]
fn a_task_stack_overflow_hits_the_guard_and_names_the_task_on_freertos_mps2() {
    nros_tests::fixtures::lane::require_platform_in_lane(
        &[PlatformId::FreertosMps2],
        "the FreeRTOS stack guard image",
    );
    if !is_qemu_available() {
        nros_tests::unmet!("qemu-system-arm not available");
    }

    let binary = build_stack_guard_freertos_mps2()
        .require("stack-guard-freertos-mps2 — run `just build freertos`");
    let mut qemu = QemuProcess::start_mps2_an385_networked(binary)
        .expect("start QEMU for the FreeRTOS stack guard image");
    let output = qemu
        .wait_for_output(Duration::from_secs(60))
        .expect("the FreeRTOS stack guard image produced no output in time");

    let high_water = output
        .lines()
        .find_map(|l| l.trim().strip_prefix(STACK_GUARD_HIGH_WATER))
        .unwrap_or_else(|| {
            panic!("the image never reported the app task's high-water mark:\n{output}")
        });
    let bytes: u32 = high_water
        .trim_end_matches(" bytes")
        .parse()
        .unwrap_or_else(|e| {
            panic!("high-water `{high_water}` is not a byte count ({e}):\n{output}")
        });
    assert!(bytes > 0, "the app task's high-water mark is 0:\n{output}");

    assert!(
        output.contains(STACK_GUARD_OVERFLOWING),
        "the image never reached the overflow:\n{output}"
    );
    assert!(
        output.contains(STACK_GUARD_CAUGHT),
        "the overflow was not reported as the task `overflow` hitting its guard:\n{output}"
    );
    assert!(
        !output.contains(FREERTOS_MALLOC_FAILED),
        "the overflow was reported as a heap failure:\n{output}"
    );
    assert!(
        !output.contains(STACK_GUARD_FAIL),
        "the image reported a failure:\n{output}"
    );
}
