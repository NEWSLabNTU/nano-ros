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
//! 2. [`nros_board_export!`] emits those `nros_board_*` symbols from a Rust
//!    board's own plain functions (`#[unsafe(no_mangle)] extern "C"`), so a
//!    C / C++ application can call into a Rust board. It is a thin 1:1 mirror
//!    of the C ABI with NO trait dependency — the header is the SSoT, and the
//!    canonical *Rust* board API (session / executor sizing / tiers) is the
//!    separate, Rust-rich `nros_platform::board` surface.
//!
//! # The config pointer
//!
//! `cfg` is an opaque `*const c_void` the board implementation casts back to
//! its concrete config type (the `config = …` arg to the macro). The generic
//! ABI never inspects it. Board crates expose their own C constructor for the
//! config object; building it is out of scope for this crate.

#![no_std]
#![allow(clippy::not_unsafe_ptr_arg_deref)]

use core::ffi::c_void;

/// User application entry, matching the `nros_board_app_fn` typedef in
/// `<nros/board.h>`. Returns `0` on success, non-zero on error.
pub type NrosBoardAppFn = extern "C" fn(user: *mut c_void) -> i32;

// ============================================================================
// Canonical ABI declarations — GENERATED (RFC-0054, phase-299)
// ----------------------------------------------------------------------------
// `include/nros/board.h` is the SSoT; src/generated.rs is committed
// bindgen output (scripts/gen-abi-bindings.sh). The nros_board_export!
// macro below stays hand-written — it EMITS the definitions (port side).
// ============================================================================

pub mod generated;
pub use generated::*;

// ============================================================================
// Export macro
// ============================================================================

/// Emit every `nros_board_*` symbol declared in `<nros/board.h>` from a
/// board's own plain functions — the Rust author's ergonomic binding to the
/// board C ABI, mirroring `nros_platform_export!` for the platform ABI.
///
/// The board C ABI header `<nros/board.h>` is the cross-language SSoT
/// (RFC-0054): a C / C++ / any-language board implements the `nros_board_*`
/// symbols directly. This macro is the Rust convenience path — a thin **1:1
/// mirror** of those symbols over functions the board already has, with **no
/// trait dependency** (the canonical *Rust* board API — session, executor
/// sizing, tiers — is [`nros_platform::board`]; that is a distinct, Rust-rich
/// surface, not this flat C ABI).
///
/// # Arguments (named)
///
/// - `config = $C:ty` — the board's concrete config type. The opaque
///   `cfg: *const c_void` is cast (`init_hardware`) / `ptr::read` (`run`) back
///   to `$C`; the generic ABI never inspects it.
/// - `init = <fn(&$C)>` — pre-run hardware bring-up.
/// - `println = <fn(&str)>` — status output (the C `msg`/`len` are decoded to a
///   `&str` first; non-UTF-8 collapses to `"<non-utf8>"`).
/// - `exit_success` / `exit_failure = <fn() -> !>` — process / firmware exit.
/// - `run = <fn($C, F) -> !  where F: FnOnce() -> Result<(), i32>>` — the full
///   entry driver. **This one function encodes the family**: a direct-exec
///   board runs `app` inline (`init → app → exit`); a kernel-spawn board
///   spawns the app task + starts the scheduler. The C `app` fn's non-zero
///   return maps to `Err(rc)`. There is no macro-side family split.
///
/// # Example
/// ```ignore
/// nros_board_export! {
///     config       = MyConfig,
///     init         = my_board::init_hardware,
///     println      = my_board::board_print,
///     exit_success = my_board::exit_success,
///     exit_failure = my_board::exit_failure,
///     run          = my_board::run_bare,
/// }
/// ```
///
/// [`nros_platform::board`]: https://docs.rs/nros-platform
#[macro_export]
macro_rules! nros_board_export {
    (
        config = $C:ty,
        init = $init:path,
        println = $println:path,
        exit_success = $exit_success:path,
        exit_failure = $exit_failure:path,
        run = $run:path $(,)?
    ) => {
        #[unsafe(no_mangle)]
        #[allow(clippy::not_unsafe_ptr_arg_deref)]
        pub extern "C" fn nros_board_init_hardware(cfg: *const ::core::ffi::c_void) {
            // SAFETY: caller passes a pointer to a live config of type `$C`
            // (see `<nros/board.h>`); it outlives this call.
            let cfg: &$C = unsafe { &*(cfg as *const $C) };
            $init(cfg);
        }

        #[unsafe(no_mangle)]
        #[allow(clippy::not_unsafe_ptr_arg_deref)]
        pub extern "C" fn nros_board_println(msg: *const u8, len: usize) {
            let bytes: &[u8] = if msg.is_null() || len == 0 {
                &[]
            } else {
                // SAFETY: caller passes a valid `len`-byte slice outliving the call.
                unsafe { ::core::slice::from_raw_parts(msg, len) }
            };
            let s = ::core::str::from_utf8(bytes).unwrap_or("<non-utf8>");
            $println(s);
        }

        #[unsafe(no_mangle)]
        pub extern "C" fn nros_board_exit_success() -> ! {
            $exit_success()
        }

        #[unsafe(no_mangle)]
        pub extern "C" fn nros_board_exit_failure() -> ! {
            $exit_failure()
        }

        #[unsafe(no_mangle)]
        #[allow(clippy::not_unsafe_ptr_arg_deref)]
        pub extern "C" fn nros_board_run(
            cfg: *const ::core::ffi::c_void,
            app: $crate::NrosBoardAppFn,
            user: *mut ::core::ffi::c_void,
        ) -> ! {
            // SAFETY: caller passes a pointer to a live, owned `$C` and does not
            // reuse it after this call (ownership transfers into `run`).
            let cfg: $C = unsafe { ::core::ptr::read(cfg as *const $C) };
            // The C `app` fn becomes the user closure; its non-zero return maps
            // to `Err`. The board's own `run` fn owns the family shape
            // (inline vs task-spawn) — the macro is family-agnostic.
            $run(cfg, move || match app(user) {
                0 => ::core::result::Result::Ok(()),
                rc => ::core::result::Result::Err(rc),
            })
        }
    };
}
