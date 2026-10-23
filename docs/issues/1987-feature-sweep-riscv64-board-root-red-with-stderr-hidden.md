---
id: 1987
title: "The feature sweep's new STANDALONE run of `nros-board-threadx-qemu-riscv64` fails its first night in CI, and the sweep's output filter drops the build-script error that would say why"
status: open
type: bug
area: [ci, boards, threadx, testing]
severity: medium
found: 2026-10-20
related: [1960, 1355]
---

## Measured

- Run: host-tests 39363542759 (tier 1, schedule, 2026-10-20 03:17 UTC, main
  at `712f1853b`).
- Job: 118434999035, `per-feature clippy (host + cross)`, step
  `just check feature-sweep`. The other two jobs in the run passed.
- The verdict, 1 of 371 combinations:

```
  FAIL nros-board-threadx-qemu-riscv64 --manifest-path packages/boards/nros-board-threadx-qemu-riscv64 (nros-board-threadx-qemu-riscv64, locked)
        error: failed to run custom build command for `nros-board-threadx v0.4.0 (/__w/nano-ros/nano-ros/packages/boards/nros-board-threadx)`
check-feature-sweep: 1 of 371 combination(s) FAILED:
```

This is the first scheduled host-tests run since the STANDALONE rows landed
(#2429, issue 1960, merged 2026-10-19 20:28 UTC). So this root had never been
linted in CI before. Every other ThreadX row in the same job passed, including
`nros-board-threadx-linux` and the `platform-threadx` cross rows that name
this board. Those rows take only the board's environment, though. This is the
one row that runs `nros-board-threadx`'s build script for
`riscv64gc-unknown-none-elf`, and so the one row that compiles the ThreadX C
kernel for riscv64.

## Why the log cannot say more

`scripts/check-feature-sweep.py` prints only the stderr lines that start with
`error`, `  -->` or `Diff in`. For a build-script failure, cargo prints the
reason in the `--- stderr` section and on `cargo:warning=` lines, and that
filter drops both. So the one line left names the crate and nothing else. The
job uploads no artifact holding the full output.

## Not reproduced locally

On a workstation (2026-10-20), the sweep's exact run passed. That is the shape
`cross_shape("nros-board-threadx-qemu-riscv64")` returns: `--target
riscv64gc-unknown-none-elf`, plus `THREADX_PORT=risc-v64/gnu`,
`THREADX_EXTRA_INCLUDES=ports/risc-v64/gnu/example_build/qemu_virt`, and
`THREADX_CONFIG_DIR`/`NETX_CONFIG_DIR` set to the board's `config/`, against
`cargo clippy --locked -- -D warnings`. The workstation used
`/usr/bin/riscv64-unknown-elf-gcc` and the store's `threadx 6.4.1` sources.
So the difference is in the CI container: its riscv64 compiler or picolibc,
or how its store's sources resolve. Which one is not known.

## What it is NOT

- Not issue 1355's original cause, as far as anything shows. 1355's port
  selection is set explicitly here (`THREADX_PORT=risc-v64/gnu`). Whether the
  container's error is the same `tx_port.h` class cannot be told from this
  log.
- Not a lint finding in Rust code. The failure is the build script, before
  clippy reaches any crate's source.
- Not issue 1980 (the `entity_wake` race test). That failed in the same night's
  scheduled gate, a different lane.

## What would close it

1. **The sweep shows a build script's reason.** Keep the `--- stderr` section
   and the `cargo:warning=` lines of a failed run, or write each failed
   combination's full stderr to a file the job uploads. Either change makes
   the next red diagnosable on its own.
2. **Then the root goes green in CI or gets a reason.** Fix whatever the
   container lacks, through the image (a runner dependency is an image fix).
   Otherwise move the root to `STANDALONE_SKIP` with the measured reason, so
   the sweep reports a skip rather than a red no one can read.

## Sightings: every scheduled night since (2026-10-20 through 2026-10-23)

The same root has failed in every scheduled `host-tests` run since it was
added. In each one it is the only failing combination:

| Night | Run | Head |
| --- | --- | --- |
| 2026-10-20 | 39363542759 | `712f1853b` |
| 2026-10-21 | 39537847371 | `6738e85a0` |
| 2026-10-22 | 39716043967 | `350203f0b` |
| 2026-10-23 | 39894233489 (job 120220810480) | `2b6c5fdbf` |

The 2026-10-23 log still shows only the first line of the failure, so the
cause is still unknown:

```
  FAIL nros-board-threadx-qemu-riscv64 --manifest-path packages/boards/nros-board-threadx-qemu-riscv64 (nros-board-threadx-qemu-riscv64, locked)
        error: failed to run custom build command for `nros-board-threadx v0.4.0 (/__w/nano-ros/nano-ros/packages/boards/nros-board-threadx)`
check-feature-sweep: 1 of 360 combination(s) FAILED:
```

The job ran on a GitHub-hosted runner (`GitHub Actions 1000110602`).
Remedy 1 above (keep the build script's stderr) has still not landed. This
lane cannot say more until it does, and it will keep failing in the same
words, so a second cause landing here would not be noticed.
