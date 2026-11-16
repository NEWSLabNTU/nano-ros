---
id: 2655
title: "Every commit rebuilds the CLI's 218k-line crate, because its freshness stamps are baked into it"
status: open
type: perf
area: [cli, build]
severity: medium
found: 2026-11-16
related: [phase-489]
---

**Home: [phase-489](../roadmap/phase-489-ci-build-speedup.md)** (build speed). Measured 2026-11-16 on host 118 (48 cores, load 11–36 from other sessions), `packages/cli` at main `c49aaaaef0`, each build in its own target dir. Raw logs and `--timings` reports: `cli-profile/` in the measuring worktree.

## Measured

`nros-cli-core/build.rs` watches the git index (`rerun-if-changed` on
`--git-path index`) and bakes `NROS_CLI_SOURCE_STAMP`, `NROS_CLI_PLANNING_STAMP`
and `NROS_PLAY_LAUNCH_SHA` into the crate with `cargo:rustc-env`, read through
`env!` at about a dozen sites. So ANY index move (a commit, rebase, `git add`,
branch switch), whether or not it touches a CLI source, re-runs the build
script and recompiles all of `nros-cli-core`:

| build | wall | units |
| --- | --- | --- |
| release, touch the index only | **38.8 s** | 2 (build script + `nros-cli-core`, 37.3 s) |
| release + incremental (issue 2654), touch the index | 5.0 s | 2 |

Who pays it (measured after issue 2654): a DIRECT cargo invocation in
`packages/cli` (`bootstrap.sh`, the `cli-tests` gate, a contributor's `cargo
build`). `just setup-cli` does not: after a commit that touches no CLI source
it asks `source-stamp`, finds the binary fresh, and runs no cargo (0.87 s).
When a pull DOES change a CLI source, `setup-cli` takes 11.4 s with
incremental (36.6 s without). The stamp's changed value re-bakes the `env!`
constants, which is the part this issue would remove.

## Opportunity

Bake the stamps into a crate that holds nothing else (or into a generated file
the binary reads), so the 218k-line crate does not depend on them. An index move
then rebuilds the tiny crate and relinks: estimated about 1–2 s, against 38.8 s
today (5 s with issue 2654).

## Cost and risk

- Medium: about 12 `env!` sites move to the new crate's constants.
- The freshness rules (issues 0561, 1018, 1336, 1360) must keep their reach:
  the stamp still has to re-bake on every index move, so the watch stays, on
  the small crate.
- `check-stale-cli-attribution` and `check-cli-source-dirs` must still hold.

## Ruling (2026-11-16, user): HELD

Not approved for now. Issue 2654 goes first, and its measured every-commit
rebuild time decides whether this comes back.
