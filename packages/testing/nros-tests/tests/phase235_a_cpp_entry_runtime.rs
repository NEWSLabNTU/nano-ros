//! Phase 235.A.4 — native C++ Entry-pkg **runtime** E2E.
//!
//! Phase 219 wired the C++ Entry-codegen path (`nano_ros_entry(LAUNCH …)`
//! → generated `main()` → `nros::board::NativeBoard::run(register_fn)`),
//! but `NativeBoard` installed a *recording* `NodeContextOps` whose ops
//! were no-ops — the register sequence was dispatched yet constructed
//! **nothing live**. `cpp_multi_node_entry.rs` guards that the build
//! system + codegen plumbing produce a linked `robot_entry` exe, but
//! explicitly does NOT check runtime behaviour.
//!
//! Phase 235.A replaced the recording ops with the real
//! `NativeNodeRuntime` (`packages/core/nros-cpp/include/nros/main.hpp`):
//! `create_node` → `nros::create_node`, `create_entity` → the matching
//! raw `nros_cpp_*_create` FFI, `record_callback_effect` → poll-loop
//! wiring (a timer-driven `Publishes` effect synthesizes a monotonic
//! `std_msgs/Int32` counter; a `Reads` effect drains its subscription).
//!
//! This test proves the runtime is **live** with the external-observer
//! style of RFC-0032 §8: build the in-tree
//! `examples/templates/multi-node-workspace-cpp` Entry pkg, boot it for a
//! bounded window (`NROS_ENTRY_SPIN_MS`), and confirm a stock native
//! Rust `listener` — a separate process subscribing to `/chatter` over
//! the same zenohd router — actually receives the talker node's samples.
//!
//! FAILS (not skips) when cmake / a C++ compiler / zenohd are absent —
//! same "no silent skip" rule the rest of the Entry-pkg suite follows.
//! Skip-via-panic (`nros_tests::skip!`) only when the host `nros` CLI
//! (with the Phase 219 `codegen entry` subcommand) isn't resolvable.

use std::{
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};

use nros_tests::{
    fixtures::{ManagedProcess, ZenohRouter, build_native_listener, require_zenohd, zenohd_unique},
    output::parse_listener,
};
use rstest::rstest;

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent() // packages/testing/
        .and_then(Path::parent) // packages/
        .and_then(Path::parent) // <root>
        .expect("workspace root")
        .to_path_buf()
}

/// Resolve a `nros` CLI binary the cmake fn can shell. Mirrors
/// `cpp_multi_node_entry.rs::resolve_nros_bin` (priority `NROS_CLI` env →
/// PATH → `~/.nros/bin/nros`).
fn resolve_nros_bin() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("NROS_CLI") {
        let p = PathBuf::from(p);
        if p.is_file() {
            return Some(p);
        }
    }
    if Command::new("nros")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success())
    {
        return Some(PathBuf::from("nros"));
    }
    let home = std::env::var_os("NROS_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| Path::new(&h).join(".nros")));
    let candidate = home?.join("bin/nros");
    candidate.is_file().then_some(candidate)
}

/// Configure + build the multi-node-workspace-cpp Entry pkg, returning the
/// `robot_entry` executable path. Panics on a real cmake/compile failure;
/// the caller handles the missing-CLI skip.
fn build_robot_entry(nros_bin: &Path) -> PathBuf {
    let root = workspace_root();
    let src = root.join("examples/templates/multi-node-workspace-cpp");
    assert!(
        src.join("src/robot_entry/CMakeLists.txt").is_file(),
        "fixture missing: src/robot_entry"
    );

    // Persist the build tree across runs would be nice, but keep it simple
    // + hermetic: a fresh tempdir per invocation (the test is heavyweight
    // and runs in its own nextest slot).
    let build = std::env::temp_dir().join(format!("nros-235a-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&build);

    let cfg = Command::new("cmake")
        .arg("-S")
        .arg(&src)
        .arg("-B")
        .arg(&build)
        .arg(format!("-DNROS_CLI_BIN={}", nros_bin.display()))
        .output()
        .expect("run cmake configure");
    assert!(
        cfg.status.success(),
        "cmake configure failed:\n--- stdout ---\n{}\n--- stderr ---\n{}",
        String::from_utf8_lossy(&cfg.stdout),
        String::from_utf8_lossy(&cfg.stderr)
    );

    let blk = Command::new("cmake")
        .arg("--build")
        .arg(&build)
        .arg("-j")
        .output()
        .expect("run cmake build");
    assert!(
        blk.status.success(),
        "cmake build failed:\n--- stdout ---\n{}\n--- stderr ---\n{}",
        String::from_utf8_lossy(&blk.stdout),
        String::from_utf8_lossy(&blk.stderr)
    );

    let exe = build.join("src/robot_entry/robot_entry");
    assert!(
        exe.is_file(),
        "robot_entry exe missing at {}",
        exe.display()
    );
    exe
}

#[rstest]
fn cpp_entry_runtime_publishes_live_samples(zenohd_unique: ZenohRouter) {
    if !Command::new("cmake")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success())
    {
        panic!("cmake not available on PATH; install cmake to run this test");
    }
    if !require_zenohd() {
        panic!("zenohd not found; build it via `just zenohd setup` to run this test");
    }

    let Some(nros_bin) = resolve_nros_bin() else {
        nros_tests::skip!(
            "no `nros` CLI resolved (tried $NROS_CLI / PATH / ~/.nros/bin); set NROS_CLI to an \
             in-tree `packages/cli/target/<profile>/nros` that supports `codegen entry`"
        );
    };
    let supports_codegen_entry = Command::new(&nros_bin)
        .args(["codegen", "entry", "--help"])
        .output()
        .is_ok_and(|o| o.status.success());
    if !supports_codegen_entry {
        nros_tests::skip!(
            "resolved `nros` CLI at `{}` lacks `codegen entry` (Phase 219); set NROS_CLI to a \
             build from this branch",
            nros_bin.display()
        );
    }

    // Heavy build: the Entry pkg links the full nros-cpp + zenoh backend.
    let robot_entry = build_robot_entry(&nros_bin);
    let listener_bin = build_native_listener().expect("build native listener");

    let locator = zenohd_unique.locator();

    // External observer first: a stock native listener on /chatter.
    let mut listener_cmd = Command::new(listener_bin);
    listener_cmd
        .env("RUST_LOG", "info")
        .env("NROS_LOCATOR", &locator)
        .env("NROS_SESSION_MODE", "client");
    let mut listener =
        ManagedProcess::spawn_command(listener_cmd, "ext-listener").expect("spawn listener");
    listener
        .wait_for_output_pattern("Waiting for", Duration::from_secs(8))
        .expect("listener did not become ready");

    // Boot the C++ Entry pkg for a bounded window — the synthesized
    // talker fires a 1 Hz Int32 counter through the live runtime.
    let mut entry_cmd = Command::new(&robot_entry);
    entry_cmd
        .env("RUST_LOG", "info")
        .env("NROS_LOCATOR", &locator)
        .env("NROS_SESSION_MODE", "client")
        .env("NROS_ENTRY_SPIN_MS", "8000");
    let mut entry =
        ManagedProcess::spawn_command(entry_cmd, "robot_entry").expect("spawn robot_entry");

    let listener_output = listener
        .wait_for_output_count("Received:", 1, Duration::from_secs(20))
        .unwrap_or_else(|e| {
            entry.kill();
            listener.kill();
            panic!("external listener never received a sample from the C++ Entry runtime: {e}");
        });

    entry.kill();
    listener.kill();

    println!("=== ext-listener output ===\n{listener_output}");
    let parsed = parse_listener(&listener_output);
    assert!(
        !parsed.values.is_empty(),
        "expected ≥1 Int32 sample from the live C++ Entry talker, got none:\n{listener_output}"
    );
    println!(
        "SUCCESS: C++ Entry runtime published {} live sample(s): {:?}",
        parsed.values.len(),
        parsed.values
    );
}
