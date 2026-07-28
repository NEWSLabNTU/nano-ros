//! Pre-task hardware init for AGX Orin SPE.
//!
//! Phase 313 W-finalize (#0243) — the legacy `nros_board_common::board_init`
//! path is RETIRED for this board: the free `run(Config, closure)` (which
//! `xTaskCreate`d an app task and returned into the FSP's `app_init`), its
//! `AppContext` + `app_task_entry` trampoline, and the FSP `xTaskCreate` /
//! `nros_platform_alloc` / `zpico_set_task_config` externs it used are all
//! gone (they had no consumer — the FSP-boots-then-`app_init` firmware calls
//! `<OrinSpe as nros_platform::board::BoardEntry>::run` in `lib.rs`, which owns
//! its own body). Only the pre-task `init_hardware` no-op survives, consumed by
//! the new `nros_platform::BoardInit` impl.

use crate::Config;

/// Pre-task hardware init.
///
/// On the SPE, FSP-managed hardware (TCU, HSP, IVC carveout setup) is already
/// initialised by the time `app_init()` runs. This function is a no-op kept for
/// API parity with other board crates.
pub fn init_hardware(_config: &Config) {}
