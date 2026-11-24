//! A real `nros build` scopes the generated CMake root to its bringup
//! (issue 2754, phase-583 W0).
//!
//! The root falls back to every workspace package when an image's launch has
//! no resolved model yet, which only a plan made before any sync can see.
//! `nros build` resolves the models in its implicit configure first, so the
//! fallback must never be taken on the real path. If it were, NXP's failure
//! would come back quietly: another bringup's package configured and failing
//! in this build.
//!
//! Compiles nothing. The `workspace-cpp-native-talker` fixture row is built
//! by the build stage through the real `nros build`. This test resolves that
//! row (missing or stale fails), then reads the root that build wrote.
//! `examples/workspaces/cpp` holds nine node packages over five bringups.
//! The packages `pubsub_bringup` launches are READ from its launch files, never
//! listed here: phase-582 W1d added `talker_clock_board.launch.xml`, the
//! hard-coded pair went stale, and the test failed on a root that was right
//! (issue 2944).

use std::collections::BTreeSet;

use nros_tests::{
    TestResult,
    fixtures::workspace_image::{PairRole, resolve_coord},
    matrix::{Lang, PlatformId, Rmw},
};

#[test]
fn a_real_build_scopes_its_root_to_the_bringup() -> TestResult<()> {
    // Resolving the row is what proves the build ran and is current.
    let _talker = resolve_coord(PlatformId::Linux, Lang::Cpp, Rmw::Zenoh, PairRole::Talker)?;
    // The generated root sits beside the row's cmake binary dir.
    let root = nros_tests::fixtures::groups::workspace_artifact_dir("workspace-cpp-native-talker")?
        .parent()
        .expect("a cmake binary dir has a parent")
        .join("CMakeLists.txt");
    let text = std::fs::read_to_string(&root)
        .unwrap_or_else(|e| panic!("the row's build wrote no root at {}: {e}", root.display()));
    let block = text
        .split("    SUBDIRS\n")
        .nth(1)
        .unwrap_or_else(|| panic!("the root has no SUBDIRS:\n{text}"));
    let got: BTreeSet<&str> = block
        .lines()
        .take_while(|l| !l.starts_with(')'))
        .filter_map(|l| l.split('#').nth(1).map(str::trim))
        .collect();
    let ws = nros_tests::project_root().join("examples/workspaces/cpp");
    let launched = launched_packages(&ws.join("src/pubsub_bringup/launch"));
    let launched: BTreeSet<&str> = launched.iter().map(String::as_str).collect();
    // A vacuous expectation would pass anything: the bringup launches some of
    // the workspace's packages, not none and not all of them.
    let workspace = node_packages(&ws.join("src"));
    assert!(
        !launched.is_empty() && launched.len() < workspace.len(),
        "pubsub_bringup's launches name {launched:?} of the workspace's {workspace:?}: \
         this test needs a bringup that launches a strict subset"
    );
    assert_eq!(
        got, launched,
        "the pubsub bringup's root adds the packages its launch names. Every package here \
         means the unscoped fallback was taken on the real `nros build` path:\n{text}"
    );
    Ok(())
}

/// Every `pkg="..."` a `<node>` in the bringup's launch files names.
fn launched_packages(launch_dir: &std::path::Path) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let entries = std::fs::read_dir(launch_dir)
        .unwrap_or_else(|e| panic!("no launch dir {}: {e}", launch_dir.display()));
    for e in entries.flatten() {
        let p = e.path();
        if !p.to_string_lossy().ends_with(".launch.xml") {
            continue;
        }
        let text = std::fs::read_to_string(&p).unwrap_or_default();
        for chunk in text.split("<node").skip(1) {
            let head = chunk.split('>').next().unwrap_or("");
            if let Some(rest) = head.split("pkg=\"").nth(1)
                && let Some(pkg) = rest.split('"').next()
            {
                out.insert(pkg.to_string());
            }
        }
    }
    out
}

/// The workspace's node packages: `src/*` directories with a `package.xml`,
/// minus the bringups (which hold launch files, not nodes).
fn node_packages(src: &std::path::Path) -> BTreeSet<String> {
    std::fs::read_dir(src)
        .unwrap_or_else(|e| panic!("no workspace src {}: {e}", src.display()))
        .flatten()
        .filter(|e| {
            e.path().join("package.xml").is_file() && !e.path().join("system.toml").is_file()
        })
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect()
}
