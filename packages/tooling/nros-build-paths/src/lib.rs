//! Build-script helper: resolves repo-relative paths used by every
//! `build.rs` and board crate, without depending on `just`/`.envrc`.
//!
//! Phase 208.B Track A — every panic site of the form
//! `env::var("NROS_PLATFORM_<X>").expect("... direnv allow, or build via just")`
//! becomes a call to a resolver here. The env var stays valid as a
//! user-supplied override (out-of-tree consumers, custom layouts);
//! the in-tree case stops requiring it.
//!
//! The repo root is found by walking up from `CARGO_MANIFEST_DIR`
//! until `nros-sdk-index.toml` is seen (the Phase 195 sentinel). All
//! sub-paths mirror `just/sdk-env.just` — that file is the SSoT for
//! the relative-path values; if a path moves, fix it there AND here.

use std::path::PathBuf;

/// Walk up from `CARGO_MANIFEST_DIR` until `nros-sdk-index.toml` is
/// found. Panics if no such ancestor exists (out-of-tree consumer
/// without a vendored nano-ros checkout — they must set the relevant
/// env vars themselves).
pub fn repo_root() -> PathBuf {
    let start = std::env::var("CARGO_MANIFEST_DIR").expect(
        "nros-build-paths: CARGO_MANIFEST_DIR not set (must be called from a build script)",
    );
    let mut dir = PathBuf::from(&start);
    loop {
        if dir.join("nros-sdk-index.toml").is_file() {
            return dir;
        }
        if !dir.pop() {
            panic!(
                "nros-build-paths: could not locate nros-sdk-index.toml walking up from {start}. \
                 Out-of-tree consumer? Set the relevant NROS_PLATFORM_* env vars explicitly."
            );
        }
    }
}

/// Resolve an env-overridable path: if `env_name` is set, use it
/// verbatim; otherwise return `repo_root().join(rel)`. Also emits a
/// `cargo:rerun-if-env-changed=<env_name>` directive.
pub fn env_or_repo_path(env_name: &str, rel: &str) -> PathBuf {
    println!("cargo:rerun-if-env-changed={env_name}");
    match std::env::var(env_name) {
        Ok(v) if !v.is_empty() => PathBuf::from(v),
        _ => repo_root().join(rel),
    }
}

// Named resolvers for every var in `just/sdk-env.just`. Use these
// instead of hand-rolling `env::var("NROS_PLATFORM_*")` in every
// build script.

/// Canonical platform-header include dir. RFC-0042 D1 / phase-241 B.2 — the
/// canonical `<nros/platform.h>` (and its `platform_{net,timer,zephyr}.h`
/// siblings) moved from `nros-platform-cffi` to `nros-platform-api` (the lowest
/// crate). The name + the `NROS_PLATFORM_CFFI_INCLUDE` env var are kept for
/// caller/cmake compatibility; both now resolve to `nros-platform-api/include`.
pub fn nros_platform_cffi_include() -> PathBuf {
    env_or_repo_path(
        "NROS_PLATFORM_CFFI_INCLUDE",
        "packages/core/nros-platform-api/include",
    )
}

pub fn nros_platform_posix_src() -> PathBuf {
    env_or_repo_path(
        "NROS_PLATFORM_POSIX_SRC",
        "packages/core/nros-platform-posix/src",
    )
}

pub fn nros_platform_freertos_src() -> PathBuf {
    env_or_repo_path(
        "NROS_PLATFORM_FREERTOS_SRC",
        "packages/core/nros-platform-freertos/src",
    )
}

pub fn nros_platform_threadx_src() -> PathBuf {
    env_or_repo_path(
        "NROS_PLATFORM_THREADX_SRC",
        "packages/core/nros-platform-threadx/src",
    )
}

pub fn nros_lan9118_lwip_dir() -> PathBuf {
    env_or_repo_path("NROS_LAN9118_LWIP_DIR", "packages/drivers/lan9118-lwip")
}

pub fn nros_virtio_net_netx_dir() -> PathBuf {
    env_or_repo_path(
        "NROS_VIRTIO_NET_NETX_DIR",
        "packages/drivers/virtio-net-netx",
    )
}

pub fn nros_c_include() -> PathBuf {
    env_or_repo_path("NROS_C_INCLUDE", "packages/core/nros-c/include")
}

pub fn nros_cpp_include() -> PathBuf {
    env_or_repo_path("NROS_CPP_INCLUDE", "packages/core/nros-cpp/include")
}

pub fn freertos_dir() -> PathBuf {
    env_or_repo_path("FREERTOS_DIR", "third-party/freertos/kernel")
}

pub fn lwip_dir() -> PathBuf {
    env_or_repo_path("LWIP_DIR", "third-party/freertos/lwip")
}

pub fn freertos_config_dir() -> PathBuf {
    env_or_repo_path(
        "FREERTOS_CONFIG_DIR",
        "packages/boards/nros-board-mps2-an385-freertos/config",
    )
}

pub fn nuttx_dir() -> PathBuf {
    env_or_repo_path("NUTTX_DIR", "third-party/nuttx/nuttx")
}

pub fn nuttx_apps_dir() -> PathBuf {
    env_or_repo_path("NUTTX_APPS_DIR", "third-party/nuttx/nuttx-apps")
}

pub fn threadx_dir() -> PathBuf {
    env_or_repo_path("THREADX_DIR", "third-party/threadx/kernel")
}

pub fn netx_dir() -> PathBuf {
    env_or_repo_path("NETX_DIR", "third-party/threadx/netxduo")
}

pub fn tband_dir() -> PathBuf {
    env_or_repo_path("TBAND_DIR", "third-party/tracing/Tonbandgeraet/tband")
}
