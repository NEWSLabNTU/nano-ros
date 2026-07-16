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

pub mod board_init;
pub mod threadx_config;
pub use board_init::{Board, BoardEntry, BoardExit, BoardInit, BoardPrint, DirectExec, run};
pub use threadx_config::ThreadxConfig;

#[cfg(feature = "build-helpers")]
pub mod manifest;
#[cfg(feature = "build-helpers")]
pub mod nuttx_ffi_build;
#[cfg(feature = "build-helpers")]
pub mod nuttx_image_link;
#[cfg(feature = "build-helpers")]
pub mod nuttx_platform_build;
/// RFC-0049 / phase-290 — per-package platform/board knob configuration.
#[cfg(feature = "build-helpers")]
pub mod platform_config;
#[cfg(feature = "build-helpers")]
pub mod policy;
#[cfg(feature = "build-helpers")]
pub mod threadx_qemu_riscv64_build;
#[cfg(feature = "build-helpers")]
pub mod threadx_sources;
