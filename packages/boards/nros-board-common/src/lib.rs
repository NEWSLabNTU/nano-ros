//! # nros-board-common
//!
//! **Shared build-script helpers for nano-ros board crates.**
//!
//! Houses the manifest parser + link-feature policy lifted out of
//! `packages/zpico/zpico-sys/build/` in Phase 149.5 so the
//! per-kernel generic crates (`nros-board-{freertos, threadx,
//! nuttx}`) and `zpico-sys` can share one canonical implementation.
//!
//! ## Use
//!
//! In your `build.rs`:
//!
//! ```ignore
//! use nros_board_common::{manifest, policy};
//!
//! fn main() {
//!     let m = manifest::PlatformManifest::load("platforms.toml".as_ref())
//!         .expect("parse platforms.toml");
//!     let resolved = m.for_platform("posix").unwrap();
//!     let link = policy::LinkFeatures::from_env()
//!         .apply(&policy::LinkPolicy::posix());
//!     // ... drive cc::Build off `resolved` + `link` ...
//! }
//! ```
//!
//! ## Modules
//!
//! - [`manifest`] — TOML schema + parser + interpolator + matcher
//!   for the per-platform build-data files (Phase 136's
//!   `zenoh_platforms.toml`; future per-kernel
//!   `<kernel>_platforms.toml`).
//! - [`policy`] — `LinkFeatures` env reader + `LinkPolicy` mask
//!   (Phase 134's per-platform link-feature gating).
//!
//! This crate is **build-host only** — never reaches a final
//! binary. Consumers declare it under `[build-dependencies]`.

pub mod board_init;
pub mod manifest;
pub mod policy;
pub mod threadx_sources;

pub use board_init::BoardInit;
