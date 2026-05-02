//! Native C / C++ API integration tests.
//!
//! Exercises the CMake-built C and C++ examples (talker, listener,
//! service-server/-client, action-server/-client) as a single parametrised
//! suite. Language-agnostic tests are generated as two cases
//! (`::case_1_C` and `::case_2_Cpp`) via `#[rstest]`; language-specific
//! tests (interop tests that always pair with Rust, the C++ goal-rejection
//! test, the C action blocking case) stay as named functions below.
//!
//! Consolidates what used to be `c_api.rs` + `cpp_api.rs` (Phase 85.1).

use nros_tests::{
    count_pattern,
    fixtures::{
        ManagedProcess, ZenohRouter, build_c_action_client, build_c_action_server,
        build_c_listener, build_c_service_client, build_c_service_server, build_c_talker,
        build_cpp_action_client, build_cpp_action_server, build_cpp_listener,
        build_cpp_service_client, build_cpp_service_server, build_cpp_talker, require_cmake,
        require_zenohd, zenohd_unique,
    },
};
use rstest::rstest;
use std::{
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};

// =============================================================================
// Language selector
// =============================================================================

/// Which of the two native FFI surfaces the parametrised tests exercise.
#[derive(Clone, Copy, Debug)]
#[allow(clippy::upper_case_acronyms)]
enum Language {
    C,
    Cpp,
}

impl Language {
    /// Process-name prefix used in log lines (e.g. `"c-talker"`).
    fn tag(&self) -> &'static str {
        match self {
            Language::C => "c",
            Language::Cpp => "cpp",
        }
    }

    /// Human-readable label for `eprintln!` output.
    fn label(&self) -> &'static str {
        match self {
            Language::C => "C",
            Language::Cpp => "C++",
        }
    }

    /// Substring that every language-native binary prints after
    /// `nros::init` / `nros_support_init` succeeds. Used to assert that
    /// the binary started cleanly.
    fn init_marker(&self) -> &'static str {
        match self {
            // C binaries print `"Support initialized"` from `nros_support_init`.
            Language::C => "Support initialized",
            // C++ binaries print `"Node created: <name>"` from `nros::create_node`.
            Language::Cpp => "Node created",
        }
    }

    fn talker_binary(&self) -> PathBuf {
        match self {
            Language::C => build_c_talker(),
            Language::Cpp => build_cpp_talker(),
        }
        .expect("failed to build native talker")
        .to_path_buf()
    }

    fn listener_binary(&self) -> PathBuf {
        match self {
            Language::C => build_c_listener(),
            Language::Cpp => build_cpp_listener(),
        }
        .expect("failed to build native listener")
        .to_path_buf()
    }

    fn service_server_binary(&self) -> PathBuf {
        match self {
            Language::C => build_c_service_server(),
            Language::Cpp => build_cpp_service_server(),
        }
        .expect("failed to build native service server")
        .to_path_buf()
    }

    fn service_client_binary(&self) -> PathBuf {
        match self {
            Language::C => build_c_service_client(),
            Language::Cpp => build_cpp_service_client(),
        }
        .expect("failed to build native service client")
        .to_path_buf()
    }

    fn action_server_binary(&self) -> PathBuf {
        match self {
            Language::C => build_c_action_server(),
            Language::Cpp => build_cpp_action_server(),
        }
        .expect("failed to build native action server")
        .to_path_buf()
    }

    fn action_client_binary(&self) -> PathBuf {
        match self {
            Language::C => build_c_action_client(),
            Language::Cpp => build_cpp_action_client(),
        }
        .expect("failed to build native action client")
        .to_path_buf()
    }
}

/// Wrap a native binary with `stdbuf -oL -eL` to force line-buffered
/// stdout/stderr. Both C's `printf` and C++'s `std::printf` fully-buffer
/// when the output is a pipe, which breaks `wait_for_output_pattern`.
fn stdbuf_command(binary: &Path) -> Command {
    let mut cmd = Command::new("stdbuf");
    cmd.args(["-oL", "-eL"]).arg(binary);
    cmd
}

/// Skip-guard shared by every test in this file.
fn require_native_env() -> bool {
    if !require_zenohd() {
        nros_tests::skip!("zenohd not found");
    }
    if !require_cmake() {
        nros_tests::skip!("cmake not found");
    }
    true
}

// =============================================================================
// Build tests (parametrised)
// =============================================================================

#[rstest]
fn test_native_talker_builds(#[values(Language::C, Language::Cpp)] lang: Language) {
    if !require_cmake() {
        nros_tests::skip!("cmake not found");
    }
    let path = lang.talker_binary();
    eprintln!(
        "[PASS] {} talker binary built: {}",
        lang.label(),
        path.display()
    );
    assert!(path.exists());
}

#[rstest]
fn test_native_listener_builds(#[values(Language::C, Language::Cpp)] lang: Language) {
    if !require_cmake() {
        nros_tests::skip!("cmake not found");
    }
    let path = lang.listener_binary();
    eprintln!(
        "[PASS] {} listener binary built: {}",
        lang.label(),
        path.display()
    );
    assert!(path.exists());
}

#[rstest]
fn test_native_service_server_builds(#[values(Language::C, Language::Cpp)] lang: Language) {
    if !require_cmake() {
        nros_tests::skip!("cmake not found");
    }
    let path = lang.service_server_binary();
    eprintln!(
        "[PASS] {} service server binary built: {}",
        lang.label(),
        path.display()
    );
    assert!(path.exists());
}

#[rstest]
fn test_native_service_client_builds(#[values(Language::C, Language::Cpp)] lang: Language) {
    if !require_cmake() {
        nros_tests::skip!("cmake not found");
    }
    let path = lang.service_client_binary();
    eprintln!(
        "[PASS] {} service client binary built: {}",
        lang.label(),
        path.display()
    );
    assert!(path.exists());
}

#[rstest]
fn test_native_action_server_builds(#[values(Language::C, Language::Cpp)] lang: Language) {
    if !require_cmake() {
        nros_tests::skip!("cmake not found");
    }
    let path = lang.action_server_binary();
    eprintln!(
        "[PASS] {} action server binary built: {}",
        lang.label(),
        path.display()
    );
    assert!(path.exists());
}

#[rstest]
fn test_native_action_client_builds(#[values(Language::C, Language::Cpp)] lang: Language) {
    if !require_cmake() {
        nros_tests::skip!("cmake not found");
    }
    let path = lang.action_client_binary();
    eprintln!(
        "[PASS] {} action client binary built: {}",
        lang.label(),
        path.display()
    );
    assert!(path.exists());
}

// =============================================================================
// Startup tests (parametrised)
// =============================================================================

fn spawn_native(binary: &Path, lang: Language, kind: &str, locator: &str) -> ManagedProcess {
    let mut cmd = stdbuf_command(binary);
    cmd.env("NROS_LOCATOR", locator);
    let name = format!("{}-{}", lang.tag(), kind);
    ManagedProcess::spawn_command(cmd, &name).unwrap_or_else(|_| panic!("Failed to start {name}"))
}

#[rstest]
fn test_native_talker_starts(
    zenohd_unique: ZenohRouter,
    #[values(Language::C, Language::Cpp)] lang: Language,
) {
    if !require_native_env() {
        return;
    }
    let locator = zenohd_unique.locator();
    let binary = lang.talker_binary();
    let mut talker = spawn_native(&binary, lang, "talker", &locator);

    // Wait for initialization — bounded by the init marker rather than
    // a fixed sleep. `wait_for_output_pattern` times out at 10s but
    // normally returns in <1s once `nros::init` completes.
    let output = talker
        .wait_for_output_pattern(lang.init_marker(), Duration::from_secs(30))
        .unwrap_or_default();

    eprintln!("{} talker output:\n{}", lang.label(), output);
    assert!(
        output.contains(lang.init_marker()),
        "{} talker failed to initialize.\nOutput:\n{}",
        lang.label(),
        output
    );
}

#[rstest]
fn test_native_listener_starts(
    zenohd_unique: ZenohRouter,
    #[values(Language::C, Language::Cpp)] lang: Language,
) {
    if !require_native_env() {
        return;
    }
    let locator = zenohd_unique.locator();
    let binary = lang.listener_binary();
    let mut listener = spawn_native(&binary, lang, "listener", &locator);

    let output = listener
        .wait_for_output_pattern(lang.init_marker(), Duration::from_secs(30))
        .unwrap_or_default();

    eprintln!("{} listener output:\n{}", lang.label(), output);
    assert!(
        output.contains(lang.init_marker()),
        "{} listener failed to initialize.\nOutput:\n{}",
        lang.label(),
        output
    );
}

#[rstest]
fn test_native_service_server_starts(
    zenohd_unique: ZenohRouter,
    #[values(Language::C, Language::Cpp)] lang: Language,
) {
    if !require_native_env() {
        return;
    }
    let locator = zenohd_unique.locator();
    let binary = lang.service_server_binary();
    let mut server = spawn_native(&binary, lang, "service-server", &locator);

    let output = server
        .wait_for_output_pattern(lang.init_marker(), Duration::from_secs(30))
        .unwrap_or_default();

    eprintln!("{} service server output:\n{}", lang.label(), output);
    assert!(
        output.contains(lang.init_marker()),
        "{} service server failed to initialize.\nOutput:\n{}",
        lang.label(),
        output
    );
}

#[rstest]
fn test_native_action_server_starts(
    zenohd_unique: ZenohRouter,
    #[values(Language::C, Language::Cpp)] lang: Language,
) {
    if !require_native_env() {
        return;
    }
    let locator = zenohd_unique.locator();
    let binary = lang.action_server_binary();
    let mut server = spawn_native(&binary, lang, "action-server", &locator);

    let output = server
        .wait_for_output_pattern(lang.init_marker(), Duration::from_secs(30))
        .unwrap_or_default();

    eprintln!("{} action server output:\n{}", lang.label(), output);
    assert!(
        output.contains(lang.init_marker()),
        "{} action server failed to initialize.\nOutput:\n{}",
        lang.label(),
        output
    );
}

// =============================================================================
// Pub/Sub communication (parametrised)
// =============================================================================

#[rstest]
fn test_native_talker_listener_communication(
    zenohd_unique: ZenohRouter,
    #[values(Language::C, Language::Cpp)] lang: Language,
) {
    if !require_native_env() {
        return;
    }
    let locator = zenohd_unique.locator();
    let listener_bin = lang.listener_binary();
    let talker_bin = lang.talker_binary();

    let mut listener = spawn_native(&listener_bin, lang, "listener", &locator);

    // Wait for the listener to subscribe. Both C and C++ listeners
    // print "Waiting for messages" once `create_subscription` returns.
    // Keep the consumed output — the init assertion below greps it.
    let listener_boot_output = listener
        .wait_for_output_pattern("Waiting for", Duration::from_secs(30))
        .expect("listener did not become ready");

    let mut talker = spawn_native(&talker_bin, lang, "talker", &locator);

    // Fixed wait for the talker to publish a handful of messages.
    // Replacing this with a count-based pattern is a Phase 85 follow-up
    // (the talker never exits on its own, so there's no natural marker).
    std::thread::sleep(Duration::from_secs(6));

    let talker_output = talker
        .wait_for_all_output(Duration::from_secs(2))
        .unwrap_or_default();
    eprintln!("{} talker output:\n{}", lang.label(), talker_output);

    let listener_tail = listener
        .wait_for_all_output(Duration::from_secs(2))
        .unwrap_or_default();
    let listener_output = listener_boot_output + &listener_tail;
    eprintln!("{} listener output:\n{}", lang.label(), listener_output);

    assert!(
        listener_output.contains(lang.init_marker()),
        "{} listener failed to initialize.\nOutput:\n{}",
        lang.label(),
        listener_output
    );

    let received_count = count_pattern(&listener_output, "Received");
    eprintln!(
        "{} listener received {} messages",
        lang.label(),
        received_count
    );
    assert!(
        received_count >= 3,
        "Expected at least 3 messages, got {}.\nOutput:\n{}",
        received_count,
        listener_output
    );
}

// =============================================================================
// Service communication (parametrised)
// =============================================================================

#[rstest]
fn test_native_service_communication(
    zenohd_unique: ZenohRouter,
    #[values(Language::C, Language::Cpp)] lang: Language,
) {
    if !require_native_env() {
        return;
    }
    let locator = zenohd_unique.locator();
    let server_bin = lang.service_server_binary();
    let client_bin = lang.service_client_binary();

    let mut server = spawn_native(&server_bin, lang, "service-server", &locator);
    // Server prints "Waiting for service requests" after queryable registration.
    server
        .wait_for_output_pattern("Waiting for", Duration::from_secs(30))
        .expect("service server did not become ready");

    let mut client = spawn_native(&client_bin, lang, "service-client", &locator);

    // Client makes 4 blocking calls then exits.
    let client_output = client
        .wait_for_output_pattern("calls succeeded", Duration::from_secs(15))
        .or_else(|_| client.wait_for_all_output(Duration::from_secs(2)))
        .unwrap_or_default();

    server.kill();

    eprintln!("{} service client output:\n{}", lang.label(), client_output);

    let ok_count = count_pattern(&client_output, "[OK]");
    eprintln!(
        "{} service client: {} successful calls",
        lang.label(),
        ok_count
    );
    assert!(
        ok_count >= 3,
        "Expected at least 3 successful service calls, got {}.\nOutput:\n{}",
        ok_count,
        client_output
    );
}

// =============================================================================
// Action communication (one function per language — c is `#[ignore]`'d)
// =============================================================================
//
// `rstest`'s `#[values]` doesn't let a single case be `#[ignore]`'d while
// another runs, so these stay as two small wrappers around a shared body.

fn native_action_communication_body(lang: Language, locator: &str) {
    let server_bin = lang.action_server_binary();
    let client_bin = lang.action_client_binary();

    let mut server = spawn_native(&server_bin, lang, "action-server", locator);
    server
        .wait_for_output_pattern("Waiting for", Duration::from_secs(30))
        .expect("action server did not become ready");

    let mut client = spawn_native(&client_bin, lang, "action-client", locator);

    // C client signals completion with "Goodbye", C++ client with "[OK]".
    let completion_marker = match lang {
        Language::C => "Goodbye",
        Language::Cpp => "[OK]",
    };
    let client_output = client
        .wait_for_output_pattern(completion_marker, Duration::from_secs(20))
        .or_else(|_| client.wait_for_all_output(Duration::from_secs(2)))
        .unwrap_or_default();

    let server_output = server
        .wait_for_all_output(Duration::from_secs(2))
        .unwrap_or_default();
    server.kill();

    eprintln!(
        "=== {} action server output ===\n{}",
        lang.label(),
        server_output
    );
    eprintln!(
        "=== {} action client output ===\n{}",
        lang.label(),
        client_output
    );

    match lang {
        Language::C => {
            // Full accept + execute assertion for C (matches the old test).
            assert!(
                client_output.contains("Goal accepted"),
                "C action client failed to send goal or get acceptance.\nOutput:\n{}",
                client_output
            );
            assert!(
                server_output.contains("ACCEPTED") || server_output.contains("Executing goal"),
                "C action server did not process the goal.\nServer output:\n{}",
                server_output
            );
            eprintln!("[PASS] C action server/client communication works");
        }
        Language::Cpp => {
            // C++ test just checks the [OK] success marker from the client.
            assert!(
                client_output.contains("[OK]"),
                "Expected action client to succeed.\nOutput:\n{}",
                client_output
            );
        }
    }
}

#[rstest]
#[ignore = "Phase 77 WIP: blocking zpico_get in send_goal returns Timeout immediately"]
fn test_c_action_communication(zenohd_unique: ZenohRouter) {
    if !require_native_env() {
        return;
    }
    native_action_communication_body(Language::C, &zenohd_unique.locator());
}

#[rstest]
fn test_cpp_action_communication(zenohd_unique: ZenohRouter) {
    if !require_native_env() {
        return;
    }
    native_action_communication_body(Language::Cpp, &zenohd_unique.locator());
}

// =============================================================================
// C++ goal rejection (Phase 83.15) — C++ only; C action examples don't
// read NROS_TEST_GOAL_ORDER yet.
// =============================================================================

#[rstest]
fn test_cpp_action_goal_rejection(zenohd_unique: ZenohRouter) {
    if !require_native_env() {
        return;
    }
    let locator = zenohd_unique.locator();
    let server_bin = Language::Cpp.action_server_binary();
    let client_bin = Language::Cpp.action_client_binary();

    let mut server = spawn_native(&server_bin, Language::Cpp, "action-server", &locator);
    server
        .wait_for_output_pattern("Waiting for", Duration::from_secs(30))
        .expect("cpp-action-server did not become ready");

    let mut client_cmd = stdbuf_command(&client_bin);
    client_cmd.env("NROS_LOCATOR", &locator);
    // Order 100 > 64 triggers the server's goal callback to return
    // `GoalResponse::Reject`.
    client_cmd.env("NROS_TEST_GOAL_ORDER", "100");
    let mut client = ManagedProcess::spawn_command(client_cmd, "cpp-action-client")
        .expect("Failed to start cpp-action-client");

    let client_output = client
        .wait_for_output_pattern("REJECTED", Duration::from_secs(20))
        .or_else(|_| client.wait_for_all_output(Duration::from_secs(2)))
        .unwrap_or_default();

    server.kill();
    eprintln!("C++ action client output:\n{}", client_output);

    assert!(
        client_output.contains("REJECTED"),
        "Expected goal rejection marker in client output.\nOutput:\n{}",
        client_output
    );
    assert!(
        !client_output.contains("[OK]"),
        "Client should not report success on a rejected goal.\nOutput:\n{}",
        client_output
    );
}

// =============================================================================
// Cross-language interop (C ↔ Rust, C++ ↔ Rust)
// =============================================================================
//
// These pair a native binary with a Rust binary, so the Language enum can
// only represent the native half. The Rust half is always the same.

fn native_rust_pubsub_interop(lang: Language, locator: &str) {
    let rust_listener = match nros_tests::fixtures::build_native_listener() {
        Ok(p) => p.to_path_buf(),
        Err(e) => {
            eprintln!("Skipping: could not build Rust listener: {}", e);
            return;
        }
    };

    let mut listener_cmd = Command::new(&rust_listener);
    listener_cmd.env("NROS_LOCATOR", locator);
    listener_cmd.env("RUST_LOG", "info");
    let mut listener = ManagedProcess::spawn_command(listener_cmd, "rust-listener")
        .expect("Failed to start Rust listener");

    // Rust listener logs "Subscriber created" once
    // `create_subscription` succeeds.
    listener
        .wait_for_output_pattern("Subscriber created", Duration::from_secs(30))
        .expect("rust-listener did not become ready");

    let talker_bin = lang.talker_binary();
    let mut talker = spawn_native(&talker_bin, lang, "talker", locator);

    std::thread::sleep(Duration::from_secs(6));
    talker.kill();

    let listener_output = listener
        .wait_for_all_output(Duration::from_secs(2))
        .unwrap_or_default();
    eprintln!(
        "Rust listener output ({} talker):\n{}",
        lang.label(),
        listener_output
    );

    let received_count = count_pattern(&listener_output, "Received");
    eprintln!(
        "Rust listener received {} messages from {} talker",
        received_count,
        lang.label()
    );
    assert!(
        received_count >= 2,
        "Expected at least 2 cross-language messages, got {}.\nOutput:\n{}",
        received_count,
        listener_output
    );
}

#[rstest]
fn test_c_rust_pubsub_interop(zenohd_unique: ZenohRouter) {
    if !require_native_env() {
        return;
    }
    native_rust_pubsub_interop(Language::C, &zenohd_unique.locator());
}

#[rstest]
fn test_cpp_rust_pubsub_interop(zenohd_unique: ZenohRouter) {
    if !require_native_env() {
        return;
    }
    native_rust_pubsub_interop(Language::Cpp, &zenohd_unique.locator());
}

fn native_rust_service_interop(lang: Language, locator: &str) {
    let rust_client = match nros_tests::fixtures::build_native_service_client() {
        Ok(p) => p.to_path_buf(),
        Err(e) => {
            eprintln!("Skipping: could not build Rust service client: {}", e);
            return;
        }
    };

    let server_bin = lang.service_server_binary();
    let mut server = spawn_native(&server_bin, lang, "service-server", locator);
    server
        .wait_for_output_pattern("Waiting for", Duration::from_secs(30))
        .expect("native service server did not become ready");
    assert!(
        server.is_running(),
        "{} service server died during startup",
        lang.label()
    );

    let mut client_cmd = Command::new(&rust_client);
    client_cmd.env("NROS_LOCATOR", locator);
    client_cmd.env("RUST_LOG", "info");
    let mut client = ManagedProcess::spawn_command(client_cmd, "rust-service-client")
        .expect("Failed to start Rust service client");

    let client_output = client
        .wait_for_output_pattern("completed successfully", Duration::from_secs(30))
        .or_else(|_| client.wait_for_all_output(Duration::from_secs(2)))
        .unwrap_or_default();

    let server_output = server
        .wait_for_all_output(Duration::from_secs(2))
        .unwrap_or_default();

    eprintln!("{} service server output:\n{}", lang.label(), server_output);
    eprintln!(
        "Rust client output ({} server):\n{}",
        lang.label(),
        client_output
    );

    let response_count = count_pattern(&client_output, "Response:");
    eprintln!(
        "Rust client received {} responses from {} server",
        response_count,
        lang.label()
    );
    assert!(
        response_count >= 2,
        "Expected at least 2 cross-language service responses, got {}.\nOutput:\n{}",
        response_count,
        client_output
    );
}

#[rstest]
#[ignore = "Phase 77 WIP: blocking zpico_get in service call returns Timeout immediately"]
fn test_c_rust_service_interop(zenohd_unique: ZenohRouter) {
    if !require_native_env() {
        return;
    }
    native_rust_service_interop(Language::C, &zenohd_unique.locator());
}

#[rstest]
fn test_cpp_rust_service_interop(zenohd_unique: ZenohRouter) {
    if !require_native_env() {
        return;
    }
    native_rust_service_interop(Language::Cpp, &zenohd_unique.locator());
}
