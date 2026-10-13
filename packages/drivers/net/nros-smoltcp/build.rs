//! Build script for nros-smoltcp
//!
//! Resolves the `NROS_SMOLTCP_*` knobs through the one ladder
//! (`nros_zephyr_build::knob`, which also reads each knob's legacy
//! `ZPICO_SMOLTCP_*` spelling) and generates `nros_smoltcp_config.rs`.

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

    // phase-400 W6 — the `[knobs.net]` platform/board rungs, resolved once.
    // `None` when no lane named a platform (every out-of-tree consumer and every
    // plain `cargo build`), and the builtins below then stand.
    //
    // Each rung goes in through `.rung()`, below the environment and above the
    // builtin: `NROS_SMOLTCP_*` still wins above it, and a builtin applies only
    // where no descriptor said anything. The old `ZPICO_SMOLTCP_*` spelling is
    // retired (phase-484 W8): set, it stops the build naming its replacement.
    //
    // This crate reads the ladder from `nros-platform-config` rather than
    // `nros-board-common`: the board crate reaches back here through
    // `nros-platform -> nros-platform-esp32-qemu -> nros-smoltcp`, and cargo
    // counts that as a cycle. The reader was moved to a leaf crate FOR this
    // tenant.
    let rungs = nros_platform_config::platform_config::BuildRungs::from_build_env()
        .map(|r| r.net_rungs())
        .unwrap_or_default();

    let max_sockets = nros_zephyr_build::knob("NROS_SMOLTCP_MAX_SOCKETS")
        .rung(rungs.max_sockets)
        .resolve(1);
    // The builtin here is FEATURE-derived, and a rung outranks it: a board that
    // names the knob has said something specific, while `rtps` is a default good
    // enough for a board that has not.
    let max_udp_sockets = nros_zephyr_build::knob("NROS_SMOLTCP_MAX_UDP_SOCKETS")
        .rung(rungs.max_udp_sockets)
        .resolve(if rtps { 4 } else { 1 });
    let buffer_size = nros_zephyr_build::knob("NROS_SMOLTCP_BUFFER_SIZE")
        .rung(rungs.buffer_size)
        .resolve(2048);
    let connect_timeout_ms = nros_zephyr_build::knob("NROS_SMOLTCP_CONNECT_TIMEOUT_MS")
        .rung(rungs.connect_timeout_ms)
        .resolve(30_000);
    let socket_timeout_ms = nros_zephyr_build::knob("NROS_SMOLTCP_SOCKET_TIMEOUT_MS")
        .rung(rungs.socket_timeout_ms)
        .resolve(10_000);

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
