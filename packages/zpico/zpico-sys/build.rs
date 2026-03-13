//! Build script for zpico-sys
//!
//! This builds:
//! 1. zenoh-pico C library (via CMake for native, sources for embedded)
//! 2. The zpico C layer (zpico.c)
//! 3. Generates C header from Rust FFI declarations (cbindgen)

use std::env;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Protocol link features read from Cargo feature flags.
///
/// Each field corresponds to a `link-*` Cargo feature that controls
/// the matching `Z_FEATURE_LINK_*` flag passed to zenoh-pico at compile time.
struct LinkFeatures {
    tcp: bool,
    udp_unicast: bool,
    udp_multicast: bool,
    serial: bool,
    raweth: bool,
    tls: bool,
}

impl LinkFeatures {
    /// Read link features from Cargo environment variables.
    fn from_env() -> Self {
        Self {
            tcp: env::var("CARGO_FEATURE_LINK_TCP").is_ok(),
            udp_unicast: env::var("CARGO_FEATURE_LINK_UDP_UNICAST").is_ok(),
            udp_multicast: env::var("CARGO_FEATURE_LINK_UDP_MULTICAST").is_ok(),
            serial: env::var("CARGO_FEATURE_LINK_SERIAL").is_ok(),
            raweth: env::var("CARGO_FEATURE_LINK_RAWETH").is_ok(),
            tls: env::var("CARGO_FEATURE_LINK_TLS").is_ok(),
        }
    }

    fn tcp_flag(&self) -> u8 {
        self.tcp as u8
    }
    fn udp_unicast_flag(&self) -> u8 {
        self.udp_unicast as u8
    }
    fn udp_multicast_flag(&self) -> u8 {
        self.udp_multicast as u8
    }
    fn serial_flag(&self) -> u8 {
        self.serial as u8
    }
    fn raweth_flag(&self) -> u8 {
        self.raweth as u8
    }
    fn tls_flag(&self) -> u8 {
        self.tls as u8
    }
}

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
    writeln!(header, "#define Z_CONFIG_SOCKET_TIMEOUT 100").unwrap();
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
    writeln!(header, "#define Z_FEATURE_ENCODING_VALUES 1").unwrap();
    writeln!(header, "#define Z_FEATURE_TCP_NODELAY 1").unwrap();
    writeln!(header, "#define Z_FEATURE_LOCAL_SUBSCRIBER 0").unwrap();
    writeln!(header, "#define Z_FEATURE_LOCAL_QUERYABLE 0").unwrap();
    writeln!(header, "#define Z_FEATURE_SESSION_CHECK 1").unwrap();
    writeln!(header, "#define Z_FEATURE_BATCHING 1").unwrap();
    writeln!(header, "#define Z_FEATURE_BATCH_TX_MUTEX 0").unwrap();
    writeln!(header, "#define Z_FEATURE_BATCH_PEER_MUTEX 0").unwrap();
    writeln!(header, "#define Z_FEATURE_MATCHING 0").unwrap();
    writeln!(header, "#define Z_FEATURE_RX_CACHE 0").unwrap();
    writeln!(header, "#define Z_FEATURE_UNICAST_PEER 0").unwrap();
    writeln!(header, "#define Z_FEATURE_AUTO_RECONNECT 1").unwrap();
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

    // Check which platform backend to use
    let use_posix = env::var("CARGO_FEATURE_POSIX").is_ok();
    let use_zephyr = env::var("CARGO_FEATURE_ZEPHYR").is_ok();
    let use_bare_metal = env::var("CARGO_FEATURE_BARE_METAL").is_ok();
    let use_freertos = env::var("CARGO_FEATURE_FREERTOS").is_ok();
    let use_nuttx = env::var("CARGO_FEATURE_NUTTX").is_ok();
    let use_threadx = env::var("CARGO_FEATURE_THREADX").is_ok();

    // Count enabled backends
    let backend_count = [
        use_posix,
        use_zephyr,
        use_bare_metal,
        use_freertos,
        use_nuttx,
        use_threadx,
    ]
    .iter()
    .filter(|&&b| b)
    .count();

    if backend_count == 0 {
        // No backend selected - just build zenoh-pico and generate bindings
        // This allows building for testing header generation
        println!("cargo:warning=No platform backend selected. Building minimal configuration.");
    }

    if backend_count > 1 {
        panic!(
            "Only one platform backend can be selected at a time \
             (posix, zephyr, bare-metal, freertos, nuttx, or threadx)"
        );
    }

    // Read link-* features for bare-metal protocol selection
    let link_features = LinkFeatures::from_env();

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

    // Build zenoh-pico and C shim
    //
    // ThreadX is checked first because it uses its own build path for both
    // native (Linux simulation) and embedded (RISC-V QEMU) targets. The
    // ThreadX build compiles zenoh-pico with our custom system.c + network.c
    // (NetX Duo BSD sockets) rather than the POSIX/CMake path.
    if use_threadx {
        // ThreadX: build zenoh-pico + custom ThreadX system/network layer + shim.
        // Uses our own system.c (ThreadX tasks/mutex/clock) + network.c (NetX Duo BSD sockets).
        // Works for both native (Linux sim) and embedded (RISC-V QEMU) targets.
        generate_config_header(&out_dir, &link_features, &buf_config);
        build_zenoh_pico_threadx(
            &zenoh_pico_src,
            &c_dir,
            &include_dir,
            &out_dir,
            &target,
            &link_features,
            &shim_config,
        );
    } else if !is_embedded_target(&target) {
        // Native: build zenoh-pico via CMake (or use system library), then shim via cc
        let zenoh_pico_include = if use_system {
            use_system_zenoh_pico()
        } else {
            build_zenoh_pico_native(&zenoh_pico_src, &out_dir, &buf_config, &link_features)
        };
        if backend_count > 0 && !use_zephyr && !use_freertos {
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
    } else if use_bare_metal {
        // Embedded + bare-metal: build zenoh-pico + platform + shim all together with cc.
        // This replaces the external build-zenoh-pico.sh shell scripts.
        // Generate config header from Cargo link-* features before building.
        generate_config_header(&out_dir, &link_features, &buf_config);
        build_zenoh_pico_embedded(
            &zenoh_pico_src,
            &c_dir,
            &include_dir,
            &out_dir,
            &target,
            &link_features,
            &shim_config,
        );
    } else if use_freertos {
        // Embedded + FreeRTOS: build zenoh-pico + FreeRTOS platform + shim with cc.
        // Uses zenoh-pico's built-in FreeRTOS+lwIP platform (system.c + lwip/network.c).
        generate_config_header(&out_dir, &link_features, &buf_config);
        build_zenoh_pico_freertos(
            &zenoh_pico_src,
            &c_dir,
            &include_dir,
            &out_dir,
            &target,
            &link_features,
            &shim_config,
        );
    } else if use_nuttx {
        // Embedded + NuttX: build zenoh-pico + unix platform (NuttX is POSIX-compatible) + shim.
        // Reuses zenoh-pico's unix/system.c + unix/network.c with ZENOH_NUTTX define.
        generate_config_header(&out_dir, &link_features, &buf_config);
        build_zenoh_pico_nuttx(
            &zenoh_pico_src,
            &c_dir,
            &include_dir,
            &out_dir,
            &target,
            &link_features,
            &shim_config,
        );
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
    }

    // Rerun triggers
    println!("cargo:rerun-if-changed=c/zpico/zpico.c");
    println!("cargo:rerun-if-changed=c/platform/bare-metal/platform.h");
    println!("cargo:rerun-if-changed=c/platform/errno_override.h");
    println!("cargo:rerun-if-changed=c/platform/zenoh_generic_config.h");
    println!("cargo:rerun-if-changed=c/platform/zenoh_generic_platform.h");
    println!("cargo:rerun-if-changed=zenoh-pico/src/system/unix/network.c");
    println!("cargo:rerun-if-changed=zenoh-pico/include/zenoh-pico/system/platform/unix.h");
    println!("cargo:rerun-if-changed=zenoh-pico/src/system/freertos/system.c");
    println!("cargo:rerun-if-changed=zenoh-pico/src/system/freertos/lwip/network.c");
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

            std::fs::write(&output_file, processed).unwrap_or_else(|e| {
                println!("cargo:warning=Failed to write header: {e}");
            });
        }
        Err(e) => {
            println!("cargo:warning=cbindgen failed: {e}");
        }
    }
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

/// Build zenoh-pico via CMake for native targets
fn build_zenoh_pico_native(
    zenoh_pico_src: &Path,
    out_dir: &Path,
    buf: &ZenohBufferConfig,
    link: &LinkFeatures,
) -> PathBuf {
    let zenoh_pico_build = out_dir.join("zenoh-pico-build");

    // Copy source to build directory to avoid modifying source tree
    copy_source_tree(zenoh_pico_src, &zenoh_pico_build);

    // Generate version header
    generate_version_header(&zenoh_pico_build);

    // Build via CMake
    // Note: Z_FEATURE_INTEREST must be enabled for proper message routing between
    // clients on different networks (e.g., Zephyr on TAP vs native on localhost).
    // Both clients must have matching INTEREST settings for the router to route properly.
    let mut cmake_cfg = cmake::Config::new(&zenoh_pico_build);
    cmake_cfg
        .define("BUILD_SHARED_LIBS", "OFF")
        .define("BUILD_EXAMPLES", "OFF")
        .define("BUILD_TESTING", "OFF")
        .define("BUILD_TOOLS", "OFF")
        .define("ZENOH_DEBUG", "0")
        .define("Z_FEATURE_LOCAL_SUBSCRIBER", "0")
        .define("Z_FEATURE_INTEREST", "1")
        .define("Z_FEATURE_MATCHING", "1")
        .define("Z_FEATURE_LINK_SERIAL", "0")
        .define(
            "Z_FEATURE_UNSTABLE_API",
            if env::var("CARGO_FEATURE_UNSTABLE_ZENOH_API").is_ok() {
                "1"
            } else {
                "0"
            },
        )
        // zenoh-pico CMakeLists.txt uses FRAG_MAX_SIZE / BATCH_*_SIZE (no Z_ prefix)
        .define("FRAG_MAX_SIZE", buf.frag_max_size.to_string())
        .define("BATCH_UNICAST_SIZE", buf.batch_unicast_size.to_string())
        .define("BATCH_MULTICAST_SIZE", buf.batch_multicast_size.to_string());

    // TLS support via mbedTLS (zenoh-pico's CMakeLists.txt handles finding mbedTLS)
    if link.tls {
        cmake_cfg.define("Z_FEATURE_LINK_TLS", "1");

        // Ubuntu's libmbedtls-dev doesn't ship pkg-config .pc files, but
        // zenoh-pico's CMakeLists.txt uses pkg_check_modules to find mbedTLS.
        // Generate .pc files so CMake can discover the system libraries.
        let pc_dir = out_dir.join("pkgconfig");
        generate_mbedtls_pc_files(&pc_dir);
        // Prepend our pc dir so CMake's FindPkgConfig picks it up first.
        let existing = env::var("PKG_CONFIG_PATH").unwrap_or_default();
        let new_path = if existing.is_empty() {
            pc_dir.display().to_string()
        } else {
            format!("{}:{existing}", pc_dir.display())
        };
        // SAFETY: build scripts are single-threaded; no other thread reads this variable.
        unsafe { env::set_var("PKG_CONFIG_PATH", &new_path) };
    }

    let dst = cmake_cfg.build();

    // Link the static library
    println!("cargo:rustc-link-search=native={}/lib", dst.display());
    println!("cargo:rustc-link-lib=static=zenohpico");

    // Link system libraries
    let target = env::var("TARGET").unwrap_or_default();
    if target.contains("linux") || target.contains("darwin") || target.contains("macos") {
        println!("cargo:rustc-link-lib=pthread");
    } else if target.contains("windows") {
        println!("cargo:rustc-link-lib=ws2_32");
    }

    // Link mbedTLS libraries (zenoh-pico's static lib references mbedTLS symbols)
    if link.tls {
        println!("cargo:rustc-link-lib=mbedtls");
        println!("cargo:rustc-link-lib=mbedx509");
        println!("cargo:rustc-link-lib=mbedcrypto");
    }

    // Return installed include dir (not source dir) so the cc shim build gets
    // the CMake-generated config.h with correct Z_FEATURE_* defines.
    dst.join("include")
}

/// Use a pre-built zenoh-pico from ZENOH_PICO_DIR (system-zenohpico feature).
///
/// Expects a CMake install prefix layout:
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

    // Link the pre-built library
    println!(
        "cargo:rustc-link-search=native={}",
        dir.join("lib").display()
    );
    println!("cargo:rustc-link-lib=static=zenohpico");

    // Link system libraries (same as build_zenoh_pico_native)
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

/// Copy source tree to build directory
fn copy_source_tree(src: &Path, dst: &Path) {
    if dst.exists() {
        // Check if we need to recopy by comparing sentinel files.
        // We check CMakeLists.txt and network.c (the latter catches submodule changes
        // that don't touch the build system, e.g. adding serial support).
        let sentinels = ["CMakeLists.txt", "src/system/unix/network.c"];
        let mut up_to_date = true;
        for sentinel in &sentinels {
            let src_file = src.join(sentinel);
            let dst_file = dst.join(sentinel);
            if !dst_file.exists() {
                up_to_date = false;
                break;
            }
            let src_meta = std::fs::metadata(&src_file).ok();
            let dst_meta = std::fs::metadata(&dst_file).ok();
            match (src_meta, dst_meta) {
                (Some(s), Some(d)) => {
                    if let (Ok(st), Ok(dt)) = (s.modified(), d.modified())
                        && dt < st
                    {
                        up_to_date = false;
                        break;
                    }
                }
                _ => {
                    up_to_date = false;
                    break;
                }
            }
        }
        if up_to_date {
            return;
        }
        let _ = std::fs::remove_dir_all(dst);
    }

    let status = Command::new("cp")
        .args(["-r", src.to_str().unwrap(), dst.to_str().unwrap()])
        .status()
        .expect("Failed to copy zenoh-pico source");

    if !status.success() {
        panic!("Failed to copy zenoh-pico source to build directory");
    }
}

/// Generate zenoh-pico.h version header
fn generate_version_header(build_dir: &Path) {
    let include_dir = build_dir.join("include");
    std::fs::create_dir_all(&include_dir).unwrap();

    let version_header = include_dir.join("zenoh-pico.h");
    let version_file = build_dir.join("version.txt");

    let version = std::fs::read_to_string(&version_file)
        .unwrap_or_else(|_| "0.0.0".to_string())
        .trim()
        .to_string();

    let parts: Vec<&str> = version.split('.').collect();
    let major = parts.first().unwrap_or(&"0");
    let minor = parts.get(1).unwrap_or(&"0");
    let patch = parts.get(2).unwrap_or(&"0");

    let template_path = include_dir.join("zenoh-pico.h.in");
    if template_path.exists() {
        let template = std::fs::read_to_string(&template_path).unwrap();
        let generated = template
            .replace("@ZENOH_PICO@", &version)
            .replace("@ZENOH_PICO_MAJOR@", major)
            .replace("@ZENOH_PICO_MINOR@", minor)
            .replace("@ZENOH_PICO_PATCH@", patch)
            .replace("@ZENOH_PICO_TWEAK@", "0");
        std::fs::write(&version_header, generated).unwrap();
    }
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

        // Platform defines — link features from Cargo features
        let has_network = link.tcp || link.udp_unicast || link.udp_multicast;
        println!("cargo:warning=zpico-sys: has_network={has_network}, link.serial={}, link.tcp={}, link.udp_unicast={}", link.serial, link.tcp, link.udp_unicast);
        if has_network {
            build.define("ZPICO_SMOLTCP", None);
            println!("cargo:warning=zpico-sys: defining ZPICO_SMOLTCP");
        }
        if link.serial && !has_network {
            build.define("ZPICO_SERIAL", None);
            println!("cargo:warning=zpico-sys: defining ZPICO_SERIAL");
        }
        build.define("ZENOH_GENERIC", None);
        build.define("Z_FEATURE_MULTI_THREAD", "0");
        build.define("Z_FEATURE_LINK_TCP", if link.tcp { "1" } else { "0" });
        build.define(
            "Z_FEATURE_LINK_UDP_UNICAST",
            if link.udp_unicast { "1" } else { "0" },
        );
        build.define(
            "Z_FEATURE_LINK_UDP_MULTICAST",
            if link.udp_multicast { "1" } else { "0" },
        );
        build.define("Z_FEATURE_LINK_SERIAL", if link.serial { "1" } else { "0" });
        build.define("Z_FEATURE_LINK_TLS", if link.tls { "1" } else { "0" });
        build.define(
            "Z_FEATURE_RAWETH_TRANSPORT",
            if link.raweth { "1" } else { "0" },
        );
        build.define("Z_FEATURE_SCOUTING_UDP", "0");
        if env::var("CARGO_FEATURE_UNSTABLE_ZENOH_API").is_ok() {
            build.define("Z_FEATURE_UNSTABLE_API", "1");
        }

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

/// Build zenoh-pico + platform layer + shim for embedded targets using cc.
///
/// Compiles all zenoh-pico sources together with our platform headers and
/// shim into a single static library (`libzenohpico.a`). This replaces the
/// external `scripts/{qemu,esp32}/build-zenoh-pico.sh` shell scripts.
#[allow(clippy::too_many_arguments)]
fn build_zenoh_pico_embedded(
    zenoh_pico_src: &Path,
    c_dir: &Path,
    include_dir: &Path,
    out_dir: &Path,
    target: &str,
    link: &LinkFeatures,
    shim: &ShimConfig,
) {
    let mut build = cc::Build::new();
    let platform_dir = c_dir.join("platform");

    // Generate version header in OUT_DIR
    let version_include_dir = out_dir.join("zenoh-pico-version");
    generate_embedded_version_header(zenoh_pico_src, &version_include_dir);

    // RISC-V toolchain setup (compiler detection, errno shadow, picolibc)
    if target.contains("riscv32imc") {
        detect_riscv_compiler(&mut build);
        build.flag("-march=rv32imc").flag("-mabi=ilp32");

        // Generate errno.h shadow that avoids picolibc's TLS-based errno.
        // picolibc declares `extern __thread int errno` which uses the tp register.
        // On bare-metal ESP32-C3, tp is never initialized → null pointer crash.
        let errno_dir = out_dir.join("errno-override");
        std::fs::create_dir_all(&errno_dir).unwrap();
        std::fs::write(
            errno_dir.join("errno.h"),
            include_bytes!("c/platform/errno_override.h"),
        )
        .unwrap();
        // errno override must be searched BEFORE picolibc headers
        build.include(&errno_dir);

        // Add picolibc sysroot for C standard library headers (stdint.h, etc.)
        // Do NOT use --specs=picolibc.specs (it enables TLS errno)
        if let Some(sysroot) = get_picolibc_sysroot() {
            build.include(sysroot.join("include"));
        }
    }

    // ARM Cortex-M cross-compilation flags
    if target.contains("thumbv7m") && !target.contains("thumbv7me") {
        build.flag("-mcpu=cortex-m3").flag("-mthumb");
    } else if target.contains("thumbv7em") {
        build
            .flag("-mcpu=cortex-m4")
            .flag("-mthumb")
            .flag("-mfpu=fpv4-sp-d16")
            .flag("-mfloat-abi=hard");
    }

    // Collect zenoh-pico core sources (excluding platform-specific system backends)
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
        add_c_sources_recursive(&mut build, &src_dir.join(subdir));
    }
    // Common system sources (shared across all platforms)
    add_c_sources_recursive(&mut build, &src_dir.join("system").join("common"));

    // Shim (high-level API wrapper)
    build.file(c_dir.join("zpico").join("zpico.c"));

    // Include paths
    // Generated config header takes precedence over the static one in platform_dir
    let generated_config_dir = out_dir.join("zenoh-config");
    build.include(&generated_config_dir);
    build.include(zenoh_pico_src.join("include"));
    build.include(&version_include_dir);
    build.include(&platform_dir);
    build.include(include_dir);

    // Platform defines
    build.define("ZENOH_GENERIC", None);
    let has_network = link.tcp || link.udp_unicast || link.udp_multicast;
    if has_network {
        build.define("ZPICO_SMOLTCP", None);
    } else if link.serial {
        build.define("ZPICO_SERIAL", None);
    }
    build.define("ZENOH_DEBUG", "0");
    // Link features are set in the generated zenoh_generic_config.h,
    // but also pass them as -D flags for consistency with any code that
    // checks these before including the config header.
    build.define("Z_FEATURE_MULTI_THREAD", "0");
    build.define("Z_FEATURE_LINK_TCP", if link.tcp { "1" } else { "0" });
    build.define(
        "Z_FEATURE_LINK_UDP_UNICAST",
        if link.udp_unicast { "1" } else { "0" },
    );
    build.define(
        "Z_FEATURE_LINK_UDP_MULTICAST",
        if link.udp_multicast { "1" } else { "0" },
    );
    build.define("Z_FEATURE_LINK_SERIAL", if link.serial { "1" } else { "0" });
    build.define("Z_FEATURE_LINK_TLS", if link.tls { "1" } else { "0" });
    build.define("Z_FEATURE_LINK_WS", "0");
    build.define("Z_FEATURE_LINK_BLUETOOTH", "0");
    build.define(
        "Z_FEATURE_RAWETH_TRANSPORT",
        if link.raweth { "1" } else { "0" },
    );
    build.define("Z_FEATURE_SCOUTING_UDP", "0");

    // Pass slot counts as -D flags so zpico.c gets them
    shim.apply_to_cc(&mut build);

    // mbedTLS — when Z_FEATURE_LINK_TLS=1:
    // 1. Add mbedTLS include paths for zenoh-pico's link/unicast/tls.c
    // 2. Compile mbedTLS library sources (for crypto/TLS primitives)
    // 3. Compile TLS platform symbols (tls_bare_metal.c, entropy_bare_metal.c)
    //
    // Everything is compiled into the same `zenohpico` archive so the linker
    // can resolve references between zenoh-pico's link layer and the TLS
    // platform implementation without circular archive dependencies.
    if link.tls {
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

        // zpico-smoltcp provides the bare-metal mbedTLS config header
        let smoltcp_c_dir = zpico_sys_dir.join("../zpico-smoltcp/c");
        if smoltcp_c_dir.exists() {
            build.include(&smoltcp_c_dir);
        }
        build.define("MBEDTLS_CONFIG_FILE", "\"mbedtls_config.h\"");

        // Compile mbedTLS library sources (excluding POSIX-only files)
        let excluded_mbedtls = ["net_sockets.c", "timing.c", "threading.c", "psa_its_file.c"];
        if let Ok(entries) = std::fs::read_dir(&mbedtls_library) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().is_some_and(|ext| ext == "c") {
                    let filename = path.file_name().unwrap().to_str().unwrap();
                    if !excluded_mbedtls.contains(&filename) {
                        build.file(&path);
                    }
                }
            }
        }

        // TLS platform symbols (bare-metal implementation via smoltcp)
        let tls_src = smoltcp_c_dir.join("tls_bare_metal.c");
        if tls_src.exists() {
            build.file(&tls_src);
        }

        // Entropy source (DWT-based, weak symbol)
        let entropy_src = smoltcp_c_dir.join("entropy_bare_metal.c");
        if entropy_src.exists() {
            build.file(&entropy_src);
        }
    }

    // Embedded-optimized compiler flags
    build
        .opt_level(2)
        .flag("-ffunction-sections")
        .flag("-fdata-sections")
        .warnings(false);

    build.compile("zenohpico");
}

/// Build zenoh-pico + FreeRTOS platform layer + shim for embedded FreeRTOS+lwIP targets.
///
/// Uses zenoh-pico's built-in FreeRTOS system implementation (`system.c`) and
/// lwIP network layer (`lwip/network.c`). FreeRTOS has real threads, so
/// `Z_FEATURE_MULTI_THREAD=1` is set (unlike bare-metal).
///
/// Required environment variables:
/// - `FREERTOS_DIR` — path to FreeRTOS kernel source (e.g., `external/freertos-kernel`)
/// - `FREERTOS_PORT` — portable layer (e.g., `GCC/ARM_CM3`)
/// - `LWIP_DIR` — path to lwIP source (e.g., `external/lwip`)
/// - `FREERTOS_CONFIG_DIR` — path to directory with `FreeRTOSConfig.h` + `lwipopts.h`
#[allow(clippy::too_many_arguments)]
fn build_zenoh_pico_freertos(
    zenoh_pico_src: &Path,
    c_dir: &Path,
    include_dir: &Path,
    out_dir: &Path,
    target: &str,
    link: &LinkFeatures,
    shim: &ShimConfig,
) {
    // Read FreeRTOS environment variables
    let freertos_dir = PathBuf::from(env::var("FREERTOS_DIR").unwrap_or_else(|_| {
        panic!(
            "FREERTOS_DIR not set. Point it at the FreeRTOS kernel source directory.\n\
             Run `just setup-freertos` to download, then set:\n\
             export FREERTOS_DIR=$PWD/external/freertos-kernel"
        );
    }));
    let freertos_port = env::var("FREERTOS_PORT").unwrap_or_else(|_| {
        panic!(
            "FREERTOS_PORT not set. Set it to the FreeRTOS portable layer.\n\
             Example: export FREERTOS_PORT=GCC/ARM_CM3"
        );
    });
    let lwip_dir = PathBuf::from(env::var("LWIP_DIR").unwrap_or_else(|_| {
        panic!(
            "LWIP_DIR not set. Point it at the lwIP source directory.\n\
             Run `just setup-freertos` to download, then set:\n\
             export LWIP_DIR=$PWD/external/lwip"
        );
    }));
    let freertos_config_dir = PathBuf::from(env::var("FREERTOS_CONFIG_DIR").unwrap_or_else(|_| {
        panic!(
            "FREERTOS_CONFIG_DIR not set. Point it at a directory containing\n\
             FreeRTOSConfig.h and lwipopts.h for your board.\n\
             Example: export FREERTOS_CONFIG_DIR=packages/boards/nros-mps2-an385-freertos/config"
        );
    }));

    // Validate directories exist
    if !freertos_dir.join("include").exists() {
        panic!(
            "FREERTOS_DIR={}: missing include/ directory. Is this a valid FreeRTOS kernel source?",
            freertos_dir.display()
        );
    }
    let port_dir = freertos_dir.join("portable").join(&freertos_port);
    if !port_dir.exists() {
        panic!(
            "FREERTOS_DIR/portable/{} not found at {}",
            freertos_port,
            port_dir.display()
        );
    }
    if !lwip_dir.join("src").join("include").exists() {
        panic!(
            "LWIP_DIR={}: missing src/include/ directory. Is this a valid lwIP source?",
            lwip_dir.display()
        );
    }

    let mut build = cc::Build::new();

    // Generate version header in OUT_DIR
    let version_include_dir = out_dir.join("zenoh-pico-version");
    generate_embedded_version_header(zenoh_pico_src, &version_include_dir);

    // ARM Cortex-M cross-compilation flags
    if target.contains("thumbv7m") && !target.contains("thumbv7me") {
        build.flag("-mcpu=cortex-m3").flag("-mthumb");
    } else if target.contains("thumbv7em") {
        build
            .flag("-mcpu=cortex-m4")
            .flag("-mthumb")
            .flag("-mfpu=fpv4-sp-d16")
            .flag("-mfloat-abi=hard");
    }

    // Collect zenoh-pico core sources (excluding platform-specific system backends)
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
        add_c_sources_recursive(&mut build, &src_dir.join(subdir));
    }
    // Common system sources (shared across all platforms)
    add_c_sources_recursive(&mut build, &src_dir.join("system").join("common"));

    // FreeRTOS platform sources (threading, clock, memory, random)
    build.file(src_dir.join("system/freertos/system.c"));
    // lwIP network layer (TCP/UDP sockets via lwIP's POSIX-compatible API)
    build.file(src_dir.join("system/freertos/lwip/network.c"));

    // Shim (high-level API wrapper)
    build.file(c_dir.join("zpico").join("zpico.c"));

    // Include paths (order matters — generated config takes precedence)
    let generated_config_dir = out_dir.join("zenoh-config");
    build.include(&generated_config_dir);
    build.include(zenoh_pico_src.join("include"));
    build.include(&version_include_dir);
    build.include(include_dir);

    // FreeRTOS kernel headers
    build.include(freertos_dir.join("include"));
    build.include(&port_dir);

    // User-provided config (FreeRTOSConfig.h, lwipopts.h)
    build.include(&freertos_config_dir);

    // lwIP headers
    build.include(lwip_dir.join("src/include"));
    // lwIP FreeRTOS port (provides arch/sys_arch.h for threaded mode)
    build.include(lwip_dir.join("contrib/ports/freertos/include"));

    // Platform defines
    // ZENOH_GENERIC: tells zenoh-pico config.h to use our generated config header
    // ZENOH_FREERTOS_LWIP: tells zenoh-pico platform.h to use FreeRTOS+lwIP types
    build.define("ZENOH_GENERIC", None);
    build.define("ZENOH_FREERTOS_LWIP", None);
    build.define("ZENOH_DEBUG", "0");

    // FreeRTOS has real threads — override the #ifndef default of 0 in config header
    build.define("Z_FEATURE_MULTI_THREAD", "1");

    // Link features (same as embedded — controlled by Cargo link-* features)
    build.define("Z_FEATURE_LINK_TCP", if link.tcp { "1" } else { "0" });
    build.define(
        "Z_FEATURE_LINK_UDP_UNICAST",
        if link.udp_unicast { "1" } else { "0" },
    );
    build.define(
        "Z_FEATURE_LINK_UDP_MULTICAST",
        if link.udp_multicast { "1" } else { "0" },
    );
    build.define("Z_FEATURE_LINK_SERIAL", if link.serial { "1" } else { "0" });
    build.define("Z_FEATURE_LINK_WS", "0");
    build.define("Z_FEATURE_LINK_BLUETOOTH", "0");
    build.define(
        "Z_FEATURE_RAWETH_TRANSPORT",
        if link.raweth { "1" } else { "0" },
    );
    build.define("Z_FEATURE_SCOUTING_UDP", "0");
    if env::var("CARGO_FEATURE_UNSTABLE_ZENOH_API").is_ok() {
        build.define("Z_FEATURE_UNSTABLE_API", "1");
    }

    // Pass shim slot counts as -D flags
    shim.apply_to_cc(&mut build);

    // Embedded-optimized compiler flags
    build
        .opt_level(2)
        .flag("-ffunction-sections")
        .flag("-fdata-sections")
        .warnings(false);

    build.compile("zenohpico");
}

/// Build zenoh-pico for NuttX targets.
///
/// NuttX is POSIX-compliant, so we reuse the unix/ platform sources (system.c + network.c)
/// with a ZENOH_NUTTX define for RNG adaptation. NuttX provides pthreads, BSD sockets,
/// clock_gettime(), and /dev/urandom — all needed by the unix platform layer.
///
/// Only requires NUTTX_DIR (for NuttX system headers). No lwIP or FreeRTOS dirs needed.
fn build_zenoh_pico_nuttx(
    zenoh_pico_src: &Path,
    c_dir: &Path,
    include_dir: &Path,
    out_dir: &Path,
    target: &str,
    link: &LinkFeatures,
    shim: &ShimConfig,
) {
    // Read NuttX environment variable
    let nuttx_dir = PathBuf::from(env::var("NUTTX_DIR").unwrap_or_else(|_| {
        panic!(
            "NUTTX_DIR not set. Point it at the NuttX OS source directory.\n\
             Run `just setup-nuttx` to download, then set:\n\
             export NUTTX_DIR=$PWD/external/nuttx"
        );
    }));

    // Validate directory exists
    if !nuttx_dir.join("include").exists() {
        panic!(
            "NUTTX_DIR={}: missing include/ directory. Is this a valid NuttX source?",
            nuttx_dir.display()
        );
    }

    let mut build = cc::Build::new();

    // Generate version header in OUT_DIR
    let version_include_dir = out_dir.join("zenoh-pico-version");
    generate_embedded_version_header(zenoh_pico_src, &version_include_dir);

    // ARM Cortex-A cross-compilation flags
    if target.contains("armv7a") {
        build.flag("-march=armv7-a");
    }

    // Collect zenoh-pico core sources (excluding platform-specific system backends)
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
        add_c_sources_recursive(&mut build, &src_dir.join(subdir));
    }
    // Common system sources (shared across all platforms)
    add_c_sources_recursive(&mut build, &src_dir.join("system").join("common"));

    // Unix platform sources (NuttX is POSIX-compatible)
    build.file(src_dir.join("system/unix/system.c"));
    build.file(src_dir.join("system/unix/network.c"));

    // Shim (high-level API wrapper)
    build.file(c_dir.join("zpico").join("zpico.c"));

    // Include paths (order matters — generated config takes precedence)
    let generated_config_dir = out_dir.join("zenoh-config");
    build.include(&generated_config_dir);
    build.include(zenoh_pico_src.join("include"));
    build.include(&version_include_dir);
    build.include(include_dir);

    // NuttX system headers (provides POSIX types: pthread, sockets, etc.)
    build.include(nuttx_dir.join("include"));

    // Platform defines
    // ZENOH_GENERIC: tells zenoh-pico config.h to use our generated config header
    // ZENOH_NUTTX: tells platform.h to use unix.h types, and system.c to use /dev/urandom
    build.define("ZENOH_GENERIC", None);
    build.define("ZENOH_NUTTX", None);
    build.define("ZENOH_DEBUG", "0");

    // NuttX has real POSIX threads
    build.define("Z_FEATURE_MULTI_THREAD", "1");

    // Link features (same as embedded — controlled by Cargo link-* features)
    build.define("Z_FEATURE_LINK_TCP", if link.tcp { "1" } else { "0" });
    build.define(
        "Z_FEATURE_LINK_UDP_UNICAST",
        if link.udp_unicast { "1" } else { "0" },
    );
    build.define(
        "Z_FEATURE_LINK_UDP_MULTICAST",
        if link.udp_multicast { "1" } else { "0" },
    );
    build.define("Z_FEATURE_LINK_SERIAL", if link.serial { "1" } else { "0" });
    build.define("Z_FEATURE_LINK_WS", "0");
    build.define("Z_FEATURE_LINK_BLUETOOTH", "0");
    build.define(
        "Z_FEATURE_RAWETH_TRANSPORT",
        if link.raweth { "1" } else { "0" },
    );
    build.define("Z_FEATURE_SCOUTING_UDP", "0");
    if env::var("CARGO_FEATURE_UNSTABLE_ZENOH_API").is_ok() {
        build.define("Z_FEATURE_UNSTABLE_API", "1");
    }

    // Pass shim slot counts as -D flags
    shim.apply_to_cc(&mut build);

    // Compiler flags
    build
        .opt_level(2)
        .flag("-ffunction-sections")
        .flag("-fdata-sections")
        .warnings(false);

    build.compile("zenohpico");
}

/// Build zenoh-pico for Eclipse ThreadX targets.
///
/// ThreadX provides threading, clock, and memory via tx_api.h.
/// Networking uses NetX Duo's BSD socket API (nxd_bsd.h).
///
/// Requires THREADX_DIR, THREADX_CONFIG_DIR, NETX_DIR, and NETX_CONFIG_DIR env vars.
/// The board crate compiles ThreadX kernel + NetX Duo library; we only use their headers.
fn build_zenoh_pico_threadx(
    zenoh_pico_src: &Path,
    c_dir: &Path,
    include_dir: &Path,
    out_dir: &Path,
    target: &str,
    link: &LinkFeatures,
    shim: &ShimConfig,
) {
    // Read ThreadX environment variables
    let threadx_dir = PathBuf::from(env::var("THREADX_DIR").unwrap_or_else(|_| {
        panic!(
            "THREADX_DIR not set. Point it at the ThreadX kernel source directory.\n\
             Run `just setup-threadx` to download, then set:\n\
             export THREADX_DIR=$PWD/external/threadx"
        );
    }));
    let threadx_config_dir = PathBuf::from(env::var("THREADX_CONFIG_DIR").unwrap_or_else(|_| {
        panic!(
            "THREADX_CONFIG_DIR not set. Point it at a directory containing tx_user.h.\n\
             Example: export THREADX_CONFIG_DIR=packages/boards/nros-threadx-linux/config"
        );
    }));
    let netx_dir = PathBuf::from(env::var("NETX_DIR").unwrap_or_else(|_| {
        panic!(
            "NETX_DIR not set. Point it at the NetX Duo source directory.\n\
             Run `just setup-threadx` to download, then set:\n\
             export NETX_DIR=$PWD/external/netxduo"
        );
    }));
    let netx_config_dir = PathBuf::from(env::var("NETX_CONFIG_DIR").unwrap_or_else(|_| {
        panic!(
            "NETX_CONFIG_DIR not set. Point it at a directory containing nx_user.h.\n\
             Example: export NETX_CONFIG_DIR=packages/boards/nros-threadx-linux/config"
        );
    }));

    // Validate directories exist
    if !threadx_dir.join("common").join("inc").exists() {
        panic!(
            "THREADX_DIR={}: missing common/inc/ directory. Is this a valid ThreadX source?",
            threadx_dir.display()
        );
    }
    if !netx_dir.join("common").join("inc").exists() {
        panic!(
            "NETX_DIR={}: missing common/inc/ directory. Is this a valid NetX Duo source?",
            netx_dir.display()
        );
    }

    let mut build = cc::Build::new();

    // Generate version header in OUT_DIR
    let version_include_dir = out_dir.join("zenoh-pico-version");
    generate_embedded_version_header(zenoh_pico_src, &version_include_dir);

    // RISC-V cross-compilation flags + picolibc sysroot
    if target.contains("riscv64") {
        build
            .flag("-march=rv64gc")
            .flag("-mabi=lp64d")
            .flag("-mcmodel=medany");

        // Generate errno.h shadow that avoids picolibc's TLS-based errno.
        // picolibc declares `extern __thread int errno` which uses the tp register.
        // On bare-metal RISC-V, tp may not be initialized → crash.
        let errno_dir = out_dir.join("errno-override");
        std::fs::create_dir_all(&errno_dir).unwrap();
        std::fs::write(
            errno_dir.join("errno.h"),
            include_bytes!("c/platform/errno_override.h"),
        )
        .unwrap();
        // errno override must be searched BEFORE picolibc headers
        build.include(&errno_dir);

        // picolibc's <machine/endian.h> defines htonl as __bswap32 on LE, which is
        // compatible with nx_port.h's #ifndef-guarded __builtin_bswap32 definitions.

        // Add picolibc sysroot for C standard library headers (stdint.h, etc.)
        // Do NOT use --specs=picolibc.specs (it enables TLS errno)
        if let Some(sysroot) = get_picolibc_sysroot() {
            build.include(sysroot.join("include"));
        }
    } else if target.contains("riscv32") {
        build.flag("-march=rv32gc").flag("-mabi=ilp32d");
    }
    // ARM Cortex-M cross-compilation flags
    if target.contains("thumbv7m") && !target.contains("thumbv7me") {
        build.flag("-mcpu=cortex-m3").flag("-mthumb");
    } else if target.contains("thumbv7em") {
        build
            .flag("-mcpu=cortex-m4")
            .flag("-mthumb")
            .flag("-mfpu=fpv4-sp-d16")
            .flag("-mfloat-abi=hard");
    }

    // Collect zenoh-pico core sources (excluding platform-specific system backends)
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
        add_c_sources_recursive(&mut build, &src_dir.join(subdir));
    }
    // Common system sources (shared across all platforms)
    add_c_sources_recursive(&mut build, &src_dir.join("system").join("common"));

    // ThreadX platform sources (our custom system + network layer)
    let platform_dir = c_dir.join("platform");
    build.file(platform_dir.join("threadx/system.c"));
    build.file(platform_dir.join("threadx/network.c"));

    // Shim (high-level API wrapper)
    build.file(c_dir.join("zpico").join("zpico.c"));

    // Include paths (order matters — generated config takes precedence)
    let generated_config_dir = out_dir.join("zenoh-config");
    build.include(&generated_config_dir);
    build.include(zenoh_pico_src.join("include"));
    build.include(&version_include_dir);
    build.include(&platform_dir);
    build.include(include_dir);

    // ThreadX kernel headers (tx_api.h, tx_thread.h, etc.)
    build.include(threadx_dir.join("common/inc"));

    // ThreadX user config (tx_user.h)
    build.include(&threadx_config_dir);

    // ThreadX port headers (tx_port.h — platform-specific types)
    // Detect port directory: Linux sim uses ports/linux/gnu/inc/, RISC-V uses ports/risc-v64/gnu/inc/
    if !is_embedded_target(target) {
        build.include(threadx_dir.join("ports/linux/gnu/inc"));
    } else if target.contains("riscv64") {
        build.include(threadx_dir.join("ports/risc-v64/gnu/inc"));
    }

    // NetX Duo headers (nx_api.h, nxd_bsd.h, etc.)
    build.include(netx_dir.join("common/inc"));
    build.include(netx_dir.join("addons/BSD"));

    // NetX Duo port headers (nx_port.h — platform-specific types)
    // Linux sim uses ports/linux/gnu/inc/, RISC-V uses the generic linux port too
    // (NetX Duo doesn't have a RISC-V port; the Linux port is architecture-agnostic)
    if !is_embedded_target(target) {
        build.include(netx_dir.join("ports/linux/gnu/inc"));
    } else if target.contains("riscv64") {
        // RISC-V QEMU uses the Linux port's nx_port.h (via board crate config)
        // The board crate supplies nx_port.h through its config dir
    }

    // NetX Duo user config (nx_user.h)
    build.include(&netx_config_dir);

    // Platform defines
    // ZENOH_GENERIC: tells zenoh-pico config.h to use our generated config header
    // ZENOH_THREADX: tells zenoh_generic_platform.h to use ThreadX types and system layer
    build.define("ZENOH_GENERIC", None);
    build.define("ZENOH_THREADX", None);
    build.define("ZENOH_DEBUG", "0");

    // NetX Duo's nxd_bsd.h remaps nx_bsd_* types to standard POSIX names
    // (suseconds_t, fd_set, in_addr_t, etc.) which conflict with system headers
    // (glibc on Linux sim, picolibc on bare-metal RISC-V).
    // NX_BSD_ENABLE_NATIVE_API keeps the nx_bsd_* prefix to avoid these conflicts.
    build.define("NX_BSD_ENABLE_NATIVE_API", None);

    // Include tx_user.h / nx_user.h from the board crate config directory
    build.define("TX_INCLUDE_USER_DEFINE_FILE", None);
    build.define("NX_INCLUDE_USER_DEFINE_FILE", None);

    // ThreadX has real threads — multi-thread support
    build.define("Z_FEATURE_MULTI_THREAD", "1");

    // Link features (controlled by Cargo link-* features)
    build.define("Z_FEATURE_LINK_TCP", if link.tcp { "1" } else { "0" });
    build.define(
        "Z_FEATURE_LINK_UDP_UNICAST",
        if link.udp_unicast { "1" } else { "0" },
    );
    build.define(
        "Z_FEATURE_LINK_UDP_MULTICAST",
        if link.udp_multicast { "1" } else { "0" },
    );
    build.define("Z_FEATURE_LINK_SERIAL", if link.serial { "1" } else { "0" });
    build.define("Z_FEATURE_LINK_WS", "0");
    build.define("Z_FEATURE_LINK_BLUETOOTH", "0");
    build.define(
        "Z_FEATURE_RAWETH_TRANSPORT",
        if link.raweth { "1" } else { "0" },
    );
    build.define("Z_FEATURE_SCOUTING_UDP", "0");
    if env::var("CARGO_FEATURE_UNSTABLE_ZENOH_API").is_ok() {
        build.define("Z_FEATURE_UNSTABLE_API", "1");
    }

    // Pass shim slot counts as -D flags
    shim.apply_to_cc(&mut build);

    // Compiler flags
    build
        .opt_level(2)
        .flag("-ffunction-sections")
        .flag("-fdata-sections")
        .warnings(false);

    build.compile("zenohpico");

    // Rerun triggers for ThreadX-specific files
    println!("cargo:rerun-if-changed=c/platform/threadx/system.c");
    println!("cargo:rerun-if-changed=c/platform/threadx/network.c");
    println!("cargo:rerun-if-changed=c/platform/threadx/platform.h");
    println!("cargo:rerun-if-env-changed=THREADX_DIR");
    println!("cargo:rerun-if-env-changed=THREADX_CONFIG_DIR");
    println!("cargo:rerun-if-env-changed=NETX_DIR");
    println!("cargo:rerun-if-env-changed=NETX_CONFIG_DIR");
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
