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
//! * **The 1 NEGATIVE cell stays runtime** — bare-metal heap WITHOUT malloc MUST
//!   FAIL to compile, and a must-fail compile can never be a passing prebuilt
//!   fixture. It is a sanctioned runtime FAIL-path, listed in the
//!   negative-diagnostic registry (`tests/negative_diagnostic_registry.rs`).
//!
//! The two-libc-set class (#27/#36) stays cross-only (it needs the RTOS sysroot +
//! `#include_next`, which only bites the platform `.c` TUs) — see the e2e lane.

use nros_tests::TestResult;
use std::{path::PathBuf, process::Command};

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

/// A C++ TU forcing the heap containers' allocator calls: `HeapString` instantiates
/// its dtor (`nros_platform_free`), `HeapSequence<int>::reserve/push_back`
/// references `nros_platform_malloc`. Absent the canonical malloc/free it fails to
/// compile — the #38 mechanism.
const HEAP_PROBE: &str = r#"
#include <nros/heap_string.hpp>
#include <nros/heap_sequence.hpp>
namespace {
void use_it() {
    nros::HeapString s;
    (void)s;
    nros::HeapSequence<int> q;
    q.reserve(4);
    q.push_back(1);
    (void)q;
}
} // namespace
"#;

/// The include set the cxx-syntax build stage uses, so the negative cell fails for
/// the SAME reason (missing malloc) the positive cells would — not a stray missing
/// header. The two generated dirs are prepended when present (the stub config
/// header in nros-cpp/include `#error`s if reached first).
fn builder_includes(root: &std::path::Path) -> Vec<PathBuf> {
    let mut inc = Vec::new();
    let gen_cpp = root.join("target/nros-cpp-generated");
    if gen_cpp.join("nros/nros_cpp_config_generated.h").is_file() {
        inc.push(gen_cpp);
    }
    let gen_c = root.join("target/nros-c-generated");
    if gen_c.join("nros/nros_config_generated.h").is_file() {
        inc.push(gen_c);
    }
    inc.push(root.join("packages/platform/nros-platform-api/include"));
    inc.push(root.join("packages/api/nros-cpp/include"));
    inc.push(root.join("packages/api/nros-c/include"));
    inc.push(root.join("cmake/compat/include"));
    inc
}

/// #38 negative gate — bare-metal default is `NROS_NO_DYNAMIC_MEMORY`, so the
/// canonical malloc/free are ABSENT and the heap containers MUST NOT compile. Both
/// directions of #38 are thus asserted (this + the `platform_hdr_baremetal_has_malloc`
/// positive fixture), so a regression in either the gate or the fix is caught.
///
/// Stays runtime (a must-fail compile can't be a passing prebuilt) — sanctioned
/// FAIL-path, negative-diagnostic registry member.
#[test]
fn baremetal_heap_without_malloc_must_not_compile() {
    assert!(
        Command::new("g++").arg("--version").output().is_ok(),
        "g++ not found — the platform-header negative gate cannot run"
    );
    let root = nros_tests::project_root();
    let tmp = tempfile::tempdir().unwrap();
    let src = tmp.path().join("probe.cpp");
    std::fs::write(&src, HEAP_PROBE).unwrap();

    let mut cmd = Command::new("g++");
    cmd.args([
        "-std=c++14",
        "-fno-exceptions",
        "-fno-rtti",
        "-fsyntax-only",
    ]);
    for i in builder_includes(&root) {
        cmd.arg("-I").arg(i);
    }
    cmd.arg("-DNROS_PLATFORM_BAREMETAL").arg(&src);

    let ok = cmd
        .output()
        .expect("spawn g++ for the platform-header negative gate")
        .status
        .success();
    assert!(
        !ok,
        "bare-metal heap containers COMPILED without NROS_PLATFORM_HAS_MALLOC — the \
         #38 capability gate regressed (nros_platform_malloc/free leaked into the \
         no-dynamic-memory default)"
    );
}
