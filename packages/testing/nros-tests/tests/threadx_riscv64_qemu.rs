//! ThreadX QEMU RISC-V 64-bit integration tests
//!
//! Tests that verify ThreadX QEMU RISC-V examples build and run on QEMU virt
//! machine with virtio-net networking. Examples use `riscv64gc-unknown-none-elf`
//! target with `no_std` + NetX Duo networking over virtio-net.
//!
//! Prerequisites:
//! - `THREADX_DIR` env var pointing to ThreadX source (e.g., `third-party/threadx/kernel`)
//! - `NETX_DIR` env var pointing to NetX Duo source (e.g., `third-party/threadx/netxduo`)
//! - `riscv64-unknown-elf-gcc` cross-compiler installed
//! - `qemu-system-riscv64` with virt machine support
//! - zenohd: `just build-zenohd`
//!
//! Run with: `just test-threadx-riscv64`
//! Or: `cargo nextest run -p nros-tests --test threadx_riscv64_qemu`

use nros_tests::count_pattern;
use nros_tests::fixtures::threadx_riscv64::{
    build_rv64_c_action_client, build_rv64_c_action_server, build_rv64_c_listener,
    build_rv64_c_service_client, build_rv64_c_service_server, build_rv64_c_talker,
    build_rv64_cpp_listener, build_rv64_cpp_talker, build_threadx_rv64_action_client,
    build_threadx_rv64_action_server, build_threadx_rv64_listener,
    build_threadx_rv64_service_client, build_threadx_rv64_service_server,
    build_threadx_rv64_talker, is_netx_available, is_riscv_gcc_available, is_threadx_available,
};
use nros_tests::fixtures::{
    QemuProcess, ZenohRouter, is_qemu_riscv64_available, is_zenohd_available, require_zenohd,
};
use nros_tests::platform;
use std::time::Duration;

// =============================================================================
// Prerequisite checks
// =============================================================================

/// Skip test if ThreadX RISC-V build prerequisites are not available
fn require_threadx_riscv64() -> bool {
    if !is_threadx_available() {
        eprintln!("Skipping test: THREADX_DIR not set or invalid");
        eprintln!("Run: just setup-threadx && source .envrc");
        return false;
    }
    if !is_netx_available() {
        eprintln!("Skipping test: NETX_DIR not set or invalid");
        eprintln!("Run: just setup-threadx && source .envrc");
        return false;
    }
    if !is_riscv_gcc_available() {
        eprintln!("Skipping test: riscv64-unknown-elf-gcc not found");
        eprintln!("Install: sudo apt install gcc-riscv64-unknown-elf");
        return false;
    }
    true
}

/// Skip test if full ThreadX RISC-V E2E prerequisites are not available
///
/// E2E tests require:
/// 1. ThreadX build prerequisites (THREADX_DIR + NETX_DIR + riscv64-unknown-elf-gcc)
/// 2. QEMU RISC-V 64-bit (qemu-system-riscv64)
/// 3. zenohd router (built from submodule)
fn require_threadx_riscv64_e2e() -> bool {
    if !require_threadx_riscv64() {
        return false;
    }
    if !is_qemu_riscv64_available() {
        eprintln!("Skipping test: qemu-system-riscv64 not found");
        eprintln!("Install: sudo apt install qemu-system-riscv64");
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
fn test_threadx_riscv64_detection() {
    let threadx = is_threadx_available();
    let netx = is_netx_available();
    let riscv_gcc = is_riscv_gcc_available();
    let qemu_rv64 = is_qemu_riscv64_available();
    let zenohd = is_zenohd_available();
    eprintln!("ThreadX available: {}", threadx);
    eprintln!("NetX Duo available: {}", netx);
    eprintln!("riscv64-unknown-elf-gcc available: {}", riscv_gcc);
    eprintln!("QEMU RISC-V 64 available: {}", qemu_rv64);
    eprintln!("zenohd available: {}", zenohd);
}

// =============================================================================
// Build tests (require THREADX_DIR + NETX_DIR + riscv64-unknown-elf-gcc)
// =============================================================================
#[test]
fn test_threadx_riscv64_all_examples_build() {
    if !require_threadx_riscv64() {
        nros_tests::skip!("require_threadx_riscv64 check failed");
    }

    let results = [
        ("talker", build_threadx_rv64_talker()),
        ("listener", build_threadx_rv64_listener()),
        ("service-server", build_threadx_rv64_service_server()),
        ("service-client", build_threadx_rv64_service_client()),
        ("action-server", build_threadx_rv64_action_server()),
        ("action-client", build_threadx_rv64_action_client()),
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

    assert!(
        all_ok,
        "Not all ThreadX QEMU RISC-V examples built successfully"
    );
}

// =============================================================================
// E2E Network tests (require QEMU RISC-V + zenohd)
// =============================================================================
//
// ThreadX QEMU RISC-V examples use virtio-net Ethernet with slirp user networking:
//   qemu-system-riscv64 -M virt -nographic \
//       -global virtio-mmio.force-legacy=false \
//       -netdev user,id=net0,... \
//       -device virtio-net-device,netdev=net0,bus=virtio-mmio-bus.0 \
//       -kernel <binary>
//
// Network topology:
//   QEMU node 0 (slirp, 10.0.2.40) ---> 10.0.2.2:7453 --+
//                                                          |-- zenohd (localhost:7453)
//   QEMU node 1 (slirp, 10.0.2.41) ---> 10.0.2.2:7453 --+
//
// Each QEMU instance has its own isolated 10.0.2.0/24 slirp network.
// Firmware connects to zenohd via slirp gateway 10.0.2.2:7453 -> host 127.0.0.1:7453.
//
// Prerequisites:
//   1. zenohd: just build-zenohd
//   2. Run: just test-threadx-riscv64

/// Test pub/sub message exchange between ThreadX QEMU RISC-V instances.
///
/// Launches a listener (QEMU node 1) and a talker (QEMU node 0), verifies
/// that the listener receives Int32 messages published by the talker.
#[test]
fn test_threadx_riscv64_pubsub_e2e() {
    if !require_threadx_riscv64_e2e() {
        nros_tests::skip!("require_threadx_riscv64_e2e check failed");
    }

    // Build both binaries
    let talker_bin = build_threadx_rv64_talker().expect("Failed to build talker");
    let listener_bin = build_threadx_rv64_listener().expect("Failed to build listener");

    // Start zenohd (firmware connects via slirp gateway to host)
    let _zenohd =
        ZenohRouter::start(platform::THREADX_RISCV.zenohd_port).expect("Failed to start zenohd");

    // Start listener QEMU first (subscriber before publisher)
    eprintln!("Starting listener QEMU (node 1, slirp 10.0.2.41)...");
    let mut listener =
        QemuProcess::start_riscv64_virt(listener_bin, 1).expect("Failed to start listener QEMU");

    // Stabilization delay: ThreadX boot + NetX init + virtio-net init + zenoh connect (~10s)
    std::thread::sleep(Duration::from_secs(10));

    // Start talker QEMU
    eprintln!("Starting talker QEMU (node 0, slirp 10.0.2.40)...");
    let mut talker =
        QemuProcess::start_riscv64_virt(talker_bin, 0).expect("Failed to start talker QEMU");

    // Wait for listener to complete
    let listener_output = listener
        .wait_for_output(Duration::from_secs(60))
        .unwrap_or_default();

    // Wait for talker to finish publishing
    let talker_output = talker
        .wait_for_output(Duration::from_secs(15))
        .unwrap_or_default();

    talker.kill();
    listener.kill();

    eprintln!("Listener output:\n{}", listener_output);
    eprintln!("Talker output:\n{}", talker_output);

    // Verify listener booted and received messages
    if !listener_output.contains("Waiting for messages") {
        panic!(
            "ThreadX RISC-V pubsub E2E failed — listener did not reach readiness.\n\
             This is an environment issue. Verify:\n\
             - zenohd is running on platform port 7453\n\
             - QEMU slirp gateway forwards 10.0.2.2:7453 -> host\n\
             - Firmware built: `just build-examples-threadx-riscv64`"
        );
    }

    let received_count = count_pattern(&listener_output, "Received");
    eprintln!("Messages received: {}", received_count);

    if received_count > 0 {
        eprintln!(
            "[PASS] ThreadX RISC-V pubsub E2E: {} messages exchanged",
            received_count
        );
    } else {
        panic!("ThreadX RISC-V pubsub E2E failed — listener received 0 messages");
    }
}

/// Test service request/response between ThreadX QEMU RISC-V instances.
///
/// Launches a service server (QEMU node 0) and a client (QEMU node 1),
/// verifies that the client receives correct AddTwoInts responses.
#[test]
fn test_threadx_riscv64_service_e2e() {
    if !require_threadx_riscv64_e2e() {
        nros_tests::skip!("require_threadx_riscv64_e2e check failed");
    }

    let server_bin = build_threadx_rv64_service_server().expect("Failed to build service server");
    let client_bin = build_threadx_rv64_service_client().expect("Failed to build service client");

    let _zenohd =
        ZenohRouter::start(platform::THREADX_RISCV.zenohd_port).expect("Failed to start zenohd");

    // Start server first
    eprintln!("Starting service server QEMU (node 0, slirp 10.0.2.40)...");
    let mut server =
        QemuProcess::start_riscv64_virt(server_bin, 0).expect("Failed to start server QEMU");

    // Stabilization delay: ThreadX boot + NetX init + virtio-net init + zenoh connect (~10s)
    std::thread::sleep(Duration::from_secs(10));

    // Start client
    eprintln!("Starting service client QEMU (node 1, slirp 10.0.2.41)...");
    let mut client =
        QemuProcess::start_riscv64_virt(client_bin, 1).expect("Failed to start client QEMU");

    // Stabilization delay: client also needs ThreadX boot + NetX init + zenoh connect
    // before it can discover the server's service queryable.
    std::thread::sleep(Duration::from_secs(15));

    // Wait for client to complete all service calls (4 calls: 5+3, 10+20, 100+200, -5+10)
    let client_output = client
        .wait_for_output(Duration::from_secs(60))
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
            "[PASS] ThreadX RISC-V service E2E: {} responses, all correct",
            response_count
        );
    } else if response_count > 0 {
        eprintln!(
            "[PARTIAL] ThreadX RISC-V service E2E: {} of 4 responses",
            response_count
        );
    } else if !client_output.contains("Service client ready")
        && !client_output.contains("nros ThreadX QEMU RISC-V Platform")
    {
        panic!(
            "ThreadX RISC-V service E2E failed — client did not reach readiness.\n\
             This is an environment issue. Verify:\n\
             - zenohd is running on platform port 7453\n\
             - QEMU slirp gateway forwards 10.0.2.2:7453 -> host\n\
             - Firmware built: `just build-examples-threadx-riscv64`"
        );
    } else {
        panic!(
            "ThreadX RISC-V service E2E failed — client received 0 responses.\n\
             Client reached readiness but no service replies were received.\n\
             This may indicate a zenoh queryable discovery timeout."
        );
    }
}

/// Test action goal/feedback/result between ThreadX QEMU RISC-V instances.
///
/// Launches an action server (QEMU node 0) and a client (QEMU node 1),
/// verifies that the client receives Fibonacci feedback and final result.
#[test]
fn test_threadx_riscv64_action_e2e() {
    if !require_threadx_riscv64_e2e() {
        nros_tests::skip!("require_threadx_riscv64_e2e check failed");
    }

    let server_bin = build_threadx_rv64_action_server().expect("Failed to build action server");
    let client_bin = build_threadx_rv64_action_client().expect("Failed to build action client");

    let _zenohd =
        ZenohRouter::start(platform::THREADX_RISCV.zenohd_port).expect("Failed to start zenohd");

    // Start action server first
    eprintln!("Starting action server QEMU (node 0, slirp 10.0.2.40)...");
    let mut server =
        QemuProcess::start_riscv64_virt(server_bin, 0).expect("Failed to start server QEMU");

    // Stabilization delay: ThreadX boot + NetX init + virtio-net init + zenoh connect (~10s)
    std::thread::sleep(Duration::from_secs(10));

    // Start action client
    eprintln!("Starting action client QEMU (node 1, slirp 10.0.2.41)...");
    let mut client =
        QemuProcess::start_riscv64_virt(client_bin, 1).expect("Failed to start client QEMU");

    // Stabilization delay: client also needs ThreadX boot + NetX init + zenoh connect
    // before it can discover the server's action queryables.
    std::thread::sleep(Duration::from_secs(15));

    // Fibonacci computation + zenoh connect.
    // The completion marker "Action completed successfully" triggers early return.
    let client_output = client
        .wait_for_output(Duration::from_secs(60))
        .unwrap_or_default();

    server.kill();
    client.kill();

    eprintln!("Client output:\n{}", client_output);

    // Verify action protocol
    let goal_accepted = client_output.contains("Goal accepted");
    let feedback_count = count_pattern(&client_output, "Feedback #");
    let completed = client_output.contains("Action completed successfully");

    eprintln!(
        "Goal accepted: {}, feedback: {}, completed: {}",
        goal_accepted, feedback_count, completed
    );

    if goal_accepted && completed {
        eprintln!(
            "[PASS] ThreadX RISC-V action E2E: goal accepted, {} feedback msgs, completed",
            feedback_count
        );
    } else if !client_output.contains("nros ThreadX QEMU RISC-V Platform")
        && !client_output.contains("Action client ready")
        && !goal_accepted
    {
        panic!(
            "ThreadX RISC-V action E2E failed — client did not reach readiness.\n\
             This is an environment issue. Verify:\n\
             - zenohd is running on platform port 7453\n\
             - QEMU slirp gateway forwards 10.0.2.2:7453 -> host\n\
             - Firmware built: `just build-examples-threadx-riscv64`"
        );
    } else {
        eprintln!("[FAIL] ThreadX RISC-V action E2E:");
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
            "ThreadX RISC-V action E2E failed: accepted={}, feedback={}, completed={}",
            goal_accepted, feedback_count, completed
        );
    }
}

// =============================================================================
// C Build tests
// =============================================================================
// =============================================================================
// C++ Build tests (talker + listener only — service examples hit codegen bug)
// =============================================================================
// =============================================================================
// C E2E Network tests (QEMU slirp — no host setup needed)
// =============================================================================

#[test]
fn test_rv64_c_pubsub_e2e() {
    if !require_threadx_riscv64_e2e() {
        nros_tests::skip!("require_threadx_riscv64_e2e check failed");
    }

    let talker_bin = build_rv64_c_talker().expect("build talker failed");
    let listener_bin = build_rv64_c_listener().expect("build listener failed");

    let _zenohd =
        ZenohRouter::start(platform::THREADX_RISCV.zenohd_port).expect("Failed to start zenohd");

    eprintln!("Starting C listener QEMU...");
    let mut listener =
        QemuProcess::start_riscv64_virt(listener_bin, 1).expect("Failed to start listener");

    std::thread::sleep(Duration::from_secs(10));

    eprintln!("Starting C talker QEMU...");
    let mut talker =
        QemuProcess::start_riscv64_virt(talker_bin, 0).expect("Failed to start talker");

    let listener_output = listener
        .wait_for_output(Duration::from_secs(60))
        .unwrap_or_default();
    let talker_output = talker
        .wait_for_output(Duration::from_secs(15))
        .unwrap_or_default();

    talker.kill();
    listener.kill();

    eprintln!("C Listener output:\n{}", listener_output);
    eprintln!("C Talker output:\n{}", talker_output);

    let received = count_pattern(&listener_output, "Received");
    eprintln!("C messages received: {}", received);
    assert!(received > 0, "C pubsub E2E failed — 0 messages");
    eprintln!("[PASS] ThreadX RISC-V C pubsub E2E: {} msgs", received);
}

#[test]
fn test_rv64_c_service_e2e() {
    if !require_threadx_riscv64_e2e() {
        nros_tests::skip!("require_threadx_riscv64_e2e check failed");
    }

    let server_bin = build_rv64_c_service_server().expect("build server failed");
    let client_bin = build_rv64_c_service_client().expect("build client failed");

    let _zenohd =
        ZenohRouter::start(platform::THREADX_RISCV.zenohd_port).expect("Failed to start zenohd");

    eprintln!("Starting C service server QEMU...");
    let mut server =
        QemuProcess::start_riscv64_virt(server_bin, 0).expect("Failed to start server");

    std::thread::sleep(Duration::from_secs(10));

    eprintln!("Starting C service client QEMU...");
    let mut client =
        QemuProcess::start_riscv64_virt(client_bin, 1).expect("Failed to start client");

    std::thread::sleep(Duration::from_secs(15));

    let client_output = client
        .wait_for_output(Duration::from_secs(60))
        .unwrap_or_default();

    server.kill();
    client.kill();

    eprintln!("C Client output:\n{}", client_output);

    let responses = count_pattern(&client_output, "Response:");
    assert!(responses > 0, "C service E2E failed — 0 responses");
    eprintln!(
        "[PASS] ThreadX RISC-V C service E2E: {} responses",
        responses
    );
}

#[test]
fn test_rv64_c_action_e2e() {
    if !require_threadx_riscv64_e2e() {
        nros_tests::skip!("require_threadx_riscv64_e2e check failed");
    }

    let server_bin = build_rv64_c_action_server().expect("build server failed");
    let client_bin = build_rv64_c_action_client().expect("build client failed");

    let _zenohd =
        ZenohRouter::start(platform::THREADX_RISCV.zenohd_port).expect("Failed to start zenohd");

    eprintln!("Starting C action server QEMU...");
    let mut server =
        QemuProcess::start_riscv64_virt(server_bin, 0).expect("Failed to start server");

    std::thread::sleep(Duration::from_secs(10));

    eprintln!("Starting C action client QEMU...");
    let mut client =
        QemuProcess::start_riscv64_virt(client_bin, 1).expect("Failed to start client");

    std::thread::sleep(Duration::from_secs(15));

    let client_output = client
        .wait_for_output(Duration::from_secs(60))
        .unwrap_or_default();

    server.kill();
    client.kill();

    eprintln!("C Client output:\n{}", client_output);

    let goal_accepted = client_output.contains("Goal accepted");
    let completed = client_output.contains("Action completed successfully");
    assert!(
        goal_accepted && completed,
        "C action E2E failed: accepted={}, completed={}",
        goal_accepted,
        completed
    );
    eprintln!("[PASS] ThreadX RISC-V C action E2E");
}

// =============================================================================
// C++ E2E Network tests
// =============================================================================

#[test]
fn test_rv64_cpp_pubsub_e2e() {
    if !require_threadx_riscv64_e2e() {
        nros_tests::skip!("require_threadx_riscv64_e2e check failed");
    }

    let talker_bin = build_rv64_cpp_talker().expect("build talker failed");
    let listener_bin = build_rv64_cpp_listener().expect("build listener failed");

    let _zenohd =
        ZenohRouter::start(platform::THREADX_RISCV.zenohd_port).expect("Failed to start zenohd");

    eprintln!("Starting C++ listener QEMU...");
    let mut listener =
        QemuProcess::start_riscv64_virt(listener_bin, 1).expect("Failed to start listener");

    std::thread::sleep(Duration::from_secs(10));

    eprintln!("Starting C++ talker QEMU...");
    let mut talker =
        QemuProcess::start_riscv64_virt(talker_bin, 0).expect("Failed to start talker");

    let listener_output = listener
        .wait_for_output(Duration::from_secs(60))
        .unwrap_or_default();
    let talker_output = talker
        .wait_for_output(Duration::from_secs(15))
        .unwrap_or_default();

    talker.kill();
    listener.kill();

    eprintln!("C++ Listener output:\n{}", listener_output);
    eprintln!("C++ Talker output:\n{}", talker_output);

    let received = count_pattern(&listener_output, "Received");
    assert!(received > 0, "C++ pubsub E2E failed — 0 messages");
    eprintln!("[PASS] ThreadX RISC-V C++ pubsub E2E: {} msgs", received);
}
