use std::env;
use std::fs;
use std::path::{Path, PathBuf};

/// Read a usize from an environment variable, falling back to a default.
fn env_usize(name: &str, default: usize) -> usize {
    println!("cargo:rerun-if-env-changed={name}");
    env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

fn main() {
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());

    let posix = env::var("CARGO_FEATURE_POSIX").is_ok();
    let bare_metal = env::var("CARGO_FEATURE_BARE_METAL").is_ok();
    let zephyr = env::var("CARGO_FEATURE_ZEPHYR").is_ok();

    let platform_count = [posix, bare_metal, zephyr].iter().filter(|&&x| x).count();
    if platform_count > 1 {
        panic!("Features `posix`, `bare-metal`, and `zephyr` are mutually exclusive");
    }

    // Read MTU from environment variable with platform-appropriate default.
    // Posix builds use 4096 for larger message support; embedded uses 512.
    let default_mtu = if posix { 4096 } else { 512 };
    let mtu = env_usize("XRCE_TRANSPORT_MTU", default_mtu);

    // Generate config headers
    generate_ucdr_config(&out_dir);
    generate_uxr_config(&out_dir, posix, mtu);

    // Compile C sources
    let mut build = cc::Build::new();
    build
        .warnings(false)
        .include(out_dir.join("include"))
        .include(manifest_dir.join("micro-cdr/include"))
        .include(manifest_dir.join("micro-xrce-dds-client/include"))
        .include(manifest_dir.join("micro-xrce-dds-client/src/c"));

    // Micro-CDR sources (5 files)
    let ucdr_src = manifest_dir.join("micro-cdr/src/c");
    build.file(ucdr_src.join("common.c"));
    for name in &["basic", "array", "sequence", "string"] {
        build.file(ucdr_src.join(format!("types/{name}.c")));
    }

    // XRCE-DDS Client core sources
    let uxr_src = manifest_dir.join("micro-xrce-dds-client/src/c");

    // Session (10 files)
    let session_dir = uxr_src.join("core/session");
    for name in &[
        "session",
        "session_info",
        "submessage",
        "object_id",
        "common_create_entities",
        "create_entities_bin",
        "create_entities_ref",
        "create_entities_xml",
        "read_access",
        "write_access",
    ] {
        build.file(session_dir.join(format!("{name}.c")));
    }

    // Stream (7 files)
    let stream_dir = session_dir.join("stream");
    for name in &[
        "input_best_effort_stream",
        "input_reliable_stream",
        "output_best_effort_stream",
        "output_reliable_stream",
        "stream_storage",
        "stream_id",
        "seq_num",
    ] {
        build.file(stream_dir.join(format!("{name}.c")));
    }

    // Serialization (3 files)
    let ser_dir = uxr_src.join("core/serialization");
    for name in &["xrce_types", "xrce_header", "xrce_subheader"] {
        build.file(ser_dir.join(format!("{name}.c")));
    }

    // Ping
    build.file(uxr_src.join("util/ping.c"));

    // Custom transport
    build.file(uxr_src.join("profile/transport/custom/custom_transport.c"));

    // Stream framing protocol (HDLC framing for serial transports)
    build.file(uxr_src.join("profile/transport/stream_framing/stream_framing_protocol.c"));

    // Platform-conditional: time.c
    // - POSIX: compile time.c (uses clock_gettime)
    // - Zephyr: skip time.c (uxr_millis/uxr_nanos provided by xrce_zephyr.c)
    // - Bare-metal: skip time.c (uxr_millis/uxr_nanos provided by platform crate)
    if posix {
        build.file(uxr_src.join("util/time.c"));
    }

    // Rust FFI shim (field accessor helpers)
    build.file(manifest_dir.join("src/shim.c"));

    build.compile("xrce_client");

    // Generate compile-time size check and Rust constants
    generate_size_check(&out_dir, &manifest_dir, posix, mtu);

    // Re-run if config changes
    println!("cargo:rerun-if-changed=build.rs");
}

fn generate_ucdr_config(out_dir: &Path) {
    let dir = out_dir.join("include/ucdr");
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("config.h"),
        "\
#ifndef _MICROCDR_CONFIG_H_
#define _MICROCDR_CONFIG_H_

#define MICROCDR_VERSION_MAJOR 2
#define MICROCDR_VERSION_MINOR 0
#define MICROCDR_VERSION_MICRO 2
#define MICROCDR_VERSION_STR \"2.0.2\"

// Little-endian for ARM, RISC-V, x86
#define UCDR_MACHINE_ENDIANNESS 1

#endif // _MICROCDR_CONFIG_H_
",
    )
    .unwrap();
}

fn generate_uxr_config(out_dir: &Path, posix: bool, mtu: usize) {
    let max_session_conn_attempts = env_usize("XRCE_MAX_SESSION_CONNECTION_ATTEMPTS", 10);
    let min_session_conn_interval = env_usize("XRCE_MIN_SESSION_CONNECTION_INTERVAL", 25);
    let min_heartbeat_time_interval = env_usize("XRCE_MIN_HEARTBEAT_TIME_INTERVAL", 100);
    let dir = out_dir.join("include/uxr/client");
    fs::create_dir_all(&dir).unwrap();

    let platform_define = if posix {
        "#define UCLIENT_PLATFORM_POSIX"
    } else {
        "/* no platform define for bare-metal */"
    };

    fs::write(
        dir.join("config.h"),
        format!(
            "\
#ifndef _UXR_CLIENT_CONFIG_H_
#define _UXR_CLIENT_CONFIG_H_

#define UXR_CLIENT_VERSION_MAJOR 3
#define UXR_CLIENT_VERSION_MINOR 0
#define UXR_CLIENT_VERSION_MICRO 1
#define UXR_CLIENT_VERSION_STR \"3.0.1\"

// Transport profiles: custom transport only
#define UCLIENT_PROFILE_CUSTOM_TRANSPORT

// Stream framing (HDLC) for serial transports
#define UCLIENT_PROFILE_STREAM_FRAMING

// Platform
{platform_define}

// Stream counts: 1 of each type
#define UXR_CONFIG_MAX_OUTPUT_BEST_EFFORT_STREAMS     1
#define UXR_CONFIG_MAX_OUTPUT_RELIABLE_STREAMS        1
#define UXR_CONFIG_MAX_INPUT_BEST_EFFORT_STREAMS      1
#define UXR_CONFIG_MAX_INPUT_RELIABLE_STREAMS         1

// Session connection (configurable via XRCE_* env vars)
#define UXR_CONFIG_MAX_SESSION_CONNECTION_ATTEMPTS    {max_session_conn_attempts}
#define UXR_CONFIG_MIN_SESSION_CONNECTION_INTERVAL    {min_session_conn_interval}
#define UXR_CONFIG_MIN_HEARTBEAT_TIME_INTERVAL        {min_heartbeat_time_interval}

// Custom transport MTU (configurable via XRCE_TRANSPORT_MTU env var)
#define UXR_CONFIG_CUSTOM_TRANSPORT_MTU               {mtu}

// Write limit tweak
#define UCLIENT_TWEAK_XRCE_WRITE_LIMIT

#endif // _UXR_CLIENT_CONFIG_H_
"
        ),
    )
    .unwrap();
}

/// Opaque blob size for `uxrSession`.
const UXR_SESSION_BLOB_SIZE: usize = 512;
/// Transport blob overhead beyond MTU.
const UXR_TRANSPORT_OVERHEAD: usize = 256;

fn generate_size_check(out_dir: &Path, manifest_dir: &Path, _posix: bool, mtu: usize) {
    // Compute Rust blob sizes from the configured MTU.
    // Transport struct embeds buffer[MTU]; session is MTU-independent.
    let session_size = UXR_SESSION_BLOB_SIZE;
    let transport_size = mtu + UXR_TRANSPORT_OVERHEAD;

    // Generate Rust constants for the blob sizes
    let constants_path = out_dir.join("xrce_constants.rs");
    fs::write(
        &constants_path,
        format!(
            "\
/// Configured XRCE transport MTU (from `XRCE_TRANSPORT_MTU` env var or platform default).
pub const XRCE_TRANSPORT_MTU: usize = {mtu};

/// Size of the opaque Rust blob for `uxrSession`.
/// Session struct is MTU-independent (stream buffers are external).
pub const UXR_SESSION_SIZE: usize = {session_size};

/// Size of the opaque Rust blob for `uxrCustomTransport`.
/// Includes embedded `buffer[MTU]` + framing I/O + callback pointers.
pub const UXR_CUSTOM_TRANSPORT_SIZE: usize = {transport_size};
"
        ),
    )
    .unwrap();

    // Compile _Static_assert to verify blob sizes at build time
    let check_src = out_dir.join("size_check.c");
    fs::write(
        &check_src,
        format!(
            "\
#include <uxr/client/core/session/session.h>
#include <uxr/client/profile/transport/custom/custom_transport.h>

#define UXR_SESSION_RUST_SIZE {session_size}
#define UXR_CUSTOM_TRANSPORT_RUST_SIZE {transport_size}

_Static_assert(
    sizeof(uxrSession) <= UXR_SESSION_RUST_SIZE,
    \"uxrSession exceeds Rust blob size — increase XRCE_TRANSPORT_MTU overhead in build.rs\"
);
_Static_assert(
    sizeof(uxrCustomTransport) <= UXR_CUSTOM_TRANSPORT_RUST_SIZE,
    \"uxrCustomTransport exceeds Rust blob size — increase XRCE_TRANSPORT_MTU overhead in build.rs\"
);
"
        ),
    )
    .unwrap();

    cc::Build::new()
        .warnings(false)
        .include(out_dir.join("include"))
        .include(manifest_dir.join("micro-cdr/include"))
        .include(manifest_dir.join("micro-xrce-dds-client/include"))
        .include(manifest_dir.join("micro-xrce-dds-client/src/c"))
        .file(&check_src)
        .compile("xrce_size_check");
}
