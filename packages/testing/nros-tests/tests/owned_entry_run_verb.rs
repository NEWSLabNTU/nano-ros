//! `nros run` on an image that boots a NAMED owned entry (RFC-0071 D19).
//!
//! An owned entry is the user's own package, so its executable is whatever
//! its own `CMakeLists.txt` calls it. `nros run` used to resolve no artifact
//! for one ("the plan carries no `elf` artifact to run"). The program is now
//! read back from CMake's file-API reply, which `nros build` asks for.
//!
//! Both halves are checked, compiling nothing. The image is the
//! `linux-c-zenoh-talker` fixture row, whose entry `native_c_talker` builds
//! `c_talker`:
//!   * resolve: `nros run <image> --dry-run --json` plans the fixture's own
//!     executable as `elf`;
//!   * run: that plan, executed as written (`nros run --plan`), reaches the
//!     talker's first publish against a router.

use std::{path::PathBuf, process::Command};

use nros_tests::{
    fixtures::{RequireFixture, Rmw, ZenohRouter, build_native_c_talker_rmw, zenohd_unique},
    lane_scope,
    matrix::PlatformId,
    output,
    process::require_zenohd,
    run,
};
use rstest::rstest;

/// The image the fixture row builds, as `nros run` names it.
const IMAGE: &str = "native_c_talker_bringup:linux";

fn require_linux_lane() {
    if !lane_scope::admits(PlatformId::Linux) {
        nros_tests::lane_skip!(
            "{}",
            lane_scope::skip_note(PlatformId::Linux, "owned-entry-run-verb")
        );
    }
}

fn workspace() -> PathBuf {
    nros_tests::project_root().join("examples/linux/c/talker")
}

/// `nros run --dry-run --json` for the image: the plan, resolved, built nothing.
fn resolve_plan() -> serde_json::Value {
    let nros = run::nros_cli_path().expect("the `nros` CLI");
    let out = Command::new(nros)
        .args(["run", IMAGE, "--dry-run", "--json", "--workspace"])
        .arg(workspace())
        .arg("--nano-ros-path")
        .arg(nros_tests::project_root())
        .output()
        .expect("spawning `nros run --dry-run`");
    assert!(
        out.status.success(),
        "`nros run {IMAGE} --dry-run` failed ({}):\n{}",
        out.status,
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).unwrap_or_else(|e| {
        panic!(
            "`nros run --dry-run --json` printed no plan ({e}):\n{}",
            String::from_utf8_lossy(&out.stdout)
        )
    })
}

#[test]
fn a_named_owned_entry_resolves_its_own_executable() {
    require_linux_lane();
    let exe = build_native_c_talker_rmw(Rmw::Zenoh).require("linux-c-zenoh-talker");
    let plan = resolve_plan();
    let elf = plan
        .pointer("/artifacts/elf")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_else(|| panic!("the plan names no `elf`:\n{plan:#}"));
    assert_eq!(
        PathBuf::from(elf).canonicalize().ok(),
        exe.canonicalize().ok(),
        "the plan's `elf` is the entry's own executable"
    );
}

#[rstest]
fn a_named_owned_entry_runs_from_its_resolved_plan(#[from(zenohd_unique)] router: ZenohRouter) {
    require_linux_lane();
    require_zenohd();
    build_native_c_talker_rmw(Rmw::Zenoh).require("linux-c-zenoh-talker");
    let mut plan = resolve_plan();
    // This test's own router, so parallel cells never share one.
    plan["link"]["locator"] = router.locator().into();
    let mut run = run::resolved_plan(&plan).expect("writing the plan");
    let out = run
        .take_command()
        .args(["--expect", output::TALKER_LOG_PREFIX, "--timeout", "30"])
        .env("NROS_LOCATOR", router.locator())
        .output()
        .expect("spawning `nros run --plan`");
    assert!(
        out.status.success(),
        "`nros run --plan` on {} did not reach its first publish within 30 s \
         ({}):\nstdout:\n{}\nstderr:\n{}",
        run.image_line,
        out.status,
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}

// ── phase-583 W2 (RFC-0071 D20): an owned entry with several programs ──────

const TWO_PROGRAMS_IMAGE: &str = "two_programs_bringup:linux";
const TWO_PROGRAMS_DIR: &str = "packages/testing/nros-tests/fixtures/two_programs_ws";

/// `nros run <image> --dry-run [extra…]` on the two-program workspace.
fn dry_run_two_programs(extra: &[&str]) -> std::process::Output {
    let nros = run::nros_cli_path().expect("the `nros` CLI");
    Command::new(nros)
        .args(["run", TWO_PROGRAMS_IMAGE, "--dry-run", "--json"])
        .args(extra)
        .arg("--workspace")
        .arg(nros_tests::project_root().join(TWO_PROGRAMS_DIR))
        .arg("--nano-ros-path")
        .arg(nros_tests::project_root())
        .output()
        .expect("spawning `nros run --dry-run`")
}

/// `program`, as the two-program workspace's image `image` built it.
fn two_programs_binary(image: &str, program: &str, row: &str) -> PathBuf {
    nros_tests::fixtures::migrated_leaf_binary_named(
        TWO_PROGRAMS_DIR,
        Rmw::Zenoh,
        nros_tests::fixtures::migrated_leaf::LeafVariant::Image(image),
        program,
    )
    .unwrap_or_else(|| panic!("{TWO_PROGRAMS_DIR} is no migrated workspace leaf"))
    .require(row)
}

fn require_two_programs(program: &str) -> PathBuf {
    two_programs_binary("linux", program, "linux-c-zenoh-two-programs")
}

/// Naming no program, when the entry builds two, is refused: the run must not
/// guess. The refusal lists both programs by their program names (not the
/// CMake targets) and names the flag that chooses one.
#[test]
fn an_entry_with_two_programs_refuses_to_guess() {
    require_linux_lane();
    require_two_programs("prog_a");
    let out = dry_run_two_programs(&[]);
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        !out.status.success(),
        "`nros run {TWO_PROGRAMS_IMAGE}` resolved a plan with two programs and none named:\n{}",
        String::from_utf8_lossy(&out.stdout)
    );
    assert!(
        err.contains("`prog_a`") && err.contains("`prog_b`") && err.contains("--exec <name>"),
        "the refusal must list both programs and name `--exec`:\n{err}"
    );
}

/// `--exec` chooses one of the entry's programs, as `ros2 run <pkg> <exe>`.
#[rstest]
#[case::first("prog_a")]
#[case::second("prog_b")]
fn exec_chooses_one_of_two_programs(#[case] program: &str) {
    require_linux_lane();
    let exe = require_two_programs(program);
    let out = dry_run_two_programs(&["--exec", program]);
    assert!(
        out.status.success(),
        "`nros run {TWO_PROGRAMS_IMAGE} --exec {program} --dry-run` failed:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let plan: serde_json::Value = serde_json::from_slice(&out.stdout).expect("a JSON plan");
    let elf = plan
        .pointer("/artifacts/elf")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_else(|| panic!("the plan names no `elf`:\n{plan:#}"));
    assert_eq!(
        PathBuf::from(elf).canonicalize().ok(),
        exe.canonicalize().ok(),
        "`--exec {program}` resolves that program"
    );
}

/// phase-583 W3 -- an owned-entry bringup's launch file chooses the program:
/// `<node pkg="two_programs" exec="prog_b">`, so the run takes `prog_b` with
/// no `--exec`.
#[test]
fn the_launch_file_chooses_one_of_two_programs() {
    require_linux_lane();
    let exe = two_programs_binary(
        "linux_launch",
        "prog_b",
        "linux-c-zenoh-two-programs-launch",
    );
    let nros = run::nros_cli_path().expect("the `nros` CLI");
    let out = Command::new(nros)
        .args([
            "run",
            "two_programs_launch_bringup:linux_launch",
            "--dry-run",
            "--json",
        ])
        .arg("--workspace")
        .arg(nros_tests::project_root().join(TWO_PROGRAMS_DIR))
        .arg("--nano-ros-path")
        .arg(nros_tests::project_root())
        .output()
        .expect("spawning `nros run --dry-run`");
    assert!(
        out.status.success(),
        "`nros run two_programs_launch_bringup:linux_launch --dry-run` failed:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let plan: serde_json::Value = serde_json::from_slice(&out.stdout).expect("a JSON plan");
    let elf = plan
        .pointer("/artifacts/elf")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_else(|| panic!("the plan names no `elf`:\n{plan:#}"));
    assert_eq!(
        PathBuf::from(elf).canonicalize().ok(),
        exe.canonicalize().ok(),
        "the launch file's `exec=\"prog_b\"` chooses `prog_b`"
    );
}

/// phase-583 W3 -- an owned-entry bringup whose launch also starts a node from
/// another package is refused at configure, naming that package and the owned
/// entry. Recorded at build time by the `owned_entry_foreign_launch_refused`
/// row: the refusal is the `nros` CLI's, before any cmake.
#[test]
fn a_launch_node_from_another_package_is_refused_at_configure() {
    require_linux_lane();
    let stamp = nros_tests::fixtures::require_compile_check("owned_entry_foreign_launch_refused")
        .require("owned_entry_foreign_launch_refused");
    let dir = stamp.parent().expect("the row dir");
    let exit = std::fs::read_to_string(dir.join("exit")).expect("the recorded exit");
    let err = std::fs::read_to_string(dir.join("stderr")).expect("the recorded stderr");
    assert_ne!(
        exit.trim(),
        "0",
        "the configure must refuse; it exited 0:\n{err}"
    );
    let err = err.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(
        err.contains("`other_pkg`") && err.contains("`two_programs`") && err.contains("choose"),
        "the refusal must name the foreign package and the owned entry:\n{err}"
    );
}
