//! C++ TYPED multi-node-workspace Entry-pkg cmake fixture (Phase 240.2b).
//!
//! Sibling of `cpp_multi_node_entry.rs`, but for the **typed** entry path
//! (`nano_ros_entry(... TYPED)` → `nros codegen entry --typed --metadata`).
//! Configuring + building `examples/templates/multi-node-workspace-cpp-typed/`
//! produces a generated TU that constructs each launch node's **component
//! object** + calls `configure(node)` + `NativeBoard::run_components` — NOT the
//! legacy register-symbol → `EntryNodeRuntime` interpreter call.
//!
//! The cmake configure + build run in the **build stage** — the
//! `cpp_robot_entry_typed` cmake fixture (`compile-check-fixtures.sh`, run by
//! `build-test-fixtures`) builds into a persistent
//! `build/cmake-fixtures/cpp_robot_entry_typed/`. This test INSPECTS the
//! prebuilt artifacts rather than running cmake at run time (issue 0034 /
//! AGENTS.md "No compilation inside tests"). Fixture absence → tier-aware
//! skip/fail via the resolver.

#[test]
fn multi_node_workspace_cpp_typed_configures_and_builds() -> nros_tests::TestResult<()> {
    let exe = nros_tests::fixtures::require_cmake_fixture(
        "cpp_robot_entry_typed",
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

    let gen_body = std::fs::read_to_string(&gen_tu).expect("read generated TU");

    // Typed shape: constructs each component + calls configure + run_components.
    assert!(
        gen_body.contains("static ::talker_pkg::Talker"),
        "generated TU missing talker_pkg::Talker component storage:\n{gen_body}"
    );
    assert!(
        gen_body.contains("static ::listener_pkg::Listener"),
        "generated TU missing listener_pkg::Listener component storage:\n{gen_body}"
    );
    assert!(
        gen_body.contains(".configure(__nros_node_0)"),
        "generated TU missing component configure() call:\n{gen_body}"
    );
    assert!(
        gen_body.contains("::nros::board::NativeBoard::run_components"),
        "generated TU missing run_components (typed real-executor entry):\n{gen_body}"
    );
    // Construct order matches launch XML (talker before listener).
    let pos_t = gen_body
        .find("static ::talker_pkg::Talker")
        .expect("talker storage");
    let pos_l = gen_body
        .find("static ::listener_pkg::Listener")
        .expect("listener storage");
    assert!(pos_t < pos_l, "component order doesn't match launch XML");

    // NOT the legacy interpreter path.
    assert!(
        !gen_body.contains("__nros_component_"),
        "typed TU must not emit the register-symbol interpreter calls:\n{gen_body}"
    );
    assert!(
        !gen_body.contains("NodeContext"),
        "typed TU must not reference NodeContext:\n{gen_body}"
    );

    // Auto-link sidecar still names both component libs (TYPED keeps the LAUNCH
    // auto-link).
    let link_body = std::fs::read_to_string(&link_libs).expect("read link sidecar");
    assert!(link_body.contains("talker_pkg_talker_component"));
    assert!(link_body.contains("listener_pkg_listener_component"));

    Ok(())
}
