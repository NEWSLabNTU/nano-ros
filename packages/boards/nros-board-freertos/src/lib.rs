//! # nros-board-freertos
//!
//! **Generic FreeRTOS + lwIP scaffolding crate for nano-ros.**
//!
//! This crate is the Layer 2 entry-point in the board / BSP
//! abstraction described in
//! `docs/design/board-bsp-integration-architecture.md`. Overlay
//! crates (`nros-board-<vendor>-<chip>-freertos`) depend on it +
//! patch vendor HAL deltas via `#[no_mangle]` hooks; see
//! `book/src/porting/vendor-overlay.md` for the cookbook.
//!
//! ## 149.1.A scaffolding
//!
//! The crate exists today as a façade: opt-in `reference-mps2`
//! feature re-exports `Config` + `run` from the existing
//! `nros-board-mps2-an385-freertos` crate so future overlays have a
//! stable name to depend on while the kernel + lwIP build glue is
//! carved out of the per-board `build.rs` into this crate's own
//! `build.rs`. That carve-out is **149.1.B** (not yet landed).
//!
//! ## Public contract (post-149.1.B)
//!
//! Once 149.1.B lands:
//!
//! - `Config` — TOML-loaded network + zenoh config; overlays extend.
//! - `run(Config, FnOnce(&Config) -> Result<(), E>)` — entry point.
//!   Initialises kernel + network + calls the user closure inside
//!   the app thread.
//! - `#[no_mangle]` hooks the overlay implements:
//!   - `nros_board_init_clocks()` — clock tree + pin mux.
//!   - `nros_board_init_eth()` — Ethernet PHY + lwIP netif binding.
//!   - `nros_board_init_extra_drivers()` — sensors / displays / etc.
//!
//! ## SDK env-var contract
//!
//! The generic `build.rs` reads (after 149.1.B):
//!
//! | Var | Default | Purpose |
//! |---|---|---|
//! | `FREERTOS_DIR` | none (required) | FreeRTOS kernel source root. |
//! | `FREERTOS_PORT` | none (required) | Portable layer, e.g. `GCC/ARM_CM3`. |
//! | `LWIP_DIR` | none (required) | lwIP source root. |
//! | `FREERTOS_CONFIG_DIR` | overlay's `config/` | `FreeRTOSConfig.h` + `lwipopts.h` dir. |
//! | `FREERTOS_CFLAGS` | none | Extra compiler flags (overlay sets per-arch). |
//! | `BOARD_LINKER_SCRIPT_DIR` | none | Overlay's linker-script dir, added to link search path. |

#![no_std]

// Phase 149.1.A — re-export the reference board's surface when the
// feature is enabled so an overlay author can depend on
// `nros-board-freertos` today and switch wiring transparently when
// 149.1.B finishes the build-glue carve-out.
#[cfg(feature = "reference-mps2")]
pub use nros_board_mps2_an385_freertos::{Config, run};
