use std::env;
use std::fs;
use std::path::{Path, PathBuf};

fn main() {
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());

    let posix = env::var("CARGO_FEATURE_POSIX").is_ok();
    let bare_metal = env::var("CARGO_FEATURE_BARE_METAL").is_ok();

    if posix && bare_metal {
        panic!("Features `posix` and `bare-metal` are mutually exclusive");
    }

    // Generate config headers
    generate_ucdr_config(&out_dir);
    generate_uxr_config(&out_dir, posix);

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

    // Platform-conditional: time.c (only with POSIX)
    if posix {
        build.file(uxr_src.join("util/time.c"));
    }

    build.compile("xrce_client");

    // Generate compile-time size check
    generate_size_check(&out_dir, &manifest_dir, posix);

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

fn generate_uxr_config(out_dir: &Path, posix: bool) {
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

// Platform
{platform_define}

// Stream counts: 1 of each type
#define UXR_CONFIG_MAX_OUTPUT_BEST_EFFORT_STREAMS     1
#define UXR_CONFIG_MAX_OUTPUT_RELIABLE_STREAMS        1
#define UXR_CONFIG_MAX_INPUT_BEST_EFFORT_STREAMS      1
#define UXR_CONFIG_MAX_INPUT_RELIABLE_STREAMS         1

// Session connection
#define UXR_CONFIG_MAX_SESSION_CONNECTION_ATTEMPTS    10
#define UXR_CONFIG_MIN_SESSION_CONNECTION_INTERVAL    25
#define UXR_CONFIG_MIN_HEARTBEAT_TIME_INTERVAL        100

// Custom transport MTU
#define UXR_CONFIG_CUSTOM_TRANSPORT_MTU               512

// Write limit tweak
#define UCLIENT_TWEAK_XRCE_WRITE_LIMIT

#endif // _UXR_CLIENT_CONFIG_H_
"
        ),
    )
    .unwrap();
}

fn generate_size_check(out_dir: &Path, manifest_dir: &Path, _posix: bool) {
    // We generate a small C file with _Static_assert to verify our
    // Rust opaque blob sizes are large enough for the actual C structs.
    // This file is compiled as part of the build to catch config.h mismatches.
    let check_src = out_dir.join("size_check.c");
    fs::write(
        &check_src,
        format!(
            "\
#include <uxr/client/core/session/session.h>
#include <uxr/client/profile/transport/custom/custom_transport.h>

// These must match the constants in src/lib.rs
#define UXR_SESSION_RUST_SIZE {session_size}
#define UXR_CUSTOM_TRANSPORT_RUST_SIZE {transport_size}

_Static_assert(
    sizeof(uxrSession) <= UXR_SESSION_RUST_SIZE,
    \"uxrSession exceeds Rust blob size — update UXR_SESSION_SIZE in lib.rs\"
);
_Static_assert(
    sizeof(uxrCustomTransport) <= UXR_CUSTOM_TRANSPORT_RUST_SIZE,
    \"uxrCustomTransport exceeds Rust blob size — update UXR_CUSTOM_TRANSPORT_SIZE in lib.rs\"
);
",
            session_size = 512,
            transport_size = 768,
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
