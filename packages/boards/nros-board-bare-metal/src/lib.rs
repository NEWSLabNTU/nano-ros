// Phase 212.N.2 — direct-exec family driver.
//
// `no_std`, zero runtime deps (only the trait surface from
// `nros-platform::board`). Per-board crates pull this in and impl
// `DirectExec` for their ZST.
#![no_std]
#![forbid(unsafe_code)]

//! Direct-exec family driver — Phase 212.N.2.
//!
//! Implements [`BoardEntry::run`] for the **direct-exec** boot model:
//! Cortex-M0+/M3/M4 bare-metal and esp-hal targets where there is **no
//! kernel and no scheduler**. The user closure runs on the **boot
//! stack**, and control falls through to [`BoardExit::exit_success`] /
//! [`BoardExit::exit_failure`] when it returns.
//!
//! ## Boot-stack lifetime
//!
//! On direct-exec targets the boot stack is *the* stack — there is no
//! task switch, no second context to migrate to. Anything the user
//! closure puts on the stack (executor storage, transport buffers,
//! local message slots) lives there for the entire process lifetime
//! and is implicitly dropped when [`exit_*`](BoardExit) is called.
//! That's fine for direct-exec because `exit_*` diverges; no Drop
//! glue is expected to run after exit.
//!
//! ## No scheduler
//!
//! Kernel-spawn families (FreeRTOS, ThreadX, NuttX) have their own
//! `run` body that allocates an app task and starts the scheduler.
//! This crate is **not** for them — they implement [`BoardEntry`] by
//! hand and route through their family driver crate
//! (`nros-board-freertos`, …).
//!
//! ## Use
//!
//! Per-board crate:
//!
//! ```ignore
//! use nros_board_bare_metal::{DirectExec, run_entry};
//! use nros_platform::{BoardEntry, BoardExit, BoardInit, BoardPrint, RuntimeCtx};
//!
//! pub struct MyBoard;
//!
//! impl BoardInit for MyBoard { /* ... */ }
//! impl BoardPrint for MyBoard { /* ... */ }
//! impl BoardExit for MyBoard { /* ... */ }
//!
//! // Opt into the direct-exec family driver.
//! impl DirectExec for MyBoard {}
//!
//! impl BoardEntry for MyBoard {
//!     fn run<F, E>(setup: F) -> Result<(), E>
//!     where
//!         F: FnOnce(&mut RuntimeCtx<'_>) -> Result<(), E>,
//!         E: core::fmt::Debug,
//!     {
//!         run_entry::<Self, F, E>(setup)
//!     }
//! }
//! ```
//!
//! User Entry pkg `main.rs`:
//!
//! ```ignore
//! fn main() {
//!     let _ = <MyBoard as BoardEntry>::run(|runtime| {
//!         run_plan(runtime) // codegen-emitted (212.N.4)
//!     });
//! }
//! ```
//!
//! ## Relationship to legacy `nros-board-common::board_init::run`
//!
//! This crate is the 212.N replacement for
//! `nros-board-common::board_init::run` (the legacy direct-exec
//! driver). The legacy `run` takes `(B::Config, FnOnce(&Config))`
//! and is `-> !`; the 212.N surface here takes
//! `FnOnce(&mut RuntimeCtx) -> Result<(), E>` and is `-> Result<(),
//! E>` (the diverging exit still happens — `run_entry` calls
//! `exit_*` directly — but the trait keeps the option to return so
//! hosted unit tests of the family driver remain possible).
//!
//! Phase 212.N.7 migrates the existing direct-exec boards
//! (`nros-board-mps2-an385`, `nros-board-stm32f4`, `nros-board-esp32-qemu`)
//! off the legacy driver and onto this crate.

// `BoardEntry` is imported only so intra-doc links in this crate
// resolve; the `run_entry` bound itself is `DirectExec` (which
// transitively requires `BoardInit + BoardPrint + BoardExit`).
#[allow(unused_imports)]
use nros_platform::BoardEntry;
use nros_platform::{BoardExit, BoardInit, BoardPrint, RuntimeCtx};

/// Opt-in marker for direct-exec boards.
///
/// Mirrors `nros-board-common::board_init::DirectExec`: per-board
/// crates carry `impl DirectExec for MyBoard {}` to advertise that
/// they participate in this family. The marker is informational —
/// the [`BoardEntry`] impl still goes by hand (a one-line delegation
/// to [`run_entry`]) because Rust's coherence rules forbid this
/// crate from blanket-impling [`BoardEntry`] for every `DirectExec`
/// downstream (`BoardEntry` is foreign to this crate).
///
/// Kernel-spawn boards (FreeRTOS, ThreadX, NuttX) MUST NOT implement
/// this marker; they have their own family driver.
pub trait DirectExec: BoardInit + BoardPrint + BoardExit {}

/// Direct-exec `BoardEntry::run` body.
///
/// Per-board crates delegate to this from their `BoardEntry` impl
/// (see crate-level docs for the boilerplate).
///
/// ## Flow
///
/// 1. [`BoardInit::init_hardware`] — clock tree, pin mux, peripheral
///    wakes (per-board HAL calls).
/// 2. Build [`RuntimeCtx`] via [`RuntimeCtx::with_runtime`] on the boot stack as a placeholder.
///    The 212.N.4 codegen will pass a populated `RuntimeCtx` via a
///    different entry point once launch overlays are wired.
/// 3. Invoke the user `setup` closure with `&mut RuntimeCtx`.
/// 4. On `Ok(())` call [`BoardExit::exit_success`]; on `Err` call
///    [`BoardExit::exit_failure`]. Both diverge, so the function
///    never actually returns at runtime — the `Result<(), E>` return
///    type exists only so the trait signature in
///    `nros_platform::BoardEntry` is satisfied.
///
/// ## Diverging in practice
///
/// `exit_success` / `exit_failure` are `-> !`, so the `match` arms
/// type-check as `!` and the compiler erases the `Result` return.
/// The signature is `-> Result<(), E>` purely for trait conformance.
pub fn run_entry<B, F, E>(setup: F) -> Result<(), E>
where
    B: DirectExec,
    F: FnOnce(&mut RuntimeCtx<'_>) -> Result<(), E>,
    E: core::fmt::Debug,
{
    // 1. Hardware init (per-board HAL).
    B::init_hardware();

    // 2. Boot-stack RuntimeCtx placeholder. 212.N.7 step-3.5 swaps
    //    this for the real `ExecutorNodeRuntime`; today the
    //    runtime slot is `NullNodeRuntime` (every register call
    //    errors loud). 212.N.4 codegen will hand a populated
    //    `RuntimeCtx` in via a different entry point.
    let mut crt = ::nros_platform::NullNodeRuntime;
    let mut ctx = RuntimeCtx::with_runtime(&mut crt);

    // 3. User closure on the boot stack.
    match setup(&mut ctx) {
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
