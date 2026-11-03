//! The host build role (RFC-0119 D3, phase-527 W2), through `nros build`'s
//! planning.
//!
//! A workspace holding one image and one runner package: the runner is built
//! for the host triple into `build/nros/host/<package>/`, an image depends on
//! the package providing its runner (so `--packages-up-to <bringup>` reaches
//! it), and the image's own generated root never lists it.
//!
//! These are PLAN tests, like `build_verb_pipeline.rs`: they assert what would
//! run and run nothing. That the two builds then leave each other alone (a
//! runner edit rebuilds only the runner, an image edit does not rebuild the
//! runner) follows from the separate cargo roots and target directories
//! asserted here; the measured run is recorded in the phase-527 doc.

use std::path::Path;

use nros_cli_core::{
    builder::plan::Driver,
    cmd::build::{Args, ResolvedBuild, SelectionArgs, plan_with_host_tools},
};

fn write(path: &Path, body: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, body).unwrap();
}

fn pkg_xml(name: &str, export: &str) -> String {
    format!(
        "<?xml version=\"1.0\"?>\n<package format=\"3\">\n<name>{name}</name>\n\
         <version>0.0.0</version>\n<description>t</description>\n\
         <maintainer email=\"a@b.c\">m</maintainer>\n<license>Apache-2.0</license>\n\
         <export>{export}</export>\n</package>\n"
    )
}

fn repo_root() -> std::path::PathBuf {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .expect("repo root")
        .to_path_buf();
    assert!(root.join("packages/boards").is_dir(), "{}", root.display());
    root
}

/// A runner package providing `name`, as `nros new --kind runner` will write
/// one.
fn runner_package(ws: &Path, package: &str, name: &str) {
    let dir = ws.join("src").join(package);
    write(
        &dir.join("package.xml"),
        &pkg_xml(
            package,
            &format!(
                "<build_type>nros_cargo</build_type>\
                 <nano_ros_provides kind=\"runner\" name=\"{name}\"/>"
            ),
        ),
    );
    write(
        &dir.join("Cargo.toml"),
        &format!("[package]\nname = \"{package}\"\nversion = \"0.0.0\"\n"),
    );
    write(&dir.join("src/main.rs"), "fn main() {}\n");
    write(
        &dir.join("nros-runner.toml"),
        "protocol = 1\nverbs = [\"run\"]\n",
    );
}

/// One node package, one bringup with one host image, one runner package that
/// provides the host image's runner (`native`).
fn fixture(ws: &Path) {
    write(
        &ws.join("src/talker_pkg/package.xml"),
        &pkg_xml("talker_pkg", ""),
    );
    write(
        &ws.join("src/talker_pkg/Cargo.toml"),
        "[package]\nname = \"talker_pkg\"\nversion = \"0.0.0\"\n",
    );
    write(
        &ws.join("src/demo_bringup/package.xml"),
        &pkg_xml("demo_bringup", ""),
    );
    write(
        &ws.join("src/demo_bringup/system.toml"),
        "[system]\nname = \"demo\"\nrmw = \"zenoh\"\ndomain_id = 0\n\n\
         [image_defaults]\nrmw = \"zenoh\"\n\n\
         [image.linux]\nboard = \"linux\"\n",
    );
    runner_package(ws, "acme_native_runner", "native");
    std::fs::create_dir_all(ws.join("build/nros")).unwrap();
}

fn args(ws: &Path, images: &[&str], select: &[&str], up_to: &[&str]) -> Args {
    Args {
        images: images.iter().map(|s| (*s).to_string()).collect(),
        workspace: Some(ws.to_path_buf()),
        nano_ros_path: Some(repo_root()),
        zephyr_workspace: None,
        board: None,
        all: false,
        dry_run: true,
        verbose: false,
        offline: false,
        selection: SelectionArgs {
            packages_select: select.iter().map(|s| (*s).to_string()).collect(),
            packages_up_to: up_to.iter().map(|s| (*s).to_string()).collect(),
            ..Default::default()
        },
        continue_on_error: false,
        cmake_args: Vec::new(),
        jobs: None,
        parallel_workers: None,
        build_base: None,
        check: None,
        native_args: Vec::new(),
    }
}

fn names(plans: &[ResolvedBuild]) -> Vec<&str> {
    plans.iter().map(|p| p.qualified.as_str()).collect()
}

fn host_plan<'a>(plans: &'a [ResolvedBuild], name: &str) -> &'a ResolvedBuild {
    plans
        .iter()
        .find(|p| p.qualified == name)
        .unwrap_or_else(|| panic!("no plan for {name}: {:?}", names(plans)))
}

#[test]
fn up_to_the_bringup_builds_the_image_and_its_runner_for_the_host() {
    let tmp = tempfile::tempdir().unwrap();
    let ws = tmp.path().canonicalize().unwrap();
    fixture(&ws);

    let plans =
        plan_with_host_tools(&args(&ws, &["linux"], &[], &["demo_bringup"])).expect("plans");
    assert_eq!(
        names(&plans),
        vec!["acme_native_runner", "demo_bringup:linux"],
        "the runner is reached through the image's edge, and planned first"
    );

    let runner = host_plan(&plans, "acme_native_runner");
    assert_eq!(runner.driver, Driver::Cargo);
    let triple = runner.target.clone().expect("the host triple");
    let shown = runner.handoff.as_ref().unwrap().display();
    assert!(shown.contains(&format!("--target {triple}")), "{shown}");
    assert!(
        shown.contains(&format!(
            "--target-dir {}",
            ws.join("build/nros/host/acme_native_runner/target")
                .display()
        )),
        "its own target directory: {shown}"
    );

    // The image's root never names the runner: its build cannot be touched by
    // the runner's sources.
    let image = host_plan(&plans, "demo_bringup:linux");
    let entry = ws.join("build/posix-zenoh/linux_entry");
    for f in ["Cargo.toml", "nros-cargo.toml"] {
        let text = std::fs::read_to_string(entry.join(f)).unwrap_or_default();
        assert!(!text.contains("acme_native_runner"), "{f}: {text}");
    }
    let image_shown = image.handoff.as_ref().unwrap().display();
    assert!(!image_shown.contains("build/nros/host"), "{image_shown}");
}

#[test]
fn a_selection_that_misses_the_bringup_does_not_reach_the_runner() {
    let tmp = tempfile::tempdir().unwrap();
    let ws = tmp.path().canonicalize().unwrap();
    fixture(&ws);
    let plans = plan_with_host_tools(&args(&ws, &["linux"], &[], &["talker_pkg"])).expect("plans");
    assert_eq!(names(&plans), vec!["demo_bringup:linux"]);
}

#[test]
fn no_selection_builds_every_host_tool() {
    let tmp = tempfile::tempdir().unwrap();
    let ws = tmp.path().canonicalize().unwrap();
    fixture(&ws);
    // A runner no image uses still builds with the workspace, as a command
    // package does.
    runner_package(&ws, "acme_other_runner", "acme-other");
    let plans = plan_with_host_tools(&args(&ws, &["linux"], &[], &[])).expect("plans");
    let mut n = names(&plans);
    n.sort_unstable();
    assert_eq!(
        n,
        vec![
            "acme_native_runner",
            "acme_other_runner",
            "demo_bringup:linux"
        ]
    );
}

#[test]
fn selecting_only_a_host_tool_builds_it_and_no_image() {
    let tmp = tempfile::tempdir().unwrap();
    let ws = tmp.path().canonicalize().unwrap();
    fixture(&ws);
    let plans = plan_with_host_tools(&args(&ws, &[], &["acme_native_runner"], &[])).expect("plans");
    assert_eq!(names(&plans), vec!["acme_native_runner"]);
}

#[test]
fn a_workspace_with_no_bringup_builds_a_host_tool_in_the_host_dir() {
    let tmp = tempfile::tempdir().unwrap();
    let ws = tmp.path().canonicalize().unwrap();
    write(&ws.join(".colcon_workspace"), "");
    runner_package(&ws, "acme_native_runner", "native");
    let plans = plan_with_host_tools(&args(&ws, &[], &[], &[])).expect("plans");
    assert_eq!(names(&plans), vec!["acme_native_runner"], "planned once");
    let shown = plans[0].handoff.as_ref().unwrap().display();
    assert!(
        shown.contains("build/nros/host/acme_native_runner"),
        "{shown}"
    );
    assert!(!ws.join("build/acme_native_runner").exists());
}
