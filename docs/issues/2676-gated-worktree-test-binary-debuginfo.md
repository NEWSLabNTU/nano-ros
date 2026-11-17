---
id: 2676
title: "One cold `just ci gate` grows a worktree to 185 GB; test binaries with full debuginfo are about 50 GB of it"
status: open
type: bug
severity: high
area: [ci, build, testing]
related: [phase-559]
found: 2026-11-17
---

**Home: [phase-559](../roadmap/phase-559-ci-workflow-organization.md).** Found
on host 175 on 2026-11-17, when two gates in a row stopped on `No space left
on device`. Earlier notes put a gated worktree at about 80 GB.

## Measured

A cold `just ci gate` on `fix-native-sim-unwind-table` (3d0ba9db2b, green,
1h24m), sampled with `du` every 10 minutes:

| time (UTC) | gate step | worktree total |
| --- | --- | --- |
| 09:20 | start | 41 G |
| 09:30 | `check::fast` | 70 G |
| 09:42 | `check::build` | 110 G |
| 10:15 | `check::build` | 137 G |
| 10:31 | `check::build` | 170 G |
| 10:52 | end (`check::out-of-lane-deselected`) | **185 G** |

Nothing is removed at the end: the peak is the final size. The 2462 branch's
gate, stopped part-way through `check::build`, grew the same way: 66 G to 174
G in 35 minutes.

Where the 185 G sits:

| directory | size | what fills it |
| --- | --- | --- |
| `target/debug` | 39 G | 190 test binaries over 50 MB: **29.5 GiB** |
| `packages/cli/target/debug` | 29 G | 54 test binaries over 100 MB: **20.6 GiB**, about 440 MB each, of which 393 MiB is `.debug_*` sections |
| `build/corrosion-cargo` | 26 G | the cmake fixtures' cargo builds |
| `target/nros-relwithdebinfo` | 19 G | |
| `build/sizes-probe` | 14 G | |
| `build/example-lint` | 11 G | |
| `build/metadata-probe` | 3.9 G | |
| the rest | about 40 G | |

`packages/cli/target/release` is 1.4 G, of which `incremental/` is 812 MB. So
the CLI release profile's `incremental = true` (296501434a) is not the cause.
A worktree whose HEAD predates that commit also had a 30 G
`packages/cli/target`.

## The largest share: one debuginfo copy per test binary

Each integration test file is its own binary, and each statically links
everything it uses, with full debuginfo under the `dev`/`test` profiles. The
54 CLI test binaries each carry about 390 MiB of debug sections for almost the
same code. The root workspace's 190 test binaries add 29.5 GiB the same way.
Together that is about 50 GiB of 185, a quarter of the gate.

## Options (not chosen; each to be measured before it lands)

- `debug = "line-tables-only"` for the `test` profile (or `dev`) in both
  workspaces. Backtraces keep file and line; a debugger loses locals.
  Expected to shrink each binary to a fraction of its size.
- `split-debuginfo = "unpacked"`: the debuginfo stays in shared object files
  rather than being copied into each binary.
- Fewer test binaries: fold a crate's integration test files into one binary
  (`tests/it/main.rs`). Also cuts link time; changes nextest's per-binary
  overrides.
- Delete a gate's test binaries once its run is over. This fixes the symptom
  only, and the next gate cannot reuse them.

The other large directories (`build/corrosion-cargo`, `target/nros-
relwithdebinfo`, `build/sizes-probe`, `build/example-lint`) are not split
further here.

## Done when

A cold `just ci gate` stays under 100 GB at its peak, measured the same way,
or this issue records why a larger figure is the floor.
