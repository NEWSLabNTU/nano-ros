// Phase 212.N.1 — trait surface only; consumers land in 212.N.2+.
// Suppress `dead_code` workspace-wide until a family driver crate
// pulls these in. Phase 216.A.1 — `DispatchStrategy` ships ahead of
// its first consumer (Phase 216.A.2 trait extension), so
// `unused_imports` joins the allowance.
#![allow(dead_code, unused_imports)]

//! Board trait family — Phase 212.N.1.
//!
//! Platform-agnostic Board taxonomy living in `nros-platform`. The
//! board crate (`nros-board-{posix,freertos,threadx,…}`) implements
//! the per-family/per-target surface; user Entry pkgs invoke
//! `<Board as BoardEntry>::run(setup)` from `main.rs`.
//!
//! ## Surface
//!
//! ```text
//! Board: BoardInit + BoardPrint + BoardExit
//!     │
//!     ├── TransportBringup: Board    // Ethernet / WiFi / serial / CAN / USB CDC / IVC
//!     ├── NetworkWait: Board         // carrier / DHCP / link-up gate
//!     └── BoardEntry: Board {
//!             fn run<F, E>(setup: F) -> Result<(), E>
//!             where F: FnOnce(&mut RuntimeCtx) -> Result<(), E>;
//!         }
//! ```
//!
//! `BoardEntry::run` owns the full boot lifecycle: hardware init →
//! transport bringup → executor lifecycle → clean exit. The `setup`
//! callback receives a [`RuntimeCtx`] for overlay (params / remaps /
//! env) plus the codegen-emitted `run_plan(runtime)` call.
//!
//! ## Status
//!
//! Phase 212.N.1 ships the trait surface only. Per-board impls
//! (212.N.2 family driver crates + 212.N.3 tier-1 per-board crates),
//! codegen (212.N.4 / N.5 — lives in standalone `nros-cli` repo per
//! CLAUDE.md), and cmake fn rename (212.N.6) follow.
//!
//! ## Relationship to existing `nros-board-common::Board*` traits
//!
//! The legacy traits in `nros-board-common::board_init`
//! (`Board`, `BoardInit`, `BoardPrint`, `BoardExit`, `BoardEntry`,
//! `DirectExec`, `run`) stay as-is during the transition. Phase
//! 212.N.7 retires the M.5.a FreeRTOS BSP baker and migrates every
//! Node pkg to the new shape; at that point the legacy
//! `nros-board-common` traits become `pub use` re-exports of this
//! module (or get retired entirely if no consumer remains).

pub mod config;
pub mod dispatch;
pub mod embassy_entry;
pub mod entry;
pub mod exit;
pub mod init;
pub mod network;
pub mod print;
pub mod rtic_entry;
pub mod runtime;
pub mod tier;
pub mod transport;

pub use config::{BoardConfig, BoardTransportConfig};
pub use dispatch::DispatchStrategy;
pub use embassy_entry::EmbassyBoardEntry;
pub use entry::{BoardEntry, DeployOverlay};
pub use exit::BoardExit;
pub use init::BoardInit;
pub use network::NetworkWait;
pub use print::BoardPrint;
pub use rtic_entry::RticBoardEntry;
pub use runtime::{
    NodeDispatchRuntime, NullNodeRuntime, RuntimeCtx, RuntimeError, SignaledCallback,
};
pub use tier::{TierSpec, freertos_priority_for, posix_nice_for, threadx_priority_for};

/// Phase 214.K.1 — backward-compat alias. The board-side dispatch
/// sink was renamed `NodeRuntime` → `NodeDispatchRuntime` to
/// disambiguate from the user-facing `nros::NodeRuntime` metadata
/// trait. This alias stays for one release cycle so external impl
/// callers (per-board crates outside the tree) get a clear
/// deprecation arrow rather than a hard break.
#[deprecated(
    note = "renamed to NodeDispatchRuntime — Phase 214.K.1. Update imports + impls within one release cycle."
)]
pub use runtime::NodeDispatchRuntime as NodeRuntime;
pub use transport::TransportBringup;

/// Super-trait every board impl carries (mirrors
/// `nros-board-common::board_init::Board`).
///
/// Blanket-implemented for any type carrying all three contracts;
/// concrete board crates do NOT impl `Board` directly — they impl
/// `BoardInit`/`BoardPrint`/`BoardExit` (and the optional mixins).
pub trait Board: BoardInit + BoardPrint + BoardExit {}
impl<T: BoardInit + BoardPrint + BoardExit> Board for T {}
