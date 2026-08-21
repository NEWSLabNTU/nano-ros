//! Issue 0466 — the Zephyr leaf-source staleness probe must see a `prj.conf`
//! edit, on a C entry as well as a Rust one.
//!
//! This is the regression the issue was filed on. `require_prebuilt_binary_
//! fresh_zephyr` used to watch only the cargo staticlib's `.d`, which lists the
//! Rust dependency closure and NOT the leaf's authored Kconfig — measured on
//! `build-ws-rs-qos-entry-zenoh`, the leaf's own `prj.conf` appeared zero times
//! among 529 deps, and the only `.conf` was the build's own generated one. A C
//! entry has no `librustapp.d` at all, so it had no freshness check whatsoever.
//! The consequence both times was a museum image failing with a plausible
//! PRODUCT-level assertion (`ros2 lifecycle nodes` listed no managed node),
//! which sent two sessions debugging code that was already fixed.
//!
//! The assertion drives the REAL resolvers, not the probe underneath them. The
//! probe (`source_dir_is_stale`) has worked since #147; the defect was that the
//! resolvers never called it. A test that reached past them would have passed on
//! the broken tree, which is the whole trap.

use nros_tests::{
    TestResult,
    fixtures::{
        Rmw, build_zephyr_cmake_example_rmw, build_zephyr_workspace_c_realtime_entry,
        build_zephyr_workspace_rust_realtime_entry,
    },
    skip,
};
use std::{fs, path::PathBuf};

/// A built `zephyr.exe` plus the leaf it came from. Returns `None` when the west
/// lane has not run here, which is the normal state on a host that never built
/// Zephyr fixtures.
fn built_leaf(build_dir: &str, leaf: &str) -> Option<(PathBuf, PathBuf)> {
    let root = nros_tests::project_root();
    let exe = root.join(format!("zephyr-workspace/{build_dir}/zephyr/zephyr.exe"));
    let src = root.join(leaf);
    (exe.is_file() && src.is_dir()).then_some((exe, src))
}

/// Editing a leaf's `prj.conf` must flip the verdict to stale, and restoring the
/// bytes must flip it back. Both directions matter: the first is the hole this
/// closes, the second is the #147 property that an mtime bump with unchanged
/// content is NOT stale (otherwise every pull reports the whole lane stale and
/// the verdict becomes noise nobody reads).
fn assert_conf_edit_is_seen(build_dir: &str, leaf: &str, resolve: fn() -> TestResult<PathBuf>) {
    let Some((_exe, src)) = built_leaf(build_dir, leaf) else {
        skip!("west lane has not built {build_dir} here — nothing to probe");
    };
    let conf = src.join("prj.conf");
    assert!(
        conf.is_file(),
        "{}: leaf has no prj.conf — the probe would watch nothing",
        conf.display()
    );
    let original = fs::read(&conf).expect("read prj.conf");

    // Drive the REAL resolver, not the probe underneath it. The bug was never
    // in the probe — `source_dir_is_stale` has worked since #147; it was that
    // the resolvers did not call it. A test that reached past them would have
    // passed on the broken tree.
    assert!(
        resolve().is_ok(),
        "{build_dir}: the resolver reports stale before any edit — the baseline \
         is wrong, so the mutation below would prove nothing"
    );

    let mut edited = original.clone();
    edited.extend_from_slice(b"\n# issue-0466 probe marker\n");
    fs::write(&conf, &edited).expect("write prj.conf");
    let saw_edit = resolve().is_err();
    fs::write(&conf, &original).expect("restore prj.conf");

    assert!(
        saw_edit,
        "{build_dir}: a prj.conf edit did NOT mark the image stale. That is \
         issue 0466 exactly: the image keeps the old Kconfig compiled in and the \
         next failure arrives as a product-level assertion."
    );
    assert!(
        resolve().is_ok(),
        "{build_dir}: still stale after restoring the original bytes — a \
         content-identical mtime bump must not be stale (#147), or every pull \
         reports the lane stale."
    );
}

/// phase-363 W5 — the shared cmake modules and the entry TEMPLATE are configure
/// inputs no hand-authored candidate list reaches. Ninja recorded them; the
/// probe reads that record. Without this half, editing the file that GENERATES
/// the entry TU left every Zephyr image reporting fresh.
#[test]
fn a_shared_cmake_input_marks_the_image_stale() {
    let root = nros_tests::project_root();
    let exe = root.join("zephyr-workspace/build-c-talker-cyclonedds/zephyr/zephyr.exe");
    if !exe.is_file() {
        skip!("build-c-talker-cyclonedds not built here — nothing to probe");
    }
    // A shared module every Zephyr configure reads, well outside the leaf.
    let shared = root.join("cmake/NanoRosCodegenCore.cmake");
    assert!(shared.is_file(), "{} vanished", shared.display());

    let resolve = || build_zephyr_cmake_example_rmw("c", "talker", Rmw::Cyclonedds);
    assert!(
        resolve().is_ok(),
        "stale before any edit — the mutation below would prove nothing"
    );

    let original = fs::read(&shared).expect("read");
    let mut edited = original.clone();
    edited.extend_from_slice(b"\n# phase-363 W5 probe\n");
    fs::write(&shared, &edited).expect("write");
    let saw = resolve().is_err();
    fs::write(&shared, &original).expect("restore");

    assert!(
        saw,
        "editing a shared cmake module did NOT mark the image stale — the \
         configure-input half is not reaching ninja's RERUN_CMAKE record"
    );
    assert!(
        resolve().is_ok(),
        "still stale after restoring the bytes — a content-identical write must \
         not be stale (#147)"
    );
}

#[test]
fn rust_entry_sees_a_prj_conf_edit() {
    assert_conf_edit_is_seen(
        "build-ws-rs-realtime-entry-zenoh",
        "examples/workspaces/realtime-rust/src/zephyr_entry",
        build_zephyr_workspace_rust_realtime_entry,
    );
}

/// The C half is the one that had NO check at all: `zephyr_staticlib_dep_file`
/// looks for `librustapp.d`, a C-only image has none, and the helper returned
/// `Ok` — "missing `.d` → existence-only fallback".
#[test]
fn c_entry_sees_a_prj_conf_edit() {
    assert_conf_edit_is_seen(
        "build-ws-c-realtime-entry-zenoh",
        "examples/workspaces/realtime-c/src/zephyr_entry",
        build_zephyr_workspace_c_realtime_entry,
    );
}
