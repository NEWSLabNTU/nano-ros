//! Build script for nros-smoltcp
//!
//! Reads NROS_SMOLTCP_* environment variables (with ZPICO_SMOLTCP_* fallback)
//! and generates `nros_smoltcp_config.rs` with compile-time configurable constants.

use std::env;
use std::path::Path;

fn main() {
    let out_dir = env::var("OUT_DIR").unwrap();

    let max_sockets = env_usize_compat("NROS_SMOLTCP_MAX_SOCKETS", "ZPICO_SMOLTCP_MAX_SOCKETS", 4);
    let max_udp_sockets =
        env_usize_compat("NROS_SMOLTCP_MAX_UDP_SOCKETS", "ZPICO_SMOLTCP_MAX_UDP_SOCKETS", 2);
    let buffer_size =
        env_usize_compat("NROS_SMOLTCP_BUFFER_SIZE", "ZPICO_SMOLTCP_BUFFER_SIZE", 2048);
    let connect_timeout_ms = env_usize_compat(
        "NROS_SMOLTCP_CONNECT_TIMEOUT_MS",
        "ZPICO_SMOLTCP_CONNECT_TIMEOUT_MS",
        30_000,
    );
    let socket_timeout_ms = env_usize_compat(
        "NROS_SMOLTCP_SOCKET_TIMEOUT_MS",
        "ZPICO_SMOLTCP_SOCKET_TIMEOUT_MS",
        10_000,
    );

    if max_sockets > 4 {
        panic!(
            "NROS_SMOLTCP_MAX_SOCKETS={max_sockets} exceeds 4. \
             Increasing beyond 4 requires adding static TCP buffer \
             declarations in nros-smoltcp/src/lib.rs."
        );
    }

    if max_udp_sockets > 2 {
        panic!(
            "NROS_SMOLTCP_MAX_UDP_SOCKETS={max_udp_sockets} exceeds 2. \
             Increasing beyond 2 requires adding static UDP buffer \
             declarations in nros-smoltcp/src/lib.rs."
        );
    }

    let contents = format!(
        "/// Maximum number of concurrent TCP sockets \
         (set via NROS_SMOLTCP_MAX_SOCKETS, default 4).\n\
         pub const MAX_SOCKETS: usize = {max_sockets};\n\
         \n\
         /// Maximum number of concurrent UDP sockets \
         (set via NROS_SMOLTCP_MAX_UDP_SOCKETS, default 2).\n\
         pub const MAX_UDP_SOCKETS: usize = {max_udp_sockets};\n\
         \n\
         /// Per-socket staging buffer size in bytes \
         (set via NROS_SMOLTCP_BUFFER_SIZE, default 2048).\n\
         pub const SOCKET_BUFFER_SIZE: usize = {buffer_size};\n\
         \n\
         /// Timeout for TCP connect in milliseconds \
         (set via NROS_SMOLTCP_CONNECT_TIMEOUT_MS, default 30000).\n\
         pub const CONNECT_TIMEOUT_MS: u64 = {connect_timeout_ms};\n\
         \n\
         /// Timeout for TCP read/write operations in milliseconds \
         (set via NROS_SMOLTCP_SOCKET_TIMEOUT_MS, default 10000).\n\
         pub const SOCKET_TIMEOUT_MS: u64 = {socket_timeout_ms};\n"
    );

    std::fs::write(
        Path::new(&out_dir).join("nros_smoltcp_config.rs"),
        contents,
    )
    .unwrap();
}

/// Read a usize from an environment variable, with fallback name for backward
/// compatibility (ZPICO_SMOLTCP_* → NROS_SMOLTCP_*).
fn env_usize_compat(name: &str, fallback_name: &str, default: usize) -> usize {
    println!("cargo:rerun-if-env-changed={name}");
    println!("cargo:rerun-if-env-changed={fallback_name}");
    env::var(name)
        .or_else(|_| env::var(fallback_name))
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}
