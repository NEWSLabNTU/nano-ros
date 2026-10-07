//! Platform-header compile gate (RFC-0042 D4) — was `tests/platform_header_matrix.rs`.
//!
//! The recurring libc/std-header + capability-macro class (issues #27/#36/#38)
//! reached `main` because nothing on the PR path compiled the C/C++ platform
//! headers — they were exercised only days late by the e2e `build-fixtures`
//! matrix. This gate compiles the canonical `<nros/platform.h>` + the nros-cpp
//! heap containers for the platform×capability combinations that are
//! host-compilable, asserting both positive AND negative outcomes.
//!
//! ## phase-329 W5 — compile out of test
//!
//! The old file drove HOST `g++`/`cc` over a local `const CELLS` matrix at test
//! time (10 cells). W5 splits that:
//!
//! * **The 9 POSITIVE cells moved to the BUILD stage** as `cxx-syntax`
//!   `compile_check_fixture` rows (`platform_hdr_*`, snippets under
//!   `fixtures/cpp_compat_snippets/` with each cell's `-D` defines baked in — the
//!   shared cxx-syntax builder takes no per-row defines). This test now CONSUMES
//!   their `.compile-ok` stamps via `require_compile_check`: a header regression
//!   leaves `.build-failed` beside the stamp and reds here in EVERY tier
//!   (`require_prebuilt_binary_fresh` distinguishes "build ran and failed" from
//!   "toolchain absent"). The local `CELLS` matrix is gone (phase-329 W6 keeps
//!   axis tables in `matrix.rs`/`interop.rs`); what remains is a plain id list.
//!   One positive cell (`platform_hdr_posix_c`) was a `cc -std=c11` check; under
//!   the cxx-syntax builder it is `c++ -std=c++14`, which still hard-errors on an
//!   undeclared malloc surface, so its intent (the canonical C header parses + the
//!   POSIX malloc surface is present) is preserved.
//!
//! * **The 1 NEGATIVE cell is a VERDICT row** (issue 1656) — bare-metal heap
//!   WITHOUT malloc MUST FAIL to compile. It stayed a runtime `g++` for as long
//!   as "a must-fail compile cannot be a passing prebuilt" was believed; issue
//!   1620 retired that premise for cargo and cmake, and the `cxx-syntax-verdict`
//!   builder retires it here: the build stage runs the SAME compile over the same
//!   include set, records its exit status and stderr, and this test asserts the
//!   recorded verdict (`platform_hdr_baremetal_heap_no_malloc`).
//!
//! The two-libc-set class (#27/#36) stays cross-only (it needs the RTOS sysroot +
//! `#include_next`, which only bites the platform `.c` TUs) — see the e2e lane.

use nros_tests::TestResult;

/// The build-stage POSITIVE cells — one `cxx-syntax` `compile_check_fixture` each,
/// the snippet baking the platform `-D` define. NOT a matrix axis table (phase-329
/// W6): a plain list of the fixture ids this consumer asserts.
const POSITIVE_SNIPPET_IDS: &[&str] = &[
    "platform_hdr_posix_cpp_heap",
    "platform_hdr_posix_c",
    "platform_hdr_baremetal_has_malloc",
    "platform_hdr_baremetal_core",
    "platform_hdr_freertos",
    "platform_hdr_zephyr",
    "platform_hdr_threadx",
    "platform_hdr_nuttx",
    "platform_hdr_esp",
];

/// Every positive platform-header cell compiled clean at the build stage. A
/// regression (a dropped/duplicated canonical malloc surface, a capability
/// special-case that wrongly withholds malloc for one platform — the #42
/// root-cause #5 gap) leaves `.build-failed` and reds here.
#[test]
fn platform_headers_compile_per_capability() -> TestResult<()> {
    for id in POSITIVE_SNIPPET_IDS {
        let stamp = nros_tests::fixtures::require_compile_check(id)?;
        assert!(
            stamp.exists(),
            "compile-ok stamp missing for `{id}`: {}",
            stamp.display()
        );
    }
    Ok(())
}

/// #38 negative gate — bare-metal default is `NROS_NO_DYNAMIC_MEMORY`, so the
/// canonical malloc/free are ABSENT and the heap containers MUST NOT compile. Both
/// directions of #38 are thus asserted (this + the `platform_hdr_baremetal_has_malloc`
/// positive fixture), so a regression in either the gate or the fix is caught.
///
/// The snippet (`fixtures/cpp_compat_snippets/platform_hdr_baremetal_heap_no_malloc.cpp`)
/// is the positive twin minus `NROS_PLATFORM_HAS_MALLOC`, compiled by the same
/// builder over the same include set, so a failure for any OTHER reason (a stray
/// missing header) is refused by the second assertion rather than read as the gate.
#[test]
fn baremetal_heap_without_malloc_must_not_compile() -> TestResult<()> {
    let v = nros_tests::fixtures::require_compile_verdict("platform_hdr_baremetal_heap_no_malloc")?;
    assert!(
        !v.outcome.success(),
        "bare-metal heap containers COMPILED without NROS_PLATFORM_HAS_MALLOC — the \
         #38 capability gate regressed (nros_platform_malloc/free leaked into the \
         no-dynamic-memory default)"
    );
    assert!(
        v.outcome.stderr.contains("nros_platform_malloc")
            || v.outcome.stderr.contains("nros_platform_free"),
        "the no-malloc snippet failed, but not on the missing allocator — fix the \
         snippet or the include set before believing the #38 gate:\n{}",
        v.outcome.stderr
    );
    Ok(())
}
