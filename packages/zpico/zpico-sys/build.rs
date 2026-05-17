//! Build script for zpico-sys
//!
//! This builds:
//! 1. zenoh-pico C library (via CMake for native, sources for embedded)
//! 2. The zpico C layer (zpico.c)
//! 3. Generates C header from Rust FFI declarations (cbindgen)

use std::{
    env,
    fmt::Write as _,
    path::{Path, PathBuf},
    process::Command,
};

// Phase 136.1 — zenoh_platforms.toml parser. Loaded + smoke-resolved
// at the top of `main()` so any TOML drift surfaces at build time
// instead of waiting for 136.3 / 136.4 to wire the resolver into
// cc-rs. The resolved data is not yet consumed by the build path.
#[path = "build/manifest.rs"]
mod manifest;

// Phase 136.2 — link-feature env reader + per-platform policy mask.
// Moved out of this file to `build/policy.rs` so the manifest layer
// can produce the same `LinkPolicy` values directly in 136.4.
#[path = "build/policy.rs"]
mod policy;

use policy::{LinkFeatures, LinkPolicy};

/// Shim slot count configuration.
///
/// Controls the maximum number of concurrent publishers, subscribers,
/// queryables, and liveliness tokens in the C shim layer.
/// Values are read from `ZPICO_MAX_*` environment variables at build time.
struct ShimConfig {
    max_publishers: usize,
    max_subscribers: usize,
    max_queryables: usize,
    max_liveliness: usize,
    max_pending_gets: usize,
    get_reply_buf_size: usize,
    get_poll_interval_ms: usize,
}

impl ShimConfig {
    fn from_env() -> Self {
        Self {
            max_publishers: env_usize("ZPICO_MAX_PUBLISHERS", 8),
            max_subscribers: env_usize("ZPICO_MAX_SUBSCRIBERS", 8),
            max_queryables: env_usize("ZPICO_MAX_QUERYABLES", 8),
            max_liveliness: env_usize("ZPICO_MAX_LIVELINESS", 16),
            max_pending_gets: env_usize("ZPICO_MAX_PENDING_GETS", 4),
            get_reply_buf_size: env_usize("ZPICO_GET_REPLY_BUF_SIZE", 4096),
            get_poll_interval_ms: env_usize("ZPICO_GET_POLL_INTERVAL_MS", 10),
        }
    }

    /// Generate `$OUT_DIR/shim_constants.rs` with Rust const declarations.
    fn generate_rust_consts(&self, out_dir: &Path) {
        let contents = format!(
            "/// Maximum number of concurrent publishers (set via ZPICO_MAX_PUBLISHERS, default 8).\n\
             pub const ZPICO_MAX_PUBLISHERS: usize = {};\n\
             /// Maximum number of concurrent subscribers (set via ZPICO_MAX_SUBSCRIBERS, default 8).\n\
             pub const ZPICO_MAX_SUBSCRIBERS: usize = {};\n\
             /// Maximum number of concurrent queryables (set via ZPICO_MAX_QUERYABLES, default 8).\n\
             pub const ZPICO_MAX_QUERYABLES: usize = {};\n\
             /// Maximum number of concurrent liveliness tokens (set via ZPICO_MAX_LIVELINESS, default 16).\n\
             pub const ZPICO_MAX_LIVELINESS: usize = {};\n\
             /// Maximum number of concurrent pending get operations (set via ZPICO_MAX_PENDING_GETS, default 4).\n\
             pub const ZPICO_MAX_PENDING_GETS: usize = {};\n",
            self.max_publishers,
            self.max_subscribers,
            self.max_queryables,
            self.max_liveliness,
            self.max_pending_gets,
        );
        std::fs::write(out_dir.join("shim_constants.rs"), contents).unwrap();
    }

    /// Add `-D` flags to a `cc::Build` so the C shim picks up the same values.
    fn apply_to_cc(&self, build: &mut cc::Build) {
        build.define(
            "ZPICO_MAX_PUBLISHERS",
            self.max_publishers.to_string().as_str(),
        );
        build.define(
            "ZPICO_MAX_SUBSCRIBERS",
            self.max_subscribers.to_string().as_str(),
        );
        build.define(
            "ZPICO_MAX_QUERYABLES",
            self.max_queryables.to_string().as_str(),
        );
        build.define(
            "ZPICO_MAX_LIVELINESS",
            self.max_liveliness.to_string().as_str(),
        );
        build.define(
            "ZPICO_MAX_PENDING_GETS",
            self.max_pending_gets.to_string().as_str(),
        );
        build.define(
            "ZPICO_GET_REPLY_BUF_SIZE",
            self.get_reply_buf_size.to_string().as_str(),
        );
        build.define(
            "ZPICO_GET_POLL_INTERVAL_MS",
            self.get_poll_interval_ms.to_string().as_str(),
        );
    }
}

/// Buffer size configuration for zenoh-pico.
///
/// These values are read from environment variables at build time, with
/// platform-appropriate defaults. Posix builds use large defaults suitable
/// for desktop/server workloads, while embedded builds use small defaults
/// to fit in constrained memory.
struct ZenohBufferConfig {
    frag_max_size: usize,
    batch_unicast_size: usize,
    batch_multicast_size: usize,
}

impl ZenohBufferConfig {
    /// Read buffer config from environment variables with platform-appropriate defaults.
    fn from_env(posix: bool) -> Self {
        let link = LinkFeatures::from_env();
        let (default_frag, default_batch_uni, default_batch_multi) = if posix {
            // Posix: large defaults for desktop/server workloads
            // Note: batch sizes must fit in u16 (zenoh protocol limit = 65535)
            (65535, 65535, 8192)
        } else if link.serial {
            // Serial transport: batch size must be >= z_serial MAX_MTU (1500).
            // zenohd's z-serial crate requires the receive buffer to be at least
            // 1500 bytes; the buffer is sized from the negotiated batch_size.
            (2048, 1500, 1024)
        } else {
            // Embedded: small defaults for memory-constrained targets
            (2048, 1024, 1024)
        };

        let frag_max_size = env_usize("ZPICO_FRAG_MAX_SIZE", default_frag);
        let batch_unicast_size = env_usize("ZPICO_BATCH_UNICAST_SIZE", default_batch_uni);
        let batch_multicast_size = env_usize("ZPICO_BATCH_MULTICAST_SIZE", default_batch_multi);

        Self {
            frag_max_size,
            batch_unicast_size,
            batch_multicast_size,
        }
    }
}

/// Read a usize from an environment variable, falling back to a default.
fn env_usize(name: &str, default: usize) -> usize {
    println!("cargo:rerun-if-env-changed={name}");
    env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

/// Generate a zenoh-pico config header in OUT_DIR based on Cargo link-* features.
///
/// This replaces the hardcoded `zenoh_generic_config.h` with a generated version
/// where `Z_FEATURE_LINK_*` values are derived from Cargo features and buffer
/// sizes are read from environment variables with platform-appropriate defaults.
fn generate_config_header(out_dir: &Path, link: &LinkFeatures, buf: &ZenohBufferConfig) {
    let config_dir = out_dir.join("zenoh-config");
    std::fs::create_dir_all(&config_dir).unwrap();

    let mut header = String::new();
    writeln!(header, "/**").unwrap();
    writeln!(header, " * zenoh_generic_config.h - Generated by build.rs").unwrap();
    writeln!(header, " *").unwrap();
    writeln!(
        header,
        " * Z_FEATURE_LINK_* values are derived from Cargo link-* features."
    )
    .unwrap();
    writeln!(
        header,
        " * Buffer sizes are configurable via ZPICO_* environment variables."
    )
    .unwrap();
    writeln!(header, " * DO NOT EDIT — regenerated on every build.").unwrap();
    writeln!(header, " */").unwrap();
    writeln!(header).unwrap();
    writeln!(header, "#ifndef ZENOH_GENERIC_CONFIG_H").unwrap();
    writeln!(header, "#define ZENOH_GENERIC_CONFIG_H").unwrap();
    writeln!(header).unwrap();
    writeln!(header, "// Buffer Sizes").unwrap();
    writeln!(header, "#define Z_FRAG_MAX_SIZE {}", buf.frag_max_size).unwrap();
    writeln!(
        header,
        "#define Z_BATCH_UNICAST_SIZE {}",
        buf.batch_unicast_size
    )
    .unwrap();
    writeln!(
        header,
        "#define Z_BATCH_MULTICAST_SIZE {}",
        buf.batch_multicast_size
    )
    .unwrap();
    // NuttX over QEMU slirp needs a longer timeout for the zenoh handshake.
    let target = std::env::var("TARGET").unwrap_or_default();
    let socket_timeout = if target.contains("nuttx") { 5000 } else { 100 };
    writeln!(header, "#define Z_CONFIG_SOCKET_TIMEOUT {}", socket_timeout).unwrap();
    writeln!(header, "#define Z_TRANSPORT_LEASE 10000").unwrap();
    writeln!(header, "#define Z_TRANSPORT_LEASE_EXPIRE_FACTOR 3").unwrap();
    writeln!(header, "#define ZP_PERIODIC_SCHEDULER_MAX_TASKS 8").unwrap();
    writeln!(header).unwrap();
    writeln!(header, "// Core Features").unwrap();
    writeln!(header, "#ifndef Z_FEATURE_MULTI_THREAD").unwrap();
    writeln!(header, "#define Z_FEATURE_MULTI_THREAD 0").unwrap();
    writeln!(header, "#endif").unwrap();
    writeln!(header, "#define Z_FEATURE_PUBLICATION 1").unwrap();
    writeln!(header, "#define Z_FEATURE_ADVANCED_PUBLICATION 0").unwrap();
    writeln!(header, "#define Z_FEATURE_SUBSCRIPTION 1").unwrap();
    writeln!(header, "#define Z_FEATURE_ADVANCED_SUBSCRIPTION 0").unwrap();
    writeln!(header, "#define Z_FEATURE_QUERY 1").unwrap();
    writeln!(header, "#define Z_FEATURE_QUERYABLE 1").unwrap();
    writeln!(header, "#define Z_FEATURE_LIVELINESS 1").unwrap();
    writeln!(header, "#define Z_FEATURE_INTEREST 1").unwrap();
    writeln!(header).unwrap();
    writeln!(
        header,
        "// Transport Link Features (from Cargo link-* features)"
    )
    .unwrap();
    writeln!(header, "#define Z_FEATURE_LINK_TCP {}", link.tcp_flag()).unwrap();
    writeln!(
        header,
        "#define Z_FEATURE_LINK_UDP_UNICAST {}",
        link.udp_unicast_flag()
    )
    .unwrap();
    writeln!(
        header,
        "#define Z_FEATURE_LINK_UDP_MULTICAST {}",
        link.udp_multicast_flag()
    )
    .unwrap();
    writeln!(
        header,
        "#define Z_FEATURE_LINK_SERIAL {}",
        link.serial_flag()
    )
    .unwrap();
    writeln!(header, "#define Z_FEATURE_LINK_BLUETOOTH 0").unwrap();
    writeln!(header, "#define Z_FEATURE_LINK_WS 0").unwrap();
    writeln!(header, "#define Z_FEATURE_LINK_SERIAL_USB 0").unwrap();
    writeln!(header, "#define Z_FEATURE_LINK_IVC {}", link.ivc_flag()).unwrap();
    writeln!(
        header,
        "#define Z_FEATURE_LINK_CUSTOM {}",
        link.custom_flag()
    )
    .unwrap();
    writeln!(header, "#define Z_FEATURE_LINK_TLS {}", link.tls_flag()).unwrap();
    writeln!(
        header,
        "#define Z_FEATURE_RAWETH_TRANSPORT {}",
        link.raweth_flag()
    )
    .unwrap();
    writeln!(header).unwrap();
    writeln!(header, "// Transport Modes").unwrap();
    writeln!(header, "#define Z_FEATURE_UNICAST_TRANSPORT 1").unwrap();
    writeln!(header, "#define Z_FEATURE_MULTICAST_TRANSPORT 0").unwrap();
    writeln!(header, "#define Z_FEATURE_SCOUTING 0").unwrap();
    writeln!(header, "#ifndef Z_FEATURE_SCOUTING_UDP").unwrap();
    writeln!(header, "#define Z_FEATURE_SCOUTING_UDP 0").unwrap();
    writeln!(header, "#endif").unwrap();
    writeln!(header).unwrap();
    // Unstable API (from Cargo unstable-zenoh-api feature)
    // Only define when enabled — zenoh-pico uses #cmakedefine (presence/absence flag)
    if env::var("CARGO_FEATURE_UNSTABLE_ZENOH_API").is_ok() {
        writeln!(header, "#define Z_FEATURE_UNSTABLE_API").unwrap();
    }
    writeln!(header).unwrap();
    writeln!(header, "// Protocol Features").unwrap();
    writeln!(header, "#define Z_FEATURE_FRAGMENTATION 1").unwrap();
    // Encoding-name strings (`text/plain`, `application/json`, ...). ROS-over-
    // Zenoh uses CDR; encoding name is never consulted. SPE drops it; other
    // platforms keep the upstream default.
    let orin_spe = env::var("CARGO_FEATURE_ORIN_SPE").is_ok();
    let encoding_values = if orin_spe { 0 } else { 1 };
    writeln!(
        header,
        "#define Z_FEATURE_ENCODING_VALUES {}",
        encoding_values
    )
    .unwrap();
    writeln!(header, "#define Z_FEATURE_TCP_NODELAY 1").unwrap();
    writeln!(header, "#define Z_FEATURE_LOCAL_SUBSCRIBER 0").unwrap();
    writeln!(header, "#define Z_FEATURE_LOCAL_QUERYABLE 0").unwrap();
    writeln!(header, "#define Z_FEATURE_SESSION_CHECK 1").unwrap();
    writeln!(header, "#define Z_FEATURE_BATCHING 0").unwrap();
    writeln!(header, "#define Z_FEATURE_BATCH_TX_MUTEX 0").unwrap();
    writeln!(header, "#define Z_FEATURE_BATCH_PEER_MUTEX 0").unwrap();
    // Phase 134.4 — MATCHING is required for proper message routing
    // between clients on different networks (e.g., Zephyr on TAP vs
    // native on localhost). Previously the CMake POSIX path forced
    // this on via `cmake_cfg.define("Z_FEATURE_MATCHING", "1")`;
    // canonicalising the header brings every path in line.
    writeln!(header, "#define Z_FEATURE_MATCHING 1").unwrap();
    writeln!(header, "#define Z_FEATURE_RX_CACHE 0").unwrap();
    writeln!(header, "#define Z_FEATURE_UNICAST_PEER 0").unwrap();
    // Auto-reconnect is dead code on the IVC link (fixed-frame mailbox, no
    // disconnect path). SPE drops it; other platforms keep the upstream
    // default for transport-level reconnect on TCP / serial / TLS.
    let auto_reconnect = if orin_spe { 0 } else { 1 };
    writeln!(
        header,
        "#define Z_FEATURE_AUTO_RECONNECT {}",
        auto_reconnect
    )
    .unwrap();
    writeln!(header, "#define Z_FEATURE_MULTICAST_DECLARATIONS 0").unwrap();
    writeln!(header, "#define Z_FEATURE_PERIODIC_TASKS 0").unwrap();
    writeln!(header).unwrap();
    writeln!(header, "#endif /* ZENOH_GENERIC_CONFIG_H */").unwrap();

    std::fs::write(config_dir.join("zenoh_generic_config.h"), header).unwrap();
}

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let target = env::var("TARGET").unwrap_or_default();

    // Phase 136.1 — parse the canonical platform manifest. Resolve
    // every declared platform so a typo or broken `inherits` chain
    // surfaces as a hard build error, not a runtime surprise after
    // 136.3 plugs the data into cc-rs.
    let platform_manifest_path = manifest_dir.join("zenoh_platforms.toml");
    println!(
        "cargo:rerun-if-changed={}",
        platform_manifest_path.display()
    );
    let platform_manifest = manifest::PlatformManifest::load(&platform_manifest_path)
        .unwrap_or_else(|e| panic!("zenoh_platforms.toml: {e}"));
    for name in platform_manifest.platform.keys() {
        platform_manifest
            .for_platform(name)
            .unwrap_or_else(|e| panic!("zenoh_platforms.toml: resolve {name}: {e}"));
    }

    // Phase 136.6 (partial) — source-list drift gate. For every
    // platform, verify each `include` root names a real directory
    // under `zenoh-pico/src/` and contains at least one `.c` file.
    // Catches submodule bumps that rename / delete dirs and typos
    // in the manifest. Full set-equality vs. the cc-rs source list
    // lands with 136.4 once the per-RTOS functions collapse into a
    // single manifest-driven path.
    let zenoh_pico_src = manifest_dir.join("zenoh-pico").join("src");
    if zenoh_pico_src.exists() {
        for (name, _) in &platform_manifest.platform {
            let resolved = platform_manifest.for_platform(name).unwrap();
            for include in &resolved.include {
                let dir = zenoh_pico_src.join(include);
                if !dir.is_dir() {
                    panic!(
                        "zenoh_platforms.toml: platform `{name}` `include = \"{include}\"` \
                         does not resolve to a directory under zenoh-pico/src/ \
                         (expected: {})",
                        dir.display()
                    );
                }
                let has_c_file = std::fs::read_dir(&dir)
                    .map(|entries| {
                        entries.flatten().any(|e| {
                            e.path().extension().is_some_and(|x| x == "c")
                                || e.file_type().map(|t| t.is_dir()).unwrap_or(false)
                        })
                    })
                    .unwrap_or(false);
                if !has_c_file {
                    panic!(
                        "zenoh_platforms.toml: platform `{name}` `include = \"{include}\"` \
                         resolves to {} but contains no .c files or subdirs",
                        dir.display()
                    );
                }
                println!("cargo:rerun-if-changed={}", dir.display());
            }
        }
    }

    // Check which platform backend to use
    let mut use_posix = env::var("CARGO_FEATURE_POSIX").is_ok();
    let use_zephyr = env::var("CARGO_FEATURE_ZEPHYR").is_ok();
    let use_bare_metal = env::var("CARGO_FEATURE_BARE_METAL").is_ok();
    let use_freertos = env::var("CARGO_FEATURE_FREERTOS").is_ok();
    let use_nuttx = env::var("CARGO_FEATURE_NUTTX").is_ok();
    let use_threadx = env::var("CARGO_FEATURE_THREADX").is_ok();
    // Phase 100.6 — AGX Orin SPE (Cortex-R5F + NVIDIA FreeRTOS FSP).
    // Builds zenoh-pico from the same C source as `freertos` but
    // skips lwIP / TCP-UDP wiring; the only link transport is IVC.
    let use_orin_spe = env::var("CARGO_FEATURE_ORIN_SPE").is_ok();

    // Phase 128.D — auto-derive `platform-posix` from `target_os` when
    // no explicit platform feature was selected. The POSIX path is
    // the only one a `cargo build` on a hosted target can infer
    // unambiguously; embedded RTOSes (FreeRTOS / NuttX / ThreadX /
    // Zephyr) all share `target_os = "none"` so the user still
    // disambiguates by enabling the matching feature. POSIX hosts
    // (Linux / macOS / *BSD) no longer need an explicit `platform-posix`
    // feature in their `Cargo.toml`.
    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let auto_posix = matches!(
        target_os.as_str(),
        "linux" | "macos" | "freebsd" | "netbsd" | "openbsd" | "android"
    );
    let any_explicit = use_posix
        || use_zephyr
        || use_bare_metal
        || use_freertos
        || use_nuttx
        || use_threadx
        || use_orin_spe;
    if !any_explicit && auto_posix {
        use_posix = true;
    }

    // Count enabled backends
    let backend_count = [
        use_posix,
        use_zephyr,
        use_bare_metal,
        use_freertos,
        use_nuttx,
        use_threadx,
        use_orin_spe,
    ]
    .iter()
    .filter(|&&b| b)
    .count();

    if backend_count == 0 {
        // No backend selected — build only enough to regenerate `zpico.h`
        // (cbindgen) and the size probe. Reached on plain `cargo doc`,
        // `cargo check --workspace`, etc. — perfectly normal, so emit
        // an `eprintln!` instead of `cargo:warning` (the latter surfaces
        // as a yellow `warning: …` line on every workspace build).
        eprintln!("zpico-sys: no platform backend selected; minimal build (header-only).");
    }

    if backend_count > 1 {
        panic!(
            "Only one platform backend can be selected at a time \
             (posix, zephyr, bare-metal, freertos, nuttx, threadx, or orin-spe)"
        );
    }

    // Read link-* features for bare-metal protocol selection
    let link_features = LinkFeatures::from_env();

    // Phase 100.4 — surface the IVC link feature as a cfg so
    // `zpico-platform-shim` can gate its `mod ivc_helpers` block on
    // `#[cfg(feature = "link_ivc")]` without depending on the cargo
    // feature flag transitively. Mirrors the `zpico_backend` pattern
    // used a few hundred lines below.
    if link_features.ivc {
        println!("cargo:rustc-cfg=feature=\"link-ivc\"");
    }
    println!("cargo:rustc-check-cfg=cfg(feature, values(\"link-ivc\"))");

    // Paths
    let zenoh_pico_src = manifest_dir.join("zenoh-pico");
    let c_dir = manifest_dir.join("c");
    let include_dir = c_dir.join("include");
    let use_system = env::var("CARGO_FEATURE_SYSTEM_ZENOHPICO").is_ok();

    // system-zenohpico is not supported for embedded targets — embedded builds need
    // the smoltcp platform layer compiled into the same archive as zenoh-pico.
    if use_system && is_embedded_target(&target) {
        panic!(
            "system-zenohpico is not supported for embedded targets ({}). \
             Embedded builds require the smoltcp platform layer to be compiled \
             together with zenoh-pico from the submodule.",
            target
        );
    }

    // Check if zenoh-pico submodule is present (not needed with system-zenohpico)
    if !use_system && !zenoh_pico_src.join("include").exists() {
        panic!(
            "zenoh-pico submodule not found at {:?}. Run: git submodule update --init",
            zenoh_pico_src
        );
    }

    // Generate C header from Rust FFI declarations
    generate_header(&manifest_dir, &include_dir);

    // Read buffer config with platform-appropriate defaults
    // NuttX is POSIX-compatible, use same defaults as posix.
    // ThreadX uses NetX Duo BSD sockets, treat as posix-like for buffer defaults.
    let buf_config = ZenohBufferConfig::from_env(use_posix || use_nuttx || use_threadx);

    // Read shim slot counts from ZPICO_MAX_* env vars and generate Rust consts
    let shim_config = ShimConfig::from_env();
    shim_config.generate_rust_consts(&out_dir);

    // Phase 134.3 — `zenoh_generic_config.h` is the single source of
    // truth for every `Z_FEATURE_LINK_*` flag. Apply the per-platform
    // `LinkPolicy` once here, generate the canonical header, and every
    // build path below reads it (cc-rs via `ZENOH_GENERIC` + include
    // path; CMake via the same — see 134.4 below).
    let link_policy = if use_orin_spe {
        LinkPolicy::orin_spe()
    } else if use_posix {
        LinkPolicy::posix()
    } else {
        LinkPolicy::passthrough()
    };
    let link_features = link_features.apply(&link_policy);
    generate_config_header(&out_dir, &link_features, &buf_config);

    // Phase 136.4 — manifest-driven unified consumer. The TOML
    // declares every per-platform datum (defines, required env
    // vars, include paths, extra sources, arch profile, compile
    // settings, pic). Drop the five per-RTOS Rust functions in
    // favour of a single `build_zenoh_pico_unified` that consumes
    // `ResolvedPlatform` + `[arch.*]`.
    let interp_ctx = manifest::InterpContext {
        nros: &manifest_dir,
        out: &out_dir,
        src: &zenoh_pico_src.join("src"),
    };
    let platform_name = if use_threadx {
        Some("threadx")
    } else if use_orin_spe {
        Some("orin-spe")
    } else if use_nuttx {
        Some("nuttx")
    } else if use_freertos {
        Some("freertos-lwip")
    } else if use_bare_metal {
        Some("bare-metal")
    } else if !is_embedded_target(&target) && !use_system {
        Some("posix")
    } else {
        None
    };
    if let Some(name) = platform_name {
        let resolved = platform_manifest
            .for_platform(name)
            .unwrap_or_else(|e| panic!("zenoh_platforms.toml: {e}"));
        build_zenoh_pico_unified(
            &resolved,
            &platform_manifest.arch,
            &interp_ctx,
            &zenoh_pico_src,
            &out_dir,
            &target,
            &link_features,
            &shim_config,
        );
    }

    // POSIX still needs the separate C shim build below (shim is
    // not included in extra_sources for posix). Native: link system
    // libs that the manifest doesn't model yet (per-target).
    if !is_embedded_target(&target) && !use_threadx {
        let zenoh_pico_include = if use_system {
            use_system_zenoh_pico()
        } else {
            // Native zenoh-pico include dir for the shim. The
            // unified consumer compiled the static archive; shim
            // build below pulls public headers.
            zenoh_pico_src.join("include")
        };
        if !use_system {
            if target.contains("linux") || target.contains("darwin") || target.contains("macos") {
                println!("cargo:rustc-link-lib=pthread");
            } else if target.contains("windows") {
                println!("cargo:rustc-link-lib=ws2_32");
            }
        }
        if backend_count > 0 && !use_zephyr && !use_freertos && !use_nuttx && !use_threadx {
            build_c_shim(
                &c_dir,
                &include_dir,
                &zenoh_pico_include,
                use_posix,
                use_bare_metal,
                &target,
                &link_features,
                &shim_config,
            );
        }
    }
    // For Zephyr: C code is built by Zephyr's build system, not Cargo.
    // For no-backend: nothing to build (minimal configuration for header generation).

    // Set cfg flags for Rust code
    if use_posix {
        println!("cargo:rustc-cfg=zpico_backend=\"posix\"");
    } else if use_zephyr {
        println!("cargo:rustc-cfg=zpico_backend=\"zephyr\"");
    } else if use_bare_metal {
        println!("cargo:rustc-cfg=zpico_backend=\"bare-metal\"");
    } else if use_freertos {
        println!("cargo:rustc-cfg=zpico_backend=\"freertos\"");
    } else if use_nuttx {
        println!("cargo:rustc-cfg=zpico_backend=\"nuttx\"");
    } else if use_threadx {
        println!("cargo:rustc-cfg=zpico_backend=\"threadx\"");
    } else if use_orin_spe {
        println!("cargo:rustc-cfg=zpico_backend=\"orin-spe\"");
    }

    // Phase 128.D.3 — opt-in alias TU that maps z_*/_z_get_time_*
    // symbols to the canonical nros_platform_* ABI. Compiled only
    // when the `platform-aliases` feature is selected; downstream
    // pairs it with disabling the matching symbols in
    // zpico-platform-shim or relies on `--allow-multiple-definition`
    // for one-cycle co-existence.
    if env::var_os("CARGO_FEATURE_PLATFORM_ALIASES").is_some() {
        let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
        let nros_platform_cffi_include = manifest_dir.join("../../core/nros-platform-cffi/include");
        let mut alias_build = cc::Build::new();
        alias_build
            .file(manifest_dir.join("c/zpico/platform_aliases.c"))
            .include(&nros_platform_cffi_include)
            .include(manifest_dir.join("c/zpico"))
            // Phase 129.D — `NROS_PLATFORM_ALIASES` unlocks the
            // alias TU's clock-variant + network wrappers, which
            // depend on the generic `z_clock_t = uint64_t` typedef
            // and the canonical `_z_sys_net_*` opaque layouts in
            // `nros_zenoh_generic_platform.h`.
            .define("NROS_PLATFORM_ALIASES", None)
            .warnings(true);
        // Phase 129.D — bare-metal cross targets
        // (`target_os = "none"`) often lack a usable newlib on the
        // host (`#include <stdint.h>` falls into gcc's own header
        // which does `#include_next` expecting newlib).
        // `-ffreestanding` tells gcc to use its own freestanding
        // `<stdint.h>` / `<stddef.h>`, which is all the alias TU
        // actually needs.
        let target_os_for_alias = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
        if target_os_for_alias == "none" {
            alias_build.flag("-ffreestanding");
        }
        alias_build.compile("zpico_platform_aliases");
        println!("cargo:rerun-if-changed=c/zpico/platform_aliases.c");
        println!("cargo:rerun-if-changed=c/zpico/nros_zenoh_generic_platform.h");
    }

    // Rerun triggers
    println!("cargo:rerun-if-changed=c/zpico/zpico.c");
    println!("cargo:rerun-if-changed=c/zpico/nuttx_clock.c");
    println!("cargo:rerun-if-changed=c/platform/bare-metal/platform.h");
    println!("cargo:rerun-if-changed=c/platform/errno_override.h");
    println!("cargo:rerun-if-changed=c/platform/zenoh_generic_config.h");
    println!("cargo:rerun-if-changed=c/platform/zenoh_generic_platform.h");
    println!("cargo:rerun-if-changed=zenoh-pico/src/system/unix/network.c");
    println!("cargo:rerun-if-changed=zenoh-pico/include/zenoh-pico/system/platform/unix.h");
    println!("cargo:rerun-if-changed=zenoh-pico/src/system/freertos/system.c");
    println!("cargo:rerun-if-changed=zenoh-pico/src/system/freertos/lwip/network.c");
    println!("cargo:rerun-if-changed=zenoh-pico/src/net/primitives.c");
    println!("cargo:rerun-if-changed=c/zenoh-pico-version.h.in");
    println!("cargo:rerun-if-changed=zenoh-pico/version.txt");
    println!("cargo:rerun-if-changed=src/ffi.rs");
    println!("cargo:rerun-if-changed=src/lib.rs");
    println!("cargo:rerun-if-changed=cbindgen.toml");
    println!("cargo:rerun-if-env-changed=FREERTOS_DIR");
    println!("cargo:rerun-if-env-changed=FREERTOS_PORT");
    println!("cargo:rerun-if-env-changed=LWIP_DIR");
    println!("cargo:rerun-if-env-changed=FREERTOS_CONFIG_DIR");
    println!("cargo:rerun-if-env-changed=NUTTX_DIR");
    println!("cargo:rerun-if-changed=c/size_probe.c");

    // Probe network type sizes from C headers and emit DEP variables.
    // zpico-platform-shim reads these to generate correctly-sized #[repr(C)] types.
    //
    // Skipped for Zephyr: the probe needs Zephyr's `<zephyr/kernel.h>` and
    // picolibc headers, which live in the Zephyr build tree and aren't
    // visible to Cargo. Zephyr doesn't use the shim's socket-stubs feature
    // (C network.c provides the real types), so the sizes aren't consumed —
    // the shim's own build.rs falls back to defaults, which is harmless.
    if backend_count > 0 && !use_zephyr {
        probe_net_type_sizes(
            &c_dir,
            &zenoh_pico_src.join("include"),
            &out_dir,
            // orin-spe routes through the bare-metal probe path
            // (ZENOH_GENERIC → bare-metal/platform.h, with ARM Cortex-R
            // cross-compile flags inside the function).
            use_bare_metal || use_orin_spe,
            use_freertos,
            use_nuttx,
            use_threadx,
        );
    }
}

/// Probe the sizes of `_z_sys_net_socket_t` and `_z_sys_net_endpoint_t` from C headers.
///
/// Compiles `c/size_probe.c` with the same platform defines as zenoh-pico,
/// reads the symbol sizes from the resulting .o file, and emits them as
/// `cargo:SOCKET_SIZE=<N>` and `cargo:ENDPOINT_SIZE=<N>` DEP variables.
/// zpico-platform-shim reads these as `DEP_ZPICO_SOCKET_SIZE` / `DEP_ZPICO_ENDPOINT_SIZE`.
#[allow(clippy::too_many_arguments)]
fn probe_net_type_sizes(
    c_dir: &Path,
    zenoh_pico_include: &Path,
    out_dir: &Path,
    use_bare_metal: bool,
    use_freertos: bool,
    use_nuttx: bool,
    use_threadx: bool,
) {
    let mut build = cc::Build::new();
    build.file(c_dir.join("size_probe.c"));
    build.include(zenoh_pico_include);

    // Set the same platform defines as the main build so platform.h selects
    // the correct platform header (unix.h, freertos/lwip.h, void.h, etc.)
    if use_bare_metal {
        // bare-metal: ZENOH_GENERIC → zenoh_generic_platform.h → bare-metal/platform.h
        build.define("ZENOH_GENERIC", None);
        let platform_dir = c_dir.join("platform");
        build.include(&platform_dir);

        // RV32 bare-metal (ESP32-C3): mirror `build_zenoh_pico_embedded` —
        // add the cross-compile flags, errno override and picolibc sysroot
        // so the probe finds <stdint.h>. Without this the probe emits
        // `fatal error: stdint.h: No such file or directory` (picolibc
        // headers aren't on the default system include path for
        // `riscv64-unknown-elf-gcc -march=rv32imc`) and falls back to the
        // hardcoded 16/8 defaults.
        let target = env::var("TARGET").unwrap_or_default();
        if target.contains("riscv32") {
            detect_riscv_compiler(&mut build);
            build.flag("-march=rv32imc").flag("-mabi=ilp32");

            let errno_dir = out_dir.join("errno-override");
            std::fs::create_dir_all(&errno_dir).ok();
            std::fs::write(
                errno_dir.join("errno.h"),
                include_bytes!("c/platform/errno_override.h"),
            )
            .ok();
            build.include(&errno_dir);

            if let Some(sysroot) = get_picolibc_sysroot() {
                build.include(sysroot.join("include"));
            }
        } else if target.contains("thumbv7m") || target.contains("thumbv7em") {
            // ARM Cortex-M bare-metal: cc crate selects arm-none-eabi-gcc
            // which ships its own newlib sysroot, so no sysroot include
            // is needed here — but the -mcpu flags are required for the
            // preprocessor to pick the right architecture-dependent
            // headers.
            if target.contains("thumbv7em") {
                build
                    .flag("-mcpu=cortex-m4")
                    .flag("-mthumb")
                    .flag("-mfpu=fpv4-sp-d16")
                    .flag("-mfloat-abi=hard");
            } else {
                build.flag("-mcpu=cortex-m3").flag("-mthumb");
            }
        } else if target.contains("armv7r") {
            // Phase 100.6 — AGX Orin SPE (Cortex-R5F). cc emits
            // `-march=armv7-r` from the target triple; we add
            // hard-float matching the FSP's vfpv3-d16 build. Same
            // flags as `zpico-platform-shim/build.rs`'s armv7r branch.
            build.flag("-mfpu=vfpv3-d16").flag("-mfloat-abi=hard");
        }
    } else if use_freertos {
        build.define("ZENOH_FREERTOS_LWIP", None);
        // lwIP + FreeRTOS headers needed
        if let Ok(dir) = env::var("FREERTOS_DIR") {
            build.include(PathBuf::from(&dir).join("include"));
            if let Ok(port) = env::var("FREERTOS_PORT") {
                build.include(PathBuf::from(&dir).join("portable").join(&port));
            }
        }
        if let Ok(dir) = env::var("FREERTOS_CONFIG_DIR") {
            build.include(dir);
        }
        if let Ok(dir) = env::var("LWIP_DIR") {
            let lwip = PathBuf::from(dir);
            build.include(lwip.join("src/include"));
        }
    } else if use_nuttx {
        build.define("ZENOH_NUTTX", None);
        build.define("ZENOH_LINUX", None);
        if let Ok(dir) = env::var("NUTTX_DIR") {
            build.include(PathBuf::from(dir).join("include"));
        }
    } else if use_threadx {
        build.define("ZENOH_GENERIC", None);
        build.define("ZENOH_THREADX", None);
        let platform_dir = c_dir.join("platform");
        build.include(&platform_dir);

        // ThreadX platform.h includes `tx_api.h` which pulls in ThreadX
        // kernel headers + a port-specific `tx_port.h`. Without these
        // the probe compile fails and we fall back to the hardcoded
        // 16/8 default — which silently skews the pass-by-value ABI
        // of `_z_sys_net_socket_t` at the FFI boundary (the Rust
        // shim ends up reading garbage from the wrong registers).
        //
        // Mirror the include set the main build uses for ThreadX.
        let target = env::var("TARGET").unwrap_or_default();
        if let Ok(dir) = env::var("THREADX_DIR") {
            let threadx_dir = PathBuf::from(&dir);
            build.include(threadx_dir.join("common/inc"));
            // Pick the port-specific header matching the target arch.
            if target.contains("riscv64") {
                build.include(threadx_dir.join("ports/risc-v64/gnu/inc"));
            } else if !is_embedded_target(&target) {
                build.include(threadx_dir.join("ports/linux/gnu/inc"));
            }
        }
        if let Ok(dir) = env::var("THREADX_CONFIG_DIR") {
            build.include(dir);
        }
        if let Ok(dir) = env::var("NETX_DIR") {
            let netx_dir = PathBuf::from(&dir);
            build.include(netx_dir.join("common/inc"));
            build.include(netx_dir.join("addons/BSD"));
            if !is_embedded_target(&target) {
                build.include(netx_dir.join("ports/linux/gnu/inc"));
            }
        }
        if let Ok(dir) = env::var("NETX_CONFIG_DIR") {
            build.include(dir);
        }

        // Cross-compile flags + C-library sysroot for bare-metal
        // RISC-V. Without these the probe fails with
        // `stdint.h: No such file or directory` and falls back to
        // the hardcoded default sizes.
        if target.contains("riscv64") {
            build
                .flag("-march=rv64gc")
                .flag("-mabi=lp64d")
                .flag("-mcmodel=medany");

            // errno-override header (picolibc's TLS errno doesn't
            // work on bare-metal). Must be searched before picolibc.
            let errno_dir = out_dir.join("errno-override");
            std::fs::create_dir_all(&errno_dir).ok();
            std::fs::write(
                errno_dir.join("errno.h"),
                include_bytes!("c/platform/errno_override.h"),
            )
            .ok();
            build.include(&errno_dir);

            if let Some(sysroot) = get_picolibc_sysroot() {
                build.include(sysroot.join("include"));
            }
        }
    } else {
        // POSIX: zenoh-pico auto-detects ZENOH_LINUX/ZENOH_MACOS from target
        let target = env::var("TARGET").unwrap_or_default();
        if target.contains("linux") {
            build.define("ZENOH_LINUX", None);
        } else if target.contains("darwin") || target.contains("macos") {
            build.define("ZENOH_MACOS", None);
        }
    }

    // Generated config header (for Z_FEATURE_LINK_TCP etc.)
    let generated_config_dir = out_dir.join("zenoh-config");
    if generated_config_dir.exists() {
        build.include(&generated_config_dir);
    }

    // Compile to a separate static library (may fail on targets without
    // C standard library headers, e.g. RISC-V without picolibc)
    build.cargo_metadata(false); // Don't emit link flags
    if let Err(e) = build.try_compile("size_probe") {
        // Fallback: emit default sizes (16/8) when probe fails.
        //
        // This is a known foot-gun: the fallback silently skews the
        // `_z_sys_net_socket_t` / `_z_sys_net_endpoint_t` pass-by-value
        // ABI when the Rust shim reads its opaque buffer from the
        // wrong call-site register. If you see this warning and the
        // target is cross-compiled (FreeRTOS / NuttX / ThreadX /
        // bare-metal), the runtime failure mode is a silent
        // `Transport(ConnectionFailed)` at session open (zero-length
        // send, no read).
        //
        // To diagnose: rerun `cargo build` with `-vv` to see the
        // underlying `cc::try_compile` error, and add the missing
        // include path to `probe_net_type_sizes` for that backend.
        println!(
            "cargo:warning=zpico-sys size_probe failed ({e}); \
             falling back to SOCKET_SIZE=16 / ENDPOINT_SIZE=8 — \
             pass-by-value ABI for _z_sys_net_socket_t will corrupt \
             if the real struct size differs"
        );
        println!("cargo:SOCKET_SIZE=16");
        println!("cargo:ENDPOINT_SIZE=8");
        return;
    }

    // Read symbol sizes from the compiled archive using llvm-nm.
    // The probe C file defines arrays whose lengths equal sizeof(type).
    let archive = out_dir.join("libsize_probe.a");
    let socket_size = read_symbol_size(&archive, "__nros_sizeof_net_socket");
    let endpoint_size = read_symbol_size(&archive, "__nros_sizeof_net_endpoint");

    // Emit as DEP variables (available to direct dependent crates as DEP_ZPICO_*)
    println!("cargo:SOCKET_SIZE={}", socket_size);
    println!("cargo:ENDPOINT_SIZE={}", endpoint_size);

    // Also emit as rustc-env so zpico-platform-shim can read them.
    // zpico-platform-shim is a dependency of zpico-sys (not the other way),
    // so DEP variables don't flow. Instead, write a shared file.
    let sizes_file = out_dir.join("net_type_sizes.txt");
    std::fs::write(&sizes_file, format!("{}\n{}\n", socket_size, endpoint_size)).unwrap();
    // Export the path so zpico-platform-shim's build.rs can find it
    println!(
        "cargo:rustc-env=ZPICO_NET_SIZES_FILE={}",
        sizes_file.display()
    );
}

/// Read the size of a symbol from a static library using llvm-nm or nm.
/// The symbol is an array `const unsigned char name[N]` — its size is N.
fn read_symbol_size(archive: &Path, symbol: &str) -> usize {
    // Try Rust's bundled llvm-nm first (handles all targets)
    let sysroot = env::var("RUSTC_SYSROOT").ok().or_else(|| {
        Command::new("rustc")
            .args(["--print", "sysroot"])
            .output()
            .ok()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
    });

    let llvm_nm_candidates = [
        sysroot
            .as_ref()
            .map(|s| {
                PathBuf::from(s)
                    .join("lib/rustlib")
                    .join(env::var("HOST").unwrap_or_default())
                    .join("bin/llvm-nm")
            })
            .unwrap_or_default(),
        PathBuf::from("llvm-nm"),
        PathBuf::from("nm"),
    ];

    for nm in &llvm_nm_candidates {
        if let Ok(output) = Command::new(nm)
            .args(["--print-size", "--defined-only"])
            .arg(archive)
            .output()
            && output.status.success()
        {
            let stdout = String::from_utf8_lossy(&output.stdout);
            for line in stdout.lines() {
                if line.contains(symbol) {
                    // Format: "00000000 00000010 R __nros_sizeof_net_socket"
                    // The second field is the hex size.
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    if parts.len() >= 2
                        && let Ok(size) = usize::from_str_radix(parts[1], 16)
                        && size > 0
                    {
                        return size;
                    }
                }
            }
        }
    }

    // Fallback: if we can't determine the size, use a safe maximum
    eprintln!(
        "cargo:warning=Could not determine size of {} from {}, using fallback 16",
        symbol,
        archive.display()
    );
    16
}

/// Check if the RISC-V GCC supports picolibc specs (provides C standard library headers)
fn has_picolibc_specs() -> bool {
    // Try riscv64-unknown-elf-gcc first (system GCC), then riscv32-esp-elf-gcc (ESP-IDF)
    for cc in &["riscv64-unknown-elf-gcc", "riscv32-esp-elf-gcc"] {
        if let Ok(status) = Command::new(cc)
            .args([
                "-march=rv32imc",
                "-mabi=ilp32",
                "--specs=picolibc.specs",
                "-E",
                "-x",
                "c",
                "/dev/null",
                "-o",
                "/dev/null",
            ])
            .status()
            && status.success()
        {
            return true;
        }
    }
    false
}

/// Check if we're building for an embedded target
fn is_embedded_target(target: &str) -> bool {
    target.contains("zephyr")
        || target.contains("none")
        || target.contains("nuttx")
        || target.contains("thumbv")
        || target.contains("riscv")
}

/// Generate C header from Rust FFI declarations using cbindgen
fn generate_header(manifest_dir: &Path, include_dir: &Path) {
    // Create include directory if needed
    if !include_dir.exists() {
        std::fs::create_dir_all(include_dir).unwrap_or_else(|e| {
            println!("cargo:warning=Failed to create include directory: {e}");
        });
    }

    let output_file = include_dir.join("zpico.h");
    let config_file = manifest_dir.join("cbindgen.toml");

    // Load cbindgen config
    let config = cbindgen::Config::from_file(&config_file).unwrap_or_else(|e| {
        println!("cargo:warning=Failed to load cbindgen config: {e}");
        cbindgen::Config::default()
    });

    // Generate header
    let mut header = Vec::new();
    let result = cbindgen::Builder::new()
        .with_crate(manifest_dir)
        .with_config(config)
        .generate();

    match result {
        Ok(bindings) => {
            bindings.write(&mut header);
            let header_str = String::from_utf8_lossy(&header);

            // Post-process: remove lines starting with "extern " and collapse blank lines
            let processed = post_process_header(&header_str);

            if !is_plausible_generated_header(&processed) {
                println!(
                    "cargo:warning=cbindgen generated an incomplete zpico.h; keeping existing header"
                );
                return;
            }

            // Race-free write under parallel cargo invocations (one per
            // example/target-dir, all rebuilding zpico-sys against this same
            // source-tree path). std::fs::write would interleave bytes on
            // concurrent writers; instead write to a per-pid temp then
            // atomic-rename. Same-content skip avoids redundant churn that
            // confuses cargo's rerun-if-changed.
            let existing = std::fs::read(&output_file).ok();
            if existing.as_deref() == Some(processed.as_bytes()) {
                return;
            }
            let tmp = output_file
                .parent()
                .map(|p| p.join(format!(".zpico.h.tmp.{}", std::process::id())))
                .unwrap_or_else(|| {
                    output_file.with_extension(format!("h.tmp.{}", std::process::id()))
                });
            if let Err(e) = std::fs::write(&tmp, processed) {
                println!("cargo:warning=Failed to write tmp header {tmp:?}: {e}");
                return;
            }
            if let Err(e) = std::fs::rename(&tmp, &output_file) {
                println!("cargo:warning=Failed to rename tmp header into place: {e}");
                let _ = std::fs::remove_file(&tmp);
            }
        }
        Err(e) => {
            println!("cargo:warning=cbindgen failed: {e}");
        }
    }
}

fn is_plausible_generated_header(header: &str) -> bool {
    header.contains("#ifndef ZPICO_H")
        && header.contains("#define ZPICO_OK")
        && header.contains("typedef void (*ZpicoCallback)")
        && header.contains("int32_t zpico_init(")
}

/// Post-process the generated header to remove duplicate declarations
fn post_process_header(header: &str) -> String {
    use std::collections::HashSet;

    let mut result = String::new();
    let mut prev_blank = false;
    let mut skip_until_semicolon = false;
    let mut seen_declarations: HashSet<String> = HashSet::new();
    let mut pending_lines: Vec<String> = Vec::new();

    for line in header.lines() {
        let trimmed = line.trim();

        // Skip extern blocks entirely (single or multiline declarations)
        if line.starts_with("extern ") {
            // For single-line extern declarations, just skip this line
            // For multiline, skip until we see the closing semicolon
            if !trimmed.ends_with(';') {
                skip_until_semicolon = true;
            }
            continue;
        }

        // Continue skipping multiline extern until we see semicolon
        if skip_until_semicolon {
            if trimmed.ends_with(';') {
                skip_until_semicolon = false;
            }
            continue;
        }

        // Track doc comments to associate with declarations
        if trimmed.starts_with("/**") || trimmed.starts_with("*") || trimmed.starts_with("*/") {
            pending_lines.push(line.to_string());
            continue;
        }

        // Check for duplicate typedef declarations
        if trimmed.starts_with("typedef ")
            && let Some(name) = extract_typedef_name(trimmed)
        {
            if seen_declarations.contains(&name) {
                // Duplicate typedef - skip it and any pending doc comments
                pending_lines.clear();
                // Skip until semicolon for multiline typedefs
                if !trimmed.ends_with(';') {
                    skip_until_semicolon = true;
                }
                continue;
            }
            seen_declarations.insert(name);
        }

        // Check for duplicate function declarations
        if (trimmed.starts_with("int32_t ")
            || trimmed.starts_with("void ")
            || trimmed.starts_with("void *")
            || trimmed.starts_with("uint32_t ")
            || trimmed.starts_with("uint64_t ")
            || trimmed.starts_with("bool "))
            && trimmed.contains('(')
            && let Some(name) = extract_function_name(trimmed)
        {
            if seen_declarations.contains(&name) {
                // Duplicate function - skip it and any pending doc comments
                pending_lines.clear();
                // Skip until semicolon for multiline functions
                if !trimmed.ends_with(';') {
                    skip_until_semicolon = true;
                }
                continue;
            }
            seen_declarations.insert(name);
        }

        // Flush pending lines (doc comments)
        for pending in pending_lines.drain(..) {
            result.push_str(&pending);
            result.push('\n');
        }

        // Collapse multiple blank lines into one
        let is_blank = trimmed.is_empty();
        if is_blank && prev_blank {
            continue;
        }
        prev_blank = is_blank;

        result.push_str(line);
        result.push('\n');
    }

    result
}

/// Extract function name from a declaration line
fn extract_function_name(line: &str) -> Option<String> {
    // Pattern: "type func_name(..." or "type *func_name(..."
    let trimmed = line.trim();

    // Find the opening parenthesis
    let paren_pos = trimmed.find('(')?;

    // Get the part before the parenthesis
    let before_paren = &trimmed[..paren_pos];

    // Split by whitespace and get the last token (function name)
    // Handle pointer returns like "void *func_name"
    let name = before_paren.split_whitespace().last()?;

    // Remove any leading asterisks from pointer returns
    let clean_name = name.trim_start_matches('*');

    Some(clean_name.to_string())
}

/// Extract typedef name from a declaration line
fn extract_typedef_name(line: &str) -> Option<String> {
    // Pattern: "typedef ... (*TypeName)(...)" for function pointers
    // Or: "typedef ... TypeName;" for simple typedefs
    let trimmed = line.trim();

    // Function pointer typedef: look for (*Name)
    if let Some(start) = trimmed.find("(*") {
        let after_star = &trimmed[start + 2..];
        if let Some(end) = after_star.find(')') {
            return Some(after_star[..end].to_string());
        }
    }

    // Simple typedef: last word before semicolon
    if trimmed.ends_with(';') {
        let without_semi = trimmed.trim_end_matches(';').trim();
        return without_semi
            .split_whitespace()
            .last()
            .map(|s| s.to_string());
    }

    None
}

/// Use a pre-built zenoh-pico from `ZENOH_PICO_DIR` (system-zenohpico
/// feature). Expects a CMake install prefix layout:
///   $ZENOH_PICO_DIR/lib/libzenohpico.a
///   $ZENOH_PICO_DIR/include/zenoh-pico.h
fn use_system_zenoh_pico() -> PathBuf {
    let zenoh_pico_dir = env::var("ZENOH_PICO_DIR").unwrap_or_else(|_| {
        panic!(
            "ZENOH_PICO_DIR environment variable is required when system-zenohpico feature is enabled.\n\
             Set it to the CMake install prefix of your zenoh-pico build, e.g.:\n\
             ZENOH_PICO_DIR=/path/to/zenoh-pico-install cargo build --features system-zenohpico"
        );
    });
    println!("cargo:rerun-if-env-changed=ZENOH_PICO_DIR");

    let dir = PathBuf::from(&zenoh_pico_dir);
    let lib_path = dir.join("lib").join("libzenohpico.a");
    let header_path = dir.join("include").join("zenoh-pico.h");
    if !lib_path.exists() {
        panic!(
            "ZENOH_PICO_DIR={}: expected static library at {}\n\
             Build zenoh-pico with: cmake --build <build> && cmake --install <build>",
            zenoh_pico_dir,
            lib_path.display()
        );
    }
    if !header_path.exists() {
        panic!(
            "ZENOH_PICO_DIR={}: expected version header at {}\n\
             Build zenoh-pico with: cmake --build <build> && cmake --install <build>",
            zenoh_pico_dir,
            header_path.display()
        );
    }
    println!(
        "cargo:rustc-link-search=native={}",
        dir.join("lib").display()
    );
    println!("cargo:rustc-link-lib=static=zenohpico");
    let target = env::var("TARGET").unwrap_or_default();
    if target.contains("linux") || target.contains("darwin") || target.contains("macos") {
        println!("cargo:rustc-link-lib=pthread");
    } else if target.contains("windows") {
        println!("cargo:rustc-link-lib=ws2_32");
    }
    println!(
        "cargo:warning=Using system zenoh-pico from {}. \
         Ensure it was built with compatible Z_FEATURE_* flags \
         (Z_FEATURE_INTEREST=1, Z_FEATURE_MATCHING=1).",
        zenoh_pico_dir
    );
    dir.join("include")
}

/// Build the C shim library
///
/// Note: For Zephyr, C code is built by Zephyr's build system, not here.
#[allow(clippy::too_many_arguments)]
fn build_c_shim(
    c_dir: &Path,
    include_dir: &Path,
    zenoh_pico_include: &Path,
    use_posix: bool,
    use_bare_metal: bool,
    target: &str,
    link: &LinkFeatures,
    shim: &ShimConfig,
) {
    let mut build = cc::Build::new();

    // Include paths
    build.include(include_dir);
    build.include(zenoh_pico_include);

    // Core shim source
    build.file(c_dir.join("zpico/zpico.c"));

    // Platform-specific configuration
    if use_posix {
        #[cfg(target_os = "linux")]
        build.define("ZENOH_LINUX", None);
        #[cfg(target_os = "macos")]
        build.define("ZENOH_MACOS", None);
    } else if use_bare_metal {
        let platform_dir = c_dir.join("platform");

        // Include platform headers
        build.include(&platform_dir);

        // Platform defines — link features from Cargo features.
        //
        // Phase 132 — `ZPICO_NO_SMOLTCP=1` lets a consumer (the
        // serial-only board crate, an XRCE-over-UART firmware, …)
        // opt out of compiling the smoltcp glue into `zpico.c`.
        // Phase 128 retired the per-transport Cargo features that
        // used to gate this; without an override the embedded build
        // always pulls smoltcp because `LinkFeatures::from_env()`
        // hardcodes tcp/udp = true. Serial-only firmware links
        // against `ZPICO_SERIAL` instead and never provides the
        // `smoltcp_init` / `smoltcp_cleanup` symbols.
        let opt_out_smoltcp = env::var("ZPICO_NO_SMOLTCP").is_ok();
        println!("cargo:rerun-if-env-changed=ZPICO_NO_SMOLTCP");
        let has_network =
            (link.tcp || link.udp_unicast || link.udp_multicast) && !opt_out_smoltcp;
        if has_network {
            build.define("ZPICO_SMOLTCP", None);
        }
        if link.serial && !has_network {
            build.define("ZPICO_SERIAL", None);
        }
        build.define("ZENOH_GENERIC", None);
        build.define("Z_FEATURE_MULTI_THREAD", "0");
        // Phase 134.4 — every `Z_FEATURE_LINK_*` / `Z_FEATURE_RAWETH_*`
        // / `Z_FEATURE_SCOUTING_UDP` / `Z_FEATURE_UNSTABLE_API` value
        // lives in `<out_dir>/zenoh-config/zenoh_generic_config.h`,
        // generated by `generate_config_header` from the resolved
        // `LinkFeatures + LinkPolicy`. The compile units that need
        // them `#include "zenoh-pico/config.h"` which dispatches into
        // our header under `ZENOH_GENERIC`. NO `Z_FEATURE_LINK_*`
        // literals scattered through `build.rs`.
        let _ = link;

        // ARM cross-compilation flags
        if target.contains("thumbv7em") {
            build
                .flag("-mcpu=cortex-m4")
                .flag("-mthumb")
                .flag("-mfpu=fpv4-sp-d16")
                .flag("-mfloat-abi=hard");
        }

        // RISC-V cross-compilation flags (ESP32-C3)
        if target.contains("riscv32imc") {
            build.flag("-march=rv32imc").flag("-mabi=ilp32");

            // picolibc provides C standard library headers (stdint.h, etc.)
            // for the system riscv64-unknown-elf-gcc toolchain
            if has_picolibc_specs() {
                build.flag("--specs=picolibc.specs");
            }
        }
    }

    // Pass shim slot counts as -D flags so zpico.c gets them
    shim.apply_to_cc(&mut build);

    build.opt_level(2);
    build.compile("zpico");
}

/// Phase 136.4 — unified zenoh-pico cc-rs builder, driven by the
/// resolved `[platform.<name>]` block from `zenoh_platforms.toml`.
/// Replaces the five per-RTOS functions (`build_zenoh_pico_{embedded,
/// orin_spe, freertos, nuttx, threadx}`) and the POSIX
/// `build_zenoh_pico_native` body. Per-platform deltas all come from
/// the manifest: defines, required env vars, include paths
/// (interpolated `{env:VAR}` / `{nros}` / `{src}` / `{out}`),
/// conditional include paths (`when.target_match` /
/// `when.target_not` / `when.if_env`), extra C sources (with
/// `if_env` and `with_define` modifiers), the `[arch.*]` profile
/// (cflags + picolibc / errno-override / riscv-compiler hooks),
/// compile settings, and the `pic` flag.
#[allow(clippy::too_many_arguments)]
fn build_zenoh_pico_unified(
    plat: &manifest::ResolvedPlatform,
    arch_table: &std::collections::BTreeMap<String, manifest::ArchEntry>,
    interp: &manifest::InterpContext<'_>,
    zenoh_pico_src: &Path,
    out_dir: &Path,
    target: &str,
    link: &LinkFeatures,
    shim: &ShimConfig,
) {
    // Step 1 — validate required env vars (loud panic with help).
    for req in &plat.required_env {
        let val = env::var(&req.name).unwrap_or_else(|_| {
            panic!("{} not set. {}", req.name, req.help);
        });
        if let Some(subdir) = &req.validate_subdir {
            let path = PathBuf::from(&val).join(subdir);
            if !path.exists() {
                panic!(
                    "{}={}: missing {} (expected at {}). {}",
                    req.name,
                    val,
                    subdir,
                    path.display(),
                    req.help
                );
            }
        }
    }

    let mut build = cc::Build::new();

    // Step 2 — version header (shared with embedded path).
    let version_include_dir = out_dir.join("zenoh-pico-version");
    generate_embedded_version_header(zenoh_pico_src, &version_include_dir);

    // Step 3 — arch profile (cflags + sysroot / errno-override /
    // riscv-cc probe). Profile is applied iff it matches the
    // target triple.
    if let Some(arch_name) = plat.arch.as_deref() {
        if let Some(arch) = arch_table.get(arch_name) {
            if arch_matches(arch, target) {
                apply_arch(arch, &mut build, out_dir);
            }
        } else {
            panic!(
                "zenoh_platforms.toml: platform `{}` references unknown arch `{}`",
                plat.name, arch_name
            );
        }
    }

    // Step 4 — core sources + per-platform extra C files.
    add_zenoh_pico_core_sources(&mut build, zenoh_pico_src);
    for extra in &plat.extra_sources {
        if let Some(env_var) = &extra.if_env {
            if env::var(env_var).is_err() {
                continue;
            }
            println!("cargo:rerun-if-env-changed={env_var}");
        }
        let path_str = manifest::interpolate(&extra.path, interp).unwrap_or_else(|e| {
            panic!(
                "zenoh_platforms.toml: platform `{}` extra_sources `{}`: {e}",
                plat.name, extra.path
            )
        });
        build.file(&path_str);
        if let Some(def) = &extra.with_define {
            let value = def.get(1).map(|s| s.as_str());
            build.define(&def[0], value);
        }
    }

    // Step 5 — include paths (unconditional + conditional).
    let zenoh_config_dir = out_dir.join("zenoh-config");
    build
        .include(&zenoh_config_dir)
        .include(zenoh_pico_src.join("include"))
        .include(&version_include_dir);
    let is_embedded = is_embedded_target(target);
    for raw in &plat.include_paths {
        let path = manifest::interpolate(raw, interp).unwrap_or_else(|e| {
            panic!(
                "zenoh_platforms.toml: platform `{}` include_paths `{raw}`: {e}",
                plat.name
            )
        });
        build.include(&path);
    }
    for cond in &plat.include_paths_conditional {
        if !manifest::matches(&cond.when, target, is_embedded) {
            continue;
        }
        let path = manifest::interpolate(&cond.path, interp).unwrap_or_else(|e| {
            panic!(
                "zenoh_platforms.toml: platform `{}` conditional include `{}`: {e}",
                plat.name, cond.path
            )
        });
        build.include(&path);
    }

    // Step 6 — defines (unconditional, key=value, env-derived).
    for define in &plat.defines {
        build.define(define, None);
    }
    for (key, value) in &plat.defines_kv {
        build.define(key, value.as_str());
    }
    for (key, env_def) in &plat.defines_env {
        let value = env::var(&env_def.env).unwrap_or_else(|_| env_def.default.clone());
        build.define(key, value.as_str());
        println!("cargo:rerun-if-env-changed={}", env_def.env);
    }

    // Step 7 — TLS / mbedtls. Manifest sets `mbedtls` to
    // `pkg-config` / `vendored` / `none`; bare-metal vendored path
    // pulls in the in-tree mbedTLS submodule's sources.
    if link.tls {
        match plat.mbedtls.as_deref() {
            Some("pkg-config") => {
                let pc_dir = out_dir.join("pkgconfig");
                generate_mbedtls_pc_files(&pc_dir);
                let existing = env::var("PKG_CONFIG_PATH").unwrap_or_default();
                let new_path = if existing.is_empty() {
                    pc_dir.display().to_string()
                } else {
                    format!("{}:{existing}", pc_dir.display())
                };
                // SAFETY: build scripts are single-threaded.
                unsafe { env::set_var("PKG_CONFIG_PATH", &new_path) };
                let lib = pkg_config::Config::new()
                    .cargo_metadata(true)
                    .probe("mbedtls")
                    .expect("mbedtls discovery via pkg-config failed");
                for include in &lib.include_paths {
                    build.include(include);
                }
            }
            Some("vendored") | None => {
                // Bare-metal default — pull vendor sources.
                let zpico_sys_dir = zenoh_pico_src.parent().unwrap();
                let mbedtls_dir = zpico_sys_dir.join("mbedtls");
                let mbedtls_include = mbedtls_dir.join("include");
                let mbedtls_library = mbedtls_dir.join("library");
                if !mbedtls_include.exists() {
                    panic!(
                        "mbedTLS submodule not found at {:?}. Run: git submodule update --init",
                        mbedtls_include
                    );
                }
                build.include(&mbedtls_include);
                build.define("MBEDTLS_CONFIG_FILE", "\"mbedtls_config.h\"");
                let excluded = ["net_sockets.c", "timing.c", "threading.c", "psa_its_file.c"];
                if let Ok(entries) = std::fs::read_dir(&mbedtls_library) {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        if path.extension().is_some_and(|ext| ext == "c") {
                            let fname = path.file_name().unwrap().to_str().unwrap();
                            if !excluded.contains(&fname) {
                                build.file(&path);
                            }
                        }
                    }
                }
            }
            Some("none") | Some(_) => {}
        }
    }

    // Step 8 — shim slot counts.
    shim.apply_to_cc(&mut build);

    // Step 9 — compile settings (opt_level / warnings / cflags).
    if let Some(level) = plat.compile.opt_level {
        build.opt_level(level);
    }
    if let Some(w) = plat.compile.warnings {
        build.warnings(w);
    } else {
        // Default warnings off across the manifest-driven path —
        // mirrors what every per-RTOS function used to set.
        build.warnings(false);
    }
    for flag in &plat.compile.cflags {
        build.flag(flag);
    }

    // Step 10 — PIC override (NuttX flat builds).
    if let Some(pic) = plat.pic {
        build.pic(pic);
    }

    // Step 11 — `link` field consumed by the policy layer earlier;
    // touch here so it doesn't go cold under the borrow checker.
    let _ = link;

    build.compile("zenohpico");

    // Step 12 — register additional rerun-if-env-changed hooks.
    for var in &plat.rerun_if_env_changed {
        println!("cargo:rerun-if-env-changed={var}");
    }
}

/// Apply an `[arch.*]` profile to a `cc::Build`.
fn apply_arch(arch: &manifest::ArchEntry, build: &mut cc::Build, out_dir: &Path) {
    for flag in &arch.cflags {
        build.flag(flag);
    }
    if arch.needs_riscv_compiler {
        detect_riscv_compiler(build);
    }
    if arch.needs_errno_override {
        let errno_dir = out_dir.join("errno-override");
        std::fs::create_dir_all(&errno_dir).unwrap();
        std::fs::write(
            errno_dir.join("errno.h"),
            include_bytes!("c/platform/errno_override.h"),
        )
        .unwrap();
        // errno override must be searched BEFORE picolibc headers
        build.include(&errno_dir);
    }
    if arch.needs_picolibc {
        if let Some(sysroot) = get_picolibc_sysroot() {
            build.include(sysroot.join("include"));
        }
    }
}

/// Returns `true` when the `[arch.*]` block's `target_match` /
/// `target_exclude` predicates allow the current target triple.
fn arch_matches(arch: &manifest::ArchEntry, target: &str) -> bool {
    if let Some(needle) = arch.target_match.as_deref() {
        if !target.contains(needle) {
            return false;
        }
    }
    if let Some(needle) = arch.target_exclude.as_deref() {
        if target.contains(needle) {
            return false;
        }
    }
    true
}

/// Phase 136.4 (pre-collapse) — add the zenoh-pico core source set
/// (8 protocol subdirs + `system/common`) to a `cc::Build`. Every
/// per-RTOS `build_zenoh_pico_*` function used to inline the same
/// 13-line block; centralising lets future bumps add or remove a
/// subdir in one place. The platform-specific `system/<plat>/` set
/// is still per-RTOS — that's the next collapse step (136.4) which
/// drives it off `ResolvedPlatform::include` from `manifest.rs`.
fn add_zenoh_pico_core_sources(build: &mut cc::Build, zenoh_pico_src: &Path) {
    let src_dir = zenoh_pico_src.join("src");
    for subdir in &[
        "api",
        "collections",
        "link",
        "net",
        "protocol",
        "session",
        "transport",
        "utils",
    ] {
        add_c_sources_recursive(build, &src_dir.join(subdir));
    }
    // Common system sources (shared across all platforms)
    add_c_sources_recursive(build, &src_dir.join("system").join("common"));
}

/// Recursively collect all .c files from a directory and add them to a cc::Build.
fn add_c_sources_recursive(build: &mut cc::Build, dir: &Path) {
    if !dir.exists() {
        return;
    }
    let mut entries: Vec<_> = std::fs::read_dir(dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .collect();
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let path = entry.path();
        if path.is_dir() {
            add_c_sources_recursive(build, &path);
        } else if path.extension().is_some_and(|ext| ext == "c") {
            build.file(&path);
        }
    }
}

/// Detect and set the RISC-V cross-compiler for cc::Build.
fn detect_riscv_compiler(build: &mut cc::Build) {
    for cc_name in &["riscv64-unknown-elf-gcc", "riscv32-esp-elf-gcc"] {
        if Command::new(cc_name).arg("--version").output().is_ok() {
            build.compiler(cc_name);
            return;
        }
    }
    // Fall through — let cc crate try to detect automatically
}

/// Get the picolibc sysroot path for RISC-V (provides C standard library headers).
fn get_picolibc_sysroot() -> Option<PathBuf> {
    for cc_name in &["riscv64-unknown-elf-gcc", "riscv32-esp-elf-gcc"] {
        if let Ok(output) = Command::new(cc_name)
            .args([
                "-march=rv32imc",
                "-mabi=ilp32",
                "--specs=picolibc.specs",
                "-print-sysroot",
            ])
            .output()
            && output.status.success()
        {
            let sysroot = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !sysroot.is_empty() {
                let path = PathBuf::from(&sysroot);
                if path.join("include").exists() {
                    return Some(path);
                }
            }
        }
    }
    // Fallback: known system location
    let fallback = PathBuf::from("/usr/lib/picolibc/riscv64-unknown-elf");
    if fallback.join("include").exists() {
        return Some(fallback);
    }
    None
}

/// Generate pkg-config `.pc` files for mbedTLS.
///
/// Ubuntu's `libmbedtls-dev` doesn't ship `.pc` files, but zenoh-pico's
/// CMakeLists.txt uses `pkg_check_modules` to discover mbedTLS. We generate
/// minimal `.pc` files pointing to the system library paths so CMake can
/// find the installed libraries.
fn generate_mbedtls_pc_files(pc_dir: &Path) {
    std::fs::create_dir_all(pc_dir).unwrap();

    // Detect library directory (multi-arch on Debian/Ubuntu)
    let lib_dir = if Path::new("/usr/lib/x86_64-linux-gnu/libmbedtls.so").exists() {
        "/usr/lib/x86_64-linux-gnu"
    } else if Path::new("/usr/lib/aarch64-linux-gnu/libmbedtls.so").exists() {
        "/usr/lib/aarch64-linux-gnu"
    } else {
        "/usr/lib"
    };

    for (name, libs, requires) in [
        ("mbedcrypto", "-lmbedcrypto", ""),
        ("mbedx509", "-lmbedx509", "mbedcrypto"),
        ("mbedtls", "-lmbedtls", "mbedx509"),
    ] {
        let pc = format!(
            "prefix=/usr\n\
             libdir={lib_dir}\n\
             includedir=/usr/include\n\n\
             Name: {name}\n\
             Description: mbed TLS - {name}\n\
             Version: 2.28.0\n\
             Libs: -L${{libdir}} {libs}\n\
             Cflags: -I${{includedir}}\n\
             Requires: {requires}\n"
        );
        std::fs::write(pc_dir.join(format!("{name}.pc")), pc).unwrap();
    }
}

/// Generate zenoh-pico version header for embedded builds.
fn generate_embedded_version_header(zenoh_pico_src: &Path, include_dir: &Path) {
    std::fs::create_dir_all(include_dir).unwrap();

    let version_file = zenoh_pico_src.join("version.txt");
    let version = std::fs::read_to_string(version_file)
        .unwrap_or_else(|_| "0.0.0".to_string())
        .trim()
        .to_string();

    let parts: Vec<&str> = version.split('.').collect();
    let major = parts.first().unwrap_or(&"0");
    let minor = parts.get(1).unwrap_or(&"0");
    let patch = parts.get(2).unwrap_or(&"0");

    let template = include_str!("c/zenoh-pico-version.h.in");
    let header = template
        .replace("@ZENOH_PICO@", &version)
        .replace("@ZENOH_PICO_MAJOR@", major)
        .replace("@ZENOH_PICO_MINOR@", minor)
        .replace("@ZENOH_PICO_PATCH@", patch);

    std::fs::write(include_dir.join("zenoh-pico.h"), header).unwrap();
}
