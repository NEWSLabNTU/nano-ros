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

Decouple the proc-macro from the CLI: move whatever `nros-macros`/`nros-build`
needs out of `nros-cli-core` into a small, dependency-light crate (or feature-
gate the CLI pull so it is off for the proc-macro path). The proc-macro should
expand from in-tree schema/types only, not the orchestration CLI. Then a plain
`cargo build -p nros-c` (and the docs/check-c lanes) need no CLI submodule.

Until then: lanes that build `nros`/`nros-c` must `git submodule update --init
packages/cli/third-party/ros-launch-manifest` first (done in `docs.yml` and the
`check` job of `pr-checks.yml`).
