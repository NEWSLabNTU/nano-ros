//! C Node-pkg workspace coverage (Phase 223).
//!
//! Verifies the mixed C/C++ reference template and the pure-C reference template
//! configure + build into a linked `robot_entry` Entry-pkg binary (no
//! pub/sub traffic asserted — the runtime instantiator for recorded C/C++
//! NodeEntityDescriptors is tracked outside Phase 223).
//!
//! The cmake configure + build run in the **build stage** — the
//! `c_mixed_workspace` / `pure_c_workspace` cmake fixtures
//! (`compile-check-fixtures.sh`, run by `build-test-fixtures`) build each
//! template into `build/cmake-fixtures/<id>/`. These tests assert the prebuilt
//! `robot_entry` binary rather than running cmake at run time (issue 0034 /
//! AGENTS.md "No compilation inside tests"). Fixture absence (no cmake /
//! `codegen entry`-capable nros / play_launch_parser) → tier-aware skip/fail.

fn assert_robot_entry(id: &str) -> nros_tests::TestResult<()> {
    let exe = nros_tests::fixtures::require_cmake_fixture(id, "src/robot_entry/robot_entry")?;
    assert!(
        exe.is_file(),
        "{id}: missing Entry pkg binary at {}",
        exe.display()
    );
    Ok(())
}

#[test]
fn c_node_pkg_links_into_cpp_entry_template() -> nros_tests::TestResult<()> {
    assert_robot_entry("c_mixed_workspace")
}

#[test]
fn c_node_pkgs_link_into_c_entry_template() -> nros_tests::TestResult<()> {
    assert_robot_entry("pure_c_workspace")
}
