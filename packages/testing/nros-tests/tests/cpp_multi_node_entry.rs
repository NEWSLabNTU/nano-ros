//! C++ multi-node-workspace Entry-pkg cmake fixture (Phase 219.acc §6).
//!
//! Guards the C++ Entry-pkg build-system + codegen plumbing: configuring +
//! building `examples/templates/multi-node-workspace-cpp/` produces
//!   * the generated TU `robot_entry_nros_main_generated.cpp` declaring + calling
//!     both Node-pkg register fns in launch order;
//!   * the Phase 219.J auto-link sidecar `robot_entry_link_libs.cmake` naming
//!     both Node-pkg static libs;
//!   * a depfile listing the launch.xml + package.xml inputs;
//!   * the linked `robot_entry` executable.
//!
//! The cmake configure + build run in the **build stage** — the `cpp_robot_entry`
//! cmake fixture (`compile-check-fixtures.sh`, run by `build-test-fixtures`)
//! builds into a persistent `build/cmake-fixtures/cpp_robot_entry/`. This test
//! INSPECTS the prebuilt artifacts rather than running cmake at run time (issue
//! 0034 / AGENTS.md "No compilation inside tests"). Fixture absence (no cmake /
//! no `codegen entry`-capable nros) → tier-aware skip/fail via the resolver.

#[test]
fn multi_node_workspace_cpp_configures_and_builds() -> nros_tests::TestResult<()> {
    let exe = nros_tests::fixtures::require_cmake_fixture(
        "cpp_robot_entry",
        "src/robot_entry/robot_entry",
    )?;
    assert!(
        exe.is_file(),
        "robot_entry executable missing at {}",
        exe.display()
    );

    let robot_dir = exe.parent().expect("robot_entry dir");
    let gen_tu = robot_dir.join("robot_entry_nros_main_generated.cpp");
    let link_libs = robot_dir.join("robot_entry_link_libs.cmake");
    let depfile = robot_dir.join("robot_entry_nros_main_generated.d");
    assert!(
        gen_tu.is_file(),
        "missing generated TU at {}",
        gen_tu.display()
    );
    assert!(
        link_libs.is_file(),
        "missing link-libs sidecar at {}",
        link_libs.display()
    );
    assert!(
        depfile.is_file(),
        "missing depfile at {}",
        depfile.display()
    );

    let gen_body = std::fs::read_to_string(&gen_tu).expect("read generated TU");
    assert!(
        gen_body.contains("__nros_component_talker_pkg_register"),
        "generated TU missing talker_pkg register call:\n{gen_body}"
    );
    assert!(
        gen_body.contains("__nros_component_listener_pkg_register"),
        "generated TU missing listener_pkg register call:\n{gen_body}"
    );
    let pos_t = gen_body
        .find("__nros_component_talker_pkg_register(context)")
        .expect("talker call");
    let pos_l = gen_body
        .find("__nros_component_listener_pkg_register(context)")
        .expect("listener call");
    assert!(
        pos_t < pos_l,
        "register call order doesn't match launch XML"
    );

    let link_body = std::fs::read_to_string(&link_libs).expect("read link sidecar");
    assert!(link_body.contains("talker_pkg_talker_component"));
    assert!(link_body.contains("listener_pkg_listener_component"));

    let dep_body = std::fs::read_to_string(&depfile).expect("read depfile");
    assert!(
        dep_body.contains("system.launch.xml"),
        "depfile missing launch.xml:\n{dep_body}"
    );
    assert!(
        dep_body.contains("package.xml"),
        "depfile missing package.xml:\n{dep_body}"
    );
    Ok(())
}
