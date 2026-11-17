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

## First lever, measured: `debug = "line-tables-only"`

Set in `[profile.dev]` of the root `Cargo.toml` and of `.cargo/config.toml`,
the copy the mirror gate holds identical and the one that reaches every other
cargo root under the checkout (the `packages/cli` workspace, the generated
probe and lint roots, the leaves). `test` inherits it. Measured on host 118,
2026-11-17: two cold `just ci gate` runs on one fresh worktree, `git clean
-fdX` between them, `du` every minute, both green.

| | before (origin/main) | after |
| --- | --- | --- |
| **peak, whole worktree** | **188 G** (39 min) | **145 G** (32 min) |
| `target/debug` | 39 G | 16.5 G |
| `packages/cli/target/debug` | 29 G | 11 G |
| `build/sizes-probe` | 19 G | 16 G |
| `build/corrosion-cargo` | 26 G | 26 G |
| `target/nros-relwithdebinfo` | 19 G | 19 G |
| `examples/` | 24 G | 24 G |
| `build/example-lint` | 11 G | 11 G |

A test panic still prints `panicked at src/lib.rs:1:38`, and its backtrace
`at ./src/lib.rs:3:21`. The binary carries no `DW_TAG_variable` entries.

**One reader of that DWARF, and it pins its own profile.**
`scripts/nros-stack-floor.py` (the stack-floor drift gate) runs `llvm-objdump
--dwarf=frames` on linked images, and under line tables only one row moved
from 90,976 to 93,040 B with no codegen change. A measurement that reads
DWARF states the debuginfo it was taken under, so the stack-floor tool builds
with `CARGO_PROFILE_DEV_DEBUG=true` (nros 133's fix), independent of the dev
default. A sweep of the tree on 2026-11-17 found no other reader:
`scripts/stack-analysis.sh` reads the `.stack_sizes` section (`-Z
emit-stack-sizes`), not DWARF, and the remaining mentions are comments or
numbers measured once from DWARF by hand.

**Why this is not yet under 100 G.** What is left is not dev debuginfo:

- `build/corrosion-cargo/metadata-probe`, 26 G: about 60 per-image target
  dirs, about 290 MB each, each a full `libnros_c.a` or `libnros_cpp.a`
  build. Their `nros-probe` profile is already `debug = 0`. This is issue
  2235's class (each C/C++ image builds the runtime again in its own target
  dir), and the dedup is the next lever, planned there before any code.
- `target/nros-relwithdebinfo`, 19 G: **considered and kept.** Its
  `debug = 1` is what a user debugging an image gets, so trimming it is a
  product decision, and the saving was not worth making one.
- `examples/` and `build/example-lint`, 35 G together: leaf target dirs
  (already line tables only) and non-Rust output, not yet broken down.

## Done when

A cold `just ci gate` stays under 100 GB at its peak, measured the same way,
or this issue records why a larger figure is the floor.
