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
    try_repo_root().unwrap_or_else(|| {
        panic!(
            "nros-build-paths: could not locate nros-sdk-index.toml walking up from {start}. \
             Out-of-tree consumer? Set the relevant NROS_PLATFORM_* env vars explicitly."
        )
    })
}

/// [`repo_root`] for callers that have a legitimate out-of-tree fallback.
///
/// phase-343 I1 — the sizes probe needs the repo root to place its SHARED
/// cache, but must still work for an out-of-tree consumer that has no nano-ros
/// checkout to find. Panicking there would be wrong, and re-implementing the
/// walk in the caller would be a second spelling of "where is the repo" — the
/// R3 drift this repo keeps paying for (a private `project_root()` in
/// `qemu.rs` was deleted for exactly this reason).
pub fn try_repo_root() -> Option<PathBuf> {
    let start = std::env::var("CARGO_MANIFEST_DIR").ok()?;
    let mut dir = PathBuf::from(start);
    loop {
        if dir.join("nros-sdk-index.toml").is_file() {
            return Some(dir);
        }
        if !dir.pop() {
            return None;
        }
    }
}

/// Resolve an env-overridable path: if `env_name` is set, use it,
/// otherwise return `repo_root().join(rel)`. The returned path is
/// CANONICAL (see [`canonical`]).
///
/// Emits NO rerun directive. `rerun-if-env-changed` on a path variable is
/// forbidden (issue 0491 — read [`canonical`] for why); what the build script
/// depends on is the CONTENT it reads, so the caller declares that with
/// [`watch_path`] (a whole first-party dir) or a per-file
/// `cargo:rerun-if-changed`. Watching is the caller's call because the paths
/// behind these variables differ in kind: `packages/platform/…/src` is a small
/// first-party tree that should be watched wholesale, while `NUTTX_DIR` names
/// a vendored SDK that its own build writes INTO — watching that would leave
/// every dependent build script permanently dirty.
pub fn env_or_repo_path(env_name: &str, rel: &str) -> PathBuf {
    let raw = match std::env::var(env_name) {
        Ok(v) if !v.is_empty() => PathBuf::from(v),
        _ => repo_root().join(rel),
    };
    canonical(&raw)
}

/// Canonicalise a path-valued build input. Emits no directive.
///
/// **Path-valued build inputs are fingerprinted by their CONTENT, never by
/// their env spelling — `cargo:rerun-if-env-changed` on one is a bug**
/// (issue 0491). Gate: `scripts/check-path-env-fingerprints.py`.
///
/// Cargo compares an env var's value as TEXT. One directory has many
/// spellings, and this repo produces at least three for the same first-party
/// source dir:
///
/// * `just` exports it absolute (`just/sdk-env.just`, rooted at
///   `justfile_directory()`);
/// * a leaf `.cargo/config.toml` writes `{ value = "../../../../packages/…",
///   relative = true }`, which cargo resolves against THAT LEAF —
///   `…/rust/talker/../../../../packages/…` vs `…/rust/listener/../…`;
/// * a bare `cargo build` with neither leaves it unset.
///
/// While every leaf had its own `target/` those spellings never met. Sharing
/// one `--target-dir` per identity group (phase-340) put them in one
/// fingerprint namespace, and each sibling then re-ran the board / zpico build
/// scripts and cascaded `UnitDependencyInfoChanged` up to the leaf bin — six
/// FreeRTOS rows that could never all be fresh. Canonicalising cannot fix it
/// from this side: the string cargo compares is the one the CONFIG produced,
/// not the one the build script resolved.
///
/// Watching the directory says what the build script actually depends on — its
/// CONTENTS — and says it identically from every leaf.
///
/// The cost, stated plainly: cargo no longer notices that the variable now
/// names a DIFFERENT directory. Nothing re-runs the script, so it keeps
/// watching the old path (contents of the old dir still trigger correctly).
/// In-tree that cannot happen — the paths are fixed by the checkout. An
/// out-of-tree consumer who repoints one of these vars at another tree must
/// `cargo clean` (or touch a source) for that build dir, the same as changing
/// any other build input cargo cannot see.
pub fn canonical(path: &std::path::Path) -> PathBuf {
    // A path that does not exist yet (an SDK not provisioned, an optional
    // overlay dir) keeps its spelling — the caller's own diagnostic is the one
    // that should fire, not a canonicalisation error.
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

/// [`canonical`] plus `cargo:rerun-if-changed=<canonical path>` — the rerun
/// trigger a path-valued build input is allowed to have.
///
/// Use it for a first-party tree the build script READS
/// (`packages/platform/…/src`, an `include/` dir, a board `config/`). Do NOT
/// point it at a vendored SDK that its own build writes into, or at a build
/// output dir: cargo takes the newest mtime under the path, so watching such a
/// tree leaves every dependent script dirty after each unrelated build of it.
pub fn watch_path(path: &std::path::Path) -> PathBuf {
    let canonical = canonical(path);
    // A trigger on a path that does NOT exist makes the unit permanently dirty
    // (issue 0490 + `scripts/check-build-rs-rerun-paths.py`), which is the same
    // never-fresh outcome this function exists to remove. An absent path is
    // therefore skipped, not declared.
    if canonical.exists() {
        println!("cargo:rerun-if-changed={}", canonical.display());
    }
    canonical
}

/// An env var that names a path, [`canonical`]ised — for the vars with no
/// in-repo default (`THREADX_DIR`, a board's `*_CONFIG_DIR`, …). Emits no
/// directive; `None` when unset or empty, so the caller keeps its own
/// diagnostic.
pub fn env_path(env_name: &str) -> Option<PathBuf> {
    match std::env::var(env_name) {
        Ok(v) if !v.is_empty() => Some(canonical(std::path::Path::new(&v))),
        _ => None,
    }
}

/// [`env_path`] plus the content watch — use it when the variable names a
/// FIRST-PARTY tree (see [`watch_path`] for which paths must not be watched).
pub fn env_path_watched(env_name: &str) -> Option<PathBuf> {
    let p = env_path(env_name)?;
    println!("cargo:rerun-if-changed={}", p.display());
    Some(p)
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
        "packages/platform/nros-platform-api/include",
    )
}

pub fn nros_platform_posix_src() -> PathBuf {
    env_or_repo_path(
        "NROS_PLATFORM_POSIX_SRC",
        "packages/platform/nros-platform-posix/src",
    )
}

pub fn nros_platform_freertos_src() -> PathBuf {
    env_or_repo_path(
        "NROS_PLATFORM_FREERTOS_SRC",
        "packages/platform/nros-platform-freertos/src",
    )
}

pub fn nros_platform_threadx_src() -> PathBuf {
    env_or_repo_path(
        "NROS_PLATFORM_THREADX_SRC",
        "packages/platform/nros-platform-threadx/src",
    )
}

pub fn nros_lan9118_lwip_dir() -> PathBuf {
    env_or_repo_path("NROS_LAN9118_LWIP_DIR", "packages/drivers/net/lan9118-lwip")
}

pub fn nros_virtio_net_netx_dir() -> PathBuf {
    env_or_repo_path(
        "NROS_VIRTIO_NET_NETX_DIR",
        "packages/drivers/net/virtio-net-netx",
    )
}

pub fn nros_c_include() -> PathBuf {
    env_or_repo_path("NROS_C_INCLUDE", "packages/api/nros-c/include")
}

pub fn nros_cpp_include() -> PathBuf {
    env_or_repo_path("NROS_CPP_INCLUDE", "packages/api/nros-cpp/include")
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

/// The NuttX include root whose `nuttx/config.h` describes THIS build's arch.
///
/// Issue 0525 — NuttX is built IN PLACE, so `$NUTTX_DIR/include/nuttx/config.h`
/// belongs to whichever arch the shared checkout was configured for LAST, and
/// one checkout serves both in-tree arches. Anything deriving a compile input
/// from it silently takes the other arch's values when two arches share a tree,
/// which `lane=tier2` does (it builds nuttx-riscv after nuttx).
///
/// That is issue 0511: the ARM image was linked with the RISC-V memory map,
/// whose `CONFIG_FLASH_SIZE` is 0, so ROM had LENGTH 0 and every byte placed in
/// it "overflowed" — read as a 400-500 KB size regression that survived clean
/// rebuilds, because the stale `.config` lives in the submodule rather than in
/// any target dir.
///
/// Lives HERE rather than in `nros-board-common` so the board crates and the
/// `nuttx-sys` bindgen build script share ONE spelling: `nuttx-sys` cannot
/// depend on the board helpers, and a second copy of this resolution is exactly
/// the drift that produced 0511 in the first place.
///
/// Prefers `nros-nuttx-export-<arch>/include` when it carries a
/// `nuttx/config.h`, else the live tree's `include/` so a pre-phase-339 checkout
/// keeps working. Emits `rerun-if-changed` on BOTH spellings whether or not they
/// exist (issue 0477's rule): the config IS the memory map and the ABI, so a
/// reconfigure must invalidate whatever was derived from it — including on the
/// branch that lost.
pub fn nuttx_include_root(nuttx_dir: &std::path::Path) -> PathBuf {
    let shared = nuttx_dir.join("include");
    println!(
        "cargo:rerun-if-changed={}",
        shared.join("nuttx/config.h").display()
    );
    // The snapshot is named for the ARCH being compiled for, which is the only
    // thing that distinguishes the two trees.
    let arch = std::env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default();
    let snapshot_arch = match arch.as_str() {
        "arm" => Some("arm"),
        "riscv32" | "riscv64" => Some("riscv"),
        _ => None,
    };
    if let Some(a) = snapshot_arch {
        let inc = nuttx_dir
            .join(format!("nros-nuttx-export-{a}"))
            .join("include");
        println!(
            "cargo:rerun-if-changed={}",
            inc.join("nuttx/config.h").display()
        );
        if inc.join("nuttx/config.h").is_file() {
            return inc;
        }
    }
    shared
}
