//! `add_subdirectory(nano-ros)` link smoke (Phase 137.4).
//!
//! A user CMake project can `add_subdirectory(nano-ros)`, link
//! `NanoRos::NanoRos`, and `nros_platform_link_app()` an executable — i.e. the
//! cmake export surface + the C entry point link correctly (link-correctness
//! only; no runtime call).
//!
//! The cmake configure + build run in the **build stage** — the
//! `cmake_add_subdir` cmake fixture (`compile-check-fixtures.sh`, run by
//! `build-test-fixtures`) builds
//! `packages/testing/nros-tests/fixtures/cmake_add_subdirectory_smoke/` into
//! `build/cmake-fixtures/cmake_add_subdir/`. This test asserts the prebuilt
//! `smoke` binary rather than running cmake at run time (issue 0034 / 0041 /
//! AGENTS.md "No compilation inside tests"). Fixture absent (no cmake /
//! `codegen entry`-capable nros) → tier-aware skip/fail.

#[test]
fn cmake_add_subdirectory_smoke() -> nros_tests::TestResult<()> {
    let smoke = nros_tests::fixtures::require_cmake_fixture("cmake_add_subdir", "smoke")?;
    assert!(
        smoke.exists(),
        "smoke binary not produced at {} — add_subdirectory link regression",
        smoke.display()
    );
    Ok(())
}
