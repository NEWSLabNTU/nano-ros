//! Build script for nros-smoltcp
//!
//! Reads NROS_SMOLTCP_* environment variables (with ZPICO_SMOLTCP_* fallback)
//! and generates `nros_smoltcp_config.rs` with compile-time configurable constants.

use std::{env, path::Path};

fn main() {
    let out_dir = env::var("OUT_DIR").unwrap();

    // Phase 204.2 — backend-derived socket-pool defaults. smoltcp is the
    // bare-metal transport, and every bare-metal board ships a *brokered* RMW
    // (zenoh-pico `tcp/` locator, or XRCE to an agent) — bare-metal DDS/RTPS
    // over smoltcp is not built today (Phase 175.B, deferred). A brokered
    // client needs **1 TCP + ≤1 UDP** (scouting), so the default is 1/1 — no
    // hand-set env per example, and the unused-slot static buffers
    // (`SOCKET_BUFFER_SIZE` × 2 per extra slot ≈ 16 KB BSS for the old 4/4)
    // never materialise. An explicit `NROS_SMOLTCP_MAX_*` env still overrides.
    //
    // The RTPS escape hatch is the `rtps` cargo feature: a board that grows a
    // bare-metal DDS path enables `nros-smoltcp/rtps` and the UDP default jumps
    // to 4 (3 RTPS sockets/participant — default-unicast + metatraffic-unicast
    // + metatraffic-multicast — plus one spare) without anyone setting an env.
    let rtps = env::var_os("CARGO_FEATURE_RTPS").is_some();
    let max_sockets = env_usize_compat("NROS_SMOLTCP_MAX_SOCKETS", "ZPICO_SMOLTCP_MAX_SOCKETS", 1);
    let max_udp_sockets = env_usize_compat(
        "NROS_SMOLTCP_MAX_UDP_SOCKETS",
        "ZPICO_SMOLTCP_MAX_UDP_SOCKETS",
        if rtps { 4 } else { 1 },
    );
    let buffer_size = env_usize_compat(
        "NROS_SMOLTCP_BUFFER_SIZE",
        "ZPICO_SMOLTCP_BUFFER_SIZE",
        2048,
    );
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

    if max_udp_sockets > 4 {
        panic!(
            "NROS_SMOLTCP_MAX_UDP_SOCKETS={max_udp_sockets} exceeds 4. \
             Increasing beyond 4 requires adding static UDP buffer \
             declarations in nros-smoltcp/src/lib.rs."
        );
    }

    let contents = format!(
        "/// Maximum number of concurrent TCP sockets \
         (NROS_SMOLTCP_MAX_SOCKETS; backend-derived default 1 — brokered).\n\
         pub const MAX_SOCKETS: usize = {max_sockets};\n\
         \n\
         /// Maximum number of concurrent UDP sockets \
         (NROS_SMOLTCP_MAX_UDP_SOCKETS; default 1 brokered / 4 with the `rtps` feature).\n\
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

    std::fs::write(Path::new(&out_dir).join("nros_smoltcp_config.rs"), contents).unwrap();
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
