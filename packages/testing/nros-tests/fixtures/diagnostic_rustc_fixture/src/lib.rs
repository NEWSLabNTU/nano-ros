// Phase 212 §Acceptance — deliberate rustc error[E0432].
//
// The unresolved-import diagnostic emitted here MUST reach the user's
// terminal verbatim per the §Non-Goals contract — no aggregation, no
// truncation. `phase212_diagnostic_verbatim::rustc_diagnostic_verbatim`
// invokes `cargo check` against this fixture and greps stderr for the
// exact `error[E0432]: unresolved import` prefix.
use nonexistent_crate_for_phase212_verbatim_test::nothing;
