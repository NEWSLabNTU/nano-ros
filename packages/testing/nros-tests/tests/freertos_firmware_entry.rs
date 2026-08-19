//! FreeRTOS QEMU MPS2-AN385 Entry-pkg firmware builds + emits a run-plan
//! (Phase 212.N.7).
//!
//! The firmware bin's `build.rs` calls `nros_build::generate_run_plan(launch)`
//! to emit `$OUT_DIR/run_plan.rs` with one `<pkg>::register(runtime)` line per
//! `<node>` in `launch/system.launch.xml`.
//!
//! The cross build (`cargo build --target thumbv7m-none-eabi -p firmware`) runs
//! in the **build stage** — the `freertos_firmware` cross-build fixture
//! (`compile-check-fixtures.sh`, run by `build-test-fixtures`) stages the
//! `multi_pkg_workspace_freertos` template + builds the firmware. This test
//! inspects the prebuilt `run_plan.rs` rather than running cargo at run time
//! (issue 0034 / AGENTS.md "No compilation inside tests"). Fixture absent
//! (thumbv7m target / arm-none-eabi-gcc / FreeRTOS+lwIP not provisioned) →
//! tier-aware skip/fail via the resolver.

use std::{fs, path::PathBuf};

fn find_run_plan(build_dir: &std::path::Path) -> Option<PathBuf> {
    let mut stack = vec![build_dir.to_path_buf()];
    while let Some(p) = stack.pop() {
        if p.is_dir() {
            for e in fs::read_dir(&p).ok()?.flatten() {
                stack.push(e.path());
            }
        } else if p.file_name().and_then(|n| n.to_str()) == Some("run_plan.rs") {
            return Some(p);
        }
    }
    None
}

#[test]
fn freertos_qemu_mps2_an385_entry_pkg_firmware_builds() -> nros_tests::TestResult<()> {
    let stamp = nros_tests::fixtures::require_compile_check("freertos_firmware")?;
    let staged = stamp.parent().expect("fixture dir");
    let build_dir = staged.join("src/firmware/target/thumbv7m-none-eabi/debug/build");
    let run_plan_path = find_run_plan(&build_dir)
        .expect("nros-build did not emit run_plan.rs under src/firmware/target");
    let run_plan = fs::read_to_string(&run_plan_path).expect("read run_plan.rs");

    // Issue 0686 — a stub is a FAILURE, not a pass.
    //
    // This arm used to `eprintln!("build smoke verified")` and fall through
    // green, so the fixture reported success while exercising no codegen at all.
    // It stayed that way for months: the node pkgs carried no
    // `[package.metadata.nros.node]`, the planner failed
    // `missing-source-metadata`, the build script fell back to the stub, and
    // this test said fine — proving the ELF LINKS, never that the launch file
    // PLANS, which is the property the fixture exists for.
    //
    // The stub carries its cause on a `// reason:` line (issue 0683); quote it,
    // because "no codegen" is the symptom and the planner error is the finding.
    assert!(
        !run_plan.contains("Placeholder"),
        "run_plan.rs is the Placeholder stub — no codegen was exercised, so this \
         fixture proves only that the ELF links. nros-build said: {}",
        run_plan
            .lines()
            .find_map(|l| l.trim().strip_prefix("// reason: "))
            .unwrap_or("no reason recorded — build.rs predates issue 0683")
    );
    {
        for pkg in ["talker_pkg", "listener_pkg"] {
            let expected = format!("{pkg}::register");
            assert!(
                run_plan.contains(&expected),
                "run_plan.rs missing `{expected}`:\n{run_plan}"
            );
        }
        assert!(
            run_plan.contains("pub fn run_plan"),
            "run_plan.rs missing `pub fn run_plan`:\n{run_plan}"
        );
    }
    Ok(())
}
