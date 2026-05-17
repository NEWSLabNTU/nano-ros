//! # nros-board-freertos
//!
//! **Generic FreeRTOS + lwIP scaffolding crate for nano-ros.**
//!
//! Layer 2 entry-point in the board / BSP abstraction described in
//! `docs/design/board-bsp-integration-architecture.md`. Overlay
//! crates (`nros-board-<vendor>-<chip>-freertos`) depend on this
//! crate + patch vendor HAL deltas via `#[no_mangle]` hooks; see
//! `book/src/porting/vendor-overlay.md` for the cookbook.
//!
//! ## Status
//!
//! - 152.1.A — scaffolding crate (façade).
//! - 152.1.B.1 — `STARTUP_C` const split into three checked-in C
//!   files.
//! - 152.1.B.2 — board-Ethernet weak-hook contract
//!   (`nros_board_register_netif` + `nros_board_poll_netif`).
//! - 152.1.B.3 — `FREERTOS_CFLAGS` env-var arch parameterisation.
//! - 152.1.B.4 — FreeRTOS kernel + lwIP + nros-platform-freertos
//!   compile lifted into this crate's `build.rs`. Emits
//!   `lib{freertos, lwip, nros_platform_freertos, freertos_glue}.a`.
//! - 152.1.B.5 — `Config` struct + `Error` enum lifted into this
//!   crate's `src/`. `node.rs` (~381 LOC of FreeRTOS-task plumbing
//!   that semihosts via `cortex_m_semihosting` and exits via QEMU
//!   semihosting) stays per-board until a `BoardPrint` /
//!   `BoardExit` trait abstraction lands (coupled with 152.4.B's
//!   `BoardInit` trait).
//!
//! ## Public contract
//!
//! - [`Config`] — TOML-loaded network + zenoh config + FreeRTOS
//!   priority knobs. Overlay extends defaults.
//! - `Error` (pub(crate)) — internal init errors.
//! - `run(Config, FnOnce(&Config) -> Result<(), E>)` — overlay
//!   re-exports from its per-board `node.rs` until the trait
//!   abstraction lands.
//! - `#[no_mangle]` hooks the overlay implements
//!   (see `c/network_glue.c`):
//!   - `nros_board_register_netif(mac, ip, netmask, gw) -> int`
//!   - `nros_board_poll_netif() -> void`
//!
//! ## SDK env-var contract
//!
//! `build.rs` reads:
//!
//! | Var | Default | Purpose |
//! |---|---|---|
//! | `FREERTOS_DIR` | none (required) | FreeRTOS kernel source root. |
//! | `FREERTOS_PORT` | `GCC/ARM_CM3` | Portable layer. |
//! | `LWIP_DIR` | none (required) | lwIP source root. |
//! | `FREERTOS_CONFIG_DIR` | none (required) | `FreeRTOSConfig.h` + `lwipopts.h` dir. |
//! | `FREERTOS_CFLAGS` | `-mcpu=cortex-m3 -mthumb` | Extra compiler flags. |

#![no_std]

mod config;
mod error;
mod node;

pub use config::Config;
pub use node::run;
pub use nros_board_common::{BoardExit, BoardInit, BoardPrint};

/// Internal re-export of the `Error` + `Result` types used by
/// per-board `node.rs` files during the 152.1.B.5 → final-lift
/// transition. Overlays import via `nros_board_freertos::__internal::*`.
/// The path is intentionally private-looking; once `node.rs` lifts
/// into this crate (coupled with 152.4.B's `BoardInit` trait), the
/// module goes away.
#[doc(hidden)]
pub mod __internal {
    pub use crate::error::{Error, Result};
}

// 152.1.A scaffolding re-export — kept for downstream consumers
// that switched to `nros-board-freertos = { features = ["reference-mps2"] }`
// during the .A → .B transition. The `Config` re-export now wins
// (both crates export the same type via this crate's `pub use
// config::Config`).
#[cfg(feature = "reference-mps2")]
pub use nros_board_mps2_an385_freertos::run;
