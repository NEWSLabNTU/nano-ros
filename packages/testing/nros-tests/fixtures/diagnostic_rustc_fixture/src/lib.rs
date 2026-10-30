// Phase 212 §Acceptance — deliberate rustc error[E0432].
//
// The unresolved-import diagnostic emitted here MUST reach the user's
// terminal verbatim per the §Non-Goals contract — no aggregation, no
// truncation. The `diagnostic_rustc_verbatim` fixture row runs `cargo check`
// against this crate in the build stage and records the verdict;
// `diagnostic_verbatim::rustc_diagnostic_verbatim` (once
// ~~`phase212_diagnostic_verbatim::rustc_diagnostic_verbatim`~~) greps its
// stderr for the exact `error[E0432]: unresolved import` prefix.
use nonexistent_crate_for_phase212_verbatim_test::nothing;
