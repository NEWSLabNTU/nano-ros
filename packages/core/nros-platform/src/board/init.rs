//! [`BoardInit`] — Phase 212.N.1.
//!
//! Per-board hardware-init contract. Mirrors the legacy
//! `nros-board-common::board_init::BoardInit`, but lives in
//! `nros-platform` so codegen + family-driver crates (212.N.2) can
//! depend on `nros-platform` alone rather than pulling
//! `nros-board-common`.
//!
//! ## Differences from the legacy trait
//!
//! - **No `Config` associated type.** Board config moves to the
//!   typed `RuntimeCtx` accessed by `BoardEntry::run`'s `setup`
//!   callback — codegen owns config plumbing, not the board. A
//!   target whose hardware genuinely needs a build-time const
//!   (clock tree pin, MMIO base) keeps that as a board crate
//!   const, not a trait associated type.
//! - **`init_hardware()` is parameterless.** The runtime
//!   `RuntimeCtx` is opened later (`BoardEntry::run` constructs it
//!   after init). Boards that need to peek at config during init
//!   read from the board crate's `pub const` / static rather than
//!   a `&Self::Config` arg.

/// Per-board hardware-init contract.
///
/// One impl per board (`pub struct Mps2An385; impl BoardInit for
/// Mps2An385`). Vendor HAL calls — clock tree, pin mux, peripheral
/// wakes — live in [`Self::init_hardware`]. Called once by
/// [`crate::board::BoardEntry::run`] before any transport bringup or
/// executor lifecycle.
pub trait BoardInit {
    /// Hardware init. Runs once on boot, before any allocation or
    /// transport bringup. Panicking from here is the same as
    /// panicking from `fn main()` — there's no recovery path.
    fn init_hardware();
}
