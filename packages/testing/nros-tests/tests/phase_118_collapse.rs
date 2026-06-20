//! Zephyr Cyclone DDS native_sim runtime E2E suite.
//!
//! History: this file began as Phase 118.A.2's collapsed-shape, per-RMW
//! **binary presence/name** matrices (`*_rmw_variant_exists`). Those were
//! build-only smokes — they asserted a prebuilt fixture binary existed with
//! the expected name, coverage already provided by `build-all` (which builds
//! every fixture from `examples/fixtures.toml`, Phase 181) and by the runtime
//! tests that consume those binaries (which fail loudly on a missing/misnamed
//! one). Phase 182.1 removed all of them.
//!
//! The surviving tests are the Phase 11W Cyclone DDS native_sim **runtime**
//! E2E + boot smokes (real pub/sub + service over SPDP multicast discovery) —
//! they spawn `ZephyrProcess` and assert delivered samples / replies.
//!
//! The file/binary name `phase_118_collapse` is retained deliberately:
//! `.config/nextest.toml` routes these via
//! `binary(phase_118_collapse) and test(cyclonedds) and (test(_boot) or
//! test(_e2e))` (host native_sim serialization). Renaming would need a
//! matching config update.

use nros_tests::fixtures::Rmw;

/// Phase 11W.9/.10 — runtime smoke for the cyclonedds native_sim Rust
/// talker. After 11W.10 the participant inits and the talker publishes
/// std_msgs/Int32 at 1 Hz, so assert an actual `Published:` line (not
/// just the boot banner).
#[test]
fn test_zephyr_rust_talker_cyclonedds_boot() {
    use std::time::Duration;

    use nros_tests::zephyr::{ZephyrPlatform, ZephyrProcess};

    let path = nros_tests::fixtures::build_zephyr_rust_example_rmw("talker", Rmw::Cyclonedds)
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
    nros_tests::output::assert_talker(&output, 1);
}

/// Phase 11W.9/.10 — runtime smoke for the cyclonedds native_sim Rust
/// listener. Asserts the participant + subscription init cleanly
/// (reaches the "Waiting for messages" log) without aborting.
#[test]
fn test_zephyr_rust_listener_cyclonedds_boot() {
    use std::time::Duration;

    use nros_tests::zephyr::{ZephyrPlatform, ZephyrProcess};

    let path = nros_tests::fixtures::build_zephyr_rust_example_rmw("listener", Rmw::Cyclonedds)
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

/// Phase 11W.12 — true talker→listener pub/sub over Cyclone DDS SPDP
/// multicast discovery on native_sim NSOS. Requires the full multicast
/// path: NSOS `getifaddrs` returning a real multicast-capable interface,
/// the host-side `IPPROTO_IP` setsockopt forwarder (so `IP_ADD_MEMBERSHIP`
/// reaches the host kernel), and *distinct* `--seed` per process —
/// native_sim's deterministic test entropy otherwise yields identical
/// Cyclone GUID prefixes, so each participant treats the other's SPDP as
/// its own and discovery never completes. `ZephyrProcess::start` injects a
/// unique seed per spawn, covering the last requirement.
#[test]
fn test_zephyr_rust_cyclonedds_pubsub_e2e() {
    use std::time::Duration;

    use nros_tests::zephyr::{ZephyrPlatform, ZephyrProcess};

    let listener_bin =
        nros_tests::fixtures::build_zephyr_rust_example_rmw("listener", Rmw::Cyclonedds)
            .unwrap_or_else(|e| {
                nros_tests::skip!("zephyr/rust/listener cyclonedds not prebuilt: {:?}", e)
            });
    let talker_bin = nros_tests::fixtures::build_zephyr_rust_example_rmw("talker", Rmw::Cyclonedds)
        .unwrap_or_else(|e| {
            nros_tests::skip!("zephyr/rust/talker cyclonedds not prebuilt: {:?}", e)
        });

    let mut listener = ZephyrProcess::start(&listener_bin, ZephyrPlatform::NativeSim)
        .expect("spawn zephyr listener (cyclonedds)");
    let _ = listener.wait_for_pattern("Waiting for messages", Duration::from_secs(30));
    let mut talker = ZephyrProcess::start(&talker_bin, ZephyrPlatform::NativeSim)
        .expect("spawn zephyr talker (cyclonedds)");

    // SPDP + SEDP + first delivered sample: allow generous margin.
    let output = listener.wait_for_pattern("Received", Duration::from_secs(20));

    listener.kill();
    talker.kill();

    eprintln!("zephyr cyclonedds e2e listener output:\n{}", output);

    assert!(
        output.contains("Received"),
        "cyclonedds listener did not receive any talker sample over SPDP \
         discovery (expected a `Received` line)"
    );
}

/// Phase 11W.12 — same talker→listener discovery, C++ surface. The
/// C++ cyclonedds overlay now matches the Rust one (16 MiB malloc arena
/// plus NSOS offload forcing) and the C++ CMake generates the Cyclone C
/// `dds_topic_descriptor_t` from std_msgs/Int32, so the C++ talker
/// publishes and a C++ listener receives over SPDP multicast discovery.
#[test]
fn test_zephyr_cpp_cyclonedds_pubsub_e2e() {
    use std::time::Duration;

    use nros_tests::zephyr::{ZephyrPlatform, ZephyrProcess};

    let listener_bin =
        nros_tests::fixtures::build_zephyr_cmake_example_rmw("cpp", "listener", Rmw::Cyclonedds)
            .unwrap_or_else(|e| {
                nros_tests::skip!("zephyr/cpp/listener cyclonedds not prebuilt: {:?}", e)
            });
    let talker_bin =
        nros_tests::fixtures::build_zephyr_cmake_example_rmw("cpp", "talker", Rmw::Cyclonedds)
            .unwrap_or_else(|e| {
                nros_tests::skip!("zephyr/cpp/talker cyclonedds not prebuilt: {:?}", e)
            });

    let mut listener = ZephyrProcess::start(&listener_bin, ZephyrPlatform::NativeSim)
        .expect("spawn zephyr cpp listener (cyclonedds)");
    let _ = listener.wait_for_pattern("Waiting for messages", Duration::from_secs(30));
    let mut talker = ZephyrProcess::start(&talker_bin, ZephyrPlatform::NativeSim)
        .expect("spawn zephyr cpp talker (cyclonedds)");

    let output = listener.wait_for_pattern("Received", Duration::from_secs(20));

    listener.kill();
    talker.kill();

    eprintln!("zephyr cpp cyclonedds e2e listener output:\n{}", output);

    assert!(
        output.contains("Received"),
        "cyclonedds cpp listener did not receive any talker sample over \
         SPDP discovery (expected a `Received` line)"
    );
}

/// Phase 11W.12 — same talker→listener discovery, C surface. The C
/// app links the C++ cyclonedds backend via the module; the C overlay
/// now matches the Rust one and the C CMake generates the Cyclone C
/// `dds_topic_descriptor_t` (the nros C codegen emits the rcl-style
/// struct but not the descriptor). C listener receives the C talker's
/// samples over SPDP multicast discovery.
#[test]
fn test_zephyr_c_cyclonedds_pubsub_e2e() {
    use std::time::Duration;

    use nros_tests::zephyr::{ZephyrPlatform, ZephyrProcess};

    let listener_bin =
        nros_tests::fixtures::build_zephyr_cmake_example_rmw("c", "listener", Rmw::Cyclonedds)
            .unwrap_or_else(|e| {
                nros_tests::skip!("zephyr/c/listener cyclonedds not prebuilt: {:?}", e)
            });
    let talker_bin =
        nros_tests::fixtures::build_zephyr_cmake_example_rmw("c", "talker", Rmw::Cyclonedds)
            .unwrap_or_else(|e| {
                nros_tests::skip!("zephyr/c/talker cyclonedds not prebuilt: {:?}", e)
            });

    let mut listener = ZephyrProcess::start(&listener_bin, ZephyrPlatform::NativeSim)
        .expect("spawn zephyr c listener (cyclonedds)");
    let _ = listener.wait_for_pattern("Waiting for messages", Duration::from_secs(30));
    let mut talker = ZephyrProcess::start(&talker_bin, ZephyrPlatform::NativeSim)
        .expect("spawn zephyr c talker (cyclonedds)");

    let output = listener.wait_for_pattern("Received", Duration::from_secs(20));

    listener.kill();
    talker.kill();

    eprintln!("zephyr c cyclonedds e2e listener output:\n{}", output);

    assert!(
        output.contains("Received"),
        "cyclonedds c listener did not receive any talker sample over \
         SPDP discovery (expected a `Received` line)"
    );
}

/// Phase 11W.12 — Cyclone DDS service request/response roundtrip on
/// native_sim NSOS. The service maps to a request + response topic; the
/// CMake generates both Cyclone C descriptors from
/// example_interfaces/srv/AddTwoInts.srv. Exercises the backend
/// `service_type_name` fix: the nros codegen emits SERVICE_NAME with a
/// trailing `_`, so the backend must strip it before appending
/// `_Request_`/`_Response_` to match the registered descriptor. Client
/// logs `Response: sum=` once the server replies.
#[test]
fn test_zephyr_rust_cyclonedds_service_e2e() {
    use std::time::Duration;

    use nros_tests::zephyr::{ZephyrPlatform, ZephyrProcess};

    let server_bin =
        nros_tests::fixtures::build_zephyr_rust_example_rmw("service-server", Rmw::Cyclonedds)
            .unwrap_or_else(|e| {
                nros_tests::skip!(
                    "zephyr/rust/service-server cyclonedds not prebuilt: {:?}",
                    e
                )
            });
    let client_bin =
        nros_tests::fixtures::build_zephyr_rust_example_rmw("service-client", Rmw::Cyclonedds)
            .unwrap_or_else(|e| {
                nros_tests::skip!(
                    "zephyr/rust/service-client cyclonedds not prebuilt: {:?}",
                    e
                )
            });

    let mut server = ZephyrProcess::start(&server_bin, ZephyrPlatform::NativeSim)
        .expect("spawn zephyr service-server (cyclonedds)");
    let _ = server.wait_for_pattern("Waiting", Duration::from_secs(30));
    let mut client = ZephyrProcess::start(&client_bin, ZephyrPlatform::NativeSim)
        .expect("spawn zephyr service-client (cyclonedds)");

    let output = client.wait_for_pattern("Response: sum=", Duration::from_secs(20));

    client.kill();
    server.kill();

    eprintln!(
        "zephyr rust cyclonedds service e2e client output:\n{}",
        output
    );

    assert!(
        output.contains("Response: sum="),
        "cyclonedds service client did not receive a reply (expected a \
         `Response: sum=` line)"
    );
}

/// Phase 11W.12 — Cyclone DDS service roundtrip, C++ surface. Same
/// overlay parity + srv descriptor generation as the Rust service; the
/// C++ client logs `[OK]` for each successful call.
#[test]
fn test_zephyr_cpp_cyclonedds_service_e2e() {
    use std::time::Duration;

    use nros_tests::zephyr::{ZephyrPlatform, ZephyrProcess};

    let server_bin = nros_tests::fixtures::build_zephyr_cmake_example_rmw(
        "cpp",
        "service-server",
        Rmw::Cyclonedds,
    )
    .unwrap_or_else(|e| {
        nros_tests::skip!("zephyr/cpp/service-server cyclonedds not prebuilt: {:?}", e)
    });
    let client_bin = nros_tests::fixtures::build_zephyr_cmake_example_rmw(
        "cpp",
        "service-client",
        Rmw::Cyclonedds,
    )
    .unwrap_or_else(|e| {
        nros_tests::skip!("zephyr/cpp/service-client cyclonedds not prebuilt: {:?}", e)
    });

    let mut server = ZephyrProcess::start(&server_bin, ZephyrPlatform::NativeSim)
        .expect("spawn zephyr cpp service-server (cyclonedds)");
    let _ = server.wait_for_pattern("Waiting", Duration::from_secs(30));
    let mut client = ZephyrProcess::start(&client_bin, ZephyrPlatform::NativeSim)
        .expect("spawn zephyr cpp service-client (cyclonedds)");

    let output = client.wait_for_pattern("[OK]", Duration::from_secs(20));

    client.kill();
    server.kill();

    eprintln!(
        "zephyr cpp cyclonedds service e2e client output:\n{}",
        output
    );

    assert!(
        output.contains("[OK]"),
        "cyclonedds cpp service client did not get a successful reply \
         (expected a `[OK]` line)"
    );
}

/// Phase 171.0.a — Cyclone DDS service roundtrip, C surface. This was the
/// last Zephyr native_sim service gap after Rust + C++ passed: the C client
/// wrote before the volatile service reader match and dropped the request.
#[test]
fn test_zephyr_c_cyclonedds_service_e2e() {
    use std::time::Duration;

    use nros_tests::zephyr::{ZephyrPlatform, ZephyrProcess};

    let server_bin = nros_tests::fixtures::build_zephyr_cmake_example_rmw(
        "c",
        "service-server",
        Rmw::Cyclonedds,
    )
    .unwrap_or_else(|e| {
        nros_tests::skip!("zephyr/c/service-server cyclonedds not prebuilt: {:?}", e)
    });
    let client_bin = nros_tests::fixtures::build_zephyr_cmake_example_rmw(
        "c",
        "service-client",
        Rmw::Cyclonedds,
    )
    .unwrap_or_else(|e| {
        nros_tests::skip!("zephyr/c/service-client cyclonedds not prebuilt: {:?}", e)
    });

    let mut server = ZephyrProcess::start(&server_bin, ZephyrPlatform::NativeSim)
        .expect("spawn zephyr c service-server (cyclonedds)");
    let _ = server.wait_for_pattern("Waiting", Duration::from_secs(30));
    let mut client = ZephyrProcess::start(&client_bin, ZephyrPlatform::NativeSim)
        .expect("spawn zephyr c service-client (cyclonedds)");

    let output = client.wait_for_pattern("Result:", Duration::from_secs(20));

    client.kill();
    server.kill();

    eprintln!("zephyr c cyclonedds service e2e client output:\n{}", output);

    assert!(
        output.contains("Result:"),
        "cyclonedds c service client did not receive a reply (expected a \
         `Result:` line)"
    );
}
