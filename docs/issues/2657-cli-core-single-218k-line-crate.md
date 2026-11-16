---
id: 2657
title: "`nros-cli-core` is one 218k-line crate and the critical path of every CLI build"
status: open
type: perf
area: [cli, build]
severity: low
found: 2026-11-16
related: [phase-489]
---

**Home: [phase-489](../roadmap/phase-489-ci-build-speedup.md)** (build speed). Measured 2026-11-16 on host 118 (48 cores, load 11–36 from other sessions), `packages/cli` at main `c49aaaaef0`, each build in its own target dir. Raw logs and `--timings` reports: `cli-profile/` in the measuring worktree.

## Measured

`nros-cli-core` is 218,173 lines, about 85 % of the CLI's own code (next:
`cargo-nano-ros` 17.6k, `rosidl-codegen` 16.4k). Its five largest files are
`entity_inventory.rs` 13.2k, `cmd/build.rs` 13.1k, `cmd/ws.rs` 10.5k,
`cmd/setup.rs` 8.0k and `sizing_descriptor.rs` 6.7k.

- Cold release: it starts at 12.7 s, once its dependencies are built, and runs
  34.7 s (frontend 18.7, codegen 16.0) of a 48.1 s build. Nothing else runs
  beside it.
- The rustc frontend is largely serial per crate, so the 18.7 s cannot use the
  48 cores.

## Opportunity

Split it along its own module seams (the `cmd/*` verbs, `entity_inventory`,
`sizing_descriptor`) into crates that compile in parallel. Estimated cold
saving 10–20 s, depending on how independent the pieces are. A warm edit then
rebuilds only the touched crate and its dependents.

## Cost and risk

High: a large refactor of the CLI's internals and its re-exports, plus
`cli-source-dirs.txt` and the stamp's closure. After issues 2654 and 2655 the
warm case is already about 5 s, so this is worth it mainly for the COLD build
(a fresh clone, a CI job). Lowest priority of the four.

## Ruling (2026-11-16, user): HELD

Not approved for now. Issue 2654 goes first, and its measured every-commit
rebuild time decides whether this comes back.
