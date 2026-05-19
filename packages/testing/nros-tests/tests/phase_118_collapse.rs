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

use nros_tests::fixtures::{
    Rmw, build_native_c_talker_rmw, build_native_listener_rmw,
    build_native_rust_example_rmw, build_native_talker_rmw,
};
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

/// Phase 118.B.1 — collapsed-shape native Rust listener.
#[rstest]
#[case::zenoh(Rmw::Zenoh)]
#[case::dds(Rmw::Dds)]
#[case::xrce(Rmw::Xrce)]
fn test_native_listener_rmw_variant_exists(#[case] rmw: Rmw) {
    let binary = build_native_listener_rmw(rmw).unwrap_or_else(|e| {
        nros_tests::skip!(
            "native/rust/listener {:?} variant not prebuilt; run \
             `just native build-fixtures` first: {:?}",
            rmw,
            e
        )
    });

    let binary: &Path = binary;
    assert!(
        binary.exists(),
        "build_native_listener_rmw({:?}) returned a path that doesn't exist: {}",
        rmw,
        binary.display()
    );
    assert_eq!(
        binary.file_name().and_then(|n| n.to_str()),
        Some("listener"),
        "unexpected binary name for {:?}: {}",
        rmw,
        binary.display()
    );
}

/// Phase 118.B.1 — service-{server,client} + action-{server,client}
/// on native/rust. Same collapse mechanism as talker / listener; one
/// parametrized test covers all 4 cases × 3 RMWs.
#[rstest]
#[case::ss_zenoh("service-server", Rmw::Zenoh)]
#[case::ss_dds("service-server", Rmw::Dds)]
#[case::ss_xrce("service-server", Rmw::Xrce)]
#[case::sc_zenoh("service-client", Rmw::Zenoh)]
#[case::sc_dds("service-client", Rmw::Dds)]
#[case::sc_xrce("service-client", Rmw::Xrce)]
#[case::as_zenoh("action-server", Rmw::Zenoh)]
#[case::as_dds("action-server", Rmw::Dds)]
#[case::as_xrce("action-server", Rmw::Xrce)]
#[case::ac_zenoh("action-client", Rmw::Zenoh)]
#[case::ac_dds("action-client", Rmw::Dds)]
#[case::ac_xrce("action-client", Rmw::Xrce)]
fn test_native_service_action_rmw_variant_exists(#[case] case: &str, #[case] rmw: Rmw) {
    let binary = build_native_rust_example_rmw(case, case, rmw).unwrap_or_else(|e| {
        nros_tests::skip!(
            "native/rust/{} {:?} variant not prebuilt; run \
             `just native build-fixtures` first: {:?}",
            case,
            rmw,
            e
        )
    });

    assert!(
        binary.exists(),
        "{} {:?} binary missing: {}",
        case,
        rmw,
        binary.display()
    );
    assert_eq!(
        binary.file_name().and_then(|n| n.to_str()),
        Some(case),
        "unexpected binary name for {} {:?}: {}",
        case,
        rmw,
        binary.display()
    );
}

/// Phase 118.A.3 — collapsed-shape C talker. XRCE deferred (main.c
/// differs significantly across RMWs in the legacy `c/xrce/talker/`
/// — manual CDR serialization vs the canonical std_msgs binding —
/// so Tier 2 owns that port).
#[rstest]
#[case::zenoh(Rmw::Zenoh)]
#[case::dds(Rmw::Dds)]
fn test_native_c_talker_rmw_variant_exists(#[case] rmw: Rmw) {
    let binary = build_native_c_talker_rmw(rmw).unwrap_or_else(|e| {
        nros_tests::skip!(
            "native/c/talker {:?} variant not prebuilt; run \
             `just native build-fixtures` first: {:?}",
            rmw,
            e
        )
    });

    let binary: &Path = binary;
    assert!(
        binary.exists(),
        "build_native_c_talker_rmw({:?}) returned a path that doesn't exist: {}",
        rmw,
        binary.display()
    );
    assert_eq!(
        binary.file_name().and_then(|n| n.to_str()),
        Some("c_talker"),
        "unexpected binary name for {:?}: {}",
        rmw,
        binary.display()
    );

    // Path contract: `examples/native/c/talker/build-<rmw>/c_talker`.
    let build_dir_name = binary
        .parent()
        .and_then(|p| p.file_name())
        .and_then(|n| n.to_str())
        .unwrap_or("");
    assert_eq!(
        build_dir_name,
        rmw.build_dir(),
        "binary {} is not under the expected build-<rmw> dir for {:?}",
        binary.display(),
        rmw
    );
}
