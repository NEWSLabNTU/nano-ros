//! Build script for `nros-rmw-xrce-cffi`.
//!
//! Compiles the K.2.0–K.2.4 C backend (`packages/xrce/nros-rmw-xrce/src/*.c`)
//! plus the vendored micro-XRCE-DDS-Client + micro-CDR sources directly
//! into a single static archive, then exposes the
//! `nros_rmw_xrce_register` symbol to the Rust side via `extern "C"`.
//!
//! Source list mirrors `packages/xrce/xrce-sys/build.rs` (proven set for
//! the same uxr core) plus the K.2 backend TUs. Keep both lists in
//! lockstep — any new file added here must land in xrce-sys's build.rs
//! and `nros-rmw-xrce/CMakeLists.txt` too.

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
    let xrce_c = workspace.join("packages/xrce/nros-rmw-xrce");
    let microcdr = xrce_sys.join("micro-cdr");
    let microxrce = xrce_sys.join("micro-xrce-dds-client");

    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());

    // Phase 129.C.1 — platform fanout driven by `target_os` alone.
    // `nros-rmw-xrce-cffi` is platform-blind after 129.NET.3: the
    // session UDP path runs `xrce_nros_udp_init` on top of
    // `nros_platform_udp_*` regardless of platform. The build script
    // only still cares about `target_os` for two narrow reasons:
    //   1. Whether to compile the upstream `udp_transport*.c` and
    //      `util/time.c` POSIX-only TUs (they call libc directly).
    //   2. Whether to define `_POSIX_C_SOURCE` (needed to unlock
    //      `clock_gettime` / `getaddrinfo` in POSIX libc headers).
    // No `CARGO_FEATURE_PLATFORM_*` reads — the features that used
    // to gate these were deleted in 129.C.1.
    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let host_is_posix = matches!(
        target_os.as_str(),
        "linux" | "macos" | "freebsd" | "netbsd" | "openbsd"
    );
    let feat_zephyr = false; // 129.C.1 — `transport_zephyr_udp` superseded by `transport_nros_udp`.
    let is_posix = host_is_posix;
    let is_embedded = !host_is_posix;

    // Generate config headers.
    generate_ucdr_config(&out_dir, &microcdr);
    generate_uxr_config(&out_dir, &microxrce, feat_zephyr, is_posix);

    let mut build = cc::Build::new();
    build
        .std("c99")
        .warnings(false)
        .define("_DEFAULT_SOURCE", None)
        .include(out_dir.join("include"))
        .include(microcdr.join("include"))
        .include(microxrce.join("include"))
        .include(microxrce.join("src/c"))
        .include(xrce_c.join("src"))
        .include(xrce_c.join("include"))
        .include(workspace.join("packages/core/nros-rmw-cffi/include"))
        .include(workspace.join("packages/core/nros-platform-cffi/include"));
    if is_posix {
        // `_POSIX_C_SOURCE` is what unlocks `clock_gettime`,
        // `getaddrinfo`, etc in `<sys/socket.h>` + `<time.h>` on
        // glibc / musl / macOS. Bare-metal & Zephyr stdlibs don't
        // ship these — gating the define keeps the embedded build
        // from pulling in headers it can't satisfy.
        build.define("_POSIX_C_SOURCE", Some("200809L"));
    }

    // K.2 backend TUs. Source-of-truth list — must stay in lockstep
    // with `nros-rmw-xrce/CMakeLists.txt`.
    let mut backend_tus = vec![
        "vtable",
        "session",
        "publisher",
        "subscriber",
        "service",
        "transport_custom",
        // Phase 129.NET.3 — platform-agnostic UDP via
        // `nros_platform_udp_*`. Compiles on every target as long
        // as the consumer links a platform-provider library that
        // satisfies the symbols. Supersedes `transport_posix_udp`
        // / `transport_zephyr_udp`.
        "transport_nros_udp",
        // Phase 129.D.2 — `uxr_millis` / `uxr_nanos` carved out
        // of the retired `xrce-platform-shim` crate.
        "platform_aliases",
    ];
    // Phase 118 — `transport_posix_{udp,serial}.c` define
    // `xrce_posix_{udp,serial}_init`. The TUs only build where
    // `<sys/socket.h>` / `<termios.h>` are available; embedded
    // targets must inject their own custom transport instead.
    // Kept alongside `transport_nros_udp` for one cycle so callers
    // that still resolve `xrce_posix_udp_init` keep working.
    if is_posix {
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
    build.file(uxr_src.join("profile/transport/custom/custom_transport.c"));
    build.file(uxr_src.join("profile/transport/stream_framing/stream_framing_protocol.c"));

    // POSIX-only TUs. `util/time.c` calls `clock_gettime` /
    // `nanosleep`. `udp_transport.c` + `udp_transport_posix.c`
    // open `socket(AF_INET, …)` directly. Embedded targets supply
    // their own time + transport via the registry.
    if is_posix {
        build.file(uxr_src.join("util/time.c"));
        build.file(uxr_src.join("profile/transport/ip/udp/udp_transport.c"));
        build.file(uxr_src.join("profile/transport/ip/udp/udp_transport_posix.c"));
    }

    if is_embedded {
        // Tell `<uxr/client/config_internal.h>` not to require the
        // POSIX TUs we've just dropped from the source list.
        build.define("UCLIENT_PLATFORM_NO_POSIX", None);
    }

    // Phase 130.6 — tunable reliable-stream history. Tight-RAM
    // targets that don't run server-side action callbacks can drop
    // from the default 16 (= 64 KiB per-session output buffer) to 8
    // or 4. `internal.h` enforces `>= 4`.
    if let Ok(v) = env::var("NROS_XRCE_STREAM_HISTORY") {
        let n: u32 = v
            .parse()
            .unwrap_or_else(|_| panic!("NROS_XRCE_STREAM_HISTORY='{}' is not a number", v));
        if n < 4 {
            panic!("NROS_XRCE_STREAM_HISTORY={} too small (minimum 4)", n);
        }
        build.define("XRCE_STREAM_HISTORY", n.to_string().as_str());
        println!("cargo:rerun-if-env-changed=NROS_XRCE_STREAM_HISTORY");
    }
    println!("cargo:rerun-if-env-changed=NROS_XRCE_STREAM_HISTORY");

    build.compile("nros_rmw_xrce_c_inline");

    // Phase 129.NET.3 — `transport_nros_udp.c` references the
    // canonical `nros_platform_udp_*` ABI. Ship the sibling
    // `nros-platform-posix` C port inside this crate's static
    // archive whenever the build resolves to a POSIX host
    // (explicit `platform-posix` feature or host-OS auto-detect).
    // Consumers that bring their own platform-provider library
    // (e.g. the C SDK linked under cmake with `nano_ros_link_platform`)
    // must opt out by NOT selecting `posix` / `platform-posix`
    // and forcing a non-host target — otherwise the link hits
    // duplicate-symbol errors.
    if is_posix {
        let posix_src = workspace.join("packages/core/nros-platform-posix/src");
        let mut posix_build = cc::Build::new();
        posix_build
            .std("c11")
            .warnings(false)
            .define("_DEFAULT_SOURCE", None)
            .define("_POSIX_C_SOURCE", Some("200809L"))
            .include(workspace.join("packages/core/nros-platform-cffi/include"))
            .file(posix_src.join("platform.c"))
            .file(posix_src.join("net.c"))
            .file(posix_src.join("timer.c"));
        posix_build.compile("nros_platform_posix_link");
        println!("cargo:rerun-if-changed={}", posix_src.display());
    }

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

fn generate_uxr_config(
    out_dir: &std::path::Path,
    microxrce: &std::path::Path,
    is_zephyr: bool,
    is_posix: bool,
) {
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
    let mut enabled = vec![
        "UCLIENT_PROFILE_DISCOVERY",
        "UCLIENT_PROFILE_CUSTOM_TRANSPORT",
        "UCLIENT_PROFILE_STREAM_FRAMING",
        "UCLIENT_TWEAK_XRCE_WRITE_LIMIT",
    ];
    let mut disabled = vec![
        "UCLIENT_PROFILE_MULTITHREAD",
        "UCLIENT_PROFILE_SHARED_MEMORY",
        "UCLIENT_PROFILE_CAN",
        "UCLIENT_HARD_LIVELINESS_CHECK",
    ];
    // Platform fanout — POSIX gets the full UDP/TCP/SERIAL profile
    // set; Zephyr emits its own platform define so any upstream
    // `#ifdef UCLIENT_PLATFORM_ZEPHYR` branch picks the right path.
    // Pure bare-metal / FreeRTOS / NuttX / ThreadX gets the
    // freestanding core only — consumers wire their own transport
    // via `nros_rmw_cffi_set_custom_transport(...)`.
    if is_posix {
        enabled.push("UCLIENT_PROFILE_UDP");
        enabled.push("UCLIENT_PROFILE_TCP");
        enabled.push("UCLIENT_PROFILE_SERIAL");
        enabled.push("UCLIENT_PLATFORM_POSIX");
        disabled.push("UCLIENT_PLATFORM_POSIX_NOPOLL");
        disabled.push("UCLIENT_PLATFORM_WINDOWS");
        disabled.push("UCLIENT_PLATFORM_FREERTOS_PLUS_TCP");
        disabled.push("UCLIENT_PLATFORM_RTEMS_BSD_NET");
        disabled.push("UCLIENT_PLATFORM_ZEPHYR");
    } else if is_zephyr {
        enabled.push("UCLIENT_PLATFORM_ZEPHYR");
        // UDP / TCP / SERIAL profile defines stay off — Zephyr's
        // transport is custom (CMake glue wires the callbacks).
        disabled.push("UCLIENT_PROFILE_UDP");
        disabled.push("UCLIENT_PROFILE_TCP");
        disabled.push("UCLIENT_PROFILE_SERIAL");
        disabled.push("UCLIENT_PLATFORM_POSIX");
        disabled.push("UCLIENT_PLATFORM_POSIX_NOPOLL");
        disabled.push("UCLIENT_PLATFORM_WINDOWS");
        disabled.push("UCLIENT_PLATFORM_FREERTOS_PLUS_TCP");
        disabled.push("UCLIENT_PLATFORM_RTEMS_BSD_NET");
    } else {
        // Bare-metal / FreeRTOS / NuttX / ThreadX.
        disabled.push("UCLIENT_PROFILE_UDP");
        disabled.push("UCLIENT_PROFILE_TCP");
        disabled.push("UCLIENT_PROFILE_SERIAL");
        disabled.push("UCLIENT_PLATFORM_POSIX");
        disabled.push("UCLIENT_PLATFORM_POSIX_NOPOLL");
        disabled.push("UCLIENT_PLATFORM_WINDOWS");
        disabled.push("UCLIENT_PLATFORM_FREERTOS_PLUS_TCP");
        disabled.push("UCLIENT_PLATFORM_RTEMS_BSD_NET");
        disabled.push("UCLIENT_PLATFORM_ZEPHYR");
    }
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
