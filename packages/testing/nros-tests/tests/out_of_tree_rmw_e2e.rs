//! RFC-0071 D16 step 2 (phase-545 W3, issue 2295) — an RMW backend nano-ros
//! does not know about, built by `nros build` and run against a router.
//!
//! `fixtures/out-of-tree-rmw` declares one image, `board = "linux"`,
//! `rmw = "acme"`, where `acme` is provided by the workspace's own
//! `src/acme_rmw` and by nothing in the nano-ros tree. The build stage
//! (`workspace-out-of-tree-rmw` in `examples/fixtures.toml`) runs `nros build`
//! on it with no edit outside the fixture; this test runs the artifact:
//!
//! * the build's own generated entry links `acme-rmw` by path (D15: the
//!   `linux` board has no `rmw-acme` feature), read from the build output the
//!   artifact came from;
//! * the image boots on backend `acme` with no `$NROS_RMW` (the selector is
//!   baked for an image that registers its own backend);
//! * the talker's samples go THROUGH `acme` (its publish slot prints
//!   [`OUT_OF_TREE_RMW_PUBLISH_MARKER`]) and reach a separate native listener
//!   over the router — `acme` delegates to zenoh, so it speaks zenoh's wire.
//!
//! The step-1 twin, `nros-cli-core/tests/out_of_tree_rmw_build.rs`, asserts
//! the same entry on a fresh copy without compiling anything.
//!
//! [`OUT_OF_TREE_RMW_PUBLISH_MARKER`]: nros_tests::output::OUT_OF_TREE_RMW_PUBLISH_MARKER

use std::{process::Command, time::Duration};

use nros_tests::{
    count_pattern,
    fixtures::{
        ManagedProcess, RequireFixture, ZenohRouter, build_native_listener,
        build_native_out_of_tree_rmw_entry, or_unmet,
    },
    output, project_root,
};

const FIXTURE: &str = "packages/testing/nros-tests/fixtures/out-of-tree-rmw";

/// The generated entry the build stage wrote for image `linux`
/// (`build/<coord>/linux_entry`), found by listing rather than spelled.
fn generated_entry_manifest() -> String {
    let build = project_root().join(FIXTURE).join("build");
    let found: Vec<_> = std::fs::read_dir(&build)
        .unwrap_or_else(|e| panic!("the fixture build wrote no {}: {e}", build.display()))
        .filter_map(|e| e.ok())
        .map(|e| e.path().join("linux_entry/Cargo.toml"))
        .filter(|p| p.is_file())
        .collect();
    assert_eq!(
        found.len(),
        1,
        "exactly one generated `linux_entry` under {}: {found:?}",
        build.display()
    );
    std::fs::read_to_string(&found[0]).expect("read the generated manifest")
}

#[test]
fn a_workspace_rmw_backend_publishes_through_nros_build() {
    let entry =
        build_native_out_of_tree_rmw_entry().require("prebuilt out-of-tree-rmw linux_entry");
    let listener_bin = build_native_listener().require("build native listener");

    // D15 — the build linked the provider itself, by path.
    let manifest = generated_entry_manifest();
    let doc: toml::Table = toml::from_str(&manifest).expect("the generated manifest parses");
    let acme = doc["dependencies"]
        .get("acme-rmw")
        .and_then(|d| d.as_table())
        .unwrap_or_else(|| {
            panic!("the generated entry does not depend on `acme-rmw`:\n{manifest}")
        });
    assert!(
        acme.get("path")
            .and_then(|p| p.as_str())
            .is_some_and(|p| p.ends_with("src/acme_rmw")),
        "`acme-rmw` must be a path dependency on the workspace provider:\n{manifest}"
    );

    let router = or_unmet(ZenohRouter::start_unique());
    let locator = router.locator();

    let mut listener_cmd = Command::new(listener_bin);
    listener_cmd
        .env("RUST_LOG", "info")
        .env("NROS_LOCATOR", &locator)
        .env("NROS_SESSION_MODE", "client");
    let mut listener =
        ManagedProcess::spawn_image(listener_cmd, "listener").expect("spawn listener");
    listener
        .wait_for_output_pattern(output::LISTENER_READY_MARKER, Duration::from_secs(8))
        .expect("listener did not become ready");

    // No `$NROS_RMW`: the image selects the backend it declared.
    let mut talker_cmd = Command::new(&entry);
    talker_cmd
        .env("RUST_LOG", "info")
        .env("NROS_LOCATOR", &locator)
        .env("NROS_SESSION_MODE", "client")
        .env_remove("NROS_RMW")
        .env("NROS_ENTRY_SPIN_MS", "10000")
        .env("NROS_ENTRY_SPIN_STEP_MS", "10");
    let mut talker = ManagedProcess::spawn_image(talker_cmd, "acme_talker").expect("spawn talker");

    talker
        .wait_for_output_pattern(
            output::OUT_OF_TREE_RMW_PUBLISH_MARKER,
            Duration::from_secs(15),
        )
        .unwrap_or_else(|e| panic!("the talker never published through backend `acme`: {e}"));
    let heard = listener
        .wait_for_output_count(output::LISTENER_LOG_PREFIX, 3, Duration::from_secs(15))
        .unwrap_or_else(|e| panic!("the listener heard nothing from the `acme` talker: {e}"));

    talker.kill();
    listener.kill();

    let received = count_pattern(&heard, output::LISTENER_LOG_PREFIX);
    assert!(
        received >= 3,
        "the `acme` talker's samples must reach a cross-process listener over the \
         router (heard {received}):\n{heard}"
    );
}
