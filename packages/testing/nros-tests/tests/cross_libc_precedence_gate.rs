//! Phase 241.A (RFC-0042 D4) — **cross tier** of the merge-time platform gate.
//!
//! The host tier (`platform_header_matrix.rs`) catches the #38 capability class
//! but CANNOT see the two-libc-set class (#27/#36): that one needs the **cross
//! toolchain** (arm-none-eabi, its own newlib) plus an RTOS sysroot header on the
//! include path. A platform `.c`/`.cpp` TU then pulls TWO `<stdlib.h>`s with
//! incompatible `div_t` shapes (the RTOS's NAMED `struct div_s` vs newlib's
//! ANONYMOUS typedef) and the C++ compile dies on `conflicting declaration
//! '…div_t'`. The fix (commits `812234321`/`7b0517121`) makes the RTOS sysroot
//! win — `${RTOS}/include/cxx` prepended / SYSTEM precedence — so `<cstdlib>`
//! resolves to the RTOS wrapper and only one `div_t` exists.
//!
//! This gate reproduces the class **self-contained** (a minimal RTOS-header stub
//! under `fixtures/cross_libc_precedence/`, no RTOS submodule) so it is cheap and
//! runs anywhere the cross toolchain is provisioned (`just nuttx setup` / the SDK
//! `arm-none-eabi-gcc`). It is a RELATIVE assertion, robust to toolchain version:
//!   * compile the probe with the RTOS sysroot NOT winning `<cstdlib>` (plain
//!     `-I`). If it compiles anyway, this toolchain's newlib `div_t` does not
//!     conflict → the class is not reproducible here → **skip**.
//!   * if it clashes (the class IS live), compile with the RTOS `include/cxx`
//!     prepended (the fix). That MUST compile — else the include-precedence wiring
//!     that keeps the RTOS sysroot winning has regressed (#27/#36 back on main).
//!
//! So the gate goes red exactly when a PR reintroduces the two-libc precedence
//! bug, on the PR — not days later in an on-demand e2e build.
//!
//! ## issue 1656 — the three compiles are build-stage VERDICTS
//!
//! This file used to run the cross g++ itself (a capability probe and the two
//! probe compiles). They are now `cxx-compile-verdict` rows in
//! `examples/fixtures.toml` over `tests/fixtures/cross_libc_precedence/`, each
//! with its argument list as a file (`cases/<id>.args`); the build stage
//! resolves `arm-none-eabi-g++` (SDK store, then PATH), records each compile's
//! exit status and stderr, and records `absent` when there is no cross compiler
//! at all. The RELATIVE logic stays here, where an assertion belongs.

use nros_tests::TestResult;

#[test]
fn cross_libc_two_set_precedence_holds() -> TestResult<()> {
    let cap = nros_tests::fixtures::require_compile_verdict("cross_libc_cxx_stdlib_probe")?;
    if cap.tool_absent() {
        nros_tests::skip!(
            "cross toolchain arm-none-eabi-g++ not provisioned — run `just nuttx setup` \
             (the #27/#36 two-libc gate needs the cross newlib); rebuild the \
             `cross_libc_*` compile-check rows after provisioning it"
        );
    }
    let gxx = cap.tool.clone().unwrap_or_default();

    // 0. Toolchain capability: the probe needs libstdc++ (`<type_traits>` /
    //    `<cstdlib>`). A C-only newlib cross can't compile it — that is an
    //    unmet precondition, not the #27/#36 clash. Skip rather than false-fail.
    if !cap.outcome.success() {
        nros_tests::skip!(
            "cross toolchain ({gxx}) has no usable libstdc++ (`<type_traits>`/`<cstdlib>` \
             absent) — the #27/#36 two-libc gate needs a C++-capable newlib cross"
        );
    }

    // 1. Broken precedence (RTOS sysroot reachable but not winning <cstdlib>).
    let broken =
        nros_tests::fixtures::require_compile_verdict("cross_libc_rtos_sysroot_not_first")?;
    if broken.outcome.success() {
        nros_tests::skip!(
            "cross toolchain ({gxx}) newlib `div_t` does not conflict with the RTOS-shape \
             decl — the #27/#36 two-libc class is not reproducible on this toolchain; \
             nothing to gate"
        );
    }
    // Sanity: the failure must be the two-libc clash we model, not an unrelated
    // error (a broken stub/probe would falsely "pass" the negative direction).
    assert!(
        models_two_libc_clash(&broken.outcome.stderr),
        "broken-precedence compile failed for a reason OTHER than the modelled \
         two-libc clash — fix the gate fixture, do not assume the precedence \
         bug:\n{}",
        broken.outcome.stderr
    );

    // 2. With the RTOS `include/cxx` prepended (the #27/#36 fix), the SAME probe
    //    MUST compile — that is the invariant the platform build wiring upholds.
    let fixed = nros_tests::fixtures::require_compile_verdict("cross_libc_rtos_sysroot_first")?;
    assert!(
        fixed.outcome.success(),
        "phase-241.A cross gate: the RTOS-cxx-first include precedence no longer clears \
         the #27/#36 two-libc `div_t` clash — the SYSTEM/`include/cxx` precedence that \
         keeps the RTOS sysroot winning has regressed (see nuttx_ffi_build.rs / the NuttX \
         NanoRos cmake SYSTEM include):\n{}",
        fixed.outcome.stderr
    );
    Ok(())
}

/// Does this compile log show the RTOS `stdlib.h` winning over the cross
/// newlib's — the #27/#36 two-libc clash — rather than some unrelated error?
///
/// Issue 0995. It has TWO manifestations, and which one you get depends on
/// which cross toolchain is installed:
///
///   * SDK store (`~/.nros/sdk/arm-none-eabi-gcc/13.2-nros1`, newlib 13.2.1):
///     newlib's own `stdlib.h` is reached FIRST, so the stub's is a
///     redefinition —
///     error: conflicting declaration 'typedef struct div_s div_t'
///
///   * the CI container's apt cross (newlib 10.3.1): the stub's `stdlib.h` is
///     reached INSTEAD of newlib's, so newlib's `<cstdlib>` finds nothing to
///     re-export —
///     /usr/include/newlib/c++/10.3.1/bits/std_abs.h:52:11:
///     error: 'abs' has not been declared in '::'
///
/// Both are the stub shadowing the real libc; only the first was modelled, so
/// the gate failed on the container with "fix the gate fixture" — correctly
/// refusing to conclude, and correctly telling us the fixture was the problem.
fn models_two_libc_clash(log: &str) -> bool {
    let lower = log.to_lowercase();
    // Manifestation 1: a redefinition, naming the type the stub redeclares.
    if lower.contains("div_t") && lower.contains("conflict") {
        return true;
    }
    // Manifestation 2: the C++ `<cstdlib>` chain cannot find the C names it
    // re-exports, because the stub's header replaced the one that declares
    // them. Keyed on BOTH halves so an unrelated "not declared" elsewhere does
    // not qualify.
    let from_cstdlib_chain = lower.contains("cstdlib") || lower.contains("std_abs.h");
    let missing_c_names = lower.contains("has not been declared in");
    from_cstdlib_chain && missing_c_names
}

#[test]
fn the_clash_predicate_accepts_both_toolchains_and_rejects_noise() {
    // Issue 0995 — REAL logs, not paraphrases: the first from this host's SDK
    // cross, the second copied from the CI run that failed (33654481082).
    let sdk_cross = "\
rtos-stub/include/stdlib.h:19:23: error: conflicting declaration 'typedef struct div_s div_t'
   19 | typedef struct div_s  div_t;
.../c++/13.2.1/cstdlib:79: note: previous declaration as 'typedef struct div_t div_t'";
    assert!(
        models_two_libc_clash(sdk_cross),
        "the div_t redefinition is the originally modelled manifestation"
    );

    let apt_cross = "\
In file included from /usr/include/newlib/c++/10.3.1/cstdlib:77,
                 from .../cross_libc_precedence/probe.cpp:9:
/usr/include/newlib/c++/10.3.1/bits/std_abs.h:52:11: error: 'abs' has not been declared in '::'
   52 |   using ::abs;";
    assert!(
        models_two_libc_clash(apt_cross),
        "the container's newlib shadows the other way and must also qualify"
    );

    // A genuinely unrelated failure must still fail the gate — that is the
    // whole point of the sanity check.
    assert!(
        !models_two_libc_clash("probe.cpp:3:10: fatal error: nowhere.h: No such file or directory"),
        "an unrelated error must NOT be read as the clash"
    );
    assert!(
        !models_two_libc_clash("error: 'frobnicate' has not been declared in '::'"),
        "a `not declared` outside the cstdlib chain must NOT qualify"
    );
}
