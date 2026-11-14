---
id: 2557
title: "Two bringups in one CMake workspace each FORCE-write NROS_FEATURES, so the last configure's capability set wins for every entry"
status: open
type: bug
area: [cmake, codegen, workspace]
severity: low
found: 2026-11-14
related: [2556, 0353, phase-555]
---

Filed from RFC-0066's open questions by phase-555 W3 (ruling A1).

## Measured (phase-331 W2, 2026-08-02)

With three bringups in the cpp workspace, `managed_bringup`'s empty capability
set erased `param_services` and `lifecycle` for the whole workspace. It was
worked around by declaring the union in every bringup. Issue 0353 had fixed
the single-bringup path only.

## Still in the code (read 2026-11-14, not re-measured)

Both generators of the declared capability axes write the cache entry with
`FORCE`:

- `packages/cli/nros-cli-core/src/cmd/codegen_system.rs`
  (`nros dev codegen system`):
  `set(NROS_FEATURES "…" CACHE STRING "nano-ros capability axes" FORCE)`
- `packages/cli/nros-cli-core/src/cmd/config.rs`
  (`nros config show --format cmake`): the same line.

A `FORCE`d cache write is last-writer-wins per configure. Whether a
multi-bringup workspace still reaches two of them in one configure was not
re-measured for this filing.

## Done when

A configure of a workspace with two bringups that declare different
capabilities either gives each entry its own system's set or refuses, naming
both bringups. A test builds that workspace and asserts it.
