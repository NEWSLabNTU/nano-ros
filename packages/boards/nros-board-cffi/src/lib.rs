//! Rust mirror of the canonical board C ABI in `<nros/board.h>`.
//!
//! The board layer sits one tier above the platform layer
//! ([`nros-platform-cffi`]): the platform supplies system primitives
//! (clock, alloc, threading); the board supplies the *entry workflow*
//! — hardware bring-up, status output, process exit — that drives the
//! user application.
//!
//! Two surfaces, mirroring `nros-platform-cffi`:
//!
//! 1. The [`unsafe extern "C"`](self) block below declares the
//!    `nros_board_*` symbols so a Rust runtime can call a board
//!    supplied from C (or a static lib).
//! 2. [`nros_board_export!`] re-emits a Rust [`Board`] impl as
//!    `#[unsafe(no_mangle)] extern "C"` symbols matching the header,
//!    so a C / C++ application can call into a Rust board.
//!
//! # The config pointer
//!
//! `cfg` is an opaque `*const c_void` the board implementation casts
//! back to its concrete [`BoardInit::Config`]. The generic ABI never
//! inspects it. Board crates expose their own C constructor for the
//! config object; building it is out of scope for this crate.
//!
//! [`Board`]: nros_board_common::Board
//! [`BoardInit::Config`]: nros_board_common::BoardInit::Config

#![no_std]
#![allow(clippy::not_unsafe_ptr_arg_deref)]

use core::ffi::c_void;

/// User application entry, matching the `nros_board_app_fn` typedef in
/// `<nros/board.h>`. Returns `0` on success, non-zero on error.
pub type NrosBoardAppFn = extern "C" fn(user: *mut c_void) -> i32;

// ============================================================================
// Canonical ABI declarations
// ----------------------------------------------------------------------------
// Hand-written mirror of `include/nros/board.h`. Names and types track
// the header. Updates land in the header first, then here.
// ============================================================================

unsafe extern "C" {
    /// Direct-exec board entry driver. Never returns.
    pub fn nros_board_run(cfg: *const c_void, app: NrosBoardAppFn, user: *mut c_void) -> !;

    /// Run board-specific hardware init for the opaque config.
    pub fn nros_board_init_hardware(cfg: *const c_void);

    /// Emit one status / banner / error line (UTF-8, `len` bytes).
    pub fn nros_board_println(msg: *const u8, len: usize);

    /// Terminate the firmware after a successful run. Never returns.
    pub fn nros_board_exit_success() -> !;

    /// Terminate the firmware after a failed run. Never returns.
    pub fn nros_board_exit_failure() -> !;
}

// ============================================================================
// Export macro
// ============================================================================

/// Emit every `nros_board_*` symbol declared in `<nros/board.h>` by
/// delegating to the [`Board`] trait impl on `$ty`.
///
/// `$ty` is the board ZST (`pub struct MyBoard;` +
/// `impl BoardInit/BoardPrint/BoardExit for MyBoard`) that also
/// implements [`BoardEntry`] — directly for kernel-spawn families, or
/// for free via the [`DirectExec`] marker for bare-metal / esp-hal
/// boards. The emitted `nros_board_run` calls `<$ty as BoardEntry>::run`,
/// so the macro serves **both** entry shapes; the four primitives
/// (`init_hardware` / `println` / `exit_*`) delegate to the split
/// traits.
///
/// The opaque `cfg: *const c_void` is read out as `<$ty>::Config`
/// (`ptr::read`). The caller (C / C++ app) passes a pointer to a live
/// config object of the board's concrete type and must not reuse it
/// after the call (ownership transfers into `run`).
///
/// [`Board`]: nros_board_common::Board
/// [`BoardEntry`]: nros_board_common::BoardEntry
/// [`DirectExec`]: nros_board_common::DirectExec
#[macro_export]
macro_rules! nros_board_export {
    ($ty:ty) => {
        #[unsafe(no_mangle)]
        pub extern "C" fn nros_board_init_hardware(cfg: *const ::core::ffi::c_void) {
            // SAFETY: caller passes a pointer to a live config of the
            // board's concrete `Config` type (see `<nros/board.h>`).
            let cfg = unsafe { &*(cfg as *const <$ty as ::nros_board_common::BoardInit>::Config) };
            <$ty as ::nros_board_common::BoardInit>::init_hardware(cfg);
        }

        #[unsafe(no_mangle)]
        pub extern "C" fn nros_board_println(msg: *const u8, len: usize) {
            // SAFETY: caller passes a valid UTF-8 byte slice of `len`
            // bytes that outlives the call; empty case collapses to "".
            let bytes: &[u8] = if msg.is_null() || len == 0 {
                &[]
            } else {
                unsafe { ::core::slice::from_raw_parts(msg, len) }
            };
            let s = ::core::str::from_utf8(bytes).unwrap_or("<non-utf8>");
            <$ty as ::nros_board_common::BoardPrint>::println(::core::format_args!("{}", s));
        }

        #[unsafe(no_mangle)]
        pub extern "C" fn nros_board_exit_success() -> ! {
            <$ty as ::nros_board_common::BoardExit>::exit_success()
        }

        #[unsafe(no_mangle)]
        pub extern "C" fn nros_board_exit_failure() -> ! {
            <$ty as ::nros_board_common::BoardExit>::exit_failure()
        }

        #[unsafe(no_mangle)]
        pub extern "C" fn nros_board_run(
            cfg: *const ::core::ffi::c_void,
            app: $crate::NrosBoardAppFn,
            user: *mut ::core::ffi::c_void,
        ) -> ! {
            // SAFETY: caller passes a pointer to a live, owned config of
            // the board's concrete type and does not reuse it after this
            // call (ownership transfers into `run`).
            let cfg = unsafe {
                ::core::ptr::read(cfg as *const <$ty as ::nros_board_common::BoardInit>::Config)
            };
            // `BoardEntry::run` is family-agnostic: direct-exec boards
            // route through `nros_board_common::run`; kernel-spawn boards
            // route through their family `run`. The C `app` fn becomes
            // the user closure; its non-zero return maps to `Err`.
            <$ty as ::nros_board_common::BoardEntry>::run(cfg, move |_cfg| match app(user) {
                0 => ::core::result::Result::Ok(()),
                rc => ::core::result::Result::Err(rc),
            })
        }
    };
}
