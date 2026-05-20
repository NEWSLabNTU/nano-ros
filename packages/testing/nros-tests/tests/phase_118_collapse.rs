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

/// Phase 118.B.5 — NuttX C / C++ collapsed cases. Zenoh only on
/// NuttX C / C++ (no pre-collapse DDS C / C++ NuttX siblings).
/// Rust collapse for NuttX is deferred — `cargo build` on the
/// collapsed 4-segment path hits a libgloss / newlib crt0 link
/// issue that the 5-segment zenoh sibling avoids.
#[rstest]
#[case::c_talker("c", "talker", "nuttx_c_talker")]
#[case::c_listener("c", "listener", "nuttx_c_listener")]
#[case::c_ss("c", "service-server", "nuttx_c_service_server")]
#[case::c_sc("c", "service-client", "nuttx_c_service_client")]
#[case::c_as("c", "action-server", "nuttx_c_action_server")]
#[case::c_ac("c", "action-client", "nuttx_c_action_client")]
#[case::cpp_talker("cpp", "talker", "nuttx_cpp_talker")]
#[case::cpp_listener("cpp", "listener", "nuttx_cpp_listener")]
#[case::cpp_ss("cpp", "service-server", "nuttx_cpp_service_server")]
#[case::cpp_sc("cpp", "service-client", "nuttx_cpp_service_client")]
#[case::cpp_as("cpp", "action-server", "nuttx_cpp_action_server")]
#[case::cpp_ac("cpp", "action-client", "nuttx_cpp_action_client")]
fn test_nuttx_cmake_case_rmw_variant_exists(
    #[case] lang: &str,
    #[case] case: &str,
    #[case] binary: &str,
) {
    let path = nros_tests::fixtures::build_nuttx_cmake_example_rmw(
        lang, case, binary, Rmw::Zenoh,
    )
    .unwrap_or_else(|e| {
        nros_tests::skip!(
            "qemu-arm-nuttx/{}/{} zenoh variant not prebuilt; run \
             `just nuttx build-fixtures` first: {:?}",
            lang,
            case,
            e
        )
    });
    assert!(
        path.exists(),
        "nuttx {}/{} zenoh binary missing: {}",
        lang,
        case,
        path.display()
    );
}

/// Phase 118.B.6 — ThreadX-RV64 Rust cases. talker + listener
/// support {zenoh, dds}; service-* + action-* are zenoh-only.
#[rstest]
#[case::talker_zenoh("talker", "qemu-riscv64-threadx-talker", Rmw::Zenoh)]
#[case::talker_dds("talker", "qemu-riscv64-threadx-talker", Rmw::Dds)]
#[case::listener_zenoh("listener", "qemu-riscv64-threadx-listener", Rmw::Zenoh)]
#[case::listener_dds("listener", "qemu-riscv64-threadx-listener", Rmw::Dds)]
#[case::ss_zenoh("service-server", "qemu-riscv64-threadx-service-server", Rmw::Zenoh)]
#[case::sc_zenoh("service-client", "qemu-riscv64-threadx-service-client", Rmw::Zenoh)]
#[case::as_zenoh("action-server", "qemu-riscv64-threadx-action-server", Rmw::Zenoh)]
#[case::ac_zenoh("action-client", "qemu-riscv64-threadx-action-client", Rmw::Zenoh)]
fn test_threadx_rv64_rust_case_rmw_variant_exists(
    #[case] case: &str,
    #[case] binary: &str,
    #[case] rmw: Rmw,
) {
    let path = nros_tests::fixtures::build_threadx_rv64_rust_example_rmw(case, binary, rmw)
        .unwrap_or_else(|e| {
            nros_tests::skip!(
                "qemu-riscv64-threadx/rust/{} {:?} not prebuilt: {:?}",
                case,
                rmw,
                e
            )
        });
    assert!(path.exists(), "threadx-rv64 {} {:?} missing: {}", case, rmw, path.display());
}

/// Phase 118.B.6 — ThreadX-RV64 C / C++ cases (zenoh only).
#[rstest]
#[case::c_talker("c", "talker", "riscv64_threadx_c_talker")]
#[case::c_listener("c", "listener", "riscv64_threadx_c_listener")]
#[case::c_ss("c", "service-server", "riscv64_threadx_c_service_server")]
#[case::c_sc("c", "service-client", "riscv64_threadx_c_service_client")]
#[case::c_as("c", "action-server", "riscv64_threadx_c_action_server")]
#[case::c_ac("c", "action-client", "riscv64_threadx_c_action_client")]
#[case::cpp_talker("cpp", "talker", "riscv64_threadx_cpp_talker")]
#[case::cpp_listener("cpp", "listener", "riscv64_threadx_cpp_listener")]
#[case::cpp_ss("cpp", "service-server", "riscv64_threadx_cpp_service_server")]
#[case::cpp_sc("cpp", "service-client", "riscv64_threadx_cpp_service_client")]
#[case::cpp_as("cpp", "action-server", "riscv64_threadx_cpp_action_server")]
#[case::cpp_ac("cpp", "action-client", "riscv64_threadx_cpp_action_client")]
fn test_threadx_rv64_cmake_case_rmw_variant_exists(
    #[case] lang: &str,
    #[case] case: &str,
    #[case] binary: &str,
) {
    let path = nros_tests::fixtures::build_threadx_rv64_cmake_example_rmw(
        lang, case, binary, Rmw::Zenoh,
    ).unwrap_or_else(|e| {
        nros_tests::skip!(
            "qemu-riscv64-threadx/{}/{} zenoh not prebuilt: {:?}", lang, case, e
        )
    });
    assert!(path.exists(), "threadx-rv64 {}/{} zenoh missing: {}", lang, case, path.display());
}

/// Phase 118.B.7 — ThreadX-Linux Rust cases.
#[rstest]
#[case::talker_zenoh("talker", "threadx-linux-talker", Rmw::Zenoh)]
#[case::talker_dds("talker", "threadx-linux-talker", Rmw::Dds)]
#[case::listener_zenoh("listener", "threadx-linux-listener", Rmw::Zenoh)]
#[case::listener_dds("listener", "threadx-linux-listener", Rmw::Dds)]
#[case::ss_zenoh("service-server", "threadx-linux-service-server", Rmw::Zenoh)]
#[case::sc_zenoh("service-client", "threadx-linux-service-client", Rmw::Zenoh)]
#[case::as_zenoh("action-server", "threadx-linux-action-server", Rmw::Zenoh)]
#[case::ac_zenoh("action-client", "threadx-linux-action-client", Rmw::Zenoh)]
fn test_threadx_linux_rust_case_rmw_variant_exists(
    #[case] case: &str,
    #[case] binary: &str,
    #[case] rmw: Rmw,
) {
    let path = nros_tests::fixtures::build_threadx_linux_rust_example_rmw(case, binary, rmw)
        .unwrap_or_else(|e| {
            nros_tests::skip!(
                "threadx-linux/rust/{} {:?} not prebuilt: {:?}",
                case,
                rmw,
                e
            )
        });
    assert!(path.exists(), "threadx-linux {} {:?} missing: {}", case, rmw, path.display());
}

/// Phase 168.3 — Zephyr Rust cases.
///
/// `build_zephyr_rust_example_rmw` resolves
/// `zephyr-workspace/build-rs-<case>-<rmw>/zephyr/zephyr.exe`.
/// Build orchestration lives in `just/zephyr.just :: build-fixtures`.
#[rstest]
#[case::talker_zenoh("talker", Rmw::Zenoh)]
#[case::talker_xrce("talker", Rmw::Xrce)]
#[case::listener_zenoh("listener", Rmw::Zenoh)]
#[case::listener_xrce("listener", Rmw::Xrce)]
#[case::ss_zenoh("service-server", Rmw::Zenoh)]
#[case::ss_xrce("service-server", Rmw::Xrce)]
#[case::sc_zenoh("service-client", Rmw::Zenoh)]
#[case::sc_xrce("service-client", Rmw::Xrce)]
#[case::as_zenoh("action-server", Rmw::Zenoh)]
#[case::as_xrce("action-server", Rmw::Xrce)]
#[case::ac_zenoh("action-client", Rmw::Zenoh)]
#[case::ac_xrce("action-client", Rmw::Xrce)]
#[case::sca_zenoh("service-client-async", Rmw::Zenoh)]
// Phase 11W — Cyclone DDS via Phase 169.5 `nros-rmw-cyclonedds-sys`.
#[case::talker_cyclonedds("talker", Rmw::Cyclonedds)]
#[case::listener_cyclonedds("listener", Rmw::Cyclonedds)]
#[case::ss_cyclonedds("service-server", Rmw::Cyclonedds)]
#[case::sc_cyclonedds("service-client", Rmw::Cyclonedds)]
#[case::as_cyclonedds("action-server", Rmw::Cyclonedds)]
#[case::ac_cyclonedds("action-client", Rmw::Cyclonedds)]
fn test_zephyr_rust_case_rmw_variant_exists(
    #[case] case: &str,
    #[case] rmw: Rmw,
) {
    let path = nros_tests::fixtures::build_zephyr_rust_example_rmw(case, rmw)
        .unwrap_or_else(|e| {
            nros_tests::skip!(
                "zephyr/rust/{} {:?} not prebuilt: {:?}",
                case, rmw, e
            )
        });
    assert!(path.exists(), "zephyr {} {:?} missing: {}", case, rmw, path.display());
}

/// Phase 168.4 — Zephyr C / C++ cases (zenoh + xrce verified; dds +
/// cpp deferred to Phase 168.X — see
/// `docs/roadmap/phase-168-X-zephyr-cmake-build-gaps.md`).
#[rstest]
// C × {zenoh, xrce}.
#[case::c_talker_zenoh("c", "talker", Rmw::Zenoh)]
#[case::c_talker_xrce("c", "talker", Rmw::Xrce)]
#[case::c_listener_zenoh("c", "listener", Rmw::Zenoh)]
#[case::c_listener_xrce("c", "listener", Rmw::Xrce)]
#[case::c_ss_zenoh("c", "service-server", Rmw::Zenoh)]
#[case::c_ss_xrce("c", "service-server", Rmw::Xrce)]
#[case::c_sc_zenoh("c", "service-client", Rmw::Zenoh)]
#[case::c_sc_xrce("c", "service-client", Rmw::Xrce)]
#[case::c_as_zenoh("c", "action-server", Rmw::Zenoh)]
#[case::c_as_xrce("c", "action-server", Rmw::Xrce)]
#[case::c_ac_zenoh("c", "action-client", Rmw::Zenoh)]
#[case::c_ac_xrce("c", "action-client", Rmw::Xrce)]
// Phase 168.X gap 1 unblocked C++ × {zenoh, xrce}.
#[case::cpp_talker_zenoh("cpp", "talker", Rmw::Zenoh)]
#[case::cpp_talker_xrce("cpp", "talker", Rmw::Xrce)]
#[case::cpp_listener_zenoh("cpp", "listener", Rmw::Zenoh)]
#[case::cpp_listener_xrce("cpp", "listener", Rmw::Xrce)]
#[case::cpp_ss_zenoh("cpp", "service-server", Rmw::Zenoh)]
#[case::cpp_ss_xrce("cpp", "service-server", Rmw::Xrce)]
#[case::cpp_sc_zenoh("cpp", "service-client", Rmw::Zenoh)]
#[case::cpp_sc_xrce("cpp", "service-client", Rmw::Xrce)]
#[case::cpp_as_zenoh("cpp", "action-server", Rmw::Zenoh)]
#[case::cpp_as_xrce("cpp", "action-server", Rmw::Xrce)]
#[case::cpp_ac_zenoh("cpp", "action-client", Rmw::Zenoh)]
#[case::cpp_ac_xrce("cpp", "action-client", Rmw::Xrce)]
// Phase 11W — Cyclone DDS C + C++ on native_sim. Compile + link
// path unblocked by llext-edk patch + cxx-compat shims + DDS_HAS_*
// `#ifdef` fixes + link stubs.
#[case::c_talker_cyclonedds("c", "talker", Rmw::Cyclonedds)]
#[case::c_listener_cyclonedds("c", "listener", Rmw::Cyclonedds)]
#[case::c_ss_cyclonedds("c", "service-server", Rmw::Cyclonedds)]
#[case::c_sc_cyclonedds("c", "service-client", Rmw::Cyclonedds)]
#[case::c_as_cyclonedds("c", "action-server", Rmw::Cyclonedds)]
#[case::c_ac_cyclonedds("c", "action-client", Rmw::Cyclonedds)]
#[case::cpp_talker_cyclonedds("cpp", "talker", Rmw::Cyclonedds)]
#[case::cpp_listener_cyclonedds("cpp", "listener", Rmw::Cyclonedds)]
#[case::cpp_ss_cyclonedds("cpp", "service-server", Rmw::Cyclonedds)]
#[case::cpp_sc_cyclonedds("cpp", "service-client", Rmw::Cyclonedds)]
#[case::cpp_as_cyclonedds("cpp", "action-server", Rmw::Cyclonedds)]
#[case::cpp_ac_cyclonedds("cpp", "action-client", Rmw::Cyclonedds)]
fn test_zephyr_cmake_case_rmw_variant_exists(
    #[case] lang: &str,
    #[case] case: &str,
    #[case] rmw: Rmw,
) {
    let path = nros_tests::fixtures::build_zephyr_cmake_example_rmw(lang, case, rmw)
        .unwrap_or_else(|e| {
            nros_tests::skip!(
                "zephyr/{}/{} {:?} not prebuilt: {:?}",
                lang, case, rmw, e
            )
        });
    assert!(path.exists(), "zephyr {}/{} {:?} missing: {}", lang, case, rmw, path.display());
}

/// Phase 11W.9/.10 — runtime smoke for the cyclonedds native_sim Rust
/// talker. After 11W.10 the participant inits and the talker publishes
/// std_msgs/Int32 at 1 Hz, so assert an actual `Published:` line (not
/// just the boot banner).
#[test]
fn test_zephyr_rust_talker_cyclonedds_boot() {
    use std::time::Duration;

    use nros_tests::zephyr::{ZephyrPlatform, ZephyrProcess};

    let path = nros_tests::fixtures::build_zephyr_rust_example_rmw(
        "talker",
        Rmw::Cyclonedds,
    )
    .unwrap_or_else(|e| {
        nros_tests::skip!("zephyr/rust/talker cyclonedds not prebuilt: {:?}", e)
    });

    let mut z = ZephyrProcess::start(&path, ZephyrPlatform::NativeSim)
        .expect("spawn zephyr talker (cyclonedds)");

    // 1 Hz timer — first publish lands ~1.1 s in; allow margin.
    let output = z
        .wait_for_output(Duration::from_secs(4))
        .unwrap_or_default();

    eprintln!("zephyr cyclonedds talker output:\n{}", output);

    assert!(
        output.contains("Booting Zephyr") || output.contains("nros"),
        "cyclonedds talker failed to print init banner"
    );
    assert!(
        output.contains("Published:"),
        "cyclonedds talker did not publish (expected a `Published:` line)"
    );
}

/// Phase 11W.9/.10 — runtime smoke for the cyclonedds native_sim Rust
/// listener. Asserts the participant + subscription init cleanly
/// (reaches the "Waiting for messages" log) without aborting.
#[test]
fn test_zephyr_rust_listener_cyclonedds_boot() {
    use std::time::Duration;

    use nros_tests::zephyr::{ZephyrPlatform, ZephyrProcess};

    let path = nros_tests::fixtures::build_zephyr_rust_example_rmw(
        "listener",
        Rmw::Cyclonedds,
    )
    .unwrap_or_else(|e| {
        nros_tests::skip!("zephyr/rust/listener cyclonedds not prebuilt: {:?}", e)
    });

    let mut z = ZephyrProcess::start(&path, ZephyrPlatform::NativeSim)
        .expect("spawn zephyr listener (cyclonedds)");

    let output = z
        .wait_for_output(Duration::from_secs(3))
        .unwrap_or_default();

    eprintln!("zephyr cyclonedds listener output:\n{}", output);

    assert!(
        output.contains("Booting Zephyr") || output.contains("nros"),
        "cyclonedds listener failed to print init banner"
    );
    assert!(
        output.contains("Waiting for messages"),
        "cyclonedds listener did not reach subscription wait state"
    );
}

/// Phase 118.B.7 — ThreadX-Linux C / C++ cases (zenoh only).
#[rstest]
#[case::c_talker("c", "talker", "threadx_c_talker")]
#[case::c_listener("c", "listener", "threadx_c_listener")]
#[case::c_ss("c", "service-server", "threadx_c_service_server")]
#[case::c_sc("c", "service-client", "threadx_c_service_client")]
#[case::c_as("c", "action-server", "threadx_c_action_server")]
#[case::c_ac("c", "action-client", "threadx_c_action_client")]
#[case::cpp_talker("cpp", "talker", "threadx_cpp_talker")]
#[case::cpp_listener("cpp", "listener", "threadx_cpp_listener")]
#[case::cpp_ss("cpp", "service-server", "threadx_cpp_service_server")]
#[case::cpp_sc("cpp", "service-client", "threadx_cpp_service_client")]
#[case::cpp_as("cpp", "action-server", "threadx_cpp_action_server")]
#[case::cpp_ac("cpp", "action-client", "threadx_cpp_action_client")]
fn test_threadx_linux_cmake_case_rmw_variant_exists(
    #[case] lang: &str,
    #[case] case: &str,
    #[case] binary: &str,
) {
    let path = nros_tests::fixtures::build_threadx_linux_cmake_example_rmw(
        lang, case, binary, Rmw::Zenoh,
    ).unwrap_or_else(|e| {
        nros_tests::skip!(
            "threadx-linux/{}/{} zenoh not prebuilt: {:?}", lang, case, e
        )
    });
    assert!(path.exists(), "threadx-linux {}/{} zenoh missing: {}", lang, case, path.display());
}
