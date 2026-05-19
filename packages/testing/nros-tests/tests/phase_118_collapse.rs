//! Phase 118.A.2 — Verify the collapsed-shape `examples/native/rust/talker/`
//! builds against every advertised RMW and the test harness's
//! `build_native_talker_rmw(Rmw)` resolver finds the per-RMW binary.
//!
//! Runs as a build-only smoke test — no zenohd / no XRCE Agent
//! required. Each RMW variant is compiled out-of-band by
//! `just native build-fixtures` (which knows about
//! `--features rmw-X --target-dir target-X/`); this test only asserts
//! the binaries are present and have the expected name.
//!
//! Lifts the cell coverage to 3 (Zenoh + DDS + XRCE) for the
//! `native/rust/talker` example without spawning agents per RMW.
//! The pubsub / service / action runtime smoke tests cover one RMW
//! per scenario already (see `native_api.rs`).

use std::path::Path;

use nros_tests::fixtures::{Rmw, build_native_talker_rmw};
use rstest::rstest;

#[rstest]
#[case::zenoh(Rmw::Zenoh)]
#[case::dds(Rmw::Dds)]
#[case::xrce(Rmw::Xrce)]
fn test_native_talker_rmw_variant_exists(#[case] rmw: Rmw) {
    let binary = build_native_talker_rmw(rmw).unwrap_or_else(|e| {
        nros_tests::skip!(
            "native/rust/talker {:?} variant not prebuilt; run \
             `just native build-fixtures` first: {:?}",
            rmw,
            e
        )
    });

    let binary: &Path = binary;
    assert!(
        binary.exists(),
        "build_native_talker_rmw({:?}) returned a path that doesn't exist: {}",
        rmw,
        binary.display()
    );
    assert_eq!(
        binary.file_name().and_then(|n| n.to_str()),
        Some("talker"),
        "unexpected binary name for {:?}: {}",
        rmw,
        binary.display()
    );

    // Path contract: `examples/native/rust/talker/target-<rmw>/release/talker`.
    let parent = binary.parent().and_then(Path::parent);
    let target_dir_name = parent
        .and_then(|p| p.file_name())
        .and_then(|n| n.to_str())
        .unwrap_or("");
    assert_eq!(
        target_dir_name,
        rmw.target_dir(),
        "binary {} is not under the expected target-<rmw> dir for {:?}",
        binary.display(),
        rmw
    );
}
