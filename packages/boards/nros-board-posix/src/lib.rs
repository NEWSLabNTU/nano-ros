//! POSIX family driver — Phase 212.N.2.
//!
//! Implements the `Board` trait family from `nros_platform` (the
//! traits live in `packages/core/nros-platform/src/board/` and are
//! re-exported at the `nros_platform` crate root) for the hosted
//! POSIX target (Linux, macOS, BSD). This is the simplest of the
//! family driver crates:
//!
//! - `init_hardware` is a no-op — libstd's runtime already brought up
//!   the heap, stdio, signal handlers and threading by the time
//!   `fn main` reaches us.
//! - `println` writes to `STDOUT_FILENO` via libstd's `Stdout` (which
//!   ultimately calls `write(2)` — matches the contract documented in
//!   `nros_platform::BoardPrint`).
//! - `exit_success` / `exit_failure` call `std::process::exit`.
//! - There is no `TransportBringup` / `NetworkWait` impl — POSIX
//!   sockets are open as soon as `init_hardware` returns. The trait
//!   surface treats both as optional mixins; their absence is the
//!   whole point.
//!
//! ## `BoardEntry::run` body
//!
//! The body sequences the lifecycle the trait surface documents:
//!
//! ```text
//! init_hardware()            // no-op
//! ↓
//! build RuntimeCtx           // empty by default; Phase 212.N.4
//!                            // codegen will populate from env / CLI
//! ↓
//! setup(&mut ctx)            // user closure — typically
//!                            // codegen-emitted `run_plan(runtime)`
//!                            // which owns nros::init + Executor::open
//!                            // + Executor::spin internally
//! ↓
//! exit_success() or          // -> !
//! exit_failure()
//! ```
//!
//! Note that the **executor lifecycle is deliberately owned by the
//! `setup` callback** rather than by `run` itself. Every existing
//! POSIX Entry pkg `main.rs` (see `examples/native/rust/talker`) opens
//! its own `Executor`, registers timers / nodes, and calls
//! `spin_blocking` from inside what becomes the `setup` closure once
//! Phase 212.N.4 codegen lands. `run` would have nothing portable to
//! say about which `Executor` instance to spin or how, so it stays out
//! of that decision. The seam is documented under "Open seams" below.

#![forbid(unsafe_op_in_unsafe_fn)]

use std::io::Write as _;

// `nros_platform::board` is `mod board;` (private); the Board trait
// family is re-exported at the crate root.
use nros_platform::{BoardEntry, BoardExit, BoardInit, BoardPrint, RuntimeCtx};

/// POSIX family driver ZST. Plug into an Entry pkg `main.rs` via:
///
/// ```ignore
/// use nros_board_posix::PosixBoard;
/// use nros_platform::board::BoardEntry;
///
/// fn main() {
///     let _ = <PosixBoard as BoardEntry>::run(|runtime| {
///         // codegen-emitted (Phase 212.N.4)
///         run_plan(runtime)
///     });
/// }
/// ```
pub struct PosixBoard;

impl BoardInit for PosixBoard {
    /// POSIX needs no hardware init: libstd's runtime already
    /// initialized the heap, stdio, signal handlers and threading
    /// before `fn main` ran. Kept as a documented no-op so the
    /// lifecycle in [`BoardEntry::run`] is uniform across families.
    #[inline]
    fn init_hardware() {}
}

impl BoardPrint for PosixBoard {
    fn println(args: core::fmt::Arguments<'_>) {
        // Write to a stdout lock so concurrent threads don't
        // interleave a single line. `libc::write(STDOUT_FILENO, …)`
        // would also satisfy the trait, but libstd's `Stdout` already
        // bottoms out in `write(2)` and adds line-buffered locking
        // that we'd otherwise have to rebuild. If the write fails
        // (closed stdout, broken pipe) we deliberately swallow the
        // error — a board-print failure shouldn't tear down the boot.
        let mut out = std::io::stdout().lock();
        let _ = writeln!(out, "{}", args);
    }
}

impl BoardExit for PosixBoard {
    fn exit_success() -> ! {
        std::process::exit(0)
    }

    fn exit_failure() -> ! {
        std::process::exit(1)
    }
}

impl BoardEntry for PosixBoard {
    /// Drive the boot → setup → exit flow. POSIX has no transport
    /// bringup or network-wait step, so the body is the minimum the
    /// trait surface documents:
    ///
    /// 1. [`BoardInit::init_hardware`] (no-op).
    /// 2. Build a [`RuntimeCtx`] via [`RuntimeCtx::with_runtime`].
    ///    Today the runtime slot is a [`NullComponentRuntime`]
    ///    placeholder; Phase 212.N.7 step-3.5 wires the real
    ///    `ExecutorComponentRuntime` here, and Phase 212.N.4 codegen
    ///    populates `params` / `remaps` / `env` from CLI args and the
    ///    launch overlay.
    /// 3. Invoke `setup(&mut ctx)`.
    /// 4. Log the result via [`BoardPrint::println`].
    /// 5. Diverge into [`BoardExit::exit_success`] or
    ///    [`BoardExit::exit_failure`].
    ///
    /// The executor open + spin happens *inside* `setup` — see the
    /// crate-level docs for why this seam lives there.
    fn run<F, E>(setup: F) -> Result<(), E>
    where
        F: FnOnce(&mut RuntimeCtx<'_>) -> Result<(), E>,
        E: core::fmt::Debug,
    {
        <Self as BoardInit>::init_hardware();

        // Phase 212.N.7 step-3.2 placeholder: `RuntimeCtx` now carries
        // a `&mut dyn ComponentRuntime`. Step-3.5 swaps this for the
        // real `ExecutorComponentRuntime`.
        let mut crt = ::nros_platform::NullComponentRuntime;
        let mut runtime = RuntimeCtx::with_runtime(&mut crt);
        match setup(&mut runtime) {
            Ok(()) => {
                <Self as BoardPrint>::println(format_args!("nros: application complete"));
                <Self as BoardExit>::exit_success();
            }
            Err(e) => {
                <Self as BoardPrint>::println(format_args!("nros: application error: {e:?}"));
                <Self as BoardExit>::exit_failure();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn init_hardware_is_noop() {
        // Smoke: calling `init_hardware` from a unit test must not
        // panic or affect global state.
        <PosixBoard as BoardInit>::init_hardware();
    }

    #[test]
    fn println_writes_without_panicking() {
        <PosixBoard as BoardPrint>::println(format_args!("nros-board-posix: hello from unit test"));
    }

    // Note: `BoardEntry::run` itself can't be unit-tested directly
    // because both exit branches diverge (`-> !`) via
    // `std::process::exit`, which would kill the test process. The
    // doc comment on `BoardEntry::run` explicitly preserves the
    // `-> Result` shape on the *callback* path so production boards
    // can still wrap the trait in a non-diverging test harness; that
    // harness lives outside this crate.
}
