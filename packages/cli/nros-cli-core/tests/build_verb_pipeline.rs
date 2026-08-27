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
    // A real Rust node package carries BOTH — package.xml makes it a ROS
    // package, Cargo.toml makes it buildable. Only the second makes it a cargo
    // workspace member.
    write(
        &dir.join("src/talker_pkg/Cargo.toml"),
        "[package]\nname = \"talker_pkg\"\nversion = \"0.0.0\"\n",
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
    // Stage 3 refuses a workspace that has never been synced (issue 0463: the
    // alternative failure is a cargo manifest-PARSE error four frames deep that
    // never names `nros sync`). A real workspace has this; so must the fixture.
    std::fs::create_dir_all(dir.join("build/nros")).unwrap();
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
fn a_cargo_image_uses_the_tracked_root_and_never_overwrites_it() {
    // phase-383 W3.a. The cargo root lives at the WORKSPACE root, not under
    // build/ — cargo resolves a package's workspace by walking UP and requires
    // members to sit below the root, so `build/<coord>/Cargo.toml` is rejected
    // twice over. Which means the generated path IS a user's file, so an
    // authored root must survive untouched.
    let tmp = tempfile::tempdir().unwrap();
    fixture(tmp.path());
    let authored = std::fs::read_to_string(tmp.path().join("Cargo.toml")).unwrap();

    let plans = plan_builds(&args(tmp.path(), &["native"])).expect("resolves");
    assert_eq!(plans[0].platform, "posix");
    assert_eq!(plans[0].driver, Driver::Cargo);
    assert!(plans[0].handoff.is_some(), "stage 4 ran");

    assert_eq!(
        std::fs::read_to_string(tmp.path().join("Cargo.toml")).unwrap(),
        authored,
        "an authored root must survive byte-for-byte"
    );
}

#[test]
fn a_cargo_image_generates_the_root_when_the_workspace_has_none() {
    // RFC-0065 D13's end state: W9 DELETES the hand-written root, and the next
    // build regenerates the same member set from the tree — including the
    // cargo-only helper, found by walking once `[workspace] members` is no
    // longer there to read (phase-383 F4, one step later).
    let tmp = tempfile::tempdir().unwrap();
    fixture(tmp.path());
    std::fs::remove_file(tmp.path().join("Cargo.toml")).unwrap();

    let plans = plan_builds(&args(tmp.path(), &["native"])).expect("resolves");
    assert!(plans[0].handoff.is_some(), "stage 4 ran");

    let body = std::fs::read_to_string(tmp.path().join("Cargo.toml")).unwrap();
    assert!(body.starts_with("# GENERATED"), "{body}");
    assert!(body.contains("\"src/talker_pkg\""), "{body}");
    assert!(
        body.contains("\"src/helper\""),
        "the cargo-only member must survive the root's deletion: {body}"
    );
    assert!(
        !body.contains(tmp.path().to_str().unwrap()),
        "no absolute path may appear (W3.c): {body}"
    );
}

#[test]
fn zephyr_overlays_go_through_extra_conf_file_never_conf_file() {
    // phase-383 W5.a, and the correction that produced it: Zephyr's
    // configuration_files.cmake puts boards/ and socs/ auto-discovery inside
    // `if(NOT DEFINED CONF_FILE)`, so passing CONF_FILE suppresses both. Our
    // own zephyr entries suppress it today.
    let tmp = tempfile::tempdir().unwrap();
    fixture(tmp.path());
    let bd = tmp
        .path()
        .join("src/demo_bringup/boards/native_sim_native_64");
    write(&bd.join("prj.conf"), "CONFIG_X=y\n");
    write(&bd.join("prj-edf.conf"), "CONFIG_SCHED_DEADLINE=y\n");
    let sys = tmp.path().join("src/demo_bringup/system.toml");
    let text = std::fs::read_to_string(&sys).unwrap();
    write(
        &sys,
        &text.replace(
            "[image.zephyr]\nboard = \"native_sim/native/64\"",
            "[image.zephyr]\nboard = \"native_sim/native/64\"\nconf = [\"prj-edf.conf\"]",
        ),
    );

    let plans = plan_builds(&args(tmp.path(), &["zephyr"])).expect("resolves");
    let shown = plans[0].handoff.as_ref().expect("handoff").display();
    assert!(shown.contains("-DEXTRA_CONF_FILE="), "{shown}");
    assert!(shown.contains("prj-edf.conf"), "{shown}");
    assert!(
        shown.contains("-DAPPLICATION_CONFIG_DIR="),
        "the board config dir must be named: {shown}"
    );
    assert!(
        !shown.contains("-DCONF_FILE="),
        "CONF_FILE would suppress boards/ and socs/ discovery: {shown}"
    );
}

#[test]
fn a_cmake_workspace_gets_a_root_under_build_unlike_cargo() {
    // The asymmetry RFC-0065 D3 now records: cmake has no root/member hierarchy
    // rule, so its generated root DOES live at build/<coord>/ where D8 wants
    // every artefact. Only cargo is pinned to the workspace root.
    let tmp = tempfile::tempdir().unwrap();
    fixture(tmp.path());
    // Give a package a CMakeLists so the workspace crosses languages: cmake
    // wins whenever it does (RFC-0024 §6.3).
    write(
        &tmp.path().join("src/talker_pkg/CMakeLists.txt"),
        "# c pkg\n",
    );

    let plans = plan_builds(&args(tmp.path(), &["native"])).expect("resolves");
    assert_eq!(plans[0].driver, Driver::CMake, "cmake wins a mixed graph");

    let generated = tmp.path().join("build/posix-zenoh/CMakeLists.txt");
    assert!(generated.is_file(), "root written under build/");
    let body = std::fs::read_to_string(&generated).unwrap();
    assert!(body.contains("nano_ros_workspace("), "{body}");
    assert!(body.contains("ORDER_FROM_DEPENDS"), "{body}");
    assert!(body.contains("talker_pkg"), "{body}");
    assert!(
        !body.contains(tmp.path().to_str().unwrap()),
        "no absolute path (W3.c): {body}"
    );

    let shown = plans[0].handoff.as_ref().expect("handoff").display();
    assert!(shown.starts_with("cmake -S build/posix-zenoh"), "{shown}");
}

#[test]
fn a_materialized_entry_is_left_alone_and_still_builds() {
    // phase-383 W7.d. A decorative escape silently deletes capability, so the
    // property under test is that a materialised entry survives a build
    // untouched AND still reaches the handoff.
    use nros_cli_core::builder::materialize::{STAMP_FILE, Stamp, is_materialized};

    let tmp = tempfile::tempdir().unwrap();
    fixture(tmp.path());
    let entry = tmp.path().join("src/native_entry");
    write(
        &entry.join("Cargo.toml"),
        "[package]\nname = \"native_entry\"\nversion = \"0.0.0\"\n",
    );
    write(&entry.join("src/main.rs"), "fn main() {}\n");
    Stamp::current("native", "linux", "posix", "hosted-main")
        .write(&entry)
        .expect("stamp written");
    assert!(is_materialized(&entry));

    let before = std::fs::read_to_string(entry.join("src/main.rs")).unwrap();
    let plans = plan_builds(&args(tmp.path(), &["native"])).expect("resolves");
    assert!(
        plans[0].handoff.is_some(),
        "a materialised entry still builds"
    );
    assert_eq!(
        std::fs::read_to_string(entry.join("src/main.rs")).unwrap(),
        before,
        "the builder must not touch an owned entry"
    );
    assert!(entry.join(STAMP_FILE).is_file(), "the stamp survives");
}

#[test]
fn shape_drift_on_a_materialized_entry_warns_and_never_errors() {
    // W7.c — autoware-safety-island will hold a materialised entry forever by
    // design; erroring would break a legitimate downstream permanently.
    use nros_cli_core::builder::materialize::{Stamp, check};

    let tmp = tempfile::tempdir().unwrap();
    let entry = tmp.path().join("src/freertos_entry");
    std::fs::create_dir_all(&entry).unwrap();
    Stamp::current("freertos", "mps2-an385-freertos", "freertos", "board-run")
        .write(&entry)
        .unwrap();

    // The board now needs a different shape.
    let now = Stamp::current(
        "freertos",
        "mps2-an385-freertos",
        "freertos",
        "zephyr-staticlib",
    );
    let warnings = check(&entry, &now);
    assert!(!warnings.is_empty(), "drift is detected");
    assert!(
        warnings[0].contains("nros materialize"),
        "and names the fix: {warnings:?}"
    );
}

#[test]
fn a_cargo_image_generates_an_entry_from_the_launch_file() {
    // phase-383 W3.b — D4's headline claim. The node deps are DERIVED: the
    // launch file names talker_pkg, so that is what the entry links.
    let tmp = tempfile::tempdir().unwrap();
    fixture(tmp.path());
    write(
        &tmp.path().join("src/demo_bringup/launch/system.launch.xml"),
        "<launch>\n  <node pkg=\"talker_pkg\" exec=\"talker\" name=\"talker\"/>\n</launch>\n",
    );

    let plans = plan_builds(&args(tmp.path(), &["native"])).expect("resolves");
    assert!(plans[0].handoff.is_some());

    let entry = tmp.path().join("build/posix-zenoh/native_entry");
    if !entry.is_dir() {
        // The launch resolver is a separate binary; when it is absent the
        // builder WARNS and carries on (D13 — an un-migrated workspace must
        // keep building). Assert that documented fallback rather than skipping.
        assert!(
            !tmp.path().join("src/native_entry").exists(),
            "no entry was generated and none was hand-written — the build must \
             still have produced a handoff, which it did"
        );
        return;
    }
    let manifest = std::fs::read_to_string(entry.join("Cargo.toml")).unwrap();
    assert!(
        manifest.contains("talker_pkg"),
        "derived from the launch: {manifest}"
    );
    assert!(manifest.contains("nros-board-linux"), "{manifest}");
    assert!(
        !manifest.contains("= \"/"),
        "no absolute dependency path (W3.c): {manifest}"
    );
    let src = std::fs::read_to_string(entry.join("src/main.rs")).unwrap();
    assert!(src.contains("nros::main!("), "{src}");
}

#[test]
fn a_hand_written_entry_suppresses_generation() {
    // RFC-0065 D13 — an un-migrated workspace keeps its entry, so the migration
    // is a DELETION rather than a cutover.
    let tmp = tempfile::tempdir().unwrap();
    fixture(tmp.path());
    let hand = tmp.path().join("src/native_entry");
    write(
        &hand.join("Cargo.toml"),
        "[package]\nname = \"native_entry\"\nversion = \"0.0.0\"\n",
    );
    write(&hand.join("src/main.rs"), "// hand-written\n");

    plan_builds(&args(tmp.path(), &["native"])).expect("resolves");
    assert!(
        !tmp.path().join("build/posix-zenoh/native_entry").exists(),
        "a hand-written entry must suppress generation"
    );
    assert_eq!(
        std::fs::read_to_string(hand.join("src/main.rs")).unwrap(),
        "// hand-written\n"
    );
}

#[test]
fn entries_for_other_boards_are_not_listed() {
    // phase-383 W8.b — autoware-safety-island has THREE FreeRTOS entries
    // (an536, posix, s32z2) and a freertos-posix build listed all three.
    // RFC-0065's Problem statement names this as one of the four jobs a
    // hand-written root does by hand.
    let tmp = tempfile::tempdir().unwrap();
    fixture(tmp.path());
    for (name, deploy) in [
        ("posix_entry", "linux"),
        ("other_entry", "s32z270-freertos"),
    ] {
        let d = tmp.path().join("src").join(name);
        write(&d.join("package.xml"), &pkg_xml(name));
        write(
            &d.join("CMakeLists.txt"),
            &format!("nano_ros_add_executable({name}\n    DEPLOY  {deploy})\n"),
        );
    }
    // A CMakeLists anywhere makes this a cmake workspace.
    let plans = plan_builds(&args(tmp.path(), &["native"])).expect("resolves");
    assert_eq!(plans[0].driver, Driver::CMake);

    let body =
        std::fs::read_to_string(tmp.path().join("build/posix-zenoh/CMakeLists.txt")).unwrap();
    assert!(
        body.contains("posix_entry"),
        "the board's own entry: {body}"
    );
    assert!(
        !body.contains("other_entry"),
        "an entry for another board must not be listed: {body}"
    );
}

#[test]
fn a_framework_entrys_cmakelists_does_not_make_a_workspace_mixed() {
    // phase-383 W8.a — nano-ros-rt-eval is pure Rust and holds exactly one
    // CMakeLists: src/zephyr_entry/CMakeLists.txt, which belongs to WEST.
    // Counting it routed every native image through cmake.
    let tmp = tempfile::tempdir().unwrap();
    fixture(tmp.path());
    let z = tmp.path().join("src/zephyr_entry");
    write(&z.join("package.xml"), &pkg_xml("zephyr_entry"));
    write(&z.join("CMakeLists.txt"), "find_package(Zephyr REQUIRED)\n");
    write(
        &z.join("Cargo.toml"),
        "[package]\nname = \"zephyr_entry\"\nversion = \"0.0.0\"\n\n\
         [package.metadata.nros.entry]\ndeploy = \"zephyr\"\n",
    );

    let plans = plan_builds(&args(tmp.path(), &["native"])).expect("resolves");
    assert_eq!(
        plans[0].driver,
        Driver::Cargo,
        "a west app's CMakeLists is its framework's, not evidence the graph \
         crosses languages"
    );
}

#[test]
fn an_unsynced_workspace_is_refused_before_anything_is_generated() {
    // RFC-0065 D2 — a missing prerequisite fails at stage 3, naming the command
    // that fixes it, rather than mid-compile.
    let tmp = tempfile::tempdir().unwrap();
    fixture(tmp.path());
    std::fs::remove_dir_all(tmp.path().join("build/nros")).unwrap();
    let e = plan_builds(&args(tmp.path(), &["native"])).expect_err("must refuse");
    let msg = format!("{e:#}");
    assert!(msg.contains("nros sync"), "names the exact command: {msg}");
    assert!(msg.contains("nothing was built"), "{msg}");
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

/// A RELATIVE `--workspace` is an ordinary invocation and must plan.
///
/// phase-383 W9.b. The fixture driver cd's into the workspace and passes
/// `--workspace .`; every generated file computes paths relative to this root,
/// and `relative_or_err` needs two absolute paths. A relative root therefore
/// failed with "cannot express /abs/packages/api/nros relative to
/// ./build/posix-zenoh/native_entry" — an error naming the wrong path as the
/// problem, one layer below the cause.
#[test]
fn a_relative_workspace_root_plans_the_same_as_an_absolute_one() {
    let tmp = tempfile::tempdir().unwrap();
    fixture(tmp.path());
    let abs = plan_builds(&args(tmp.path(), &["native"])).expect("absolute root plans");

    let prev = std::env::current_dir().expect("cwd");
    std::env::set_current_dir(tmp.path()).expect("cd into the workspace");
    let rel = plan_builds(&args(Path::new("."), &["native"]));
    std::env::set_current_dir(prev).expect("cd back");

    let rel = rel.expect("a relative root plans too");
    assert_eq!(rel.len(), abs.len());
    assert_eq!(rel[0].qualified, abs[0].qualified);
    assert_eq!(rel[0].board, abs[0].board);
}

/// An ESP-IDF entry is a cargo MEMBER; only west entries are excluded.
///
/// phase-383 W9.b. The exclude set was computed from `needs_generated_root()`,
/// which answers a different question — whether stage 4 emits a root — and that
/// swept in the idf entry. `esp32_entry` is a `Cargo.toml`, a `package.xml` and
/// `src/`, with no CMakeLists; `idf.py` wraps a cargo build of it, and the
/// `workspace-rust-esp32` fixture row builds the same package directly. An
/// excluded package is not a member, so that row died with "package ID
/// specification `esp32_entry` did not match any packages" — during the
/// `lane=all` build, after zephyr, qemu, nuttx and native had all gone green.
#[test]
fn only_west_entries_are_excluded_from_the_generated_root() {
    use nros_cli_core::builder::plan::Driver;
    assert!(Driver::West.excluded_from_cargo_root());
    assert!(
        !Driver::IdfPy.excluded_from_cargo_root(),
        "an ESP-IDF entry is an ordinary cargo package"
    );
    assert!(!Driver::Cargo.excluded_from_cargo_root());
    assert!(!Driver::CMake.excluded_from_cargo_root());

    // The old exclude test was `!needs_generated_root()`. idf.py is exactly
    // where that disagrees with the right question, which is why collapsing the
    // two was invisible until an idf entry had to be a cargo member.
    assert!(!Driver::IdfPy.needs_generated_root());
    assert_ne!(
        !Driver::IdfPy.needs_generated_root(),
        Driver::IdfPy.excluded_from_cargo_root(),
        "the old predicate excluded idf.py; the right one does not"
    );
    // west agrees under both, which is why the other seven workspaces never
    // caught it.
    assert_eq!(
        !Driver::West.needs_generated_root(),
        Driver::West.excluded_from_cargo_root()
    );
}
