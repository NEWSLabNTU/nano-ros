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
/// # The ratio is for an UNDECLARED image only (issue 1290)
///
/// The scaling above reads `ARENA_SIZE` as a budget for `MAX_CBS` slots, so
/// `ARENA_SIZE / MAX_CBS` is one slot's worth and `cbs` of them is `cbs` slots'
/// worth. That is exactly what `ARENA_SIZE` is while nothing declared: the
/// derivation budgets `max_cbs` worst-case entries plus a base overhead.
///
/// It stopped being that in phase-403 step 3 for an image that DECLARES its
/// entities. There `nros-node/build.rs` sums the model per KIND — subscriptions
/// at their own depth and type, timers at 64 bytes, services, action clients —
/// and `MAX_CBS` is a separate knob that takes no part. Dividing by it then
/// divides by a number with no relationship to the sum, and multiplying by
/// `cbs` scales a total that was never per-slot. Measured on
/// `packages/testing/nros-bench/large-msg-baremetal` with one declared
/// subscription: the model is 14,424 bytes and `arena_size_for(2)` returned
/// 7,212. Half.
///
/// So when the image declares, this asks the model: `ARENA_SIZE` IS the model
/// (floored, and `executor::arena_oracle` refuses at compile time to let a
/// stated knob sit below it), and every per-entry executor gets all of it.
///
/// **That over-provisions a TIERED boot**, where one tier holds a subset of the
/// image and now carries the whole model. Deliberate, and the direction is the
/// safe one: the alternative needs the ENTRY's own per-kind counts where the
/// macro emits the sizing, and those do not reach it today (issue 1290 records
/// the choice). An entry that wants less states its own
/// [`ExecutorSizing`] and holds it to what it registers with
/// [`ExecutorSizing::assert_covers_model`], which is a compile-time check the
/// ratio never had.
///
/// [`ExecutorSizing`]: crate::executor::ExecutorSizing
/// [`ExecutorSizing::assert_covers_model`]: crate::executor::ExecutorSizing::assert_covers_model
pub const fn arena_size_for(cbs: usize) -> usize {
    // `!= 0` and not `> 0`: on a build that declares nothing the generated
    // const IS the literal `0`, and clippy's `absurd_extreme_comparisons`
    // const-propagates it and refuses `> 0` as always-false under
    // `-D warnings`. Same predicate, and it reads the same way -- `0` means
    // "no model", never "a model of nothing".
    if arena_model::REQUIRED != 0 {
        return ARENA_SIZE;
    }
    let scaled = (cbs * ARENA_SIZE).div_ceil(if MAX_CBS == 0 { 1 } else { MAX_CBS });
    // Never below one default slot's worth so a tiny `cbs` still has headroom for
    // the base overhead the derivation folds into `ARENA_SIZE`.
    let floor = ARENA_SIZE.div_ceil(if MAX_CBS == 0 { 1 } else { MAX_CBS });
    if scaled < floor { floor } else { scaled }
}

#[cfg(test)]
mod arena_size_for_tests {
    use super::{ARENA_SIZE, MAX_CBS, arena_model, arena_size_for};

    /// Every number in this module goes through here, and that is not style.
    ///
    /// The build script emits `arena_model::REQUIRED` as a LITERAL, and on a
    /// build that declares nothing the literal is `0` — the minimum of
    /// `usize`. clippy then const-folds every comparison against it and
    /// `absurd_extreme_comparisons` / `assertions_on_constants` refuse the
    /// whole module under `-D warnings`, which is what `check-workspace-all`
    /// runs. `black_box` hides only the CONSTANT-ness; the value, and every
    /// assertion's meaning, is unchanged. Deleting it does not weaken a test,
    /// it stops the crate compiling.
    fn opaque(v: usize) -> usize {
        core::hint::black_box(v)
    }

    /// issue 1290 — a per-entry arena must cover the model the image declares.
    ///
    /// This is the property `arena_size_for` violated: it scaled `ARENA_SIZE`
    /// by `cbs / MAX_CBS` on an image where `ARENA_SIZE` is a per-KIND sum and
    /// `MAX_CBS` takes no part in it, so the image's only executor got a
    /// fraction of its own requirement.
    ///
    /// It is only a claim about a build that DECLARED — `REQUIRED` is 0
    /// otherwise, and then the ratio is the right arithmetic over the right
    /// number. The `node-std-tests` lane runs this test a second time with the
    /// reference island's declared shape in the environment, which is what
    /// makes the first branch reachable at all; the `assert!` on `REQUIRED`
    /// there is what stops that run passing vacuously.
    #[test]
    fn a_per_entry_arena_covers_the_declared_model() {
        if opaque(arena_model::REQUIRED) == 0 {
            // Nothing declared: `ARENA_SIZE` is a budget over `MAX_CBS` slots,
            // so the ratio is meaningful and a full-width entry gets all of it.
            assert_eq!(opaque(arena_size_for(MAX_CBS)), opaque(ARENA_SIZE));
            return;
        }
        for cbs in 0..=MAX_CBS.max(1) * 2 {
            assert!(
                opaque(arena_size_for(cbs)) >= opaque(arena_model::REQUIRED),
                "arena_size_for({cbs}) = {} is below the {} bytes this image's \
                 declared entities are modelled at — every registration past it \
                 fails with BufferTooSmall (issue 1290)",
                arena_size_for(cbs),
                arena_model::REQUIRED,
            );
        }
    }

    /// The POSITIVE CONTROL, and the reason the test above is not free.
    ///
    /// `#[ignore]`d because it can only say anything in a build that DECLARED,
    /// and a `cargo test` of this crate declares nothing. `node-std-tests` runs
    /// it with the reference island's shape in the environment; the first
    /// assertion is what stops that run passing vacuously if the declaration
    /// ever stops arriving.
    ///
    /// The arithmetic this replaced is RESTATED here — the one place in the
    /// tree it may be, because what is being asserted is precisely that it was
    /// WRONG. Without it a reader cannot tell whether the property above holds
    /// because the fix works or because the numbers never collided.
    #[test]
    #[ignore = "needs NROS_ENTITY_COUNT_* in the build environment; run by `just check node-std-tests`"]
    fn the_scaling_this_replaced_was_short_of_the_declared_model() {
        assert!(
            opaque(arena_model::REQUIRED) > opaque(arena_model::FLOOR),
            "this build declared no entities (REQUIRED = {}, FLOOR = {}), so \
             nothing below is a claim about anything. Set NROS_ENTITY_COUNT_* \
             for the build, as `just check node-std-tests` does (issue 1290).",
            arena_model::REQUIRED,
            arena_model::FLOOR,
        );
        // Above the floor and with no knob stated, the derived arena IS the
        // model — which is what makes the comparison below arithmetic rather
        // than luck.
        assert_eq!(opaque(ARENA_SIZE), opaque(arena_model::REQUIRED));
        assert!(
            opaque(MAX_CBS) >= 2,
            "the ratio is only a ratio for MAX_CBS >= 2"
        );

        let divisor = MAX_CBS;
        let scaled = |cbs: usize| {
            let s = (cbs * ARENA_SIZE).div_ceil(divisor);
            let floor = ARENA_SIZE.div_ceil(divisor);
            if s < floor { floor } else { s }
        };
        // One slot short of the full width is where it bit: the bench sizes its
        // ONE executor with `arena_size_for(2)` against a `MAX_CBS` of 4.
        let cbs = MAX_CBS - 1;
        assert!(
            opaque(scaled(cbs)) < opaque(arena_model::REQUIRED),
            "the cbs/MAX_CBS scaling gave {} for cbs={cbs} against a declared \
             model of {} — it was not short here, so this test is asserting \
             nothing",
            scaled(cbs),
            arena_model::REQUIRED,
        );
        assert!(
            opaque(arena_size_for(cbs)) >= opaque(arena_model::REQUIRED),
            "arena_size_for({cbs}) = {} still does not cover the declared model \
             of {} (issue 1290)",
            arena_size_for(cbs),
            arena_model::REQUIRED,
        );
    }
}
