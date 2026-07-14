//! Build script for zpico-sys
//!
//! This builds:
//! 1. zenoh-pico C library (via CMake for native, sources for embedded)
//! 2. The zpico C layer (zpico.c)
//! 3. Generates C header from Rust FFI declarations (cbindgen)

use std::{
    env,
    path::{Path, PathBuf},
};

// Phase 149.5 — shared manifest/policy parser moved to
// `nros-board-common`. Re-import as local module aliases so every
// `manifest::*` / `policy::*` reference below stays unchanged.
use nros_board_common::{manifest, policy};

use policy::{LinkFeatures, LinkPolicy};

use crate::{
    add_zenoh_pico_core_sources, apply_arch, arch_matches, detect_riscv_compiler,
    get_picolibc_sysroot, has_picolibc_specs, is_embedded_target, read_symbol_size,
};

type ShimConfig = crate::ShimConfig;
type ZenohBufferConfig = crate::ZenohBufferConfig;

fn shim_config_from_env() -> ShimConfig {
    ShimConfig {
        max_publishers: env_usize("ZPICO_MAX_PUBLISHERS", 8),
        max_subscribers: env_usize("ZPICO_MAX_SUBSCRIBERS", 8),
        max_queryables: env_usize("ZPICO_MAX_QUERYABLES", 8),
        max_liveliness: env_usize("ZPICO_MAX_LIVELINESS", 16),
        max_pending_gets: env_usize("ZPICO_MAX_PENDING_GETS", 4),
        get_reply_buf_size: env_usize("ZPICO_GET_REPLY_BUF_SIZE", 4096),
        get_poll_interval_ms: env_usize("ZPICO_GET_POLL_INTERVAL_MS", 10),
        tx_batch: env_usize("ZPICO_TX_BATCH", 0) != 0,
        tx_batch_flush_ms: env_usize("ZPICO_TX_BATCH_FLUSH_MS", 50),
    }
}

/// Read buffer config from environment variables with platform-appropriate defaults.
fn zenoh_buffer_config_from_env(posix: bool) -> ZenohBufferConfig {
    // Phase 204.7 — `NROS_LINK_IP=0` (a serial-only node) gates the IP link
    // C off; rerun the build script when it changes.
    println!("cargo:rerun-if-env-changed=NROS_LINK_IP");
    let link = LinkFeatures::from_env();
    let (default_frag, default_batch_uni, default_batch_multi) = if posix {
        (65535, 65535, 8192)
    } else if link.serial {
        (2048, 1500, 1024)
    } else {
        (2048, 1024, 1024)
    };

    ZenohBufferConfig {
        frag_max_size: env_usize("ZPICO_FRAG_MAX_SIZE", default_frag),
        batch_unicast_size: env_usize("ZPICO_BATCH_UNICAST_SIZE", default_batch_uni),
        batch_multicast_size: env_usize("ZPICO_BATCH_MULTICAST_SIZE", default_batch_multi),
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
    let target = std::env::var("TARGET").unwrap_or_default();
    let header = crate::config_header(
        link,
        buf,
        &target,
        env::var("CARGO_FEATURE_UNSTABLE_ZENOH_API").is_ok(),
        env::var("CARGO_FEATURE_ORIN_SPE").is_ok(),
        // rerun-if-env already emitted by shim_config_from_env's env_usize.
        env::var("ZPICO_TX_BATCH")
            .ok()
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(0)
            != 0,
        {
            println!("cargo:rerun-if-env-changed=ZPICO_TX_SPLIT_LOCK");
            env::var("ZPICO_TX_SPLIT_LOCK")
                .ok()
                .and_then(|v| v.parse::<usize>().ok())
                .unwrap_or(0)
                != 0
        },
    );
    std::fs::write(config_dir.join("zenoh_generic_config.h"), header).unwrap();
}

pub fn run() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let target = env::var("TARGET").unwrap_or_default();
    // 192.3: first-party include located via env (default in sdk-env.just),
    // not a build.rs walk-up — used in the platform-gated C-build paths below.
    println!("cargo:rerun-if-env-changed=NROS_PLATFORM_CFFI_INCLUDE");

    // Phase 136.1 — parse the canonical platform manifest. Resolve
    // every declared platform so a typo or broken `inherits` chain
    // surfaces as a hard build error, not a runtime surprise after
    // 136.3 plugs the data into cc-rs.
    //
    // Phase 136.7-E2E.3 — `ZPICO_PLATFORMS_TOML` env var redirects
    // the manifest to a caller-supplied path. Used by the drift-gate
    // test (`tests/zpico_drift_gate.rs`) to point at sandboxed
    // manifests; also a documented out-of-tree override hook for
    // downstream boards. Empty value falls through to the canonical
    // in-tree manifest.
    println!("cargo:rerun-if-env-changed=ZPICO_PLATFORMS_TOML");
    let platform_manifest_path = match env::var_os("ZPICO_PLATFORMS_TOML").filter(|v| !v.is_empty())
    {
        Some(path) => PathBuf::from(path),
        None => manifest_dir.join("zenoh_platforms.toml"),
    };
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
        for name in platform_manifest.platform.keys() {
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
        "linux" | "freebsd" | "netbsd" | "openbsd" | "android"
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
    let buf_config = zenoh_buffer_config_from_env(use_posix || use_nuttx || use_threadx);

    // Read shim slot counts from ZPICO_MAX_* env vars and generate Rust consts
    let shim_config = shim_config_from_env();
    std::fs::write(out_dir.join("shim_constants.rs"), shim_config.rust_consts()).unwrap();

    // Phase 134.3 — `zenoh_generic_config.h` is the single source of
    // truth for every `Z_FEATURE_LINK_*` flag. Apply the per-platform
    // `LinkPolicy` once here, generate the canonical header, and every
    // build path below reads it (cc-rs via `ZENOH_GENERIC` + include
    // path; CMake via the same — see 134.4 below).
    let link_policy = if use_orin_spe {
        LinkPolicy::orin_spe()
    } else if use_posix {
        LinkPolicy::posix()
    } else if use_freertos {
        // Phase 146.2 — FreeRTOS has no serial / raweth / TLS
        // backend; force them off so the upstream "Serial not
        // supported" `#error` doesn't fire and the alias TU
        // doesn't have to stub `_z_*_serial_internal`.
        LinkPolicy::freertos_lwip()
    } else if use_nuttx {
        // Phase 146.2 — NuttX has no serial / raweth / TLS
        // backend either. Same shape as freertos_lwip().
        LinkPolicy::nuttx()
    } else if use_threadx {
        // Phase 146.2 — ThreadX uses platform_aliases.c for
        // network ops (no serial wrapper there); force serial
        // off to skip building zenoh-pico's serial.c against
        // undefined `_z_*_serial_internal` symbols.
        LinkPolicy::threadx()
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
            if target.contains("linux") {
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
                &out_dir,
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

    // Phase 160 — probe vendor `_z_sys_net_*_t` sizes BEFORE the alias
    // TU compile so the resulting `net_type_sizes.txt` can be read at
    // alias-TU configure time. Moved earlier than the historical
    // location (post-Rust-cfg block below) so the
    // `_Static_assert(sizeof(nros_zp_alias_*_t) == VENDOR_SIZE)` drift
    // guards inside `platform_aliases.c` actually evaluate; without
    // this reorder the file doesn't exist when alias_build runs the
    // first time after a clean and `#if defined(...)` silently
    // suppresses the assert.
    if backend_count > 0 && !use_zephyr {
        probe_net_type_sizes(
            &c_dir,
            &zenoh_pico_src.join("include"),
            &out_dir,
            use_bare_metal || use_orin_spe,
            use_freertos,
            use_nuttx,
            use_threadx,
        );
    }

    // Phase 128.D.3 — opt-in alias TU that maps z_*/_z_get_time_*
    // symbols to the canonical nros_platform_* ABI. Compiled only
    // when the `platform-aliases` feature is selected; downstream
    // pairs it with disabling the matching symbols in
    // zpico-platform-shim or relies on `--allow-multiple-definition`
    // for one-cycle co-existence.
    // Phase 154 — FreeRTOS pulls vendor `system/freertos/system.c` +
    // `system/freertos/lwip/network.c` which already provide `z_malloc`
    // / `_z_task_*` / `_z_open_tcp` / etc. with the matching small-
    // socket-struct ABI. Compiling the alias TU on top would emit
    // duplicate symbols at link. Skip on FreeRTOS — vendor src is
    // the single source of truth there.
    //
    // Phase 214.G — gate alias TU emission on an *explicit* platform
    // feature. The TU emits ~30 `_z_*` forwarders that reference
    // canonical `nros_platform_*` symbols (`nros_platform_mutex_*`,
    // `nros_platform_condvar_*`, `nros_platform_time_*`, …). Those
    // symbols come from a paired provider crate that the consumer
    // pulls when it selects a platform — `nros-platform-cffi`'s
    // `posix-c-port` for POSIX hosts, RTOS-specific equivalents
    // elsewhere. Without an explicit platform feature, no provider
    // is guaranteed to be on the link line and the alias TU lands a
    // wall of `undefined symbol: nros_platform_*` errors at every
    // workspace test binary that pulls `zpico-sys` transitively
    // (the `cargo test --workspace` link failure that motivated this
    // gate). Auto-posix (the `target_os = "linux" | "macos" | …`
    // path above) is a build-script convenience for `cargo check` /
    // `cargo build` of `zpico-sys` itself — it does NOT imply the
    // downstream test target enabled `nros-platform/platform-posix`,
    // so it must NOT trigger alias-TU emission. Consumers that want
    // the alias TU keep enabling a platform feature on
    // `nros-rmw-zenoh` / `zpico-sys` (the existing contract); their
    // dep tree carries `nros-platform-cffi/posix-c-port` to satisfy
    // the forwarders. Standalone `cargo test -p zpico-sys` and
    // workspace `--workspace` builds without an explicit feature
    // now get a header-only `zpico-sys` rlib that links anywhere
    // (the resulting rlib must not be loaded at runtime — that's
    // already the same contract the no-backend-selected path emits
    // above).
    // phase-230 1c + Wave 2 (RFC-0034) — FreeRTOS scalar-only alias TU. The
    // full alias path below is gated `!use_freertos` because its net + task +
    // mutex/condvar sections collide with FreeRTOS's vendored
    // `system/freertos/*` primitives (the reason FreeRTOS was excluded). The
    // scalar-only mode emits the `z_malloc`/`z_realloc`/`z_free` (1c) +
    // `z_sleep_*` + `z_random_*` (Wave 2) forwarders → `nros_platform_*`; the
    // vendored copies are guarded out by
    // `Z_FEATURE_NROS_PLATFORM_{ALLOC,SLEEP,RANDOM}` (Step 6.5 on the
    // zenoh-pico build), so exactly these land on the link. Clock/time + the
    // opaque services stay vendored. Same `CARGO_FEATURE_PLATFORM_ALIASES`
    // opt-in keeps the guard + alias coupled.
    if env::var_os("CARGO_FEATURE_PLATFORM_ALIASES").is_some() && use_freertos && any_explicit {
        let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
        let nros_platform_cffi_include = nros_build_paths::nros_platform_cffi_include();
        let mut alias_build = cc::Build::new();
        alias_build
            .file(manifest_dir.join("c/zpico/platform_aliases.c"))
            .include(&nros_platform_cffi_include)
            .include(manifest_dir.join("c/zpico"))
            .define("NROS_ZP_ALIAS_SCALAR_ONLY", None)
            .warnings(true);
        let target_os_for_alias = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
        if target_os_for_alias == "none" {
            alias_build.flag("-ffreestanding");
        }
        // Match zpico.c's `[arch.*]` float-ABI / march / mabi so
        // platform_aliases.o lands in the same archive without an ABI clash.
        if let Some(name) = platform_name
            && let Ok(resolved) = platform_manifest.for_platform(name)
        {
            for arch_name in &resolved.arch {
                if let Some(arch) = platform_manifest.arch.get(arch_name.as_str())
                    && arch_matches(arch, &target)
                {
                    apply_arch(arch, &mut alias_build, &out_dir);
                    break;
                }
            }
        }
        alias_build.compile("zpico_platform_aliases");
        println!("cargo:rerun-if-changed=c/zpico/platform_aliases.c");
        println!("cargo:rerun-if-changed=c/zpico/nros_zenoh_generic_platform.h");
    }

    if env::var_os("CARGO_FEATURE_PLATFORM_ALIASES").is_some() && !use_freertos && any_explicit {
        let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
        let nros_platform_cffi_include = nros_build_paths::nros_platform_cffi_include();
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
        // Phase 160 (ESP32 talker fix, supersedes Phase 156 / 159 /
        // 160.C `USES_UNIX` skip-list) — alias TU's network section is
        // only safe to emit on bare-metal. Every other platform pulls a
        // vendor `system/<rtos>/network.c` into `extra_sources` that
        // provides `_z_open_tcp` / `_z_send_tcp` / etc. with the
        // per-platform `_z_sys_net_{socket,endpoint}_t` layout (4-byte
        // `int _fd` on unix.h, struct embedding `TX_THREAD *` on
        // threadx, etc.). The alias TU's generic 16/32-byte opaque
        // layouts have a DIFFERENT pass-by-value ABI on RV32 (hidden
        // pointer vs. inline registers), so even when symbol resolution
        // picks the vendor copy at link time, any cross-call into the
        // alias TU breaks the calling convention. Worst case (ESP32-C3
        // bare-metal): vendor TX (`link/unicast/tcp.c`) compiles
        // against bare-metal/platform.h's 6-byte endpoint and passes
        // it inline in a1/a2; alias TU's `_z_open_tcp` tail-calls
        // `nros_platform_tcp_open` treating a1 as a pointer →
        // `lhu a2, 2(a1)` faults with a1 = 10.0.2.2 (the IP value).
        // Same class of bug closed NuttX (`nros_support_init -> -4`)
        // in Phase 159. Emit the network section ONLY for bare-metal;
        // every other platform's vendor network.c is the single source
        // of truth and the alias TU stays out of the link.
        if use_threadx {
            // Phase 160 follow-up — threadx uses NROS_PLATFORM_ALIASES
            // (vendor sees the 16/32-byte opaque struct from
            // `nros_zenoh_generic_platform.h`); alias TU emits its
            // network section with the same opaque shape so the
            // by-value pass uses hidden-pointer ABI consistently on
            // both sides. POSIX/NuttX/Zephyr/FreeRTOS bring their own
            // vendor `system/<rtos>/network.c` and stay out of both
            // gates; bare-metal uses the small-struct gate below.
            alias_build.define("NROS_ZP_ALIAS_OPAQUE_NET", None);
        }
        if use_bare_metal {
            alias_build.define("NROS_ZP_ALIAS_BARE_METAL_NET", None);

            // Phase 160 — feed vendor-side `_z_sys_net_socket_t` /
            // `_z_sys_net_endpoint_t` sizes (extracted by `size_probe.c`
            // against the vendor `bare-metal/platform.h`) into the alias
            // TU as preprocessor constants so a `_Static_assert` inside
            // `platform_aliases.c` traps any silent ABI drift between
            // the alias TU's local typedefs and the vendor's. The
            // probe writes `net_type_sizes.txt` (line 985 above) on
            // every bare-metal build; missing file means the probe
            // fell into the warning fallback and we deliberately omit
            // the defines so the static assert is skipped (the
            // fallback already screams).
            let sizes_file = out_dir.join("net_type_sizes.txt");
            if let Ok(contents) = std::fs::read_to_string(&sizes_file) {
                let mut lines = contents.lines();
                let (socket_size, endpoint_size) = (
                    lines.next().and_then(|s| s.trim().parse::<usize>().ok()),
                    lines.next().and_then(|s| s.trim().parse::<usize>().ok()),
                );
                if let (Some(ss), Some(es)) = (socket_size, endpoint_size) {
                    alias_build
                        .define("NROS_ZP_VENDOR_NET_SOCKET_SIZE", ss.to_string().as_str())
                        .define("NROS_ZP_VENDOR_NET_ENDPOINT_SIZE", es.to_string().as_str());
                }
            }
        }
        // Phase 146.1 — ThreadX's `c/platform/threadx/task.c`
        // already provides every `_z_task_*` symbol because the
        // `_z_task_t` layout embeds a `TX_THREAD` struct. Skip the
        // generic alias-TU versions so both TUs can land in the
        // same archive without a duplicate-symbol link error.
        if use_threadx {
            alias_build.define("NROS_PLATFORM_ALIASES_SKIP_TASK", None);
        }
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
        // Apply the same `[arch.*]` cflags the manifest hands to
        // zpico.c, so platform_aliases.o uses the matching float-ABI
        // / march / mabi / mcmodel. Without this, riscv64gc targets
        // (ThreadX-RV64) hit `cannot link object files with different
        // floating-point ABI` — rustc emits lp64d while cc-rs picks
        // the bare-metal toolchain default (lp64).
        if let Some(name) = platform_name
            && let Ok(resolved) = platform_manifest.for_platform(name)
        {
            for arch_name in &resolved.arch {
                if let Some(arch) = platform_manifest.arch.get(arch_name.as_str())
                    && arch_matches(arch, &target)
                {
                    apply_arch(arch, &mut alias_build, &out_dir);
                    break;
                }
            }
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
    // c/platform/zenoh_generic_config.h was a stale pre-134.3 copy that could
    // shadow the OUT_DIR-generated header on the bare-metal shim include path;
    // deleted in the #135 fix (every TU now consumes the generated config).
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

    // Phase 160 — probe moved to before alias-TU compile (see
    // comment above `probe_net_type_sizes` call earlier in this
    // function).
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
    // Issue #135 — the ZENOH_GENERIC branches below used to find
    // <zenoh_generic_config.h> via the stale c/platform copy (deleted);
    // give every probe the OUT_DIR-generated config the library uses.
    build.include(out_dir.join("zenoh-config"));

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
                include_bytes!("../../zpico-sys/c/platform/errno_override.h"),
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
                include_bytes!("../../zpico-sys/c/platform/errno_override.h"),
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
    let rustc_sysroot = env::var("RUSTC_SYSROOT").ok();
    let host = env::var("HOST").unwrap_or_default();
    let socket_size = read_symbol_size(
        &archive,
        "__nros_sizeof_net_socket",
        rustc_sysroot.as_deref(),
        &host,
    );
    let endpoint_size = read_symbol_size(
        &archive,
        "__nros_sizeof_net_endpoint",
        rustc_sysroot.as_deref(),
        &host,
    );

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
            let processed = crate::post_process_header(&header_str);

            if !crate::is_plausible_generated_header(&processed) {
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
    if target.contains("linux") {
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
    out_dir: &Path,
    use_posix: bool,
    use_bare_metal: bool,
    target: &str,
    link: &LinkFeatures,
    shim: &ShimConfig,
) {
    let mut build = cc::Build::new();

    // Include paths
    //
    // Issue #135 — the generated zenoh config MUST come first and the shim
    // MUST compile with `ZENOH_GENERIC`, exactly like the zenoh-pico library
    // TUs built by `build_zenoh_pico_unified`. `z_get_options_t` (and other
    // public structs) change LAYOUT with `Z_FEATURE_LOCAL_QUERYABLE` /
    // `Z_FEATURE_LOCAL_SUBSCRIBER`; a shim TU falling back to the in-tree
    // `zenoh-pico/config.h` defaults while the library uses the generated
    // header is an ABI mismatch: the library's `z_get` read the shim's
    // `opts.target` (`Z_QUERY_TARGET_ALL` = 1) as `opts.allowed_destination`
    // (`Z_LOCALITY_SESSION_LOCAL` = 1), so every cross-process query was
    // silently downgraded to session-local and never reached the router.
    build.include(out_dir.join("zenoh-config"));
    build.define("ZENOH_GENERIC", None);
    build.include(include_dir);
    build.include(zenoh_pico_include);
    // Phase 154 — `zpico.c` now `#include <nros/platform_net.h>` from
    // `nros-platform-cffi`. The unified (embedded) builder picks the
    // path up via the manifest's `include_paths`; the legacy
    // `build_c_shim` path (POSIX + bare-metal) still needs it added
    // explicitly so `cargo check --workspace` on the host doesn't
    // fail with `nros/platform_net.h: No such file or directory`.
    build.include(nros_build_paths::nros_platform_cffi_include());

    // Core shim source
    build.file(c_dir.join("zpico/zpico.c"));

    // Platform-specific configuration
    if use_posix {
        #[cfg(target_os = "linux")]
        build.define("ZENOH_LINUX", None);
        // Mirror `zenoh_platforms.toml [platform.posix] defines_kv` — the
        // generated header's `#ifndef Z_FEATURE_MULTI_THREAD` fallback is 0,
        // so without this the shim would flip single-threaded while the
        // library runs multi-threaded (same ABI-divergence class as #135).
        build.define("Z_FEATURE_MULTI_THREAD", "1");
        build.define("ZENOH_DEBUG", "0");
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
        let has_network = (link.tcp || link.udp_unicast || link.udp_multicast) && !opt_out_smoltcp;
        if has_network {
            build.define("ZPICO_SMOLTCP", None);
        }
        if link.serial && !has_network {
            build.define("ZPICO_SERIAL", None);
        }
        // ZENOH_GENERIC is set unconditionally above (issue #135).
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
    // riscv-cc probe). Platform's `arch` is now a Vec (Phase 148);
    // walk entries in order and apply the first whose `target_match`
    // hits the build target. Multi-arch platforms (bare-metal across
    // cortex-m3 + riscv32imc) thus map to the right profile per
    // target instead of being hard-coded to one arch.
    for arch_name in &plat.arch {
        let arch = arch_table.get(arch_name.as_str()).unwrap_or_else(|| {
            panic!(
                "zenoh_platforms.toml: platform `{}` references unknown arch `{}`",
                plat.name, arch_name
            )
        });
        if arch_matches(arch, target) {
            apply_arch(arch, &mut build, out_dir);
            break;
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
    build.include(nros_build_paths::nros_platform_cffi_include());
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
    //
    // #189 — the Phase-132 serial-only opt-out, restored post-manifest-
    // migration. Phase 136.4 moved the bare-metal compile from
    // `build_c_shim` (which honored `ZPICO_NO_SMOLTCP`) into this manifest
    // path, whose `[platform.bare-metal] defines` hardcode `ZPICO_SMOLTCP`
    // — so every serial-only firmware compiled the smoltcp spin branch,
    // whose clock (`smoltcp_clock_now_ms`) is frozen without a smoltcp
    // iface: `zpico_spin_once(10)` could only return on router traffic
    // (~2.5 s keepalives) and the no_std executor credits just the
    // requested 10 ms per spin, so timers never came due (serial pubsub
    // published 0 forever). With the opt-out set and a serial-only link
    // set, swap in `ZPICO_SERIAL` — the branch built for exactly this.
    let opt_out_smoltcp = env::var("ZPICO_NO_SMOLTCP").is_ok();
    println!("cargo:rerun-if-env-changed=ZPICO_NO_SMOLTCP");
    let serial_only = link.serial && !(link.tcp || link.udp_unicast || link.udp_multicast);
    for define in &plat.defines {
        if define == "ZPICO_SMOLTCP" && opt_out_smoltcp && serial_only {
            build.define("ZPICO_SERIAL", None);
            continue;
        }
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

    // Step 6.5 — phase-230 1c (RFC-0034): scalar-alloc funnel guard.
    // When the consumer pulls the memory-only alias TU (the `platform-aliases`
    // feature) AND this is the FreeRTOS unified build, define
    // `Z_FEATURE_NROS_PLATFORM_ALLOC` so `system/freertos/system.c` drops its
    // vendored `z_malloc`/`z_realloc`/`z_free` (→ `pvPortMalloc`). The
    // alias TU then supplies them as `nros_platform_alloc`/`_realloc`/
    // `_dealloc` forwarders — one heap funnel, one stats counter. Coupled to
    // `CARGO_FEATURE_PLATFORM_ALIASES`: a serial-only node that drops
    // `platform-aliases` (via `default-features = false`) gets neither the
    // guard nor the alias, so the vendored heap stays intact (no undefined
    // `z_malloc`). The matching memory-only alias compile lives in the alias
    // gate below.
    if env::var_os("CARGO_FEATURE_PLATFORM_ALIASES").is_some()
        && env::var_os("CARGO_FEATURE_FREERTOS").is_some()
    {
        build.define("Z_FEATURE_NROS_PLATFORM_ALLOC", None);
        // phase-230 Wave 2 — extend the scalar funnel to sleep + random
        // (clock/time stay vendored: `z_clock_t` is FreeRTOS's `TickType_t`).
        // The scalar alias TU emits the matching forwarders (see the
        // `NROS_ZP_ALIAS_SCALAR_ONLY` compile below).
        build.define("Z_FEATURE_NROS_PLATFORM_SLEEP", None);
        build.define("Z_FEATURE_NROS_PLATFORM_RANDOM", None);
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
    // Phase 204.9 — `opt_level` is numeric (`2`) or a string (`"s"`/`"z"`
    // for size); the size forms map to cc-rs `opt_level_str`.
    match &plat.compile.opt_level {
        Some(manifest::OptLevel::Num(level)) => {
            build.opt_level(*level);
        }
        Some(manifest::OptLevel::Str(level)) => {
            build.opt_level_str(level);
        }
        None => {}
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

    for (name, pc) in crate::mbedtls_pc_files(lib_dir) {
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

    let template = include_str!("../../zpico-sys/c/zenoh-pico-version.h.in");
    let header = crate::embedded_version_header(&version, template);

    std::fs::write(include_dir.join("zenoh-pico.h"), header).unwrap();
}
