//! Build script for `nros-rmw-xrce-cffi`.
//!
//! Compiles the K.2.0–K.2.4 C backend (`packages/rmw/xrce/nros-rmw-xrce/src/*.c`)
//! plus the vendored micro-XRCE-DDS-Client + micro-CDR sources directly
//! into a single static archive, then exposes the
//! `nros_rmw_xrce_register` symbol to the Rust side via `extern "C"`.
//!
//! phase-321 W1.d — this build script is now the SOLE Rust-side owner of the
//! source list. It used to say "mirrors `packages/rmw/xrce/xrce-sys/build.rs` …
//! keep both lists in lockstep", i.e. it was a maintained duplicate of a build
//! script belonging to a crate with zero dependents, excluded from every
//! workspace build. That crate is deleted; only the DIRECTORY survives, because
//! it hosts the micro-XRCE-DDS-Client and micro-CDR submodules that this script
//! and `nros-rmw-xrce/CMakeLists.txt` both compile from.
//!
//! One mirror remains and is real: a new source file added here must also land
//! in `nros-rmw-xrce/CMakeLists.txt`, which is the C/C++ consumers' entry point.

use std::{env, fs, path::PathBuf};

// Phase 214.C.2 — single source-of-truth for XRCE transport MTU defaults.
// UDP/TCP/custom share a 4096-byte default; serial uses a smaller 512-byte
// default (UART throughput floor).
const XRCE_TRANSPORT_MTU_DEFAULT: &str = "4096";
const XRCE_SERIAL_MTU_DEFAULT: &str = "512";

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    // phase-321 W2.d — FOUR parents, not three. The crate sits at
    // packages/rmw/xrce/nros-rmw-xrce-cffi/, one level deeper than the old
    // packages/xrce/nros-rmw-xrce-cffi/. With three the walk stopped at
    // `packages/` and every vendored path came out doubled
    // (`<repo>/packages/packages/rmw/xrce/...`). A `.parent()` chain is a
    // relative path that no grep for "../" can find — only a build does.
    let workspace = manifest_dir
        .parent()
        .and_then(|p| p.parent())
        .and_then(|p| p.parent())
        .and_then(|p| p.parent())
        .expect("workspace root")
        .to_path_buf();
    let xrce_sys = workspace.join("packages/rmw/xrce/xrce-sys");
    let xrce_c = workspace.join("packages/rmw/xrce/nros-rmw-xrce");
    let microcdr = xrce_sys.join("micro-cdr");
    let microxrce = xrce_sys.join("micro-xrce-dds-client");

    // Phase 145.4 — source-list drift / submodule-presence gate (mirrors the
    // zpico-sys 136.6 gate). The vendored uxr / micro-cdr C sources come from
    // git submodules; a missing checkout or an upstream bump that renamed a
    // source dir would otherwise surface as a confusing cc-rs "file not found"
    // mid-compile. Verify each vendored root resolves to a directory with .c
    // files (or subdirs) up front, with a clear init hint, and emit
    // rerun-if-changed so a submodule bump retriggers the build.
    for (label, root, hint) in [
        (
            "micro-xrce-dds-client",
            microxrce.join("src/c"),
            // #0390 — lead with the `nros setup` vocabulary a CLI-provisioned
            // user has; the label IS the `[source.*]` name. git line kept as the
            // underlying mechanism.
            "nros setup --source micro-xrce-dds-client  \
             (or: git submodule update --init packages/rmw/xrce/xrce-sys/micro-xrce-dds-client)",
        ),
        (
            "micro-cdr",
            microcdr.join("src/c"),
            "nros setup --source micro-cdr  \
             (or: git submodule update --init packages/rmw/xrce/xrce-sys/micro-cdr)",
        ),
        (
            "nros-rmw-xrce",
            xrce_c.join("src"),
            "in-repo wrapper — expected at packages/rmw/xrce/nros-rmw-xrce/src",
        ),
    ] {
        let has_sources = std::fs::read_dir(&root)
            .map(|entries| {
                entries.flatten().any(|e| {
                    e.path().extension().is_some_and(|x| x == "c")
                        || e.file_type().map(|t| t.is_dir()).unwrap_or(false)
                })
            })
            .unwrap_or(false);
        if !root.is_dir() || !has_sources {
            panic!(
                "nros-rmw-xrce-cffi: vendored `{label}` source root {} is missing or has no \
                 .c files — submodule not initialised or upstream layout drifted. Fix: {hint}",
                root.display()
            );
        }
        println!("cargo:rerun-if-changed={}", root.display());
    }

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
    // Phase 204.7 — `NROS_LINK_IP=0` drops the IP (UDP/TCP) transport on a
    // serial-only hosted node (mirrors the zenoh `Z_FEATURE_LINK_*` gate). It gates
    // both the upstream `udp_transport*.c` sources and the `UCLIENT_PROFILE_UDP/TCP`
    // defines below. Embedded XRCE already excludes IP (custom transport), so this
    // only matters on POSIX. Default (unset) → IP on, unchanged.
    println!("cargo:rerun-if-env-changed=NROS_LINK_IP");
    let ip = !matches!(
        env::var("NROS_LINK_IP").ok().as_deref(),
        Some("0") | Some("false") | Some("off")
    );

    // Generate config headers.
    generate_ucdr_config(&out_dir, &microcdr);
    generate_uxr_config(&out_dir, &microxrce, feat_zephyr, is_posix);

    let mut build = cc::Build::new();
    // issue 0383 — implicit-function-declaration / int-conversion as errors
    // (`warnings(false)` only omits `-Wall`/`-Wextra`; cc-rs passes no `-w`).
    nros_cc_flags::strict_decls(&mut build);
    build
        .std("c99")
        .warnings(false)
        // Phase 204.9 — size: `-Os` + per-fn/data sections so the embedded
        // link path's `--gc-sections` (204.8) can strip unused XRCE surface.
        .opt_level_str("s")
        .flag_if_supported("-ffunction-sections")
        .flag_if_supported("-fdata-sections")
        .define("_DEFAULT_SOURCE", None)
        .include(out_dir.join("include"))
        .include(microcdr.join("include"))
        .include(microxrce.join("include"))
        .include(microxrce.join("src/c"))
        .include(xrce_c.join("src"))
        .include(xrce_c.join("include"))
        .include(workspace.join("packages/core/nros-rmw-abi/include"))
        .include(workspace.join("packages/platform/nros-platform-api/include"));
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
        if ip {
            build.file(uxr_src.join("profile/transport/ip/udp/udp_transport.c"));
            build.file(uxr_src.join("profile/transport/ip/udp/udp_transport_posix.c"));
        }
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
    // phase-400 W6 — the `[knobs.xrce]` rungs sit under the env front-end and
    // above the builtins below.
    let xrce_rungs = nros_platform_config::platform_config::BuildRungs::from_build_env()
        .map(|r| r.xrce_rungs())
        .unwrap_or_default();
    if let Some(v) = knob("NROS_XRCE_STREAM_HISTORY")
        .or_else(|| xrce_rungs.stream_history.map(|n| n.to_string()))
    {
        let n: u32 = v
            .parse()
            .unwrap_or_else(|_| panic!("NROS_XRCE_STREAM_HISTORY='{}' is not a number", v));
        if n < 4 {
            panic!("NROS_XRCE_STREAM_HISTORY={} too small (minimum 4)", n);
        }
        build.define("XRCE_STREAM_HISTORY", n.to_string().as_str());
    }
    println!("cargo:rerun-if-env-changed=NROS_XRCE_STREAM_HISTORY");
    println!("cargo:rerun-if-env-changed=NROS_XRCE_CUSTOM_TRANSPORT_MTU");
    println!("cargo:rerun-if-env-changed=NROS_XRCE_TRANSPORT_MTU");

    // Phase 207.6 — per-session pool sizes. A pub-only bare-metal node
    // can drop `MAX_SUBSCRIBERS` to 1 (zero-length arrays aren't
    // standard C; 1 is the practical minimum), `MAX_SERVICE_SERVERS` /
    // `MAX_SERVICE_CLIENTS` to 1, `SUBSCRIBER_RING_DEPTH` to 1, and
    // `BUFFER_SIZE` to 256. Combined with `STREAM_HISTORY=4` +
    // `NROS_XRCE_CUSTOM_TRANSPORT_MTU=512` the session struct drops
    // from ~390 KB to ~10–20 KB.
    for (env_name, define_name, min) in [
        ("NROS_XRCE_MAX_SUBSCRIBERS", "XRCE_MAX_SUBSCRIBERS", 1),
        (
            "NROS_XRCE_MAX_SERVICE_SERVERS",
            "XRCE_MAX_SERVICE_SERVERS",
            1,
        ),
        (
            "NROS_XRCE_MAX_SERVICE_CLIENTS",
            "XRCE_MAX_SERVICE_CLIENTS",
            1,
        ),
        (
            "NROS_XRCE_SUBSCRIBER_RING_DEPTH",
            "XRCE_SUBSCRIBER_RING_DEPTH",
            1,
        ),
        ("NROS_XRCE_BUFFER_SIZE", "XRCE_BUFFER_SIZE", 64),
    ] {
        if let Some(v) = knob(env_name) {
            let n: u32 = v
                .parse()
                .unwrap_or_else(|_| panic!("{env_name}='{v}' is not a number"));
            if n < min {
                panic!("{env_name}={n} too small (minimum {min})");
            }
            build.define(define_name, n.to_string().as_str());
        }
        println!("cargo:rerun-if-env-changed={env_name}");
    }

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
        let posix_src = workspace.join("packages/platform/nros-platform-posix/src");
        let mut posix_build = cc::Build::new();
        // issue 0383 — implicit-function-declaration / int-conversion as errors.
        nros_cc_flags::strict_decls(&mut posix_build);
        posix_build
            .std("c11")
            .warnings(false)
            .define("_DEFAULT_SOURCE", None)
            .define("_POSIX_C_SOURCE", Some("200809L"))
            .include(workspace.join("packages/platform/nros-platform-api/include"))
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
    // issue 0968 — the general transport MTU, min-guarded like CUSTOM's.
    //
    // `knob_usize`, NOT `env::var`: a Zephyr RUST image inherits none of cmake's
    // `set(ENV{...})` exports, so a bare read yields the crate default whatever
    // Kconfig says (issue 0460). `check-kconfig-knob-forwarding` caught exactly
    // that in the first version of this fix, which would have repaired the C
    // lane and left the Rust one on 4096 — half-fixing the thing this change is
    // about.
    let xrce_transport_mtu = nros_zephyr_build::knob_usize(
        "NROS_XRCE_TRANSPORT_MTU",
        "CONFIG_NROS_XRCE_TRANSPORT_MTU",
        XRCE_TRANSPORT_MTU_DEFAULT
            .parse()
            .expect("XRCE_TRANSPORT_MTU_DEFAULT is a number"),
    );
    if xrce_transport_mtu < 128 {
        panic!("NROS_XRCE_TRANSPORT_MTU={xrce_transport_mtu} too small (minimum 128)");
    }
    let xrce_transport_mtu = xrce_transport_mtu.to_string();

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
        // Phase 214.C.2 — MTU defaults from named consts at file top.
        // issue 0968 — UDP and TCP honour `NROS_XRCE_TRANSPORT_MTU` too. Only
        // CUSTOM was tunable, so a UDP image (which every Zephyr XRCE example
        // is — it dials an agent at `CONFIG_NROS_XRCE_AGENT_ADDR:PORT`) was
        // pinned to 4096 whatever Kconfig said. `STREAM_BUFFER_SIZE = MTU x
        // STREAM_HISTORY` twice per session, so the default cost 131072 bytes
        // where the configured 512 costs 16384.
        .replace("@UCLIENT_UDP_TRANSPORT_MTU@", &xrce_transport_mtu)
        .replace("@UCLIENT_TCP_TRANSPORT_MTU@", &xrce_transport_mtu)
        .replace("@UCLIENT_SERIAL_TRANSPORT_MTU@", XRCE_SERIAL_MTU_DEFAULT)
        .replace(
            "@UCLIENT_CUSTOM_TRANSPORT_MTU@",
            // Phase 207.6 — env-tunable so RAM-tight bare-metal nodes can
            // drop the per-session stream buffers (`STREAM_BUFFER_SIZE =
            // CUSTOM_TRANSPORT_MTU × STREAM_HISTORY`) by an order of
            // magnitude. Min 128 (smaller breaks the framing/header
            // assumptions); default tracks XRCE_TRANSPORT_MTU_DEFAULT.
            // phase-400 W6 — env, then the `[knobs.xrce]` rung, then the
            // default. `knob` rather than a bare `env::var` so the Kconfig
            // front-end reaches this the way it reaches the pool sizes above
            // (issue 0460).
            &knob("NROS_XRCE_CUSTOM_TRANSPORT_MTU")
                .or_else(|| {
                    nros_platform_config::platform_config::BuildRungs::from_build_env()
                        .and_then(|r| r.xrce_rungs().custom_transport_mtu)
                        .map(|n| n.to_string())
                })
                .unwrap_or_else(|| XRCE_TRANSPORT_MTU_DEFAULT.into()),
        )
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
    // Phase 204.7 — recomputed here (separate fn from the source-file build);
    // gates the UDP/TCP profile defines to match the gated source files.
    let ip = !matches!(
        env::var("NROS_LINK_IP").ok().as_deref(),
        Some("0") | Some("false") | Some("off")
    );
    if is_posix {
        if ip {
            enabled.push("UCLIENT_PROFILE_UDP");
            enabled.push("UCLIENT_PROFILE_TCP");
        } else {
            disabled.push("UCLIENT_PROFILE_UDP");
            disabled.push("UCLIENT_PROFILE_TCP");
        }
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

/// A pool knob's value: explicit env var, else the Zephyr Kconfig it is
/// resolved from, else `None` (the vendored header's own default stands).
///
/// issue 0460 — every one of these is forwarded by `_nros_resolve_knob()` in
/// `zephyr/cmake/nros_cargo_build.cmake` with `set(ENV{...})`, which reaches
/// the C lane's re-baked command and NOT the Rust lane's: zephyr-lang-rust's
/// `rust_cargo_application` builds its own cargo invocation and inherits
/// nothing. So a Zephyr Rust image read every one of these as unset whatever
/// Kconfig said. The env name here is the Kconfig name minus `CONFIG_`, so the
/// pair is derived; `check-kconfig-knob-forwarding` proves the cmake list and
/// the readers agree.
fn knob(env_name: &str) -> Option<String> {
    if let Ok(v) = env::var(env_name) {
        return Some(v);
    }
    nros_zephyr_build::dotconfig_usize(&format!("CONFIG_{env_name}")).map(|v| v.to_string())
}
