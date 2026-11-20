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
//! `examples/workspaces/cpp` holds nine node packages over five bringups;
//! `pubsub_bringup` launches two of them.

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
    let root = nros_tests::project_root()
        .join("examples/workspaces/cpp/build/posix-zenoh-linux-pubsub_bringup/CMakeLists.txt");
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
    assert_eq!(
        got,
        BTreeSet::from(["listener_pkg", "talker_pkg"]),
        "the pubsub bringup's root adds the packages its launch names. Every package here \
         means the unscoped fallback was taken on the real `nros build` path:\n{text}"
    );
    Ok(())
}
