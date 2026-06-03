// Phase 212.N.1 — trait surface only; consumers land in 212.N.2+.
// Suppress `dead_code` workspace-wide until a family driver crate
// pulls these in.
#![allow(dead_code)]

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
//! Component pkg to the new shape; at that point the legacy
//! `nros-board-common` traits become `pub use` re-exports of this
//! module (or get retired entirely if no consumer remains).

pub mod config;
pub mod entry;
pub mod exit;
pub mod init;
pub mod network;
pub mod print;
pub mod runtime;
pub mod transport;

pub use config::{BoardConfig, BoardTransportConfig};
pub use entry::BoardEntry;
pub use exit::BoardExit;
pub use init::BoardInit;
pub use network::NetworkWait;
pub use print::BoardPrint;
pub use runtime::{
    ComponentDispatchFn, ComponentInitFn, ComponentRegisterFn, ComponentRuntime, ComponentTickFn,
    NullComponentRuntime, RuntimeCtx, RuntimeError,
};
// Phase 212.N.12 — Component → Node rename aliases. The user-facing
// trait/typedef surface is "Node*"; the legacy "Component*" names stay
// as deprecated re-export aliases for one release.
pub use runtime::{
    ComponentDispatchFn as NodeDispatchFn, ComponentInitFn as NodeInitFn,
    ComponentRegisterFn as NodeRegisterFn, ComponentRuntime as NodeRuntime,
    ComponentTickFn as NodeTickFn, NullComponentRuntime as NullNodeRuntime,
};
pub use transport::TransportBringup;

/// Super-trait every board impl carries (mirrors
/// `nros-board-common::board_init::Board`).
///
/// Blanket-implemented for any type carrying all three contracts;
/// concrete board crates do NOT impl `Board` directly — they impl
/// `BoardInit`/`BoardPrint`/`BoardExit` (and the optional mixins).
pub trait Board: BoardInit + BoardPrint + BoardExit {}
impl<T: BoardInit + BoardPrint + BoardExit> Board for T {}
