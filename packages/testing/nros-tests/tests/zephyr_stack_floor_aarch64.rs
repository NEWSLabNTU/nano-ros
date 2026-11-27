//! Issue 2897 — the stack-floor walk on a 64-bit Arm Zephyr image.
//!
//! An aarch64 image reaches frameless code by direct calls: the toolchain's
//! libgcc outline atomics and the arm64 arch assembly (`__start`,
//! `z_arm64_context_switch`, ...). The walk used to refuse every root of such
//! an image for want of their frames. This reads the build's `report.json`
//! for the SMP realtime entry with SNTP re-sync (fixture
//! `workspace-zephyr-c-realtime-smp-sntp-resync`) and asserts that every role
//! the image has got a floor: the main thread, the sched task, and the SNTP
//! re-sync thread (issue 2846's 64-bit half). The floors are upper bounds
//! (RFC-0143 narrows them); what this pins is that the walk completes.

use nros_tests::{
    fixtures::{RequireFixture, build_zephyr_workspace_c_realtime_smp_sntp_resync_report},
    matrix::PlatformId,
};

#[test]
fn every_role_of_an_aarch64_zephyr_image_gets_a_floor() {
    // The SMP row's coordinate is the Zephyr C platform (the a53 board is a
    // row field, not a platform of its own), as for the SMP sched-dims cell.
    nros_tests::fixtures::lane::require_platform_in_lane(
        &[PlatformId::ZephyrNativeSim],
        "the aarch64 Zephyr stack-floor walk",
    );
    let path = build_zephyr_workspace_c_realtime_smp_sntp_resync_report().require(
        "zephyr realtime-c SMP image with SNTP re-sync (build-ws-c-realtime-entry-smp-sntp-resync)",
    );
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{} (the build writes it): {e}", path.display()));
    let report: serde_json::Value = serde_json::from_str(&text).expect("report.json parses");
    assert_eq!(
        report["target"],
        "aarch64-unknown-none",
        "{}",
        path.display()
    );
    let refused = report["refused"].as_array().expect("`refused` is a list");
    assert!(refused.is_empty(), "no role refused: {refused:?}");
    let roles = report["roles"].as_object().expect("`roles` is an object");
    for role in ["OPEN", "TASK_HIGH", "SNTP_RESYNC"] {
        let r = roles
            .get(role)
            .unwrap_or_else(|| panic!("role {role} missing: {:?}", roles.keys()));
        let floor = r["floor"].as_u64().unwrap_or(0);
        assert!(floor > 0, "{role} has a floor: {r}");
        assert!(
            r["unmeasured_reason"].is_null(),
            "{role} was walked to the end: {r}"
        );
    }
}
