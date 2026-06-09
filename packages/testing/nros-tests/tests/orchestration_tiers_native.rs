//! Phase 228.G — multi-tier `nros::main!()` emit round-trip.
//!
//! Stages the `orchestration_tiers_native` fixture into a tempdir, rewrites
//! `@NANO_ROS_ROOT@` to absolute `path =` deps, and `cargo check`s the Entry
//! pkg. Because `system.toml` declares `[tiers.high]` + `[tiers.low]`, the
//! `nros::main!()` macro resolves a 2-tier table and emits
//! `<NativeBoard>::run_tiers(TIERS, run_plan)` (RFC-0032 §5). The check passing
//! proves that multi-tier emit produces valid code; the negative test proves the
//! instance-identity guard (RFC-0032 §7) fires.
//!
//! Run with: `cargo test -p nros-tests --test orchestration_tiers_native`

use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

fn workspace_root() -> PathBuf {
    nros_tests::project_root()
}

fn fixture_src() -> PathBuf {
    workspace_root().join("packages/testing/nros-tests/fixtures/orchestration_tiers_native")
}

fn stage_fixture() -> (tempfile::TempDir, PathBuf) {
    let src = fixture_src();
    let dst = tempfile::tempdir().expect("tempdir");
    copy_tree(&src, dst.path()).expect("copy fixture");
    let root_str = workspace_root().to_str().expect("utf-8").to_string();
    rewrite_placeholders(dst.path(), &root_str).expect("rewrite placeholders");
    let root = dst.path().to_path_buf();
    (dst, root)
}

fn copy_tree(src: &Path, dst: &Path) -> std::io::Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let from = entry.path();
        let to = dst.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_tree(&from, &to)?;
        } else {
            fs::copy(&from, &to)?;
        }
    }
    Ok(())
}

fn rewrite_placeholders(root: &Path, replacement: &str) -> std::io::Result<()> {
    let mut stack = vec![root.to_path_buf()];
    while let Some(p) = stack.pop() {
        if p.is_dir() {
            for e in fs::read_dir(&p)? {
                stack.push(e?.path());
            }
        } else if let Ok(text) = fs::read_to_string(&p) {
            if text.contains("@NANO_ROS_ROOT@") {
                fs::write(&p, text.replace("@NANO_ROS_ROOT@", replacement))?;
            }
        }
    }
    Ok(())
}

fn cargo_check(root: &Path) -> std::process::Output {
    Command::new("cargo")
        .args(["check", "-p", "demo_entry", "--manifest-path"])
        .arg(root.join("Cargo.toml"))
        .output()
        .expect("spawn cargo check")
}

fn cargo_build(root: &Path) -> std::process::Output {
    Command::new("cargo")
        .args(["build", "-p", "demo_entry", "--manifest-path"])
        .arg(root.join("Cargo.toml"))
        .output()
        .expect("spawn cargo build")
}

#[test]
fn multi_tier_main_macro_emits_run_tiers_and_compiles() {
    let (_g, root) = stage_fixture();
    let out = cargo_check(&root);
    assert!(
        out.status.success(),
        "multi-tier `nros::main!()` emit failed to compile.\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
}

#[test]
fn multi_tier_binary_boots_into_run_tiers() {
    // Build the emitted multi-tier binary and run it with NO zenohd. The boot
    // tier opens the one session, fails (no router), and `PosixBoard::run_tiers`
    // prints its unique abort line. Seeing that message proves the macro emitted
    // `run_tiers` AND the boot tier executed end-to-end — the single-tier
    // `BoardEntry::run` path prints a *different* line ("proceeding with
    // NullNodeRuntime"), so this needle is specific to the multi-tier emit. (No
    // router needed — the boot lifecycle is the proof, same as the entry-poc
    // gate.)
    let (_g, root) = stage_fixture();
    let build = cargo_build(&root);
    assert!(
        build.status.success(),
        "multi-tier binary failed to build.\nstderr:\n{}",
        String::from_utf8_lossy(&build.stderr),
    );
    let bin = root.join("target/debug/demo_entry");
    assert!(bin.is_file(), "binary not produced at {}", bin.display());

    let out = Command::new(&bin).output().expect("spawn demo_entry");
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        combined.contains("multi-tier entry needs a live session"),
        "binary did not reach the run_tiers boot path (no multi-tier abort line).\n\
         output:\n{combined}",
    );
}

#[test]
fn multi_tier_binary_runs_both_tiers_with_router() {
    // The complement to the no-router boot test: with a live zenohd (NSOS —
    // native host networking, no QEMU), the emitted binary gets *past*
    // `Executor::open`, spawns the low tier, and the boot tier prints the unique
    // `multi-tier run — N tier(s)` marker before its forever-spin (which the
    // timeout kills). Seeing the marker — and NOT the abort line — proves the
    // emitted multi-tier binary actually entered the per-tier run over a real
    // shared session.
    if !nros_tests::fixtures::require_zenohd() {
        nros_tests::skip!("zenohd not found");
    }
    let (_g, root) = stage_fixture();
    let build = cargo_build(&root);
    assert!(
        build.status.success(),
        "build failed.\nstderr:\n{}",
        String::from_utf8_lossy(&build.stderr),
    );
    let bin = root.join("target/debug/demo_entry");
    assert!(bin.is_file(), "binary not produced at {}", bin.display());

    let domain = nros_tests::unique_ros_domain_id();
    let port = 17_400u16 + u16::from(domain);
    let router = nros_tests::fixtures::ZenohRouter::start(port).expect("start zenohd router");
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
}

#[test]
fn single_tier_system_takes_the_legacy_boardentry_run_path() {
    // 228.F gate-parity guard: the SAME multi-tier fixture, with the `[tiers.*]`
    // blocks stripped from `system.toml`, must fall back to the unchanged
    // single-tier `BoardEntry::run` emit (RFC-0032 §5 / macro gate G.4 —
    // `has_tiers == false` → `resolved_tiers = None`). The components still
    // declare `callback_groups`, but with no `[tiers.*]` the macro never
    // resolves a tier table, so the legacy path is selected. Proof: the binary
    // prints the single-tier `NullNodeRuntime` line and NONE of the multi-tier
    // markers. (Pairs with `multi_tier_binary_boots_into_run_tiers`, which
    // asserts the opposite branch on the un-stripped fixture — together they
    // bracket the gate.)
    let (_g, root) = stage_fixture();
    let system_toml = root.join("src/demo_bringup/system.toml");
    let body = fs::read_to_string(&system_toml).expect("read system.toml");
    let cut = body
        .find("[tiers.")
        .expect("fixture system.toml should declare [tiers.*]");
    let single = &body[..cut];
    assert!(
        !single.contains("[tiers."),
        "tier blocks not fully stripped"
    );
    fs::write(&system_toml, single).expect("write single-tier system.toml");

    let build = cargo_build(&root);
    assert!(
        build.status.success(),
        "single-tier (tiers-stripped) fixture failed to build.\nstderr:\n{}",
        String::from_utf8_lossy(&build.stderr),
    );
    let bin = root.join("target/debug/demo_entry");
    assert!(bin.is_file(), "binary not produced at {}", bin.display());

    // No router — the legacy boot path opens the session, fails, and falls back
    // to NullNodeRuntime (its unique single-tier line).
    let out = Command::new(&bin).output().expect("spawn demo_entry");
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        combined.contains("proceeding with NullNodeRuntime"),
        "tiers-stripped binary did not take the legacy BoardEntry::run path.\noutput:\n{combined}",
    );
    assert!(
        !combined.contains("multi-tier"),
        "tiers-stripped binary emitted a multi-tier marker — the gate leaked into run_tiers.\n\
         output:\n{combined}",
    );
}

#[test]
fn instance_identity_mismatch_is_a_compile_error() {
    // RFC-0032 §7: a launch node that carries callback groups but does not name
    // a `[[component]]` is a hard error. Rename one launch node away from its
    // component name and assert the macro rejects it.
    let (_g, root) = stage_fixture();
    let launch = root.join("src/demo_bringup/launch/system.launch.xml");
    let body = fs::read_to_string(&launch).expect("read launch.xml");
    let bad = body.replace("name=\"control_node\"", "name=\"control_typo\"");
    assert_ne!(body, bad, "expected control_node in fixture launch");
    fs::write(&launch, bad).expect("write launch.xml");

    let out = cargo_check(&root);
    assert!(
        !out.status.success(),
        "expected a compile error for the instance-identity mismatch, but check succeeded.\n\
         stderr:\n{}",
        String::from_utf8_lossy(&out.stderr),
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("not a `[[component]]`") || stderr.contains("must match a `system.toml`"),
        "expected the instance-identity diagnostic, stderr:\n{stderr}",
    );
}
