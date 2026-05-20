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

/// Per-board stdout contract for the generic kernel-family `run`
/// to emit banner / status / error messages without knowing
/// whether the board writes to QEMU semihosting, a UART, an
/// FSP debug TCU, or something else.
///
/// Implementing overlays typically wrap one of:
///
/// - `cortex_m_semihosting::hprintln!` (QEMU Cortex-M boards)
/// - Vendor printf bridge (orin-spe `tcu_print_msg`, NXP DCD)
/// - Serial UART writer
///
/// The signature takes `core::fmt::Arguments` so the generic
/// crate can pass `format_args!(...)` directly without forcing
/// any allocation or fixed-size buffer at the trait level —
/// each board picks its own staging strategy.
pub trait BoardPrint {
    /// Emit a line ending with `\n`.
    fn println(args: core::fmt::Arguments<'_>);
}

/// Per-board exit contract for the generic kernel-family `run`
/// to terminate after the user closure returns.
///
/// QEMU boards typically call `cortex_m_semihosting::debug::exit`.
/// Real-hardware boards may reset the chip, halt in a `wfi`
/// loop, or signal a watchdog. Both methods diverge (`-> !`)
/// because the generic `run` itself is `-> !` and never returns.
pub trait BoardExit {
    /// Terminate after a successful user closure.
    fn exit_success() -> !;

    /// Terminate after a user closure returned `Err` or an
    /// init step failed.
    fn exit_failure() -> !;
}

/// Phase 173.1 — the single board-entry super-trait.
///
/// Bundles the three split contracts so the generic [`run`] driver
/// takes one bound instead of three. Blanket-implemented for any
/// type carrying all three, so existing
/// `BoardInit + BoardPrint + BoardExit` impls satisfy `Board` for
/// free — no per-board boilerplate.
pub trait Board: BoardInit + BoardPrint + BoardExit {}
impl<T: BoardInit + BoardPrint + BoardExit> Board for T {}

/// Phase 173.1 — the one **direct-exec** board entry driver.
///
/// `init_hardware` → run the user closure → exit. This is the shape
/// the *direct-exec* board families use — bare-metal (MPS2-AN385,
/// STM32F4) and esp-hal (ESP32), where the closure runs on the boot
/// stack and control falls through to `exit_*` when it returns.
///
/// **Kernel-spawn families keep their own `run`.** FreeRTOS
/// (`nros-board-freertos`) and ThreadX (`nros-board-threadx`) must
/// allocate an app task, hand the closure to it, and start the
/// scheduler — the closure runs in *task* context and the
/// scheduler-start never returns, so the result is consumed inside
/// the task, not here. That kernel bring-up is the bounded
/// essential variation (Phase 173 factor 5); those crates still
/// converge on the `Board` trait + `B::Config`, just not on this
/// `run` body.
///
/// # Type parameters
/// - `B: Board` — the board ZST (provides `init_hardware` / `println`
///   / `exit_*` + the associated `Config`).
/// - `F: FnOnce(&B::Config) -> Result<(), E>` — the user closure.
/// - `E: Debug` — closure error type.
pub fn run<B, F, E>(cfg: B::Config, f: F) -> !
where
    B: Board,
    F: FnOnce(&B::Config) -> Result<(), E>,
    E: core::fmt::Debug,
{
    B::init_hardware(&cfg);
    match f(&cfg) {
        Ok(()) => {
            B::println(format_args!("nros: application complete"));
            B::exit_success()
        }
        Err(e) => {
            B::println(format_args!("nros: application error: {e:?}"));
            B::exit_failure()
        }
    }
}
