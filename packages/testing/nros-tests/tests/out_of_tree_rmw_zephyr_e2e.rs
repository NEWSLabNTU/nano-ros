//! RFC-0071 D16 step 3 (issue 2372) — the out-of-tree RMW image of
//! `out_of_tree_rmw_e2e.rs`, on `board = "zephyr"` (native_sim), through
//! `builder/west_app.rs`.
//!
//! `fixtures/out-of-tree-rmw` declares `[image.zephyr]` (native_sim) with
//! `rmw = "acme"`, a backend provided by the workspace's own `src/acme_rmw`
//! and by nothing in the nano-ros tree. The west lane (row
//! `workspace-zephyr-out-of-tree-rmw` in `examples/fixtures.toml`) runs
//! `nros build demo_bringup:zephyr` on it; this test consumes the
//! artifact:
//!
//! * the west application the build generated depends on `acme-rmw` at the
//!   provider's own directory (D15 amended rulings 2 and 3: on an image with a
//!   west half, the west entry's backend dependency IS D15's, and its
//!   directory is `ResolvedRmwIn::package_dir`), read from the build output
//!   the artifact came from;
//! * the image boots on Zephyr, its samples go THROUGH `acme` (its publish
//!   slot logs [`OUT_OF_TREE_RMW_PUBLISH_MARKER`] through `nros_log`, which on
//!   native_sim lands on Zephyr's logger), and they reach a separate native
//!   listener over a router — `acme` delegates to zenoh, so it speaks zenoh's
//!   wire, over the zenoh-pico Zephyr port the module builds for
//!   `[rmw.zephyr] module_backend = "zenoh"`.
//!
//! The step-1 twin, `nros-cli-core/tests/out_of_tree_rmw_build.rs`, asserts the
//! same application and the module's Kconfig choice on a fresh copy without
//! compiling anything.
//!
//! [`OUT_OF_TREE_RMW_PUBLISH_MARKER`]: nros_tests::output::OUT_OF_TREE_RMW_PUBLISH_MARKER

use std::time::Duration;

use nros_tests::{
    count_pattern,
    fixtures::{
        ManagedProcess, RequireFixture, ZenohRouter, ZephyrPlatform, ZephyrProcess,
        build_native_listener, build_zephyr_out_of_tree_rmw_entry, or_unmet,
    },
    output, project_root,
};

const FIXTURE: &str = "packages/testing/nros-tests/fixtures/out-of-tree-rmw";

/// The west application the build stage generated for image `zephyr`
/// (`build/<coord>/zephyr_entry`), found by listing rather than spelled,
/// with the directory it sits in.
fn generated_west_application() -> (std::path::PathBuf, String) {
    let build = project_root().join(FIXTURE).join("build");
    let found: Vec<_> = std::fs::read_dir(&build)
        .unwrap_or_else(|e| panic!("the fixture build wrote no {}: {e}", build.display()))
        .filter_map(|e| e.ok())
        .map(|e| e.path().join("zephyr_entry"))
        .filter(|d| d.join("Cargo.toml").is_file() && d.join("CMakeLists.txt").is_file())
        .collect();
    assert_eq!(
        found.len(),
        1,
        "exactly one generated `zephyr_entry` west application under {}: {found:?}",
        build.display()
    );
    let text =
        std::fs::read_to_string(found[0].join("Cargo.toml")).expect("read the generated manifest");
    (found[0].clone(), text)
}

#[test]
fn a_workspace_rmw_backend_publishes_on_zephyr_through_the_west_road() {
    nros_tests::fixtures::lane::require_platform_in_lane(
        &[nros_tests::matrix::PlatformId::ZephyrNativeSim],
        "the out-of-tree RMW Zephyr image",
    );
    let entry =
        build_zephyr_out_of_tree_rmw_entry().require("prebuilt out-of-tree-rmw native_sim image");
    let listener_bin = build_native_listener().require("build native listener");

    // D15 on the west road — the application links the provider itself, by
    // path, at the directory the provider scan resolved.
    let (app, manifest) = generated_west_application();
    let doc: toml::Table = toml::from_str(&manifest).expect("the generated manifest parses");
    let acme = doc["dependencies"]
        .get("acme-rmw")
        .and_then(|d| d.as_table())
        .unwrap_or_else(|| {
            panic!("the west application does not depend on `acme-rmw`:\n{manifest}")
        });
    let path = acme
        .get("path")
        .and_then(|p| p.as_str())
        .unwrap_or_else(|| panic!("`acme-rmw` must be a path dependency:\n{manifest}"));
    assert_eq!(
        app.join(path)
            .canonicalize()
            .expect("the dependency's directory"),
        project_root()
            .join(FIXTURE)
            .join("src/acme_rmw")
            .canonicalize()
            .expect("the provider's directory"),
        "`acme-rmw` must point at the workspace provider:\n{manifest}"
    );

    let router = or_unmet(ZenohRouter::start_unique());
    let locator = router.locator();

    let mut listener_cmd = std::process::Command::new(listener_bin);
    listener_cmd
        .env("RUST_LOG", "info")
        .env("NROS_LOCATOR", &locator)
        .env("NROS_SESSION_MODE", "client");
    let mut listener =
        ManagedProcess::spawn_image(listener_cmd, "listener").expect("spawn listener");
    listener
        .wait_for_output_pattern(output::LISTENER_READY_MARKER, Duration::from_secs(8))
        .expect("listener did not become ready");

    // `start_with_locator` seeds the image (distinct `--seed` per process, the
    // native_sim GUID/port pitfall) and hands it this router at run time.
    let mut zephyr = ZephyrProcess::start_with_locator(&entry, ZephyrPlatform::NativeSim, &locator)
        .unwrap_or_else(|e| panic!("boot the acme native_sim image: {e}"));

    let seen = zephyr.wait_for_pattern(
        output::OUT_OF_TREE_RMW_PUBLISH_MARKER,
        Duration::from_secs(30),
    );
    let heard = listener
        .wait_for_output_count(output::LISTENER_LOG_PREFIX, 3, Duration::from_secs(20))
        .unwrap_or_else(|e| {
            panic!("the listener heard nothing from the `acme` Zephyr talker: {e}\nimage:\n{seen}")
        });

    zephyr.kill();
    listener.kill();

    assert!(
        seen.contains(output::OUT_OF_TREE_RMW_PUBLISH_MARKER),
        "the Zephyr talker never published through backend `acme`:\n{seen}"
    );
    let received = count_pattern(&heard, output::LISTENER_LOG_PREFIX);
    assert!(
        received >= 3,
        "the `acme` Zephyr talker's samples must reach a native listener over the \
         router (heard {received}):\n{heard}"
    );
}
