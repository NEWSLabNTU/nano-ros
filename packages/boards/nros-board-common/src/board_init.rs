//! Phase 152.4.B — `BoardInit` trait.
//!
//! Kernel-agnostic contract every per-board overlay implements so
//! a generic kernel-family crate (`nros-board-freertos`,
//! `nros-board-threadx`, `nros-board-nuttx`) can drive boot +
//! hardware init through a single `run<B: BoardInit>` entry
//! without knowing the board's specific `Config` shape.
//!
//! ## Use
//!
//! Overlay-side:
//!
//! ```ignore
//! use nros_board_common::BoardInit;
//!
//! pub struct QemuArmVirt;
//!
//! impl BoardInit for QemuArmVirt {
//!     type Config = MyConfig;
//!
//!     fn init_hardware(cfg: &Self::Config) {
//!         // vendor-specific clock tree, pin mux, driver wakes...
//!     }
//! }
//! ```
//!
//! Generic-kernel-crate side:
//!
//! ```ignore
//! pub fn run<B, F, E>(cfg: B::Config, f: F) -> !
//! where
//!     B: BoardInit,
//!     F: FnOnce(&B::Config) -> Result<(), E>,
//!     E: core::fmt::Debug,
//! {
//!     B::init_hardware(&cfg);
//!     // kernel-specific bring-up (scheduler start, etc.)
//!     match f(&cfg) { ... }
//! }
//! ```
//!
//! ## Why kernel-agnostic
//!
//! All four supported kernels (FreeRTOS, ThreadX, NuttX, bare-metal)
//! share the same `Config + init_hardware` shape at the
//! overlay/generic boundary even though their `run` internals
//! differ wildly (FreeRTOS spawns app task + scheduler-start vs.
//! NuttX returns into a normal `std::process::exit`). One trait
//! captures the overlay-side contract; the generic-crate-side
//! `run` is kernel-specific.

/// Per-board init contract. One impl per overlay (`pub struct
/// MyBoard; impl BoardInit for MyBoard`).
pub trait BoardInit {
    /// Board-specific config struct. TOML-loaded by the user app
    /// (overlay typically provides `Config::from_toml`).
    type Config;

    /// Hardware init the generic kernel-family `run` invokes
    /// before handing control to the user closure. Vendor HAL
    /// calls (clock tree, pin mux, peripheral wakes) go here.
    fn init_hardware(cfg: &Self::Config);
}
