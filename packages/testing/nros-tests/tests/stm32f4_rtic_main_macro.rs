//! `nros::main!()` RTIC macro expansion for an stm32f4 Entry pkg type-checks.
//!
//! The macro emit for an Entry carrying `[package.metadata.nros.entry] deploy =
//! "rtic-stm32f4"` produces a `#[rtic::app(device = stm32f4xx_hal::pac,
//! dispatchers = [...])] mod app` skeleton delegating init to
//! `<RticStm32F4 as RticBoardEntry>::init_hardware`, plus the `__nros_spin` /
//! `__nros_dispatch` task sidekicks.
//!
//! The proof that the emit compiles is the **build** of the
//! `examples/stm32f4/rust/talker-rtic` fixture (`examples/fixtures.toml` →
//! `stm32f4-rtic-talker`), which `build-test-fixtures` / `just stm32f4
//! build-fixtures` compiles for `thumbv7em-none-eabihf`. If the macro expansion
//! didn't type-check, that fixture build fails in the build stage.
//!
//! This test consumes the prebuilt artifact rather than running `cargo check`
//! at run time — see issue 0034 and AGENTS.md "No compilation inside tests".
//! Fixture absence is handled by the shared resolver: a hard failure in the
//! full `test-all` tier (real gap → `just build-test-fixtures`), a `[SKIPPED]`
//! under `NROS_FIXTURES_OPTIONAL=1` (light tier without the cross toolchain).
//!
//! Implements Phase 216.B Acceptance #6.

#[test]
fn rtic_main_macro_expansion_builds() -> nros_tests::TestResult<()> {
    let bin = nros_tests::fixtures::build_rtic_talker()?;
    assert!(
        bin.exists(),
        "stm32f4-rtic-talker fixture path does not exist: {}",
        bin.display()
    );
    Ok(())
}
