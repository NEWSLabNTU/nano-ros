//! Build script for zephyr-posix-sys.
//!
//! Extracts include paths from a Zephyr build tree's compile_commands.json,
//! then runs bindgen to generate Rust FFI bindings for Zephyr POSIX sockets.
//!
//! Requires:
//!   ZEPHYR_BUILD_DIR — Path to a Zephyr build directory (e.g., build-talker/)
//!     Must contain compile_commands.json and zephyr/include/generated/

use std::env;
use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-changed=wrapper.h");
    println!("cargo:rerun-if-env-changed=ZEPHYR_BUILD_DIR");

    // Auto-detect Zephyr build dir following the Zephyr workspace convention:
    // ZEPHYR_BUILD_DIR env var, or ZEPHYR_WORKSPACE/build-talker (same logic as justfile).
    let build_dir = if let Ok(d) = env::var("ZEPHYR_BUILD_DIR") {
        PathBuf::from(d)
    } else {
        // Try zephyr-workspace/build-talker (in-repo), then ../nano-ros-workspace/build-talker
        let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
        let repo_root = manifest_dir
            .parent().unwrap()  // drivers/
            .parent().unwrap()  // packages/
            .parent().unwrap(); // repo root
        let candidates = [
            repo_root.join("zephyr-workspace/build-talker"),
            repo_root.parent().unwrap_or(repo_root).join("nano-ros-workspace/build-talker"),
        ];
        match candidates.iter().find(|p| p.join("compile_commands.json").exists()) {
            Some(p) => p.clone(),
            None => {
                emit_placeholder_bindings();
                return;
            }
        }
    };

    let compile_commands = build_dir.join("compile_commands.json");
    if !compile_commands.exists() {
        emit_placeholder_bindings();
        return;
    }

    // Extract -I paths and -D defines from compile_commands.json
    let (includes, defines) = match extract_flags(&compile_commands) {
        Some(flags) => flags,
        None => {
            emit_placeholder_bindings();
            return;
        }
    };

    let mut builder = bindgen::Builder::default()
        .header("wrapper.h")
        .use_core()
        .ctypes_prefix("core::ffi")
        // Types
        .allowlist_type("zsock_addrinfo")
        .allowlist_type("sockaddr")
        .allowlist_type("sockaddr_in")
        .allowlist_type("sockaddr_in6")
        .allowlist_type("timeval")
        .allowlist_type("linger")
        // Functions
        .allowlist_function("zsock_socket")
        .allowlist_function("zsock_connect")
        .allowlist_function("zsock_bind")
        .allowlist_function("zsock_listen")
        .allowlist_function("zsock_accept")
        .allowlist_function("zsock_recv")
        .allowlist_function("zsock_recvfrom")
        .allowlist_function("zsock_send")
        .allowlist_function("zsock_sendto")
        .allowlist_function("zsock_setsockopt")
        .allowlist_function("zsock_close")
        .allowlist_function("zsock_shutdown")
        .allowlist_function("zsock_fcntl")
        .allowlist_function("zsock_getaddrinfo")
        .allowlist_function("zsock_freeaddrinfo")
        .allowlist_function("zsock_select")
        // Also allow POSIX compat names (macros → zsock_*)
        .allowlist_function("socket")
        .allowlist_function("connect")
        .allowlist_function("getaddrinfo")
        .allowlist_function("freeaddrinfo")
        // Constants
        .allowlist_var("AF_INET")
        .allowlist_var("AF_INET6")
        .allowlist_var("AF_UNSPEC")
        .allowlist_var("SOCK_STREAM")
        .allowlist_var("SOCK_DGRAM")
        .allowlist_var("IPPROTO_TCP")
        .allowlist_var("IPPROTO_UDP")
        .allowlist_var("SOL_SOCKET")
        .allowlist_var("SO_RCVTIMEO")
        .allowlist_var("SO_SNDTIMEO")
        .allowlist_var("SO_KEEPALIVE")
        .allowlist_var("SO_REUSEADDR")
        .allowlist_var("SO_LINGER")
        .allowlist_var("TCP_NODELAY")
        .allowlist_var("O_NONBLOCK")
        .allowlist_var("F_GETFL")
        .allowlist_var("F_SETFL")
        .allowlist_var("SHUT_RDWR")
        .allowlist_var("ZSOCK_SHUT_RDWR")
        // No layout tests
        .layout_tests(false)
        .derive_debug(false)
        .derive_default(true)
        .derive_copy(true);

    // Zephyr native_sim builds with GCC for x86_64
    builder = builder
        .clang_arg("--target=x86_64-linux-gnu")
        .clang_arg("-D__x86_64__");

    // Add all include paths and defines from the Zephyr build
    // (includes already have -I or -isystem prefix from extract_flags)
    for inc in &includes {
        builder = builder.clang_arg(inc);
    }
    for def in &defines {
        builder = builder.clang_arg(format!("-D{def}"));
    }

    match builder.generate() {
        Ok(bindings) => {
            let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
            bindings
                .write_to_file(out_dir.join("bindings.rs"))
                .expect("Failed to write bindings");
        }
        Err(e) => {
            println!("cargo:warning=bindgen failed: {e}. Using placeholder bindings.");
            emit_placeholder_bindings();
        }
    }
}

/// Extract -I, -isystem, and -D flags from compile_commands.json.
/// Looks for a C file containing "socket" or "network" in its path.
fn extract_flags(compile_commands: &std::path::Path) -> Option<(Vec<String>, Vec<String>)> {
    let data = std::fs::read_to_string(compile_commands).ok()?;
    let commands: Vec<serde_json::Value> = serde_json::from_str(&data).ok()?;

    for cmd in &commands {
        let file = cmd.get("file")?.as_str()?;
        if file.contains("socket") || file.contains("network") || file.contains("net_core") {
            let command = cmd.get("command")?.as_str()?;
            let mut includes: Vec<String> = Vec::new();
            let mut defines: Vec<String> = Vec::new();
            let words: Vec<&str> = command.split_whitespace().collect();
            let mut i = 0;
            while i < words.len() {
                let w = words[i];
                if w.starts_with("-I") {
                    includes.push(format!("-I{}", &w[2..]));
                } else if w == "-isystem" && i + 1 < words.len() {
                    includes.push(format!("-isystem{}", words[i + 1]));
                    i += 1;
                } else if w.starts_with("-isystem") {
                    includes.push(w.to_string());
                } else if w.starts_with("-D") {
                    defines.push(w[2..].to_string());
                }
                i += 1;
            }
            if !includes.is_empty() {
                return Some((includes, defines));
            }
        }
    }
    None
}

fn emit_placeholder_bindings() {
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    std::fs::write(
        out_dir.join("bindings.rs"),
        r#"
// Placeholder — Zephyr build tree not available.
// Set ZEPHYR_BUILD_DIR to a Zephyr build directory for real bindings.

pub type socklen_t = usize;

pub const AF_INET: u32 = 1;
pub const AF_UNSPEC: u32 = 0;
pub const SOCK_STREAM: u32 = 1;
pub const SOCK_DGRAM: u32 = 2;
pub const IPPROTO_TCP: u32 = 6;
pub const IPPROTO_UDP: u32 = 17;
pub const SOL_SOCKET: u32 = 1;
pub const SO_KEEPALIVE: u32 = 9;
pub const SO_LINGER: u32 = 13;
pub const SO_RCVTIMEO: u32 = 20;
pub const SO_SNDTIMEO: u32 = 21;
pub const SO_REUSEADDR: u32 = 2;
pub const TCP_NODELAY: u32 = 1;
pub const F_GETFL: u32 = 3;
pub const F_SETFL: u32 = 4;
pub const O_NONBLOCK: u32 = 0x4000;
pub const SHUT_RDWR: u32 = 2;
"#,
    )
    .unwrap();
}
