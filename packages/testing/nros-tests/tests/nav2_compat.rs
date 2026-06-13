//! nav2-style launch.xml copy-paste compat (§212.N.11) — build-stage fixture
//! (issue 0041).
//!
//! The Entry pkg's `build.rs` drives `nros_build::generate_run_plan` against a
//! nav2-shaped `launch/system.launch.xml` (the v1 tag set: `<arg>`, `<include>`,
//! `<param value="$(var …)"/>`, `<remap>`, `$(find <pkg>)`). The codegen path
//! must accept every directive + emit a `run_plan(runtime)` body registering
//! both `primary_node` and `secondary_node`, plus an `nros-plan.json` carrying
//! the resolved `<arg>` / `<remap>` / `<include>` evidence.
//!
//! Per issue 0041 the compile runs in the **build stage**: the `o5_nav2_compat`
//! build-fixture (`compile-check-fixtures.sh`) stages the workspace (rewriting
//! `@NANO_ROS_ROOT@` + `@NROS_CLI_ROOT@`) + `cargo build -p demo_entry` in the
//! `demo_entry/` subdir (excluded from the fixture root workspace), with
//! `play_launch_parser` on PATH. This test INSPECTS the prebuilt
//! `out/run_plan.rs` + `out/nros-system/nros-plan.json` — no cargo at run time.
//! If the build emitted the offline `Placeholder` stub (no `play_launch_parser`
//! at build time), there is no codegen evidence → the test skips.

use std::{
    fs,
    path::{Path, PathBuf},
};

fn walk(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(p) = stack.pop() {
        if p.is_dir() {
            if let Ok(rd) = fs::read_dir(&p) {
                for e in rd.flatten() {
                    stack.push(e.path());
                }
            }
        } else {
            out.push(p);
        }
    }
    out
}

#[test]
fn n11_launch_xml_ros2_compat_smoke() -> nros_tests::TestResult<()> {
    // Build-stage `cargo build -p demo_entry` succeeded (`.compile-ok` stamp).
    let stamp = nros_tests::fixtures::require_compile_check("o5_nav2_compat")?;
    let staged = stamp.parent().expect("stamp dir");

    // Locate the emitted run_plan.rs under demo_entry/target/.../build/
    // demo_entry-<hash>/out/ (the hash is build-specific → walk).
    let build_dir = staged.join("demo_entry/target/debug/build");
    let run_plan_path = walk(&build_dir)
        .into_iter()
        .find(|e| e.file_name().and_then(|n| n.to_str()) == Some("run_plan.rs"))
        .unwrap_or_else(|| {
            panic!(
                "nros-build did not emit run_plan.rs under {} — was the o5_nav2_compat \
                 build-fixture built? (`just build-test-fixtures`)",
                build_dir.display()
            )
        });
    let run_plan = fs::read_to_string(&run_plan_path).expect("read run_plan.rs");

    // Offline gate: a build without `play_launch_parser` writes a Placeholder
    // stub (compiles, but no N.11 directives exercised) → skip with the reason.
    if run_plan.contains("Placeholder") {
        nros_tests::skip!(
            "o5_nav2_compat build-fixture emitted the offline Placeholder stub at {} \
             (play_launch_parser absent at build time) — no codegen evidence to assert",
            run_plan_path.display()
        );
    }

    // Every `<node>` in the launch graph → a `<pkg>::register(runtime)` call.
    for pkg in ["primary_node", "secondary_node"] {
        let expected = format!("{pkg}::register");
        assert!(
            run_plan.contains(&expected),
            "run_plan.rs missing `{expected}` — directive not honoured:\n{run_plan}"
        );
    }
    assert!(
        run_plan.contains("pub fn run_plan"),
        "run_plan.rs missing `pub fn run_plan` declaration:\n{run_plan}"
    );

    // Sibling plan json — params / remaps / include evidence.
    let plan_json_path = run_plan_path
        .parent()
        .expect("run_plan.rs parent dir")
        .join("nros-system/nros-plan.json");
    assert!(
        plan_json_path.is_file(),
        "nros-plan.json missing at {} (run_plan.rs emitted but not the plan json?)",
        plan_json_path.display()
    );
    let plan_json = fs::read_to_string(&plan_json_path).expect("read nros-plan.json");

    // `<arg name="namespace" default="robot1"/>` → `<param value="$(var namespace)"/>`.
    assert!(
        plan_json.contains("robot1"),
        "nros-plan.json missing the `<arg>`-propagated default `robot1`:\n{plan_json}"
    );
    // `<remap from="chatter" to="primary/chatter"/>`.
    assert!(
        plan_json.contains("primary/chatter"),
        "nros-plan.json missing the `<remap>` target `primary/chatter`:\n{plan_json}"
    );
    // `<include file="$(find secondary_node)/launch/secondary.launch.xml"/>`.
    assert!(
        plan_json.contains("secondary_node") && plan_json.contains("primary_node"),
        "nros-plan.json missing a package ref — `<include>` sub-tree not expanded:\n{plan_json}"
    );
    Ok(())
}
