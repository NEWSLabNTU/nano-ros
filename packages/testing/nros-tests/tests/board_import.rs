//! Phase 215.E.2 — verify `nano_ros_use_board()` layers a board
//! crate's artifacts into a downstream Zephyr app build.
//!
//! Skips if Zephyr / west / FVP SDK env not provisioned.

use nros_tests::{project_root, skip};

fn have(cmd: &str) -> bool {
    std::process::Command::new("which")
        .arg(cmd)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[test]
fn board_import_fvp_builds_via_nano_ros_use_board() {
    // Prereqs: Zephyr SDK, west on PATH, ARM_FVP_DIR for the SDK
    // gate (the gated pkg per board.cmake — even though FVP runtime
    // isn't needed at build time, the toolchain may be).
    if std::env::var("ZEPHYR_BASE").is_err() {
        skip!("ZEPHYR_BASE not set; Phase 215.E.2 needs a provisioned Zephyr SDK");
    }
    if !have("west") {
        skip!("west not on PATH");
    }

    let fixture = project_root().join("packages/testing/nros-tests/fixtures/board_import_fvp");
    let build_dir = fixture.join("build");
    let _ = std::fs::remove_dir_all(&build_dir);

    let status = std::process::Command::new("west")
        .args([
            "build",
            "-d",
            build_dir.to_str().unwrap(),
            fixture.to_str().unwrap(),
        ])
        .status()
        .expect("spawn west");
    assert!(status.success(), "west build failed");

    let cache =
        std::fs::read_to_string(build_dir.join("CMakeCache.txt")).expect("read CMakeCache.txt");
    assert!(
        cache.contains("BOARD:STRING=fvp_baser_aemv8r/fvp_aemv8r_aarch64/smp")
            || cache.contains("BOARD:UNINITIALIZED=fvp_baser_aemv8r/fvp_aemv8r_aarch64/smp"),
        "BOARD did not propagate from board.cmake into CMakeCache.txt"
    );
    assert!(
        cache.contains("NANO_ROS_RMW:STRING=cyclonedds"),
        "NANO_ROS_RMW default did not propagate"
    );
    assert!(
        cache.contains("NROS_BOARD_RUNNER:STRING=armfvp"),
        "NROS_BOARD_RUNNER did not cache for `west fvp run`"
    );
}
