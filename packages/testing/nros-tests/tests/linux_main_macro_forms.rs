//! `nros::main!()` proc-macro round-trip — the accepted invocation forms
//! compile.
//!
//! 1. `nros::main!();` — reads `[package.metadata.nros.entry] deploy = "native"`,
//!    emits `::demo_entry::register(runtime)?;` (lib-self bringup).
//! 2. (retired) `nros::main!(board = X)` — phase-484 W8c made it a compile
//!    error; its verdict is `linux_main_macro_misuse`'s `main_macro_misuse_board_arg`.
//! 3. `nros::main!(model = "demo_bringup");` — pkg-index walk → committed
//!    SystemModel → `::talker_pkg::register(runtime)?;` (the canonical form).
//! 4. explicit model file (the deprecated `model =` arm).
//!
//! The compile proof lives in the **build stage**: each form is a compile-check
//! fixture (`scripts/build/compile-check-fixtures.sh`, run by
//! `build-test-fixtures`) — it stages the `n9_workspace` template, writes the
//! form's `demo_entry/src/main.rs`, runs `cargo check`, and stamps `.compile-ok`
//! on success. These tests assert the stamps rather than running `cargo check`
//! at run time (issue 0034 / AGENTS.md "No compilation inside tests").

fn assert_form(id: &str) -> nros_tests::TestResult<()> {
    let stamp = nros_tests::fixtures::require_compile_check(id)?;
    assert!(
        stamp.exists(),
        "compile-check stamp missing for {id}: {}",
        stamp.display()
    );
    Ok(())
}

#[test]
fn main_macro_form1_no_args_compiles() -> nros_tests::TestResult<()> {
    assert_form("main_macro_form1")
}

#[test]
fn main_macro_form3_model_default_compiles() -> nros_tests::TestResult<()> {
    assert_form("main_macro_form3")
}

#[test]
fn main_macro_form4_all_explicit_compiles() -> nros_tests::TestResult<()> {
    assert_form("main_macro_form4")
}
