//! # nros-board-freertos
//!
//! **Generic FreeRTOS + lwIP scaffolding crate for nano-ros.**
//!
//! Layer 2 entry-point in the board / BSP abstraction described in
//! `docs/design/0012-board-bsp-integration-architecture.md`. Overlay
//! crates (`nros-board-<vendor>-<chip>-freertos`) depend on this
//! crate + patch vendor HAL deltas via `#[no_mangle]` hooks; see
//! `book/src/extending/add-a-board.md` for the cookbook.
//!
//! ## What it provides
//!
//! - The board startup C, as checked-in C files under `c/`.
//! - The board-Ethernet weak-hook contract
//!   (`nros_board_register_netif` + `nros_board_poll_netif`).
//! - `FREERTOS_CFLAGS` env-var arch parameterisation.
//! - The FreeRTOS kernel + lwIP + nros-platform-freertos compile, in
//!   this crate's `build.rs`. Emits
//!   `lib{freertos, lwip, nros_platform_freertos, freertos_glue}.a`.
//! - The `Config` struct + `Error` enum.
//! - [`run_entry`] over the [`nros_platform::Board`] trait set: the
//!   only entry point, and every per-board `impl BoardEntry` delegates
//!   to it.
//!
//! ## Public contract
//!
//! - [`Config`] — TOML-loaded network + zenoh config + FreeRTOS
//!   priority knobs. Overlay extends defaults.
//! - `Error` (pub(crate)) — internal init errors.
//! - [`run_entry`] —
//!   `(Config, FnOnce(&mut RuntimeCtx<'_>) -> Result<(), E>) -> Result<(), E>`
//!   entry point over the `nros-platform::board` traits. Per-board
//!   `impl BoardEntry` impls delegate here.
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
//! | `FREERTOS_PORT` | none (the board states it) | Upstream's port enum; a listed path still works, deprecated. |
//! | `LWIP_DIR` | none (required) | lwIP source root. |
//! | `FREERTOS_CONFIG_DIR` | none (required) | `FreeRTOSConfig.h` + `lwipopts.h` dir. |
//! | `FREERTOS_CFLAGS` | `-mcpu=cortex-m3 -mthumb` | Extra compiler flags. |

#![no_std]

mod config;
mod entry;
mod error;

// Phase 313 W-finalize (#0243) — the legacy generic `node::run<B>` (+ its
// ~~`nros_board_common::board_init`~~ trait re-exports) is RETIRED. Every FreeRTOS
// board moved to `run_bare` / the `nros_platform::board::BoardEntry` path
// (entry.rs), which is independent of `node.rs`; the file is gone.

pub use config::Config;
// phase-337 W5 — `Config.base` is a `BaseConfig`, so consumers that build one
// literally need the type. Re-export rather than making every leaf dep
// `nros-board-common` directly.
pub use entry::{run_bare, run_entry, run_sched_tasks_entry};
pub use nros_board_common::BaseConfig;

/// The type of one spawned sched task's executor backing slot, so a board crate
/// that wraps this one's `run_sched_tasks` can name its parameter without
/// depending on `nros` itself.
pub use ::nros::SchedTaskExecutorBackingSlot;

/// One spawned sched task's RTOS task memory row (stack + TCB), for the same
/// reason: a wrapping board crate names it without depending on `nros`.
pub use ::nros::SchedTaskMemoryRaw;

/// Internal re-export of the `Error` + `Result` types used by
/// per-board `node.rs` files until they lift into this crate. Overlays
/// import via `nros_board_freertos::__internal::*`. The path is
/// intentionally private-looking; once `node.rs` lifts into this crate,
/// the module goes away.
#[doc(hidden)]
pub mod __internal {
    pub use crate::error::{Error, Result};
}

// Phase 313 W-freertos (#0243) — the 152.1.A `reference-mps2` scaffolding
// re-export of the per-board free `run` is retired along with that free `run`.
// The `reference-mps2` feature still pulls the board crate as an optional dep.
