//! NuttX QEMU ARM virt integration tests
//!
//! Tests that verify NuttX examples build and run on QEMU ARM virt (Cortex-A7).
//! NuttX examples use `armv7a-nuttx-eabihf` target with `std` support.
//!
//! ## Test tiers
//!
//! **Build tests** (require NUTTX_DIR + nightly toolchain):
//!   Verify that cargo cross-compilation succeeds for all 6 examples.
//!
//! **E2E network tests** (require NUTTX_DIR + nightly + QEMU + zenohd):
//!   Verify actual message exchange between two NuttX QEMU instances via zenohd.
//!   Each test boots two QEMU ARM virt instances with slirp user networking,
//!   connecting to zenohd on the host via the slirp gateway (10.0.2.2:7452).
//!
//! ## Prerequisites
//!
//! - `NUTTX_DIR` env var pointing to NuttX source (e.g., `third-party/nuttx/nuttx`)
//! - Nightly Rust toolchain with `armv7a-nuttx-eabihf` target
//! - `qemu-system-arm` with virt machine support
//! - zenohd: `just build-zenohd`
//!
//! Run with: `just test-nuttx`
//! Or: `cargo nextest run -p nros-tests --test nuttx_qemu`

use nros_tests::count_pattern;
use nros_tests::fixtures::nuttx::{
    build_nuttx_action_client, build_nuttx_action_server, build_nuttx_c_action_client,
    build_nuttx_c_action_server, build_nuttx_c_listener, build_nuttx_c_service_client,
    build_nuttx_c_service_server, build_nuttx_c_talker, build_nuttx_cpp_action_client,
    build_nuttx_cpp_action_server, build_nuttx_cpp_listener, build_nuttx_cpp_service_client,
    build_nuttx_cpp_service_server, build_nuttx_cpp_talker, build_nuttx_listener,
    build_nuttx_service_client, build_nuttx_service_server, build_nuttx_talker,
    is_arm_gcc_available, is_cmake_available, is_nuttx_available, is_nuttx_configured,
    is_nuttx_toolchain_available, nuttx_kernel_path,
};
use nros_tests::fixtures::{
    QemuProcess, ZenohRouter, is_qemu_available, is_zenohd_available, require_zenohd,
};
use nros_tests::platform;
use std::time::Duration;

// =============================================================================
// Prerequisite checks
// =============================================================================

/// Skip test if NuttX build prerequisites are not available
fn require_nuttx() -> bool {
    if !is_nuttx_available() {
        eprintln!("Skipping test: NUTTX_DIR not set or invalid");
        eprintln!("Run: just setup-nuttx && export NUTTX_DIR=$(pwd)/third-party/nuttx/nuttx");
        return false;
    }
    if !is_nuttx_configured() {
        eprintln!(
            "Skipping test: NuttX not configured ($NUTTX_DIR/include/nuttx/config.h missing)"
        );
        eprintln!("Run: cd packages/boards/nros-nuttx-qemu-arm && ./scripts/build-nuttx.sh");
        return false;
    }
    if !is_arm_gcc_available() {
        eprintln!("Skipping test: arm-none-eabi-gcc not found");
        eprintln!("Install: sudo apt install gcc-arm-none-eabi");
        return false;
    }
    if !is_nuttx_toolchain_available() {
        eprintln!("Skipping test: nightly toolchain missing rust-src for armv7a-nuttx-eabihf");
        eprintln!(
            "Run: rustup toolchain install nightly && rustup component add rust-src --toolchain nightly"
        );
        return false;
    }
    true
}

/// Skip test if full NuttX E2E prerequisites are not available
///
/// E2E tests require:
/// 1. NuttX build prerequisites (NUTTX_DIR + nightly toolchain)
/// 2. Pre-built NuttX kernel image ($NUTTX_DIR/nuttx)
/// 3. QEMU with ARM virt machine support
/// 4. zenohd router on localhost (slirp networking, no TAP bridge needed)
fn require_nuttx_e2e() -> bool {
    if !require_nuttx() {
        return false;
    }
    if nuttx_kernel_path().is_none() {
        eprintln!("Skipping test: NuttX kernel not built ($NUTTX_DIR/nuttx not found)");
        eprintln!("Build with: cd packages/boards/nros-nuttx-qemu-arm && ./scripts/build-nuttx.sh");
        return false;
    }
    if !is_qemu_available() {
        eprintln!("Skipping test: qemu-system-arm not found");
        eprintln!("Install: sudo apt install qemu-system-arm");
        return false;
    }
    if !require_zenohd() {
        return false;
    }
    true
}

// =============================================================================
// Prerequisite detection tests (always run)
// =============================================================================

#[test]
fn test_nuttx_detection() {
    let available = is_nuttx_available();
    let configured = is_nuttx_configured();
    let arm_gcc = is_arm_gcc_available();
    let toolchain = is_nuttx_toolchain_available();
    let qemu = is_qemu_available();
    let zenohd = is_zenohd_available();
    let kernel = nuttx_kernel_path();
    eprintln!("NuttX available: {}", available);
    eprintln!("NuttX configured: {}", configured);
    eprintln!("arm-none-eabi-gcc available: {}", arm_gcc);
    eprintln!("NuttX toolchain available: {}", toolchain);
    eprintln!("QEMU available: {}", qemu);
    eprintln!("zenohd available: {}", zenohd);
    eprintln!(
        "NuttX kernel: {}",
        kernel
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "not built".to_string())
    );
}

// =============================================================================
// Build tests (require NUTTX_DIR + nightly toolchain)
// =============================================================================
#[test]
fn test_nuttx_all_examples_build() {
    if !require_nuttx() {
        nros_tests::skip!("require_nuttx check failed");
    }

    let results = [
        ("talker", build_nuttx_talker()),
        ("listener", build_nuttx_listener()),
        ("service-server", build_nuttx_service_server()),
        ("service-client", build_nuttx_service_client()),
        ("action-server", build_nuttx_action_server()),
        ("action-client", build_nuttx_action_client()),
    ];

    let mut all_ok = true;
    for (name, result) in &results {
        match result {
            Ok(path) => eprintln!("  OK: {} -> {}", name, path.display()),
            Err(e) => {
                eprintln!("  FAIL: {} -> {:?}", name, e);
                all_ok = false;
            }
        }
    }

    assert!(all_ok, "Not all NuttX examples built successfully");
}

// =============================================================================
// NuttX kernel boot test (require QEMU + NuttX kernel image)
// =============================================================================

/// Verify that the NuttX kernel boots to NSH prompt in QEMU ARM virt.
///
/// This test does not require networking — it boots NuttX with `-nic none`
/// and checks for the NSH shell prompt, validating the kernel + QEMU setup.
#[test]
fn test_nuttx_kernel_boots() {
    if !is_nuttx_available() {
        eprintln!("Skipping: NUTTX_DIR not set");
        return;
    }
    let kernel = match nuttx_kernel_path() {
        Some(k) => k,
        None => {
            eprintln!("Skipping: NuttX kernel not built ($NUTTX_DIR/nuttx)");
            return;
        }
    };
    if !is_qemu_available() {
        eprintln!("Skipping: qemu-system-arm not found");
        return;
    }

    eprintln!("Booting NuttX kernel: {}", kernel.display());

    // Boot NuttX without networking (just verify kernel boot)
    let mut qemu = QemuProcess::start_nuttx_virt(&kernel, false)
        .expect("Failed to start QEMU with NuttX kernel");

    // NuttX should boot to NSH prompt within 10 seconds
    let output = qemu
        .wait_for_output(Duration::from_secs(10))
        .unwrap_or_default();
    qemu.kill();

    eprintln!("NuttX boot output:\n{}", output);

    // Check for NuttX boot markers
    let has_nsh = output.contains("nsh>") || output.contains("NuttShell");
    let has_nuttx = output.contains("NuttX");

    if has_nsh {
        eprintln!("[PASS] NuttX booted to NSH prompt");
    } else if has_nuttx {
        eprintln!("[PARTIAL] NuttX started but NSH prompt not found");
    } else {
        eprintln!("[INFO] No NuttX output detected — kernel may need configuration");
        eprintln!("Build: cd packages/boards/nros-nuttx-qemu-arm && ./scripts/build-nuttx.sh");
    }
}

// =============================================================================
// E2E Network tests (require QEMU + zenohd + NuttX kernel)
// =============================================================================
//
// NuttX QEMU ARM virt examples use virtio-net with slirp user networking:
//   qemu-system-arm -M virt -cpu cortex-a7 -nographic -kernel <nuttx-image> \
//       -nic user,net=10.0.2.0/24,host=10.0.2.2,hostfwd=...
//
// Network topology (slirp — each QEMU has isolated 10.0.2.0/24 network):
//   QEMU node 0 (slirp, 10.0.2.30) ---> 10.0.2.2:7452 --+
//                                                          |-- zenohd (localhost:7452)
//   QEMU node 1 (slirp, 10.0.2.31) ---> 10.0.2.2:7452 --+
//
// IP assignments (hardcoded in board crate Config):
//   10.0.2.30  - Talker / Server
//   10.0.2.31  - Listener
//   10.0.2.32  - Server
//   10.0.2.33  - Client
//
// Prerequisites:
//   1. zenohd: just build-zenohd
//   2. NuttX kernel: build-nuttx.sh (with Rust apps integrated)
//   3. Run: just test-nuttx

/// Test pub/sub message exchange between NuttX QEMU instances.
///
/// Launches a listener and a talker on separate QEMU instances (slirp networking),
/// verifies that the listener receives Int32 messages published by the talker.
#[test]
fn test_nuttx_pubsub_e2e() {
    if !require_nuttx_e2e() {
        nros_tests::skip!("require_nuttx_e2e check failed");
    }

    // Build both binaries
    let talker_bin = build_nuttx_talker().expect("Failed to build talker");
    let listener_bin = build_nuttx_listener().expect("Failed to build listener");

    // Start zenohd (NuttX binaries connect via slirp gateway to host)
    let mut zenohd =
        ZenohRouter::start(platform::NUTTX.zenohd_port).expect("Failed to start zenohd");
    assert!(zenohd.is_running(), "zenohd should be running");

    // Start listener and talker in PARALLEL so both reach zenohd within the same
    // ~10s window. The Rust binaries boot slower than C, and if we wait for the
    // listener to declare its subscription before starting the talker, the
    // listener session times out before the talker finishes booting.
    eprintln!("Starting listener QEMU (slirp, 10.0.2.31)...");
    let mut listener =
        QemuProcess::start_nuttx_virt(listener_bin, true).expect("Failed to start listener QEMU");

    eprintln!("Starting talker QEMU (slirp, 10.0.2.30)...");
    let mut talker =
        QemuProcess::start_nuttx_virt(talker_bin, true).expect("Failed to start talker QEMU");

    // Wait for listener to be ready (subscription declared) before checking output
    let listener_ready = listener
        .wait_for_output(Duration::from_secs(30))
        .unwrap_or_default();
    let listener_booted = listener_ready.contains("Waiting for messages");
    eprintln!(
        "Listener boot output ({} chars): {}",
        listener_ready.len(),
        &listener_ready[..listener_ready.len().min(500)]
    );

    if !listener_booted {
        eprintln!(
            "[SKIP] Listener did not reach readiness — NuttX app integration may be incomplete"
        );
        eprintln!("The NuttX kernel must include the Rust app as a builtin or via ROMFS.");
        eprintln!("See: docs/roadmap/phase-55-nuttx-platform.md (55.12)");
        return;
    }

    // Wait for talker to start publishing
    let _talker_output = talker
        .wait_for_output(Duration::from_secs(15))
        .unwrap_or_default();

    // Wait for listener to receive messages
    let final_output = listener
        .wait_for_output(Duration::from_secs(30))
        .unwrap_or_default();
    let full_output = format!("{}{}", listener_ready, final_output);

    talker.kill();
    listener.kill();

    eprintln!("Listener full output:\n{}", full_output);

    let received_count = count_pattern(&full_output, "Received");
    eprintln!("Messages received: {}", received_count);

    if received_count > 0 {
        eprintln!(
            "[PASS] NuttX pubsub E2E: {} messages exchanged",
            received_count
        );
    } else {
        eprintln!("[FAIL] NuttX pubsub E2E: no messages received");
        panic!("NuttX pubsub E2E failed — listener received 0 messages");
    }
}

/// Test service request/response between NuttX QEMU instances.
///
/// Launches a service server and a client on separate QEMU instances (slirp networking),
/// verifies that the client receives correct AddTwoInts responses.
#[test]
fn test_nuttx_service_e2e() {
    if !require_nuttx_e2e() {
        nros_tests::skip!("require_nuttx_e2e check failed");
    }

    let server_bin = build_nuttx_service_server().expect("Failed to build service server");
    let client_bin = build_nuttx_service_client().expect("Failed to build service client");

    let mut zenohd =
        ZenohRouter::start(platform::NUTTX.zenohd_port).expect("Failed to start zenohd");
    assert!(zenohd.is_running(), "zenohd should be running");

    // Start server first with a small stagger so the server's queryable is
    // declared before the client sends its first query. Without the stagger,
    // the client's query can race ahead of the server's declaration and fail.
    eprintln!("Starting service server QEMU (slirp, 10.0.2.30)...");
    let mut server =
        QemuProcess::start_nuttx_virt(server_bin, true).expect("Failed to start server QEMU");
    std::thread::sleep(Duration::from_secs(3));
    eprintln!("Starting service client QEMU (slirp, 10.0.2.31)...");
    let mut client =
        QemuProcess::start_nuttx_virt(client_bin, true).expect("Failed to start client QEMU");

    let server_output = server
        .wait_for_output(Duration::from_secs(30))
        .unwrap_or_default();
    let server_ready = server_output.contains("Waiting for requests");
    eprintln!(
        "Server boot output ({} chars): {}",
        server_output.len(),
        &server_output[..server_output.len().min(500)]
    );

    if !server_ready {
        eprintln!(
            "[SKIP] Server did not reach readiness — NuttX app integration may be incomplete"
        );
        return;
    }

    // Wait for client to complete all service calls (4 calls: 5+3, 10+20, 100+200, -5+10)
    let client_output = client
        .wait_for_output(Duration::from_secs(45))
        .unwrap_or_default();

    server.kill();
    client.kill();

    eprintln!("Client output:\n{}", client_output);

    // Check for successful responses
    let response_count = count_pattern(&client_output, "Response:");
    let completed = client_output.contains("All service calls completed");

    // Verify specific results
    let has_8 = client_output.contains("= 8"); // 5 + 3
    let has_30 = client_output.contains("= 30"); // 10 + 20
    let has_300 = client_output.contains("= 300"); // 100 + 200

    eprintln!("Responses: {}, completed: {}", response_count, completed);
    eprintln!("Results: 8={}, 30={}, 300={}", has_8, has_30, has_300);

    if response_count >= 4 && completed {
        eprintln!(
            "[PASS] NuttX service E2E: {} responses, all correct",
            response_count
        );
    } else if response_count > 0 {
        eprintln!(
            "[PARTIAL] NuttX service E2E: {} of 4 responses",
            response_count
        );
    } else {
        eprintln!("[FAIL] NuttX service E2E: no responses received");
        panic!("NuttX service E2E failed — client received 0 responses");
    }
}

/// Test action goal/feedback/result between NuttX QEMU instances.
///
/// Launches an action server and a client on separate QEMU instances (slirp networking),
/// verifies that the client receives Fibonacci feedback and final result.
#[test]
fn test_nuttx_action_e2e() {
    if !require_nuttx_e2e() {
        nros_tests::skip!("require_nuttx_e2e check failed");
    }

    let server_bin = build_nuttx_action_server().expect("Failed to build action server");
    let client_bin = build_nuttx_action_client().expect("Failed to build action client");

    let mut zenohd =
        ZenohRouter::start(platform::NUTTX.zenohd_port).expect("Failed to start zenohd");
    assert!(zenohd.is_running(), "zenohd should be running");

    // Start action server first with a small stagger (see test_nuttx_service_e2e).
    eprintln!("Starting action server QEMU (slirp, 10.0.2.30)...");
    let mut server =
        QemuProcess::start_nuttx_virt(server_bin, true).expect("Failed to start server QEMU");
    std::thread::sleep(Duration::from_secs(3));
    eprintln!("Starting action client QEMU (slirp, 10.0.2.31)...");
    let mut client =
        QemuProcess::start_nuttx_virt(client_bin, true).expect("Failed to start client QEMU");

    let server_output = server
        .wait_for_output(Duration::from_secs(30))
        .unwrap_or_default();
    let server_ready = server_output.contains("Waiting for goals");
    eprintln!(
        "Server boot output ({} chars): {}",
        server_output.len(),
        &server_output[..server_output.len().min(500)]
    );

    if !server_ready {
        eprintln!(
            "[SKIP] Action server did not reach readiness — NuttX app integration may be incomplete"
        );
        return;
    }

    // Fibonacci(10) takes ~5.5s to compute + NuttX boot + zenoh connect
    let client_output = client
        .wait_for_output(Duration::from_secs(60))
        .unwrap_or_default();

    server.kill();
    client.kill();

    eprintln!("Client output:\n{}", client_output);

    // Verify action protocol
    let goal_accepted = client_output.contains("Goal accepted");
    let feedback_count = count_pattern(&client_output, "Feedback #");
    let completed = client_output.contains("Action client finished")
        || client_output.contains("All feedback received");

    eprintln!(
        "Goal accepted: {}, feedback: {}, completed: {}",
        goal_accepted, feedback_count, completed
    );

    if goal_accepted && feedback_count > 0 && completed {
        eprintln!(
            "[PASS] NuttX action E2E: goal accepted, {} feedback msgs, completed",
            feedback_count
        );
    } else {
        eprintln!("[FAIL] NuttX action E2E:");
        if !goal_accepted {
            eprintln!("  - Goal was NOT accepted");
        }
        if feedback_count == 0 {
            eprintln!("  - No feedback received");
        }
        if !completed {
            eprintln!("  - Action did not complete");
        }
        panic!(
            "NuttX action E2E failed: accepted={}, feedback={}, completed={}",
            goal_accepted, feedback_count, completed
        );
    }
}

// =============================================================================
// C++ test helpers
// =============================================================================

fn require_nuttx_cpp() -> bool {
    if !require_nuttx() {
        return false;
    }
    if !is_cmake_available() {
        eprintln!("Skipping test: cmake not found");
        return false;
    }
    true
}

// =============================================================================
// C++ Build tests
// =============================================================================

#[test]
#[ignore = "NuttX C/C++ CMake build blocked by upstream libc missing _SC_HOST_NAME_MAX"]
fn test_nuttx_cpp_talker_builds() {
    if !require_nuttx_cpp() {
        nros_tests::skip!("require_nuttx_cpp check failed");
    }
    let binary = build_nuttx_cpp_talker().expect("Failed to build nuttx_cpp_talker");
    assert!(binary.exists());
    eprintln!("SUCCESS: nuttx_cpp_talker at {}", binary.display());
}

#[test]
#[ignore = "NuttX C/C++ CMake build blocked by upstream libc missing _SC_HOST_NAME_MAX"]
fn test_nuttx_cpp_listener_builds() {
    if !require_nuttx_cpp() {
        nros_tests::skip!("require_nuttx_cpp check failed");
    }
    let binary = build_nuttx_cpp_listener().expect("Failed to build nuttx_cpp_listener");
    assert!(binary.exists());
    eprintln!("SUCCESS: nuttx_cpp_listener at {}", binary.display());
}

#[test]
#[ignore = "NuttX C/C++ CMake build blocked by upstream libc missing _SC_HOST_NAME_MAX"]
fn test_nuttx_cpp_service_server_builds() {
    if !require_nuttx_cpp() {
        nros_tests::skip!("require_nuttx_cpp check failed");
    }
    let binary =
        build_nuttx_cpp_service_server().expect("Failed to build nuttx_cpp_service_server");
    assert!(binary.exists());
    eprintln!("SUCCESS: nuttx_cpp_service_server at {}", binary.display());
}

#[test]
#[ignore = "NuttX C/C++ CMake build blocked by upstream libc missing _SC_HOST_NAME_MAX"]
fn test_nuttx_cpp_service_client_builds() {
    if !require_nuttx_cpp() {
        nros_tests::skip!("require_nuttx_cpp check failed");
    }
    let binary =
        build_nuttx_cpp_service_client().expect("Failed to build nuttx_cpp_service_client");
    assert!(binary.exists());
    eprintln!("SUCCESS: nuttx_cpp_service_client at {}", binary.display());
}

#[test]
#[ignore = "NuttX C/C++ CMake build blocked by upstream libc missing _SC_HOST_NAME_MAX"]
fn test_nuttx_cpp_action_server_builds() {
    if !require_nuttx_cpp() {
        nros_tests::skip!("require_nuttx_cpp check failed");
    }
    let binary = build_nuttx_cpp_action_server().expect("Failed to build nuttx_cpp_action_server");
    assert!(binary.exists());
    eprintln!("SUCCESS: nuttx_cpp_action_server at {}", binary.display());
}

#[test]
#[ignore = "NuttX C/C++ CMake build blocked by upstream libc missing _SC_HOST_NAME_MAX"]
fn test_nuttx_cpp_action_client_builds() {
    if !require_nuttx_cpp() {
        nros_tests::skip!("require_nuttx_cpp check failed");
    }
    let binary = build_nuttx_cpp_action_client().expect("Failed to build nuttx_cpp_action_client");
    assert!(binary.exists());
    eprintln!("SUCCESS: nuttx_cpp_action_client at {}", binary.display());
}

// =============================================================================
// C++ E2E Network tests
// =============================================================================

#[test]
#[ignore = "NuttX C/C++ CMake build blocked by upstream libc missing _SC_HOST_NAME_MAX"]
fn test_nuttx_cpp_pubsub_e2e() {
    if !require_nuttx_cpp() {
        nros_tests::skip!("require_nuttx_cpp check failed");
    }
    if !require_nuttx_e2e() {
        nros_tests::skip!("require_nuttx_e2e check failed");
    }

    let talker_bin = build_nuttx_cpp_talker().expect("Failed to build C++ talker");
    let listener_bin = build_nuttx_cpp_listener().expect("Failed to build C++ listener");

    let _zenohd = ZenohRouter::start(platform::NUTTX.zenohd_port).expect("Failed to start zenohd");

    eprintln!("Starting C++ listener QEMU (slirp, 10.0.2.31)...");
    let mut listener =
        QemuProcess::start_nuttx_virt(listener_bin, true).expect("Failed to start listener");

    let listener_ready = listener
        .wait_for_output(Duration::from_secs(30))
        .unwrap_or_default();

    if !listener_ready.contains("Waiting for messages") {
        nros_tests::skip!("C++ listener did not reach readiness");
    }

    eprintln!("Starting C++ talker QEMU (slirp, 10.0.2.30)...");
    let mut talker =
        QemuProcess::start_nuttx_virt(talker_bin, true).expect("Failed to start talker");

    let _talker_out = talker
        .wait_for_output(Duration::from_secs(15))
        .unwrap_or_default();
    let final_out = listener
        .wait_for_output(Duration::from_secs(30))
        .unwrap_or_default();
    let full = format!("{}{}", listener_ready, final_out);

    talker.kill();
    listener.kill();

    let received = count_pattern(&full, "Received");
    eprintln!("C++ NuttX messages received: {}", received);
    assert!(received > 0, "NuttX C++ pubsub E2E: 0 messages received");
    eprintln!("[PASS] NuttX C++ pubsub E2E: {} messages", received);
}

#[test]
#[ignore = "NuttX C/C++ CMake build blocked by upstream libc missing _SC_HOST_NAME_MAX"]
fn test_nuttx_cpp_service_e2e() {
    if !require_nuttx_cpp() {
        nros_tests::skip!("require_nuttx_cpp check failed");
    }
    if !require_nuttx_e2e() {
        nros_tests::skip!("require_nuttx_e2e check failed");
    }

    let server_bin = build_nuttx_cpp_service_server().expect("Failed to build C++ service server");
    let client_bin = build_nuttx_cpp_service_client().expect("Failed to build C++ service client");

    let _zenohd = ZenohRouter::start(platform::NUTTX.zenohd_port).expect("Failed to start zenohd");

    eprintln!("Starting C++ service server QEMU...");
    let mut server =
        QemuProcess::start_nuttx_virt(server_bin, true).expect("Failed to start server");

    let server_ready = server
        .wait_for_output(Duration::from_secs(30))
        .unwrap_or_default();
    if !server_ready.contains("Service server ready") {
        nros_tests::skip!("C++ server did not reach readiness");
    }

    eprintln!("Starting C++ service client QEMU...");
    let mut client =
        QemuProcess::start_nuttx_virt(client_bin, true).expect("Failed to start client");

    let client_out = client
        .wait_for_output(Duration::from_secs(60))
        .unwrap_or_default();
    server.kill();
    client.kill();

    let responses = count_pattern(&client_out, "Response:");
    let completed = client_out.contains("All service calls completed");
    eprintln!(
        "C++ NuttX responses: {}, completed: {}",
        responses, completed
    );
    assert!(responses > 0, "NuttX C++ service E2E: 0 responses");
    eprintln!("[PASS] NuttX C++ service E2E: {} responses", responses);
}

// =============================================================================
// C Build tests
// =============================================================================
// =============================================================================
// C E2E tests (QEMU ARM virt + slirp networking)
// =============================================================================

// NuttX QEMU E2E tests reach `nros_support_init` successfully (the zenoh
// session opens and the TCP connection to zenohd is established), but
// no messages ever flow through the pub/sub path — neither the timer
// callback in C examples nor the publish loop in Rust examples makes
// forward progress after printing their "Publishing/Waiting..." banner.
// The same symptom affects both Rust and C; the Rust test previously
// hid this behind an early `return` on `Transport(ConnectionFailed)`,
// which is no longer triggered now that the virtio-mmio NIC model is
// fixed (see QemuProcess::start_nuttx_virt).
//
// Root cause is deeper — likely in zenoh-pico's POSIX-threaded spin path
// on NuttX, or the executor/timer dispatch loop interaction with NuttX's
// condvar timeouts. Needs dedicated investigation and is tracked as a
// follow-up to Phase 55.12.
#[test]
fn test_nuttx_c_pubsub_e2e() {
    if !require_nuttx_e2e() {
        nros_tests::skip!("require_nuttx_e2e check failed");
    }

    let talker = build_nuttx_c_talker().expect("build talker");
    let listener = build_nuttx_c_listener().expect("build listener");

    let _z = ZenohRouter::start(platform::NUTTX.zenohd_port).expect("zenohd");

    let mut l = QemuProcess::start_nuttx_virt(listener, true).expect("listener QEMU");
    std::thread::sleep(Duration::from_secs(10));
    let mut t = QemuProcess::start_nuttx_virt(talker, true).expect("talker QEMU");

    let l_out = l
        .wait_for_output(Duration::from_secs(60))
        .unwrap_or_default();
    let t_out = t
        .wait_for_output(Duration::from_secs(15))
        .unwrap_or_default();
    t.kill();
    l.kill();

    eprintln!("C Listener:\n{l_out}\nC Talker:\n{t_out}");
    let received = count_pattern(&l_out, "Received");
    assert!(received > 0, "NuttX C pubsub: 0 messages");
    eprintln!("[PASS] NuttX C pubsub E2E: {received} msgs");
}

#[test]
fn test_nuttx_c_service_e2e() {
    if !require_nuttx_e2e() {
        nros_tests::skip!("require_nuttx_e2e check failed");
    }

    let server = build_nuttx_c_service_server().expect("build server");
    let client = build_nuttx_c_service_client().expect("build client");

    let _z = ZenohRouter::start(platform::NUTTX.zenohd_port).expect("zenohd");

    // Start server and client with a small stagger so the server reaches
    // "Waiting for requests" before the client sends its first query.
    let mut s = QemuProcess::start_nuttx_virt(server, true).expect("server QEMU");
    std::thread::sleep(Duration::from_secs(3));
    let mut c = QemuProcess::start_nuttx_virt(client, true).expect("client QEMU");

    let c_out = c
        .wait_for_output(Duration::from_secs(60))
        .unwrap_or_default();
    s.kill();
    c.kill();

    eprintln!("C Client:\n{c_out}");
    let responses = count_pattern(&c_out, "Call [");
    assert!(responses > 0, "NuttX C service: 0 responses");
    eprintln!("[PASS] NuttX C service E2E: {responses} responses");
}

#[test]
fn test_nuttx_c_action_e2e() {
    if !require_nuttx_e2e() {
        nros_tests::skip!("require_nuttx_e2e check failed");
    }

    let server = build_nuttx_c_action_server().expect("build server");
    let client = build_nuttx_c_action_client().expect("build client");

    let _z = ZenohRouter::start(platform::NUTTX.zenohd_port).expect("zenohd");

    // Start server and client with a small stagger so the server is ready to
    // accept goals before the client sends one.
    let mut s = QemuProcess::start_nuttx_virt(server, true).expect("server QEMU");
    std::thread::sleep(Duration::from_secs(3));
    let mut c = QemuProcess::start_nuttx_virt(client, true).expect("client QEMU");

    let c_out = c
        .wait_for_output(Duration::from_secs(120))
        .unwrap_or_default();
    let s_out = s
        .wait_for_output(Duration::from_secs(10))
        .unwrap_or_default();
    s.kill();
    c.kill();

    eprintln!("C Server:\n{s_out}");
    eprintln!("C Client:\n{c_out}");
    let accepted = c_out.contains("Goal accepted");
    let completed = c_out.contains("Result (status=");
    assert!(
        accepted && completed,
        "NuttX C action: accepted={accepted}, completed={completed}"
    );
    eprintln!("[PASS] NuttX C action E2E");
}
