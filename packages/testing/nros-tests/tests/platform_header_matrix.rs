//! Phase 241.A (RFC-0042 D4) — merge-time platform-header compile gate.
//!
//! The recurring libc/std-header + capability-macro class (issues #27/#36/#38)
//! reached `main` because nothing on the PR path compiled the C/C++ platform
//! headers — they were exercised only by the on-demand e2e `build-fixtures`
//! matrix, days late. This test is the cheap safety net: it drives the HOST
//! `g++`/`cc` over the *real* `<nros/platform.h>` + the nros-cpp heap containers
//! for the platform×capability combinations that are host-compilable, asserting
//! both positive AND negative outcomes.
//!
//! Scope (host tier). Only POSIX and bare-metal headers parse without an RTOS
//! sysroot; FreeRTOS/Zephyr/NuttX/ThreadX headers `#include <FreeRTOS.h>` /
//! `<zephyr/...>` / `tx_api.h` and need the cross toolchain + export, so they
//! stay on the e2e lane. This tier still catches:
//!   * #38-class capability gating — bare-metal WITHOUT `NROS_PLATFORM_HAS_MALLOC`
//!     MUST fail to compile the heap containers (no `nros_platform_malloc`), WITH
//!     it MUST succeed. Both directions are asserted, so a regression in either
//!     the gate or the fix is caught.
//!   * the RFC-0042 D1/D2/D3 migration churn — collapsing to one canonical
//!     header, single-sourcing the malloc/free shim, and capability-driven
//!     lowering all edit these headers; this gate fails loudly if any of them
//!     drops or duplicates the canonical surface.
//! The two-libc-set class (#27/#36) is cross-only (needs the RTOS sysroot +
//! `#include_next`); it is NOT covered here — see the e2e build-fixtures lane.

use std::{path::PathBuf, process::Command};

fn repo_root() -> PathBuf {
    // packages/testing/nros-tests -> nth(3) = repo root
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .unwrap()
        .to_path_buf()
}

#[derive(Clone, Copy)]
enum Lang {
    Cpp,
    C,
}

struct Cell {
    name: &'static str,
    lang: Lang,
    defines: &'static [&'static str],
    src: &'static str,
    expect_pass: bool,
}

/// C++ TU that forces the heap containers' allocator calls: declaring a
/// `HeapString` instantiates its dtor (`nros_platform_free`), and instantiating
/// `HeapSequence<int>` + calling `reserve`/`push_back` references
/// `nros_platform_malloc`. If the platform header does not declare the canonical
/// malloc/free, this fails to compile — exactly the #38 mechanism.
const HEAP_PROBE: &str = r#"
#include <nros/heap_string.hpp>
#include <nros/heap_sequence.hpp>
namespace {
void use_it() {
    nros::HeapString s; (void)s;
    nros::HeapSequence<int> q;
    q.reserve(4);
    q.push_back(1);
    (void)q;
}
} // namespace
"#;

/// C++ TU that uses only the non-heap platform surface (atomics) — proves the
/// bare-metal *core* compiles without a heap, so the negative heap cell isn't
/// just "the header is broken".
const CORE_PROBE: &str = r#"
#include <nros/platform.h>
namespace {
bool roundtrip(bool* p) {
    nros_platform_atomic_store_bool(p, true);
    return nros_platform_atomic_load_bool(p);
}
} // namespace
"#;

/// C TU that parses the canonical header and uses the malloc surface a POSIX
/// build provides.
const C_PROBE: &str = r#"
#include <nros/platform.h>
static void* use_it(void) {
    void* p = nros_platform_malloc(8);
    nros_platform_free(p);
    return p;
}
"#;

const CELLS: &[Cell] = &[
    // POSIX needs the feature macros every nros-platform-posix .c sets
    // (`clock_gettime`/`CLOCK_MONOTONIC` are gated behind them under strict
    // -std=c11; g++ supplies _GNU_SOURCE by default but cc does not).
    Cell {
        name: "posix/cpp/heap",
        lang: Lang::Cpp,
        defines: &[
            "NROS_PLATFORM_POSIX",
            "_POSIX_C_SOURCE=200809L",
            "_DEFAULT_SOURCE",
        ],
        src: HEAP_PROBE,
        expect_pass: true,
    },
    Cell {
        name: "posix/c/platform",
        lang: Lang::C,
        defines: &[
            "NROS_PLATFORM_POSIX",
            "_POSIX_C_SOURCE=200809L",
            "_DEFAULT_SOURCE",
        ],
        src: C_PROBE,
        expect_pass: true,
    },
    // #38 negative gate: bare-metal default is NROS_NO_DYNAMIC_MEMORY, so the
    // canonical malloc/free are absent and the heap containers MUST NOT compile.
    Cell {
        name: "baremetal/cpp/heap-no-malloc(must-fail)",
        lang: Lang::Cpp,
        defines: &["NROS_PLATFORM_BAREMETAL"],
        src: HEAP_PROBE,
        expect_pass: false,
    },
    // #38 fix gate: opting in exposes malloc/free over alloc/dealloc, so the
    // heap containers MUST compile.
    Cell {
        name: "baremetal/cpp/heap-has-malloc",
        lang: Lang::Cpp,
        defines: &["NROS_PLATFORM_BAREMETAL", "NROS_PLATFORM_HAS_MALLOC"],
        src: HEAP_PROBE,
        expect_pass: true,
    },
    // bare-metal core (no heap) still compiles.
    Cell {
        name: "baremetal/cpp/core-no-malloc",
        lang: Lang::Cpp,
        defines: &["NROS_PLATFORM_BAREMETAL"],
        src: CORE_PROBE,
        expect_pass: true,
    },
];

fn compiler(lang: Lang) -> &'static str {
    match lang {
        Lang::Cpp => "g++",
        Lang::C => "cc",
    }
}

/// Returns (compiled_ok, stderr).
fn try_compile(cell: &Cell) -> (bool, String) {
    let root = repo_root();
    let tmp = tempfile::tempdir().unwrap();
    let ext = match cell.lang {
        Lang::Cpp => "cpp",
        Lang::C => "c",
    };
    let src_path = tmp.path().join(format!("probe.{ext}"));
    std::fs::write(&src_path, cell.src).unwrap();

    let mut cmd = Command::new(compiler(cell.lang));
    match cell.lang {
        Lang::Cpp => {
            cmd.args([
                "-std=c++14",
                "-fno-exceptions",
                "-fno-rtti",
                "-fsyntax-only",
            ]);
        }
        Lang::C => {
            cmd.args([
                "-std=c11",
                "-fsyntax-only",
                "-Werror=implicit-function-declaration",
            ]);
        }
    }
    cmd.arg("-I")
        .arg(root.join("packages/core/nros-cpp/include"));
    cmd.arg("-I").arg(root.join("packages/core/nros-c/include"));
    for d in cell.defines {
        cmd.arg(format!("-D{d}"));
    }
    cmd.arg(&src_path);

    let out = cmd.output().unwrap_or_else(|e| {
        panic!(
            "failed to spawn {} (the platform-header gate needs a host C/C++ \
             compiler on PATH): {e}",
            compiler(cell.lang)
        )
    });
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

#[test]
fn platform_header_compile_matrix() {
    // Precondition: a host C/C++ compiler must exist, else the gate is vacuous.
    // (CLAUDE.md: tests must FAIL on unmet preconditions, never silently pass.)
    assert!(
        Command::new("g++").arg("--version").output().is_ok(),
        "g++ not found — the platform-header compile gate cannot run"
    );

    let mut failures = Vec::new();
    for cell in CELLS {
        let (ok, stderr) = try_compile(cell);
        if ok != cell.expect_pass {
            failures.push(format!(
                "  [{}] expected {}, got {}\n    stderr:\n{}",
                cell.name,
                if cell.expect_pass {
                    "COMPILE-OK"
                } else {
                    "COMPILE-FAIL"
                },
                if ok { "COMPILE-OK" } else { "COMPILE-FAIL" },
                stderr
                    .lines()
                    .take(8)
                    .map(|l| format!("      {l}"))
                    .collect::<Vec<_>>()
                    .join("\n"),
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "platform-header compile matrix mismatches ({}):\n{}",
        failures.len(),
        failures.join("\n"),
    );
}
