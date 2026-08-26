//! `nros build` stages 1→2→4 compose (phase-383 W2.c).
//!
//! An integration test because the property under test is that the stages
//! COMPOSE: discovery feeds image resolution feeds board resolution feeds
//! driver choice. Both halves were unit-tested and the composition still had a
//! real defect — the driver was chosen from the board NAME rather than its
//! platform, so a Zephyr image resolved to `cargo` and would have demanded a
//! generated root a west app does not need.
//!
//! Calls `plan_builds` directly rather than spawning the binary. Two reasons,
//! and the second is the important one: a test that locates `target/*/nros`
//! finds whatever was last built there, which during this work was a binary
//! predating the verb (`unrecognized subcommand 'build'`); and a test that
//! SKIPS when the binary is absent reports PASS on the very host it was meant
//! to warn about — the vacuous-test antipattern this repository gates against.
//! `plan_builds` performs no I/O beyond reading the workspace, so there is
//! nothing to spawn.

use std::path::Path;

use nros_cli_core::{
    builder::plan::Driver,
    cmd::build::{Args, plan_builds},
};

fn write(path: &Path, body: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, body).unwrap();
}

fn pkg_xml(name: &str) -> String {
    format!(
        "<?xml version=\"1.0\"?>\n<package format=\"3\">\n<name>{name}</name>\n\
         <version>0.0.0</version>\n<description>t</description>\n\
         <maintainer email=\"a@b.c\">m</maintainer>\n<license>Apache-2.0</license>\n</package>\n"
    )
}

/// The nano-ros checkout under test — the board catalog lives there, not in
/// the fixture workspace.
fn repo_root() -> std::path::PathBuf {
    // CARGO_MANIFEST_DIR is packages/cli/nros-cli-core.
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .expect("repo root")
        .to_path_buf();
    assert!(
        root.join("packages/boards").is_dir(),
        "board catalog not found under {} — the test's path arithmetic is wrong, \
         which must fail rather than silently skip",
        root.display()
    );
    root
}

/// A workspace with a node package, a cargo-only helper, and one bringup
/// declaring two images on different platforms.
fn fixture(dir: &Path) {
    write(
        &dir.join("src/talker_pkg/package.xml"),
        &pkg_xml("talker_pkg"),
    );
    write(
        &dir.join("src/demo_bringup/package.xml"),
        &pkg_xml("demo_bringup"),
    );
    write(
        &dir.join("src/demo_bringup/system.toml"),
        "[system]\nname = \"demo\"\nrmw = \"zenoh\"\ndomain_id = 0\n\n\
         [image_defaults]\nrmw = \"zenoh\"\n\n\
         [image.zephyr]\nboard = \"native_sim/native/64\"\n\n\
         [image.native]\nboard = \"linux\"\n",
    );
    // phase-383 F4 — a cargo member carrying no package.xml.
    write(
        &dir.join("src/helper/Cargo.toml"),
        "[package]\nname = \"helper\"\nversion = \"0.0.0\"\n",
    );
    write(
        &dir.join("Cargo.toml"),
        "[workspace]\nmembers = [\"src/talker_pkg\", \"src/helper\"]\n",
    );
}

fn args(ws: &Path, images: &[&str]) -> Args {
    Args {
        images: images.iter().map(|s| (*s).to_string()).collect(),
        workspace: Some(ws.to_path_buf()),
        nano_ros_path: Some(repo_root()),
        all: false,
        dry_run: true,
        offline: false,
        native_args: Vec::new(),
    }
}

#[test]
fn a_zephyr_image_resolves_to_west_and_needs_no_generated_root() {
    // The composition defect this test exists for. `native_sim/native/64` says
    // nothing about being Zephyr, so choosing a driver on the board NAME picked
    // cargo and demanded a root a west app does not need (RFC-0065 D3).
    let tmp = tempfile::tempdir().unwrap();
    fixture(tmp.path());
    let plans = plan_builds(&args(tmp.path(), &["zephyr"])).expect("resolves");

    assert_eq!(plans.len(), 1);
    assert_eq!(plans[0].platform, "zephyr");
    assert_eq!(plans[0].driver, Driver::West);
    let hand = plans[0]
        .handoff
        .as_ref()
        .expect("west needs no generated root, so a handoff exists today");
    assert!(
        hand.display()
            .contains("west build -b native_sim/native/64"),
        "the handoff carries the FRAMEWORK board string: {}",
        hand.display()
    );
}

#[test]
fn several_images_and_no_default_lists_them_and_fails() {
    // RFC-0065 D1 — never guess. PlatformIO builds every environment when none
    // is named; here that would be an expensive way to learn the default.
    let tmp = tempfile::tempdir().unwrap();
    fixture(tmp.path());
    let e = plan_builds(&args(tmp.path(), &[])).expect_err("must not guess");
    let msg = format!("{e:#}");
    assert!(msg.contains("demo_bringup:native"), "{msg}");
    assert!(msg.contains("demo_bringup:zephyr"), "{msg}");
    assert!(msg.contains("default_images"), "offers the fix: {msg}");
}

#[test]
fn default_images_removes_the_ambiguity() {
    let tmp = tempfile::tempdir().unwrap();
    fixture(tmp.path());
    let sys = tmp.path().join("src/demo_bringup/system.toml");
    let text = std::fs::read_to_string(&sys).unwrap();
    write(
        &sys,
        &text.replace(
            "domain_id = 0",
            "domain_id = 0\ndefault_images = [\"zephyr\"]",
        ),
    );
    let plans = plan_builds(&args(tmp.path(), &[])).expect("resolves");
    assert_eq!(plans.len(), 1);
    assert_eq!(plans[0].driver, Driver::West);
}

#[test]
fn a_cargo_image_reports_no_handoff_rather_than_building_the_wrong_thing() {
    // Honest partial delivery: W3/W4 are unwritten, so a cargo image must yield
    // no handoff and let the caller say why, not silently produce something.
    let tmp = tempfile::tempdir().unwrap();
    fixture(tmp.path());
    let plans = plan_builds(&args(tmp.path(), &["native"])).expect("resolves");
    assert_eq!(plans[0].platform, "posix");
    assert_eq!(plans[0].driver, Driver::Cargo);
    assert!(
        plans[0].handoff.is_none(),
        "a generated root is required and does not exist yet"
    );
}

#[test]
fn an_unknown_board_lists_the_boards_that_exist() {
    let tmp = tempfile::tempdir().unwrap();
    fixture(tmp.path());
    let sys = tmp.path().join("src/demo_bringup/system.toml");
    let text = std::fs::read_to_string(&sys).unwrap();
    write(
        &sys,
        &text.replace("board = \"linux\"", "board = \"linux-x86_64\""),
    );
    let e = plan_builds(&args(tmp.path(), &["native"])).expect_err("must reject");
    let msg = format!("{e:#}");
    assert!(msg.contains("matches no board"), "{msg}");
    assert!(msg.contains("linux"), "lists the real ones: {msg}");
}

#[test]
fn several_images_can_be_built_in_one_invocation() {
    // phase-383 F10 — `cargo build -p native_entry -p peer_entry` is
    // nano-ros-rt-eval's actual `just build`.
    let tmp = tempfile::tempdir().unwrap();
    fixture(tmp.path());
    let plans = plan_builds(&args(tmp.path(), &["zephyr", "native"])).expect("resolves");
    assert_eq!(plans.len(), 2);
}

#[test]
fn a_cargo_only_member_does_not_break_discovery() {
    // phase-383 F4 — src/helper has a Cargo.toml and no package.xml. Before the
    // union it was dropped, and a generated root missing it fails on an
    // unresolved path dep pointing at a file the user never edited.
    let tmp = tempfile::tempdir().unwrap();
    fixture(tmp.path());
    let members = nros_cli_core::builder::discover::cargo_workspace_members(tmp.path());
    let found =
        nros_cli_core::builder::discover::discover(tmp.path(), &members).expect("discovers");
    let names: Vec<&str> = found.packages.iter().map(|p| p.name.as_str()).collect();
    assert!(
        names.contains(&"helper"),
        "cargo-only member survives: {names:?}"
    );
    assert!(names.contains(&"talker_pkg"), "{names:?}");
}
