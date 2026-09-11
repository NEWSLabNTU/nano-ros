//! C Node-pkg workspace coverage (Phase 223).
//!
//! Verifies the mixed C/C++ reference template and the pure-C reference template
//! build into a linked Entry binary (no pub/sub traffic asserted — the runtime
//! instantiator for recorded C/C++ NodeEntityDescriptors is tracked outside
//! Phase 223).
//!
//! The build runs in the **build stage** — the `c_mixed_workspace` /
//! `pure_c_workspace` cmake fixtures (`compile-check-fixtures.sh`, run by
//! `build-test-fixtures`) build each template into `build/cmake-fixtures/<id>/`.
//! Since phase-445 W5 the templates have no root build file (RFC-0098 D9), so
//! that build is `nros build` over the bringup's `[image.native]`, and the
//! entry is the GENERATED `native_entry` in place of the deleted hand-written
//! `src/robot_entry`. These tests assert the prebuilt binary rather than
//! building at run time (issue 0034 / AGENTS.md "No compilation inside tests").
//! Fixture absence (no cmake / `codegen entry`-capable nros / play_launch_parser)
//! → tier-aware skip/fail.

fn assert_entry(id: &str) -> nros_tests::TestResult<()> {
    let exe =
        nros_tests::fixtures::require_cmake_fixture(id, "build/posix-native/cmake/native_entry")?;
    assert!(
        exe.is_file(),
        "{id}: missing Entry binary at {}",
        exe.display()
    );
    Ok(())
}

#[test]
fn c_node_pkg_links_into_cpp_entry_template() -> nros_tests::TestResult<()> {
    assert_entry("c_mixed_workspace")
}

#[test]
fn c_node_pkgs_link_into_c_entry_template() -> nros_tests::TestResult<()> {
    assert_entry("pure_c_workspace")
}
