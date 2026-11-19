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
