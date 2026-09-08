---
id: 1224
title: "`nros-rmw-cyclonedds` declares `<build_type>nros_cmake</build_type>` while cargo is what builds it, and RFC-0094 W3 makes that declaration load-bearing"
status: resolved
area: build, rmw, cli
severity: medium
found: 2026-09-08
resolved_in: phase-439 W3
related: [1207, RFC-0094, RFC-0087]
---

# Resolution: the declaration is correct, and the premise this issue rested on is not

**Nothing was changed.** The `<build_type>nros_cmake</build_type>` on
`packages/rmw/cyclonedds/nros-rmw-cyclonedds` stays, and RFC-0094 D3's rule
stays exactly as written — no widening, no second exception class.

The issue proposed two fixes and asked W3 to pick one. Both rested on a claim
about the package that turns out to be false, and W3 measured it before
choosing. The claim was:

> Its `CMakeLists.txt` is `project(nros_rmw_cyclonedds … LANGUAGES C CXX)` — a
> separate C/C++ wrapper with its own RPATH handling for the in-tree
> `libddsc.so`, built for the backend's own test binaries.

## What was measured

That `CMakeLists.txt` is not a test wrapper. It is the **production build of the
Cyclone backend's C++ library**, and three independent places in the tree say so:

* `packages/rmw/cyclonedds/nros-rmw-cyclonedds/CMakeLists.txt:128` —
  `add_library(nros_rmw_cyclonedds STATIC …)`. That is the archive the root
  whole-archives into every C/C++ image on the cyclone backend. The CTest
  harness the issue described is line 355,
  `option(NROS_RMW_CYCLONEDDS_BUILD_TESTS "Build CTest harness"
  "${PROJECT_IS_TOP_LEVEL}")` — **OFF whenever the project is added as a
  subdirectory**, i.e. in every build that is not someone running the wrapper
  standalone.
* `nros-rmw.toml` declares `[rmw.provides.cmake] dir = "." target =
  "nros_rmw_cyclonedds"`, and the root `CMakeLists.txt:419` does
  `add_subdirectory("${NROS_RMW_CMAKE_DIR}" …)` on it — the phase-439 W4 seam.
  So cmake enters this directory as a package, by declaration, on the live path.
* The repo-root `Cargo.toml`'s own comment on member 87 (line 88):
  *"The C++ backend is still built by the sibling CMake project."*

Cargo genuinely builds the Rust crate too — that half of the issue is right.
But the two facts are not in competition, because **they are answers to
different questions**:

    <build_type>   how does a build system enter this DIRECTORY as a package?
                   Only cmake does: `add_subdirectory`, from the descriptor.
    a path dep     how does a consumer reach the CRATE?
                   As a dependency of a manifest — which routing never touches.

RFC-0094 D3 asks the first question. `nros_cmake` is its correct answer.

## And the hazard the issue predicted does not exist

> The day it lands, any workspace whose walk reaches this package drops it from
> the cargo members list on the strength of a declaration that is wrong.

Two things make that unreachable, both checked:

* The declaration is not wrong (above).
* The crate's cargo membership is in the repo-root `/Cargo.toml`, which is
  **hand-written and tracked**. `cargo_root::has_tracked_root` returns true for
  it, so `cargo_root::render` — the site W3 changed — is never called for the
  repo root at all. W3 cannot move member 87, whatever the declaration says.

So the package stays inside the class RFC-0094 D3 already names and justifies:
a dual-file package declaring a cmake type, which leaves the generated cargo
members list. `scripts/check/check-package-routing.py --report` still prints it
under "Packages that CHANGE SIDE" with `NEITHER — read this one`, and that
column is doing its job: it flagged the one row worth reading, and reading it is
what produced this measurement.

## What would still be an improvement, and is not this issue

The issue's option 2 — one directory, one buildable thing — remains the tidier
shape, and would remove the need for a reader to work any of the above out. It
is not worth the churn today: the `CMakeLists.txt` is reached by absolute path
from the root, from `zephyr/cmake/nros_rmw_cyclonedds.cmake`, from
`packages/api/nros-cpp`, and from six ThreadX examples, and moving it buys
legibility rather than correctness. Filing it as its own issue would be
inventing work; it is recorded here instead.

## Reproduce

    python3 scripts/check/check-package-routing.py --report
    grep -n 'add_library(nros_rmw_cyclonedds' packages/rmw/cyclonedds/nros-rmw-cyclonedds/CMakeLists.txt
    grep -n 'NROS_RMW_CYCLONEDDS_BUILD_TESTS' packages/rmw/cyclonedds/nros-rmw-cyclonedds/CMakeLists.txt
    grep -n 'add_subdirectory("${NROS_RMW_CMAKE_DIR}"' CMakeLists.txt
