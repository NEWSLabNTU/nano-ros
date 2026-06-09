//! # nros-board-mps2-an385-freertos
//!
//! Board crate for running nros on the MPS2-AN385 (Cortex-M3 + LAN9118)
//! with FreeRTOS + lwIP.
//!
//! Handles hardware init, FreeRTOS kernel startup, lwIP networking, and
//! LAN9118 Ethernet.  Users call [`run()`] with a closure that receives
//! `&Config` and creates an `Executor` for full API access.
//!
//! # Differences from bare-metal (`nros-board-mps2-an385`)
//!
//! - Networking via lwIP sockets (not smoltcp)
//! - FreeRTOS tasks, mutexes, semaphores (real RTOS primitives)
//! - zenoh-pico runs its own background read task
//! - No `zpico-platform-*` or `zpico-smoltcp` crates needed

#![no_std]
// Phase 121.3 — the canonical `nros_platform_*` symbols come from the
// FreeRTOS C port (`packages/core/nros-platform-freertos/src/*.c`)
// compiled in-tree by build.rs. The Rust kernel crate that previously
// emitted the same symbols via the `nros_platform_export!` macro was
// deleted in Phase 121.3.deprecate-rust-remove.
//
// Keep `nros-platform` linked even for DDS-only examples that do not
// reference it directly; its `global-allocator` feature installs the
// Rust allocator adapter over the FreeRTOS C heap.
extern crate nros_platform as _;

// Force-link the zenoh-pico C transport + platform shim when
// `rmw-zenoh` is active. DDS-only builds drop both deps via
// `default-features = false` and reach the linker without the
// zenoh-pico symbol set.
#[cfg(feature = "rmw-zenoh")]
extern crate zpico_sys;

// Phase 152.1.B.5 — `Config` + the FreeRTOS-task plumbing live in
// the generic `nros-board-freertos` crate. The overlay only
// implements the three `BoardInit` / `BoardPrint` / `BoardExit`
// traits + provides a thin non-generic `run()` wrapper.
pub use nros_board_freertos::Config;
use nros_board_freertos::{BoardExit, BoardInit, BoardPrint};

/// Per-board marker for trait dispatch into
/// `nros_board_freertos::run::<Mps2An385, _, _>`.
pub struct Mps2An385;

impl BoardInit for Mps2An385 {
    type Config = Config;

    fn init_hardware(_cfg: &Config) {
        // FreeRTOS network init requires the scheduler to be
        // running (tcpip_init creates tcpip_thread). All
        // meaningful init happens inside the app task created
        // by the generic `run`.
    }
}

impl BoardPrint for Mps2An385 {
    fn println(args: core::fmt::Arguments<'_>) {
        // `hprintln!` only takes a format string + args, not a
        // pre-built `Arguments`. Use `hio::hstdout` + `writeln!`
        // so we can forward `Arguments` straight through.
        use core::fmt::Write;
        if let Ok(mut stdout) = cortex_m_semihosting::hio::hstdout() {
            let _ = writeln!(stdout, "{}", args);
        }
    }
}

impl BoardExit for Mps2An385 {
    fn exit_success() -> ! {
        exit_success()
    }

    fn exit_failure() -> ! {
        exit_failure()
    }
}

/// Initialise pre-scheduler hardware. Delegates through the
/// trait so the overlay's `init_hardware` is reachable both
/// from `run()` (via the generic `nros_board_freertos::run<B>`)
/// and standalone (e.g. board-side bring-up tests).
pub fn init_hardware(cfg: &Config) {
    <Mps2An385 as BoardInit>::init_hardware(cfg);
}

/// Run an application on QEMU MPS2-AN385 with FreeRTOS + lwIP.
///
/// Thin wrapper over `nros_board_freertos::run::<Mps2An385, _, _>`
/// so users do not have to spell the trait turbofish themselves.
///
/// # Example
///
/// ```ignore
/// use nros_board_mps2_an385_freertos::{Config, run};
/// use nros::prelude::*;
///
/// run(Config::default(), |config| {
///     let exec_config = ExecutorConfig::new(config.zenoh_locator)
///         .domain_id(config.domain_id);
///     let mut executor = Executor::open(&exec_config)?;
///     // ...
///     Ok::<(), NodeError>(())
/// })
/// ```
pub fn run<F, E: core::fmt::Debug>(config: Config, f: F) -> !
where
    F: FnOnce(&Config) -> core::result::Result<(), E>,
{
    register_log_writer();
    nros_board_freertos::run::<Mps2An385, F, E>(config, f)
}

/// Phase 88 — register a semihosting writer with `nros-platform-freertos`'s
/// log fn-ptr slot. Called once from `run()` before any task spawns.
/// Idempotent: re-calling overrides the previous registration with the
/// same writer.
fn register_log_writer() {
    use core::fmt::Write as _;
    unsafe extern "C" fn writer(
        severity: u8,
        name_ptr: *const u8,
        name_len: usize,
        msg_ptr: *const u8,
        msg_len: usize,
    ) {
        let Ok(mut out) = cortex_m_semihosting::hio::hstderr() else {
            return;
        };
        let label = match severity {
            0 => "TRACE",
            1 => "DEBUG",
            2 => "INFO",
            3 => "WARN",
            4 => "ERROR",
            5 => "FATAL",
            _ => "?",
        };
        // SAFETY: caller passes valid `&[u8]` slices that outlive
        // the call; empty-name case collapses to an empty slice.
        let name: &[u8] = if name_ptr.is_null() || name_len == 0 {
            &[]
        } else {
            unsafe { core::slice::from_raw_parts(name_ptr, name_len) }
        };
        let msg: &[u8] = if msg_ptr.is_null() || msg_len == 0 {
            &[]
        } else {
            unsafe { core::slice::from_raw_parts(msg_ptr, msg_len) }
        };
        let name_str = core::str::from_utf8(name).unwrap_or("");
        let msg_str = core::str::from_utf8(msg).unwrap_or("");
        if !name_str.is_empty() {
            let _ = writeln!(out, "[{}] {}: {}", label, name_str, msg_str);
        } else {
            let _ = writeln!(out, "[{}] {}", label, msg_str);
        }
    }
    // SAFETY: extern decl matches `<nros/platform.h>`. The writer
    // satisfies the documented contract (slice validity, no panics
    // inside the writer beyond the fmt::Result we ignore).
    unsafe {
        nros_platform_cffi::nros_platform_register_log_writer(Some(writer), None);
    }
}

/// Re-export semihosting for the `println!` macro.
pub use cortex_m_semihosting;

/// Print to QEMU semihosting console.
#[macro_export]
macro_rules! println {
    () => {
        $crate::cortex_m_semihosting::hprintln!()
    };
    ($($arg:tt)*) => {
        $crate::cortex_m_semihosting::hprintln!($($arg)*)
    };
}

/// Exit QEMU with success status.
pub fn exit_success() -> ! {
    cortex_m_semihosting::debug::exit(cortex_m_semihosting::debug::EXIT_SUCCESS);
    #[allow(clippy::empty_loop)]
    loop {}
}

/// Exit QEMU with failure status.
pub fn exit_failure() -> ! {
    cortex_m_semihosting::debug::exit(cortex_m_semihosting::debug::EXIT_FAILURE);
    #[allow(clippy::empty_loop)]
    loop {}
}

// ---------------------------------------------------------------------------
// Phase 212.N.3 — additive impls of the new `nros_platform::board` traits.
//
// The legacy `nros_board_common::{BoardInit, BoardPrint, BoardExit}` impls
// above stay in place during the 212.N transition. The new trait set lives
// in `nros_platform::board::*` (parameterless `init_hardware`, identical
// `BoardPrint::println` / `BoardExit::exit_*` shape) and is required by the
// 212.N.2 family driver `nros_board_freertos::run_entry`.
//
// Bodies mirror the legacy impls so both entry surfaces (`run()` and
// `<Mps2An385 as nros_platform::BoardEntry>::run`) share the same hardware
// behaviour.
// ---------------------------------------------------------------------------

impl nros_platform::BoardInit for Mps2An385 {
    fn init_hardware() {
        // FreeRTOS network init requires the scheduler to be running
        // (`tcpip_init` creates `tcpip_thread`). All meaningful init
        // happens inside the app task created by the family driver;
        // pre-scheduler init is a no-op on MPS2-AN385. Identical to the
        // legacy `BoardInit::init_hardware(&Config)` body above.
    }
}

impl nros_platform::BoardPrint for Mps2An385 {
    fn println(args: core::fmt::Arguments<'_>) {
        use core::fmt::Write;
        if let Ok(mut stdout) = cortex_m_semihosting::hio::hstdout() {
            let _ = writeln!(stdout, "{}", args);
        }
    }
}

impl nros_platform::BoardExit for Mps2An385 {
    fn exit_success() -> ! {
        exit_success()
    }

    fn exit_failure() -> ! {
        exit_failure()
    }
}

impl nros_platform::BoardEntry for Mps2An385 {
    /// Drive boot → user-closure → exit via the family driver.
    ///
    /// The 212.N.1 `BoardEntry::run` signature takes only `setup`; the
    /// FreeRTOS family driver `run_entry` needs a `Config` (MAC / IP /
    /// task priorities). Bridge by constructing `Config::default()`
    /// here — matches the legacy `run()` wrapper's call site.
    fn run<F, E>(setup: F) -> Result<(), E>
    where
        F: FnOnce(&mut nros_platform::RuntimeCtx<'_>) -> Result<(), E>,
        E: core::fmt::Debug,
    {
        // Mirror legacy `run()` — wire the semihosting log writer
        // before any task spawns. Idempotent.
        register_log_writer();
        nros_board_freertos::run_entry::<Mps2An385, F, E>(Config::default(), setup)
    }
}

impl Mps2An385 {
    /// Phase 228.E.2 — per-tier multi-task entry; delegates to
    /// [`nros_board_freertos::run_tiers_entry`]. The `nros::main!()` macro emits
    /// `<Mps2An385>::run_tiers(TIERS, run_plan)` for multi-tier systems
    /// (single-tier keeps the `BoardEntry::run` path).
    pub fn run_tiers<F, E>(
        tiers: &'static [nros_platform::TierSpec<'static>],
        setup: F,
    ) -> Result<(), E>
    where
        F: Fn(&mut nros_platform::RuntimeCtx<'_>) -> Result<(), E> + Copy,
        E: core::fmt::Debug,
    {
        register_log_writer();
        nros_board_freertos::run_tiers_entry::<Mps2An385, F, E>(Config::default(), tiers, setup)
    }
}
