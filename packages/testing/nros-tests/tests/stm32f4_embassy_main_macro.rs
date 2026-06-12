//! `nros::main!()` Embassy macro expansion for an stm32f4 Entry pkg type-checks
//! (Phase 216.C.4).
//!
//! The macro emit for an Entry carrying `deploy = "embassy-stm32f4"` produces an
//! `#[embassy_executor::main]`-shaped skeleton. The proof that it type-checks is
//! a build-stage `cargo check --target thumbv7em-none-eabihf` of
//! `examples/stm32f4/rust/talker-embassy` — a CHECK (not a link), because that
//! example intentionally doesn't link standalone (missing board memory layout).
//!
//! The check runs in the **build stage** — the `embassy_main_macro` cargo-check
//! fixture (`compile-check-fixtures.sh`, run by `build-test-fixtures`) stamps
//! `.compile-ok` on success. This test asserts the stamp rather than running
//! `cargo check` at run time (issue 0034 / AGENTS.md "No compilation inside
//! tests"). Absent (cross target not installed / check failed) → tier-aware
//! skip/fail via the resolver.

#[test]
fn embassy_main_macro_expansion_compiles() -> nros_tests::TestResult<()> {
    let stamp = nros_tests::fixtures::require_compile_check("embassy_main_macro")?;
    assert!(
        stamp.exists(),
        "compile-check stamp missing: {}",
        stamp.display()
    );
    Ok(())
}
