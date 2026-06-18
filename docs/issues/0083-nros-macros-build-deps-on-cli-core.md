---
id: 83
title: nros-macros (proc-macro) build-depends on nros-cli-core — every nros build pulls the whole CLI
status: open
type: tech-debt
area: build
related: [phase-253, phase-256]
---

## Problem

`nros-macros` (the proc-macro crate every nros application uses) build-depends
on `nros-build → nros-cli-core`, which in turn pulls
`ros-launch-manifest-types` from the nested `packages/cli/third-party/
ros-launch-manifest` submodule. So building ANYTHING that depends on `nros`
(incl. `nros-c`, every example, every fixture) drags in the entire in-tree CLI
core + its submodule.

Dependency chain (from `cargo tree -p nros-c`):

```
nros-c → nros → nros-macros (proc-macro)
              → nros-build → nros-cli-core → ros-launch-manifest-types
                                              (packages/cli/third-party/ros-launch-manifest, submodule)
```

## Impact

- **CI fragility.** A plain `cargo build -p nros-c` fails on any checkout that
  hasn't initialised the `ros-launch-manifest` submodule. This bit two lanes
  after the coupling landed (a 256-era change): the `docs` rustdoc build
  (fixed by initialising the submodule) and `check-c` (the nros-c build emits
  the `*_OPAQUE_U64S` macro header; the silent failure left it ungenerated →
  `'EXECUTOR_OPAQUE_U64S' undeclared`). Both are worked around by initialising
  the submodule, but the coupling is the root cause.
- **Build weight.** Every embedded/app build now compiles a slice of the CLI
  (codegen/orchestration) just to expand a proc-macro. The proc-macro should
  not need the CLI at build time.

## Direction

**Filed as [phase-262](../roadmap/phase-262-decouple-macros-from-cli-core.md)**
(design explored 2026-06-18). The macro uses only two self-contained nros-cli-core
modules — `pkg_index` + `launch_parser` — and NEITHER touches
`ros-launch-manifest-types` (that's incidental, via `orchestration/manifest.rs`).
Extract both into leaf crates (`nros-pkg-index`, `nros-launch-parser`); nros-cli-core
re-exports them (consumers unchanged); nros-macros depends on the leaves directly +
drops `nros-build`. App / nros-c builds then compile neither nros-cli-core nor the
submodule — no feature-gating needed. Removes the docs/check-c submodule-init
workarounds.

Until then: lanes that build `nros`/`nros-c` must `git submodule update --init
packages/cli/third-party/ros-launch-manifest` first (done in `docs.yml`, and in
`pr-checks.yml`'s build tier which provisions the CLI + sources). The push lane
avoids the problem entirely — `check-fast` runs only the buildless `check-c-fmt`/
`check-cpp-fmt` (clang-format); the compile gates `check-c`/`check-cpp` live in
`check-build` (PR/nightly), where the submodule + zenoh-pico source are present.
