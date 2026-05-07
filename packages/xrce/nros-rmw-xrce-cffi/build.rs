//! Build script for `nros-rmw-xrce-cffi`.
//!
//! Compiles the K.2.0–K.2.4 C backend (`packages/xrce/nros-rmw-xrce-c/src/*.c`)
//! plus the vendored micro-XRCE-DDS-Client + micro-CDR sources directly
//! into a single static archive, then exposes the
//! `nros_rmw_xrce_register` symbol to the Rust side via `extern "C"`.
//!
//! Source list mirrors `packages/xrce/xrce-sys/build.rs` (proven set for
//! the same uxr core) plus the K.2 backend TUs. Keep both lists in
//! lockstep — any new file added here must land in xrce-sys's build.rs
//! and `nros-rmw-xrce-c/CMakeLists.txt` too.

use std::{env, fs, path::PathBuf};

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let workspace = manifest_dir
        .parent()
        .and_then(|p| p.parent())
        .and_then(|p| p.parent())
        .expect("workspace root")
        .to_path_buf();
    let xrce_sys = workspace.join("packages/xrce/xrce-sys");
    let xrce_c = workspace.join("packages/xrce/nros-rmw-xrce-c");
    let microcdr = xrce_sys.join("micro-cdr");
    let microxrce = xrce_sys.join("micro-xrce-dds-client");

    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());

    // Generate config headers.
    generate_ucdr_config(&out_dir, &microcdr);
    generate_uxr_config(&out_dir, &microxrce);

    let mut build = cc::Build::new();
    build
        .std("c99")
        .warnings(false)
        .define("_POSIX_C_SOURCE", Some("200809L"))
        .define("_DEFAULT_SOURCE", None)
        .include(out_dir.join("include"))
        .include(microcdr.join("include"))
        .include(microxrce.join("include"))
        .include(microxrce.join("src/c"))
        .include(xrce_c.join("src"))
        .include(xrce_c.join("include"))
        .include(workspace.join("packages/core/nros-rmw-cffi/include"));

    // K.2 backend TUs. Source-of-truth list — must stay in lockstep
    // with `nros-rmw-xrce-c/CMakeLists.txt`.
    let mut backend_tus = vec![
        "vtable",
        "session",
        "publisher",
        "subscriber",
        "service",
        "transport_custom",
    ];
    // Phase 118 — `transport_posix_{udp,serial}.c` define
    // `xrce_posix_{udp,serial}_init`. The TUs only build where
    // `<sys/socket.h>` / `<termios.h>` are available; bare-metal
    // targets must inject their own custom transport instead.
    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if matches!(
        target_os.as_str(),
        "linux" | "macos" | "freebsd" | "netbsd" | "openbsd"
    ) {
        backend_tus.push("transport_posix_udp");
        backend_tus.push("transport_posix_serial");
    }
    for name in &backend_tus {
        build.file(xrce_c.join(format!("src/{name}.c")));
    }

    // Micro-CDR (5 files).
    let ucdr_src = microcdr.join("src/c");
    build.file(ucdr_src.join("common.c"));
    for name in &["basic", "array", "sequence", "string"] {
        build.file(ucdr_src.join(format!("types/{name}.c")));
    }

    // micro-XRCE-DDS-Client core.
    let uxr_src = microxrce.join("src/c");
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
    let ser_dir = uxr_src.join("core/serialization");
    for name in &["xrce_types", "xrce_header", "xrce_subheader"] {
        build.file(ser_dir.join(format!("{name}.c")));
    }
    build.file(uxr_src.join("util/ping.c"));
    build.file(uxr_src.join("util/time.c"));
    build.file(uxr_src.join("profile/transport/custom/custom_transport.c"));
    build.file(uxr_src.join("profile/transport/stream_framing/stream_framing_protocol.c"));
    build.file(uxr_src.join("profile/transport/ip/udp/udp_transport.c"));
    build.file(uxr_src.join("profile/transport/ip/udp/udp_transport_posix.c"));

    build.compile("nros_rmw_xrce_c_inline");

    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed={}", xrce_c.join("src").display());
    println!(
        "cargo:rerun-if-changed={}",
        xrce_c.join("include").display()
    );
}

fn generate_ucdr_config(out_dir: &std::path::Path, microcdr: &std::path::Path) {
    let template = fs::read_to_string(microcdr.join("include/ucdr/config.h.in"))
        .expect("read ucdr config.h.in");
    // Project version 2.0.2 (matches micro-CDR upstream tag at our pin).
    let header = template
        .replace("@PROJECT_VERSION_MAJOR@", "2")
        .replace("@PROJECT_VERSION_MINOR@", "0")
        .replace("@PROJECT_VERSION_PATCH@", "2")
        .replace("@PROJECT_VERSION@", "2.0.2")
        // ucdrEndianness enum: BIG=0, LITTLE=1. Set 1 for x86 / ARM.
        .replace("@CONFIG_MACHINE_ENDIANNESS@", "1");
    let dir = out_dir.join("include/ucdr");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("config.h"), header).unwrap();
}

fn generate_uxr_config(out_dir: &std::path::Path, microxrce: &std::path::Path) {
    let template = fs::read_to_string(microxrce.join("include/uxr/client/config.h.in"))
        .expect("read uxr config.h.in");
    // Substitute @TOKEN@ placeholders.
    let mut h = template
        .replace("@PROJECT_VERSION_MAJOR@", "2")
        .replace("@PROJECT_VERSION_MINOR@", "4")
        .replace("@PROJECT_VERSION_PATCH@", "1")
        .replace("@PROJECT_VERSION@", "2.4.1")
        .replace("@UCLIENT_MAX_OUTPUT_BEST_EFFORT_STREAMS@", "1")
        .replace("@UCLIENT_MAX_OUTPUT_RELIABLE_STREAMS@", "1")
        .replace("@UCLIENT_MAX_INPUT_BEST_EFFORT_STREAMS@", "1")
        .replace("@UCLIENT_MAX_INPUT_RELIABLE_STREAMS@", "1")
        .replace("@UCLIENT_MAX_SESSION_CONNECTION_ATTEMPTS@", "10")
        .replace("@UCLIENT_MIN_SESSION_CONNECTION_INTERVAL@", "1000")
        .replace("@UCLIENT_MIN_HEARTBEAT_TIME_INTERVAL@", "100")
        .replace("@UCLIENT_UDP_TRANSPORT_MTU@", "4096")
        .replace("@UCLIENT_TCP_TRANSPORT_MTU@", "4096")
        .replace("@UCLIENT_SERIAL_TRANSPORT_MTU@", "512")
        .replace("@UCLIENT_CUSTOM_TRANSPORT_MTU@", "4096")
        .replace("@UCLIENT_SHARED_MEMORY_MAX_ENTITIES@", "4")
        .replace("@UCLIENT_SHARED_MEMORY_STATIC_MEM_SIZE@", "10")
        .replace("@UCLIENT_HARD_LIVELINESS_CHECK_TIMEOUT@", "10000");

    // #cmakedefine handling. The template uses `#cmakedefine NAME` —
    // CMake replaces with `#define NAME` when var is set, `/* #undef
    // NAME */` otherwise.
    let enabled = [
        "UCLIENT_PROFILE_DISCOVERY",
        "UCLIENT_PROFILE_UDP",
        "UCLIENT_PROFILE_TCP",
        "UCLIENT_PROFILE_SERIAL",
        "UCLIENT_PROFILE_CUSTOM_TRANSPORT",
        "UCLIENT_PROFILE_STREAM_FRAMING",
        "UCLIENT_TWEAK_XRCE_WRITE_LIMIT",
        "UCLIENT_PLATFORM_POSIX",
    ];
    let disabled = [
        "UCLIENT_PROFILE_MULTITHREAD",
        "UCLIENT_PROFILE_SHARED_MEMORY",
        "UCLIENT_PROFILE_CAN",
        "UCLIENT_HARD_LIVELINESS_CHECK",
        "UCLIENT_PLATFORM_POSIX_NOPOLL",
        "UCLIENT_PLATFORM_WINDOWS",
        "UCLIENT_PLATFORM_FREERTOS_PLUS_TCP",
        "UCLIENT_PLATFORM_RTEMS_BSD_NET",
        "UCLIENT_PLATFORM_ZEPHYR",
    ];
    // Match the entire line (`\n` boundary) so e.g. the
    // `UCLIENT_PLATFORM_POSIX` rule does not accidentally also
    // match `UCLIENT_PLATFORM_POSIX_NOPOLL`.
    for name in enabled {
        h = h.replace(
            &format!("#cmakedefine {name}\n"),
            &format!("#define {name}\n"),
        );
    }
    for name in disabled {
        h = h.replace(
            &format!("#cmakedefine {name}\n"),
            &format!("/* #undef {name} */\n"),
        );
    }
    let dir = out_dir.join("include/uxr/client");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("config.h"), h).unwrap();
}
