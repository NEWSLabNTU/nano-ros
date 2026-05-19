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
    Rmw, build_native_c_example_rmw, build_native_c_talker_rmw,
    build_native_cpp_example_rmw, build_native_listener_rmw,
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

/// Phase 118.A.3 / 118.B.2 — collapsed-shape C talker, RMW-parametrized.
/// The canonical zenoh-style main.c uses the typed `std_msgs_msg_int32_publish`
/// binding and builds under all three RMWs — XRCE is no longer deferred,
/// the legacy `c/xrce/<case>/` siblings' manual-CDR variant is redundant
/// and gets dropped in Tier 5 cleanup.
#[rstest]
#[case::zenoh(Rmw::Zenoh)]
#[case::dds(Rmw::Dds)]
#[case::xrce(Rmw::Xrce)]
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

/// Phase 118.B.2 — collapsed-shape native C listener / service /
/// action cases. XRCE included — the canonical zenoh-style main.c
/// uses the typed bindings and builds under all three RMWs.
#[rstest]
#[case::listener_zenoh("listener", "c_listener", Rmw::Zenoh)]
#[case::listener_dds("listener", "c_listener", Rmw::Dds)]
#[case::listener_xrce("listener", "c_listener", Rmw::Xrce)]
#[case::ss_zenoh("service-server", "c_service_server", Rmw::Zenoh)]
#[case::ss_dds("service-server", "c_service_server", Rmw::Dds)]
#[case::ss_xrce("service-server", "c_service_server", Rmw::Xrce)]
#[case::sc_zenoh("service-client", "c_service_client", Rmw::Zenoh)]
#[case::sc_dds("service-client", "c_service_client", Rmw::Dds)]
#[case::sc_xrce("service-client", "c_service_client", Rmw::Xrce)]
#[case::as_zenoh("action-server", "c_action_server", Rmw::Zenoh)]
#[case::as_dds("action-server", "c_action_server", Rmw::Dds)]
#[case::as_xrce("action-server", "c_action_server", Rmw::Xrce)]
#[case::ac_zenoh("action-client", "c_action_client", Rmw::Zenoh)]
#[case::ac_dds("action-client", "c_action_client", Rmw::Dds)]
#[case::ac_xrce("action-client", "c_action_client", Rmw::Xrce)]
fn test_native_c_listener_service_action_rmw_variant_exists(
    #[case] case: &str,
    #[case] binary: &str,
    #[case] rmw: Rmw,
) {
    let path = build_native_c_example_rmw(case, binary, rmw).unwrap_or_else(|e| {
        nros_tests::skip!(
            "native/c/{} {:?} variant not prebuilt; run \
             `just native build-fixtures` first: {:?}",
            case,
            rmw,
            e
        )
    });
    assert!(
        path.exists(),
        "{} {:?} binary missing: {}",
        case,
        rmw,
        path.display()
    );
    assert_eq!(
        path.file_name().and_then(|n| n.to_str()),
        Some(binary),
        "unexpected binary name for {} {:?}: {}",
        case,
        rmw,
        path.display()
    );
}

/// Phase 118.B.3 — collapsed-shape native C++ examples. Six cases,
/// three RMWs. Mirror of the C path; typed nros-cpp binding works
/// across all three RMWs without per-RMW source changes.
#[rstest]
#[case::talker_zenoh("talker", "cpp_talker", Rmw::Zenoh)]
#[case::talker_dds("talker", "cpp_talker", Rmw::Dds)]
#[case::talker_xrce("talker", "cpp_talker", Rmw::Xrce)]
#[case::listener_zenoh("listener", "cpp_listener", Rmw::Zenoh)]
#[case::listener_dds("listener", "cpp_listener", Rmw::Dds)]
#[case::listener_xrce("listener", "cpp_listener", Rmw::Xrce)]
#[case::ss_zenoh("service-server", "cpp_service_server", Rmw::Zenoh)]
#[case::ss_dds("service-server", "cpp_service_server", Rmw::Dds)]
#[case::ss_xrce("service-server", "cpp_service_server", Rmw::Xrce)]
#[case::sc_zenoh("service-client", "cpp_service_client", Rmw::Zenoh)]
#[case::sc_dds("service-client", "cpp_service_client", Rmw::Dds)]
#[case::sc_xrce("service-client", "cpp_service_client", Rmw::Xrce)]
#[case::as_zenoh("action-server", "cpp_action_server", Rmw::Zenoh)]
#[case::as_dds("action-server", "cpp_action_server", Rmw::Dds)]
#[case::as_xrce("action-server", "cpp_action_server", Rmw::Xrce)]
#[case::ac_zenoh("action-client", "cpp_action_client", Rmw::Zenoh)]
#[case::ac_dds("action-client", "cpp_action_client", Rmw::Dds)]
#[case::ac_xrce("action-client", "cpp_action_client", Rmw::Xrce)]
fn test_native_cpp_rmw_variant_exists(
    #[case] case: &str,
    #[case] binary: &str,
    #[case] rmw: Rmw,
) {
    let path = build_native_cpp_example_rmw(case, binary, rmw).unwrap_or_else(|e| {
        nros_tests::skip!(
            "native/cpp/{} {:?} variant not prebuilt; run \
             `just native build-fixtures` first: {:?}",
            case,
            rmw,
            e
        )
    });
    assert!(
        path.exists(),
        "{} {:?} binary missing: {}",
        case,
        rmw,
        path.display()
    );
    assert_eq!(
        path.file_name().and_then(|n| n.to_str()),
        Some(binary),
        "unexpected binary name for {} {:?}: {}",
        case,
        rmw,
        path.display()
    );
}

/// Phase 118.B.4 — collapsed-shape FreeRTOS Rust talker. Single
/// `examples/qemu-arm-freertos/rust/talker/` builds against zenoh +
/// dds via Cargo features. DDS-only build adds `extern crate alloc`
/// + the `nros-platform-critical-section` registration; zenoh path
/// stays exactly as before. Same `--target-dir` isolation pattern.
#[rstest]
#[case::zenoh(Rmw::Zenoh)]
#[case::dds(Rmw::Dds)]
fn test_freertos_talker_rmw_variant_exists(#[case] rmw: Rmw) {
    let path = nros_tests::fixtures::build_freertos_rust_example_rmw(
        "talker",
        "qemu-freertos-talker",
        rmw,
    )
    .unwrap_or_else(|e| {
        nros_tests::skip!(
            "qemu-arm-freertos/rust/talker {:?} variant not prebuilt; run \
             `just freertos build-fixtures` first: {:?}",
            rmw,
            e
        )
    });
    assert!(
        path.exists(),
        "FreeRTOS talker {:?} binary missing: {}",
        rmw,
        path.display()
    );
}

/// Phase 118.B.4 — full FreeRTOS Rust collapse coverage (6 cases).
/// talker + listener support {zenoh, dds}; service-* + action-*
/// are zenoh-only (no pre-collapse DDS sibling).
#[rstest]
#[case::talker_zenoh("talker", "qemu-freertos-talker", Rmw::Zenoh)]
#[case::talker_dds("talker", "qemu-freertos-talker", Rmw::Dds)]
#[case::listener_zenoh("listener", "qemu-freertos-listener", Rmw::Zenoh)]
#[case::listener_dds("listener", "qemu-freertos-listener", Rmw::Dds)]
#[case::ss_zenoh("service-server", "qemu-freertos-service-server", Rmw::Zenoh)]
#[case::sc_zenoh("service-client", "qemu-freertos-service-client", Rmw::Zenoh)]
#[case::as_zenoh("action-server", "qemu-freertos-action-server", Rmw::Zenoh)]
#[case::ac_zenoh("action-client", "qemu-freertos-action-client", Rmw::Zenoh)]
fn test_freertos_rust_case_rmw_variant_exists(
    #[case] case: &str,
    #[case] binary: &str,
    #[case] rmw: Rmw,
) {
    let path = nros_tests::fixtures::build_freertos_rust_example_rmw(case, binary, rmw)
        .unwrap_or_else(|e| {
            nros_tests::skip!(
                "qemu-arm-freertos/rust/{} {:?} variant not prebuilt; run \
                 `just freertos build-fixtures` first: {:?}",
                case,
                rmw,
                e
            )
        });
    assert!(
        path.exists(),
        "freertos {} {:?} binary missing: {}",
        case,
        rmw,
        path.display()
    );
}

/// Phase 118.B.4 — FreeRTOS C / C++ collapsed cases. Zenoh only on
/// FreeRTOS for C / C++ (no pre-collapse DDS C / C++ siblings).
#[rstest]
#[case::c_talker("c", "talker", "freertos_c_talker")]
#[case::c_listener("c", "listener", "freertos_c_listener")]
#[case::c_ss("c", "service-server", "freertos_c_service_server")]
#[case::c_sc("c", "service-client", "freertos_c_service_client")]
#[case::c_as("c", "action-server", "freertos_c_action_server")]
#[case::c_ac("c", "action-client", "freertos_c_action_client")]
#[case::cpp_talker("cpp", "talker", "freertos_cpp_talker")]
#[case::cpp_listener("cpp", "listener", "freertos_cpp_listener")]
#[case::cpp_ss("cpp", "service-server", "freertos_cpp_service_server")]
#[case::cpp_sc("cpp", "service-client", "freertos_cpp_service_client")]
#[case::cpp_as("cpp", "action-server", "freertos_cpp_action_server")]
#[case::cpp_ac("cpp", "action-client", "freertos_cpp_action_client")]
fn test_freertos_cmake_case_rmw_variant_exists(
    #[case] lang: &str,
    #[case] case: &str,
    #[case] binary: &str,
) {
    let path = nros_tests::fixtures::build_freertos_cmake_example_rmw(
        lang, case, binary, Rmw::Zenoh,
    )
    .unwrap_or_else(|e| {
        nros_tests::skip!(
            "qemu-arm-freertos/{}/{} zenoh variant not prebuilt; run \
             `just freertos build-fixtures` first: {:?}",
            lang,
            case,
            e
        )
    });
    assert!(
        path.exists(),
        "freertos {}/{} zenoh binary missing: {}",
        lang,
        case,
        path.display()
    );
}
