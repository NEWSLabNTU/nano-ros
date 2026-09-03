//! # nros-board-common
//!
//! **Shared helpers for nano-ros board crates.**
//!
//! Two distinct surfaces under one crate name:
//!
//! - [`BoardInit`] trait — kernel-agnostic per-board init contract
//!   (Phase 152.4.B). `no_std`, zero deps. Always available; safe
//!   to pull from a bare-metal firmware crate under
//!   `default-features = false`.
//! - `build-helpers` (default-on feature) — manifest parser +
//!   link-feature policy + ThreadX source helpers. Used from
//!   `build.rs` files; pulls `serde` + `toml` + `cc` transitively.
//!   Disable when only the trait is needed.
//!
//! ## Use
//!
//! Trait-only consumer (overlay's runtime `lib.rs`):
//!
//! ```toml
//! [dependencies]
//! nros-board-common = { path = "...", default-features = false }
//! ```
//!
//! Build-helper consumer (`build.rs`):
//!
//! ```toml
//! [build-dependencies]
//! nros-board-common = { path = "..." }  # default features include build-helpers
//! ```

#![cfg_attr(not(feature = "build-helpers"), no_std)]

// Phase 313 W6 (#0243) — the `board_init` module (the legacy `Board` / `BoardInit`
// / `BoardPrint` / `BoardExit` / `BoardEntry` / `DirectExec` traits + the generic
// direct-exec `run`) is DELETED. The canonical board entry API is now
// `nros_platform::board` (Rust, session/sizing/tiers-aware) + the `<nros/board.h>`
// C ABI (`nros-board-cffi`). `ThreadxConfig` (a config trait, never part of
// `board_init`) stays.
pub mod threadx_config;
pub use threadx_config::ThreadxConfig;

// phase-337 W1.b — the `{mac, ip, netmask, gateway, locator, domain_id}` core
// twelve board `Config` structs re-declare. ADDITIVE: no board adopts it yet;
// each board wave migrates its own `Config` as that wave's first step.
pub mod base_config;
pub use base_config::{BaseConfig, netmask_from_prefix, prefix_from_netmask};

// phase-337 W5.d — the FreeRTOS family's scheduling defaults, shared by the
// runtime `Config` and the `build.rs` `NROS_APP_CONFIG` emitter so the two
// cannot drift (they had, by 128 KiB of app stack). `no_std`, dep-free.
pub mod freertos_config;
pub use freertos_config::FreertosScheduling;

#[cfg(feature = "build-helpers")]
pub mod arch_flags;
/// issue 0491 — re-export, so a board build script that already build-deps this
/// crate reaches the ONE path-input helper (`env_or_repo_path` / `watch_path`)
/// without a new dependency edge in every leaf's `Cargo.lock`.
#[cfg(feature = "build-helpers")]
pub use nros_build_paths;
/// phase-337 W5.d — shared `build.rs` helpers for the FreeRTOS + lwIP family.
#[cfg(feature = "build-helpers")]
pub mod freertos_build;
/// The manifest parser, re-exported from `nros-platform-config`.
///
/// phase-400 W6 — it MOVED to a leaf crate so a driver build script can read
/// the ladder. Re-exported rather than relocated in the callers because the
/// spelling `nros_board_common::manifest::…` is correct either way and a
/// nine-file rename would bury the one change that matters.
#[cfg(feature = "build-helpers")]
pub use nros_platform_config::manifest;
/// phase-339 — the per-arch export snapshot a NuttX consumer links against.
#[cfg(feature = "build-helpers")]
pub mod nuttx_export;
#[cfg(feature = "build-helpers")]
pub mod nuttx_ffi_build;
#[cfg(feature = "build-helpers")]
pub mod nuttx_image_link;
#[cfg(feature = "build-helpers")]
pub mod nuttx_platform_build;
/// RFC-0049 / phase-290 — per-package platform/board knob configuration.
///
/// phase-400 W6 — re-exported from `nros-platform-config`; see `manifest` above.
#[cfg(feature = "build-helpers")]
pub use nros_platform_config::platform_config;
#[cfg(feature = "build-helpers")]
pub mod policy;
#[cfg(feature = "build-helpers")]
pub mod threadx_qemu_riscv64_build;
#[cfg(feature = "build-helpers")]
pub mod threadx_sources;

/// issue 0288 — host-tooling detection for board build scripts.
///
/// Gated with the other `build.rs` helpers: this crate is `no_std` for its
/// runtime surface (the `BoardInit` trait), and the guard needs `std::env` and
/// `println!`.
#[cfg(feature = "build-helpers")]
pub mod host_probe {
    /// issue 0288 — is this build HOST TOOLING rather than a firmware build?
    ///
    /// Board build scripts cross-compile C and assembly for their target. Run for a
    /// host triple they hand target flags to the host compiler and die before rustc
    /// starts:
    ///
    /// ```text
    /// cc: error: unrecognized command-line option '-mthumb'
    /// tx_trace_object_register.c:292: Error: no such instruction: `csrrci …'
    /// ```
    ///
    /// which makes every package depping such a board un-host-buildable. That is
    /// not academic: the source-metadata probe host-compiles a component to record
    /// its callback slots, and a standalone example deps its board crate directly,
    /// so those packages never got exact executor sizing.
    ///
    /// `needs` lists the `TARGET` prefixes this build is FOR (`["thumb", "arm"]`).
    /// Returns `true` — after emitting a warning — when the target matches none of
    /// them, and the caller should return early.
    ///
    /// Deliberately keyed on `TARGET` and not on the SDK env vars several of these
    /// scripts already test. Those say where the sources ARE, which is equally true
    /// during host tooling; only the target says what we are building for. The
    /// `FREERTOS_DIR`-style guards missed exactly this case, because an example's
    /// `[env]` block sets them for both kinds of build.
    ///
    /// Callers whose board legitimately targets a host triple (`threadx-linux`)
    /// must NOT use this: discriminate on their port instead, or they will skip a
    /// build that has to happen.
    pub fn skip_cross_build(crate_name: &str, needs: &[&str]) -> bool {
        let target = std::env::var("TARGET").unwrap_or_default();
        if needs.iter().any(|p| target.starts_with(p)) {
            return false;
        }
        println!(
            "cargo:warning={crate_name}: TARGET={target} is not one of {needs:?}; skipping the \
             cross-compile. The crate still compiles as a Rust shell so host tooling can build \
             packages that dep it (issue 0288)."
        );
        true
    }
}
