//! Where the executor arena's floor applies -- issue 1971, RFC-0107 step 2.
//!
//! `build.rs` includes this file with `#[path]`, and so does
//! `tests/arena_floor.rs`, so the rule the build runs is the rule the test
//! checks. It must stay free of anything only a build script has (`env`,
//! `OUT_DIR`).
//!
//! The floor is POLICY for an image whose demand is UNKNOWN (issue 1190, the
//! comment above the arithmetic in `build.rs`): nobody declared the entities,
//! so the build budgets a worst case over `max_cbs` slots and will not hand
//! out less than [`ARENA_FLOOR`]. Where the demand is DERIVED -- every entity
//! count declared, for the image or for one executor tier -- the arena is the
//! model, unfloored. The model's terms are asserted upper bounds
//! (`executor/arena.rs`), `executor::arena_oracle` refuses at compile time a
//! stated arena below `arena_model::REQUIRED`, and a short arena is still
//! `NodeError::BufferTooSmall` naming the registration. Flooring a derived
//! arena only charged it up to 8 KiB it was modelled not to need, once per
//! tier on a tiered image.

/// The smallest arena the derivation hands out when the demand is unknown.
pub(crate) const ARENA_FLOOR: usize = 8192;

/// The derived arena: the model when there is one, otherwise the worst-case
/// `unknown_budget`, floored.
pub(crate) fn arena_for_demand(model: Option<usize>, unknown_budget: usize) -> usize {
    match model {
        Some(required) => required,
        None => unknown_budget.max(ARENA_FLOOR),
    }
}

/// One executor tier's arena (issue 1970's `[[tier]]` rows): its own model,
/// unfloored. `None` -- the tier's counts are not all declared -- is not
/// floored into a guess; the caller leaves the whole image on the inline
/// executor instead, sized as the image.
pub(crate) fn tier_arena(model: Option<usize>) -> Option<usize> {
    model.map(|required| arena_for_demand(Some(required), 0))
}
