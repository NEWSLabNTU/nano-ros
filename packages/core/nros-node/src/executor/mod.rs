//! Embedded executor with build-time configured arena.
//!
//! Provides `Executor` and `Node` that work with the compile-time
//! selected RMW backend (zenoh, XRCE-DDS, or C FFI).
//!
//! # Example
//!
//! ```ignore
//! use nros_node::executor::*;
//! use std_msgs::msg::Int32;
//!
//! let config = ExecutorConfig::from_env().node_name("my_node");
//! let mut executor = Executor::open(&config)?;
//! let mut node = executor.create_node("my_node")?;
//!
//! let publisher = node.create_publisher::<Int32>("/chatter")?;
//! publisher.publish(&Int32 { data: 42 })?;
//!
//! loop {
//!     executor.spin_once(core::time::Duration::from_millis(10));
//! }
//! ```

#[cfg(any(has_rmw, test))]
pub mod action_core;
#[cfg(any(has_rmw, test))]
mod arena;
// phase-412 #4 — the compile-time half of "is the arena big enough?". Not
// `has_rmw`-gated: it reads only `config`, and the check it carries must run in
// every compilation of this crate, whatever the backend.
pub mod arena_oracle;
// phase-392 W6 — the named `.bss` static the `alloc` convenience constructors
// serve their backing from. `alloc`-gated because it exists to replace a
// `Box::leak`: a no-alloc entry already supplies its own `static` through
// `open_in`, and reserving a second one there would be pure waste in exactly
// the images this campaign is about.
#[cfg(all(any(has_rmw, test), feature = "alloc"))]
mod backing;
// issue 1571 — the spawned tiers' half of the same machinery: the ENTRY owns
// the static (it knows the tier count), boards hand each tier a slot.
#[cfg(all(any(has_rmw, test), feature = "alloc"))]
pub use backing::{
    EXECUTOR_BACKING_DEFAULT_U64S, TIER_TASK_TCB_U64S, TierBackingShort, TierExecutorBacking,
    TierExecutorBackingSlot, TierSetupGate, TierSetupGuard, TierTaskMemory, TierTaskMemoryRaw,
    TierTaskMemorySet, check_tier_executor_backing,
};
// Phase 8 (autoware-safety-island `docs/design/callback_tracing.rst`) —
// callback-level dispatch tracing. Same gating shape as `wake_probe`, for the
// same reason: hot-path hooks that must vanish in production. The module ALSO
// self-gates (`#![cfg(feature = "trace-callbacks")]`), so the call sites use
// paired stubs rather than naming it directly.
#[cfg(all(any(has_rmw, test), feature = "trace-callbacks"))]
pub mod callback_trace;
// phase-463 W7 — the host profile (`profile-mode`, host-only: it needs `std`
// and no RTOS umbrella resolves it).
#[cfg(all(any(has_rmw, test), feature = "profile-mode"))]
pub mod profile;
// phase-457 W5 (issue 1522) — the shape a DECLARED subscription lowers to.
// UNGATED on purpose: the executor half needs an RMW seam, but the recorder
// half runs in a probe that opens no executor, and both must read the same
// enum.
pub mod declared_shape;
#[cfg(any(has_rmw, test))]
mod handles;
// issue 0669 / phase-359 W10 — `pub mod handoff` stood here. `Handoff<M, N>`
// was optional sugar over `Arc<Mutex<heapless::Vec<M, N>>>` for the
// cross-priority bridge pattern (phase-104.E.3), it was `std`-gated on
// `std::sync::Mutex` alone, and it had no consumer anywhere in the tree — not
// one call site in `packages/` or `examples/` since it landed. Deleted rather
// than ported: the only portable mutex reachable from here costs a `spin`
// dependency edge on every build of this crate (measured: `spin` is in NO
// board's graph today), and an API with no consumer has no evidence its shape
// is the right one. The pattern it wrapped is six lines and is written out in
// issue 0669.
#[cfg(any(has_rmw, test))]
pub mod monitor;
#[cfg(any(has_rmw, test))]
mod node;
#[cfg(any(has_rmw, test))]
pub mod node_record;
#[cfg(any(has_rmw, test))]
mod node_wake;
// phase-436 -- one backend wake slot shared by every executor over a session.
#[cfg(any(all(feature = "alloc", feature = "rmw-cffi"), test))]
pub(crate) mod wake_fanout;
// phase-397 W2 -- the per-entity wake: an arrival names its slot and owner.
#[cfg(all(feature = "alloc", feature = "rmw-cffi"))]
pub(crate) mod entity_wake;
#[cfg(any(has_rmw, test))]
/// phase-436 W7 — installing a port's park primitive, with the wake-object
/// constraint stated once instead of per board.
pub mod port_park;
// phase-359 W10 — the allocate/spawn/join helper moved to
// `nros_platform_api::task`, beside the ABI it wraps, once `nros-cpp` became a
// third caller. Reached through a `use` below rather than a module here.
#[cfg(any(has_rmw, test))]
pub(crate) mod ready_set;
// phase-457 W3 — the per-endpoint half of `registration_path`, reported by the
// one site that knows it. Behind a feature only a metadata probe turns on.
#[cfg(feature = "registration-observer")]
pub mod registration_observer;
pub mod sched_context;
// Issue 1971 -- the image's launch remap table, one `const` table in flash
// that every executor reads (RFC-0107 step 1). No RMW: it is plain data.
pub mod launch_remap;

/// phase-509 P9b -- the executor's lock (RFC-0108 "As built in P9b").
pub mod exec_lock;
// phase-509 P7 (RFC-0108 D8) -- each executor's level and the image's per-level
// liveness. Plain atomics, no RMW: every build carries it.
pub mod level_liveness;
pub use level_liveness::{
    MAX_LEVELS, every_level_alive, level_count, level_last_spin_us, level_spins, set_level_count,
    take_every_level_alive, tier_level, tier_level_by,
};
// phase-509 P6 (RFC-0108 D3/D7) -- the per-core scheduler and the per-level
// worker the executor's dispatcher splits into.
#[cfg(any(has_rmw, test))]
pub(crate) mod scheduler;
#[cfg(any(has_rmw, test))]
pub(crate) mod spin;
#[cfg(any(has_rmw, test))]
pub(crate) mod spsc_ring;
#[cfg(any(has_rmw, test))]
mod storage;
#[cfg(any(has_rmw, test))]
pub(crate) mod triple_buffer;
mod types;
#[cfg(all(any(has_rmw, test), feature = "wake-latency-probe"))]
pub mod wake_probe;

#[cfg(any(has_rmw, test))]
pub mod action;

// MockSession-based tests. Disabled when any rmw-* feature is active because
// feature unification under `cargo test --workspace` flips `ConcreteSession`
// to a real backend handle (e.g. UorbSession when rmw-uorb is on transitively
// via the workspace), breaking the type signatures the tests expect.
// `feature = "alloc"` is part of the gate, not decoration. The crate is
// `#![no_std]`; `tests.rs` uses `std::` in 155 places (in scope because
// `lib.rs` brings `std` in under `cfg(test)`) and calls `from_session`, which
// is itself `#[cfg(feature = "alloc")]`. Without the
// feature in the gate, a plain `cargo test -p nros-node` compiles this module
// into a no_std crate and produces 252 errors, 192 of them "cannot find module
// or crate `std`" — an incomprehensible wall for anyone running the obvious
// command, and unrelated to whatever they were changing.
//
// These tests genuinely need a host: they spawn threads, sleep, and read the
// wall clock. Gating them is not hiding coverage, it is declaring what they
// already required. `cargo test -p nros-node --features alloc` runs them; the
// test harness has `std` whatever the features say (phase-359 W10).
#[cfg(all(test, feature = "alloc", not(feature = "rmw-cffi")))]
mod tests;

// phase-444 — the two `wait_for_*` decisions, in their own file rather
// than appended to `tests.rs`. Same gate, same fixture (`MockSession`);
// separate because what they pin is a CONTRACT choice, not executor
// mechanics, and a reader looking for "why does Unsupported not wait?"
// should not have to find it inside a 10 000-line file.
#[cfg(all(test, feature = "alloc", not(feature = "rmw-cffi")))]
mod graph_wait_tests;

// Flat re-exports so users write `executor::Executor` etc.
#[cfg(any(has_rmw, test))]
pub use action::{
    ActionClientRawHandle, ActionServerHandle, ActionServerRawHandle, RawActionClientSpec,
    RawActionServerSpec,
};
#[cfg(any(has_rmw, test))]
pub use action_core::{ActionClientCore, ActionServerCore, RawActiveGoal, action_channel_type};
// phase-430 W4 — DEFINED in `crate::timer` (ungated) and re-exported here, so
// `nros_node::executor::TimerClockSource` keeps resolving for every caller
// phase-425 wrote while the declarative metadata layer, which builds with no
// transport, can name the same enum instead of inventing a second one.
pub use crate::timer::{TimerCallInfo, TimerClockSource};
#[cfg(any(has_rmw, test))]
pub use arena::TimerOverrunPolicy;
#[cfg(any(has_rmw, test))]
pub use handles::*;
#[cfg(any(has_rmw, test))]
// `Node` joined this line in 2026-09-12 (issue 1351). It was the only one of
// the three that `mod node` kept to itself, and it was not a smaller surface for
// it: `Executor::node_mut` is `pub` and RETURNS a `Node`, the `nros` crate's
// own first doc example is `executor.node_mut(node).create_publisher::<Int32>(…)`,
// and 15 in-tree binaries call its methods. What the missing `pub use` bought
// was that the type had no PATH — a user could call all ~25 methods and could
// not write `fn setup(ctx: &mut Node)` to factor two of them out, and the
// Rust parity extractor (which walks the umbrella's reachable surface) had never
// produced a single row for any of them.
pub use node::{CallbackGroup, Node, NodeHandle};
#[cfg(any(has_rmw, test))]
pub use node_record::{NodeBuilder, NodeId, NodeRecord};
#[cfg(any(has_rmw, test))]
pub use spin::Executor;
// phase-509 P9b item 2 -- a band worker over a shared executor.
#[cfg(any(has_rmw, test))]
pub use scheduler::{Band, BandWaitFn, BandWaiter, BandWorker, WorkerWakeFn};
// phase-436 — the wake-source seam's types. `Executor::last_park` RETURNS a
// `WakeSourceId` and `register_wake_source` / `set_park_primitive` TAKE the two
// fn-pointer types, so all four are already public API; without these a caller
// outside the crate could receive a value it cannot name or match on.
pub use launch_remap::{BadLaunchRemap, LaunchRemap, launch_remaps, set_launch_remaps};
#[cfg(any(has_rmw, test))]
pub use spin::SessionHandle;
#[cfg(all(any(has_rmw, test), feature = "rmw-cffi"))]
pub use spin::SessionSpec;
#[cfg(any(has_rmw, test))]
pub use spin::{MAX_WAKE_SOURCES, NextDeadlineFn, ParkUntilFn, RemapRefused, WakeSourceId};
#[cfg(any(has_rmw, test))]
pub use storage::{
    ExecutorInlineStorage, ExecutorSizing, INLINE_BACKING_U64S, INLINE_SIZING, RegionUnit,
    RegionUnits, executor_storage_layout, executor_storage_layout_with, executor_storage_u64_len,
    tier_backing_bytes, tier_executor_sizing, tier_sizing,
};
pub use types::*;
