//! C++ multi-node-workspace Entry-pkg cmake fixture — the typed entry path, now
//! the ONLY C++ multi-node entry form (phase-257 retired the legacy
//! register-symbol interpreter; the non-typed template + its test were deleted).
//!
//! `nano_ros_entry(... TYPED)` → `nros codegen entry --typed --metadata`.
//! Configuring + building `examples/templates/multi-node-workspace-cpp/` produces
//! a generated TU that constructs each launch node's **component object** + calls
//! `configure(node)` + `NativeBoard::run_components` — NOT the legacy
//! register-symbol → `EntryNodeRuntime` interpreter call.
//!
//! The cmake configure + build run in the **build stage** — the
//! `cpp_robot_entry` cmake fixture (`compile-check-fixtures.sh`, run by
//! `build-test-fixtures`) builds into a persistent
//! `build/cmake-fixtures/cpp_robot_entry/`. This test INSPECTS the
//! prebuilt artifacts rather than running cmake at run time (issue 0034 /
//! AGENTS.md "No compilation inside tests"). Fixture absence → tier-aware
//! skip/fail via the resolver.

use nros_tests::fixtures::zenohd_unique;

#[test]
fn multi_node_workspace_cpp_typed_configures_and_builds() -> nros_tests::TestResult<()> {
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

    // Phase 211.H (issue #52) — the talker node's launch `<param
    // name="qos_overrides./chatter.publisher.reliability" value="best_effort"/>`
    // is baked by emit_cpp into a `set_qos_overrides` call BEFORE
    // `configure(node_0)`, with role/policy/value mapped to the C-ABI codes
    // (publisher=0, reliability=0, best_effort=0). Codegen-on-a-real-cmake-build
    // evidence (the bake runs in `emit_typed`, driven by `nano_ros_entry`'s
    // `nros codegen entry --typed` shell-out) — the one path the nros-cli-core
    // unit tests can't reach. The build above linking proves the bake compiles
    // against the real `nros::Node::set_qos_overrides`.
    assert!(
        gen_body.contains("__nros_qos_0[]"),
        "generated TU missing the baked qos_overrides table:\n{gen_body}"
    );
    assert!(
        gen_body.contains("{ \"/chatter\", 0, 0, 0 }"),
        "qos_overrides table missing the best_effort publisher override (codes 0,0,0):\n{gen_body}"
    );
    assert!(
        gen_body.contains("__nros_node_0.set_qos_overrides(__nros_qos_0, 1)"),
        "generated TU missing the set_qos_overrides install call:\n{gen_body}"
    );
    let set_at = gen_body
        .find("set_qos_overrides")
        .expect("set_qos_overrides present");
    let cfg_at = gen_body
        .find(".configure(__nros_node_0)")
        .expect("configure present");
    assert!(
        set_at < cfg_at,
        "set_qos_overrides must be installed BEFORE configure(node_0)"
    );

    Ok(())
}

/// Runtime E2E: the typed entry boots + runs **real** callbacks on the executor.
/// `robot_entry` runs both nodes (talker + listener) in one process; same-session
/// has no loopback, so we run **two** processes vs a router — each listener
/// receives the other's talker pubs. Asserts ≥1 `Received` (the typed
/// `bind_subscription_raw` callback fired) and that the talker published.
///
/// Phase 211.H (issue #52) — the talker's `/chatter` publisher is created under
/// the baked `qos_overrides…reliability=best_effort` (see the launch + the
/// generated-TU assertions above), so this is also the cmake C++ qos_override
/// runtime-delivery proof: delivery succeeds with the override applied.
#[rstest::rstest]
fn multi_node_workspace_cpp_typed_pubsub_e2e(
    zenohd_unique: nros_tests::fixtures::ZenohRouter,
) -> nros_tests::TestResult<()> {
    use nros_tests::{TestError, count_pattern, fixtures::ManagedProcess};
    use std::{process::Command, time::Duration};

    let zenohd = zenohd_unique;
    if !nros_tests::fixtures::require_zenohd() {
        nros_tests::skip!("zenohd not found");
    }
    let exe = nros_tests::fixtures::require_cmake_fixture(
        "cpp_robot_entry",
        "src/robot_entry/robot_entry",
    )?;
    let locator = zenohd.locator();

    let spawn = |name: &str| -> nros_tests::TestResult<ManagedProcess> {
        let mut cmd = Command::new(&exe);
        cmd.env("NROS_LOCATOR", &locator)
            .env("NROS_SESSION_MODE", "client")
            .env("NROS_ENTRY_SPIN_MS", "20000");
        ManagedProcess::spawn_command(cmd, name.to_string())
    };

    let mut a = spawn("entry_a")?;
    a.wait_for_output_pattern("Waiting for messages", Duration::from_secs(10))
        .map_err(|e| TestError::ProcessFailed(format!("entry_a not ready: {e:?}")))?;
    let mut b = spawn("entry_b")?;

    // a's listener receives b's talker pubs (cross-process).
    let out = a
        .wait_for_output_pattern("Received", Duration::from_secs(20))
        .unwrap_or_default();
    a.kill();
    b.kill();

    let received = count_pattern(&out, "Received");
    eprintln!("[typed-entry] received: {received}\n{out}");
    assert!(
        received > 0,
        "typed multi-node entry pubsub E2E — 0 messages received"
    );
    Ok(())
}
