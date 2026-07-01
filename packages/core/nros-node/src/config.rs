//! Build-time configurable constants.
//!
//! Values are set via environment variables at build time.
//! See build.rs for env var names and defaults.

include!(concat!(env!("OUT_DIR"), "/nros_node_config.rs"));

/// phase-271 — default arena bytes for a per-entry executor holding `cbs`
/// callback slots. Scales the build-time [`ARENA_SIZE`] (which is sized for the
/// default [`MAX_CBS`]) linearly by `cbs`, so a per-entry [`ExecutorSizing`] with
/// a caller-chosen `cbs` gets the same per-slot arena budget the global default
/// used — a fat entry (more callbacks) gets a larger arena, a lean one a smaller
/// one — without a workspace-global `NROS_EXECUTOR_ARENA_SIZE`. Floored at the
/// full default so no per-entry executor is ever smaller than a single-slot
/// build would have been (the arena is a worst-case upper bound; over-provision
/// is safe, under-provision fails entity creation).
///
/// [`ExecutorSizing`]: crate::executor::ExecutorSizing
pub const fn arena_size_for(cbs: usize) -> usize {
    let scaled = (cbs * ARENA_SIZE).div_ceil(if MAX_CBS == 0 { 1 } else { MAX_CBS });
    // Never below one default slot's worth so a tiny `cbs` still has headroom for
    // the base overhead the derivation folds into `ARENA_SIZE`.
    let floor = ARENA_SIZE.div_ceil(if MAX_CBS == 0 { 1 } else { MAX_CBS });
    if scaled < floor { floor } else { scaled }
}
