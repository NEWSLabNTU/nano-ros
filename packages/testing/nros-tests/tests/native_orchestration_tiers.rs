//! Multi-tier `nros::main!()` emit round-trip (RFC-0032 §5).
//!
//! When `system.toml` declares `[tiers.*]`, the macro resolves a tier table and
//! emits `<LinuxBoard>::run_tiers(TIERS, run_plan)`; with the tier blocks
//! stripped it falls back to the legacy single-tier `BoardEntry::run`.
//!
//! Both shapes are **build-stage fixtures** (`compile-check-fixtures.sh`, run by
//! `build-test-fixtures`): `orch_tiers_multi` (verbatim fixture) and
//! `orch_tiers_single` (the same fixture with the `cases/orch_tiers_single/`
//! overlay, a bringup `system.toml` without the `[tiers.*]` tables). The tests assert/run the prebuilt `demo_entry` binaries instead of
//! running `cargo build` at run time (issue 0034 / AGENTS.md "No compilation
//! inside tests"). The boot lifecycle (no router) or the per-tier run marker
//! (live router) is the proof the emitted code took the right branch.

use std::process::Command;

fn multi_tier_bin() -> nros_tests::TestResult<std::path::PathBuf> {
    nros_tests::fixtures::require_compile_check_bin("orch_tiers_multi", "target/debug/demo_entry")
}

fn single_tier_bin() -> nros_tests::TestResult<std::path::PathBuf> {
    nros_tests::fixtures::require_compile_check_bin("orch_tiers_single", "target/debug/demo_entry")
}

#[test]
fn multi_tier_main_macro_emits_run_tiers_and_compiles() -> nros_tests::TestResult<()> {
    // The build of `orch_tiers_multi` IS the multi-tier emit compile proof.
    let bin = multi_tier_bin()?;
    assert!(
        bin.exists(),
        "multi-tier fixture binary missing: {}",
        bin.display()
    );
    Ok(())
}

/// `nros::main!(launch = "<bringup>[:file]")` is the SUPPORTED entry spelling
/// (phase-330 W7): an entry names its INPUT and the build owns the model.
///
/// issue 1620 — this was `native_orchestration_misuse::launch_arm_resolves_the_bringup`,
/// which staged this same fixture, wrote the same `main.rs` line the fixture
/// already commits, and ran `cargo check` at TEST time — so it compiled what the
/// `orch_tiers_multi` build row already builds, and hard-FAILED where the
/// resolver was missing instead of reporting a precondition. The build is the
/// proof; what this adds is the pin that the build row is still compiling the
/// launch arm, because a fixture that quietly moved to `model =` would keep
/// every assertion above green (issue 0438 is that silent collapse).
#[test]
fn launch_arm_resolves_the_bringup() -> nros_tests::TestResult<()> {
    let main_rs = nros_tests::fixtures::fixture_dir("orchestration_tiers_native")
        .join("src/demo_entry/src/main.rs");
    let src = std::fs::read_to_string(&main_rs)
        .unwrap_or_else(|e| panic!("read {}: {e}", main_rs.display()));
    let invocation: Vec<&str> = src
        .lines()
        .map(str::trim)
        .filter(|l| !l.starts_with("//") && l.contains("nros::main!"))
        .collect();
    assert_eq!(
        invocation,
        ["nros::main!(launch = \"demo_bringup\");"],
        "the `orch_tiers_multi` fixture must compile the LAUNCH arm — it is the multi-tier \
         compile proof of it ({})",
        main_rs.display()
    );
    let bin = multi_tier_bin()?;
    assert!(
        bin.exists(),
        "launch-arm fixture binary missing: {}",
        bin.display()
    );
    Ok(())
}

#[test]
fn multi_tier_binary_boots_into_run_tiers() -> nros_tests::TestResult<()> {
    // Proof the macro emitted `run_tiers` AND the boot tier executed: the binary
    // emits a `multi-tier` marker either way — `multi-tier entry needs a live
    // session` (no router → abort) or `multi-tier run — N tier(s)` (a router was
    // reachable → it entered the per-tier run, then forever-spins). The
    // single-tier path emits NEITHER, so the `multi-tier` substring is the
    // branch-specific signal. Router presence is environmental (an orphaned /
    // other-user zenohd can be scouted), so wrap in `timeout` and accept both.
    let bin = multi_tier_bin()?;
    let out = Command::new("timeout")
        .args(["6", bin.to_str().expect("bin path utf-8")])
        .output()
        .expect("spawn demo_entry");
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        combined.contains("multi-tier"),
        "binary did not reach the run_tiers boot path (no multi-tier marker).\noutput:\n{combined}",
    );
    Ok(())
}

#[test]
fn multi_tier_binary_runs_both_tiers_with_router() -> nros_tests::TestResult<()> {
    // With a live zenohd the binary gets past `Executor::open`, spawns the low
    // tier, and the boot tier prints the `multi-tier run — N tier(s)` marker
    // before its forever-spin (the timeout kills it).
    if !nros_tests::fixtures::require_zenohd() {
        nros_tests::skip!("zenohd not found");
    }
    let bin = multi_tier_bin()?;
    let domain = nros_tests::unique_ros_domain_id();
    let port = 17_400u16 + u16::from(domain);
    let router = nros_tests::fixtures::or_skip(nros_tests::fixtures::ZenohRouter::start(port));
    let locator = router.locator();

    let out = Command::new("timeout")
        .args(["6", bin.to_str().expect("bin path utf-8")])
        .env("NROS_LOCATOR", &locator)
        .env("ROS_DOMAIN_ID", domain.to_string())
        .output()
        .expect("spawn demo_entry");
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        combined.contains("nros: multi-tier run — 2 tier(s)"),
        "binary did not enter the per-tier run with a live session.\noutput:\n{combined}",
    );
    assert!(
        !combined.contains("multi-tier entry needs a live session"),
        "binary aborted at session open despite a live router.\noutput:\n{combined}",
    );
    Ok(())
}

#[test]
fn single_tier_system_takes_the_legacy_boardentry_run_path() -> nros_tests::TestResult<()> {
    // The `[tiers.*]`-stripped variant must fall back to the legacy single-tier
    // `BoardEntry::run` emit (macro gate G.4): it emits NO `multi-tier` marker
    // (the branch-distinguishing invariant) AND reaches the lifecycle — either
    // `application complete` (a router was reachable → ran to completion) or
    // `proceeding with NullNodeRuntime` (no router → legacy fallback). Both are
    // router-agnostic; wrap in `timeout` so a scouted router can't hang it.
    let bin = single_tier_bin()?;
    let out = Command::new("timeout")
        .args(["6", bin.to_str().expect("bin path utf-8")])
        .output()
        .expect("spawn demo_entry");
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        !combined.contains("multi-tier"),
        "tiers-stripped binary emitted a multi-tier marker — the gate leaked into run_tiers.\noutput:\n{combined}",
    );
    assert!(
        combined.contains("application complete")
            || combined.contains("proceeding with NullNodeRuntime"),
        "tiers-stripped binary did not reach the legacy BoardEntry::run lifecycle.\noutput:\n{combined}",
    );
    Ok(())
}

/// The `orch_tiers_single` overlay (`cases/orch_tiers_single/`, a file since
/// issue 1656 — it was a `sed` inside `compile-check-fixtures.sh`, outside the
/// row's signature) is the template's bringup `system.toml` up to its first
/// `[tiers.` table and nothing else, so the two rows differ ONLY in the tier
/// tables. Reads sources only.
#[test]
fn single_tier_overlay_is_the_template_without_its_tier_tables() {
    let fx = nros_tests::project_root()
        .join("packages/testing/nros-tests/fixtures/orchestration_tiers_native");
    let read = |rel: &str| {
        std::fs::read_to_string(fx.join(rel)).unwrap_or_else(|e| panic!("read {rel}: {e}"))
    };
    let template = read("src/demo_bringup/system.toml");
    let overlay = read("cases/orch_tiers_single/src/demo_bringup/system.toml.case");
    let (head, _tiers) = template
        .split_once("\n[tiers.")
        .expect("the multi-tier template declares `[tiers.*]` tables");
    assert_eq!(
        overlay.trim_end(),
        head.trim_end(),
        "the single-tier overlay drifted from src/demo_bringup/system.toml: it must be \
         the template up to (not including) its first `[tiers.` table"
    );
    assert!(
        !overlay.lines().any(|l| l.starts_with("[tiers.")),
        "the single-tier overlay declares a tier table"
    );
}
