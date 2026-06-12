//! `nano_ros_use_board()` layers a board crate's artifacts into a downstream
//! Zephyr app build (Phase 215.E.2).
//!
//! The `west build` (FVP board) runs in the **build stage** — the
//! `west_board_import` west fixture (`scripts/build/west-fixtures.sh`, run by
//! `just zephyr build-fixtures`) configures `board_import_fvp`. This test
//! inspects the prebuilt `CMakeCache.txt` (BOARD / NANO_ROS_RMW / NROS_BOARD_RUNNER
//! propagation) rather than running west at run time (issue 0034 / 0041). Fixture
//! absent (no west / Zephyr+FVP SDK) → tier-aware skip/fail via the resolver.

#[test]
fn board_import_fvp_builds_via_nano_ros_use_board() -> nros_tests::TestResult<()> {
    let cache_path =
        nros_tests::fixtures::require_west_fixture("west_board_import", "CMakeCache.txt")?;
    let cache = std::fs::read_to_string(&cache_path).expect("read CMakeCache.txt");
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
    Ok(())
}
