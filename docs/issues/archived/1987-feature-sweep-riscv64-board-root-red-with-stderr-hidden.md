---
id: 1987
title: "The feature sweep's new STANDALONE run of `nros-board-threadx-qemu-riscv64` fails its first night in CI, and the sweep's output filter drops the build-script error that would say why"
status: resolved
type: bug
area: [ci, boards, threadx, testing]
severity: medium
found: 2026-10-20
related: [1960, 1355, 0680, 0678]
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

## Resolution (2026-10-23) — the job compiled a newlib-only port with a picolibc-only compiler

Both remedies landed together.

### 1. The sweep prints a build script's reason

`scripts/check-feature-sweep.py` routes a failed run's stderr through
`diagnostic_lines()`. That keeps clippy's `error…`/`  -->`/`Diff in` lines as
before, and adds cargo's `process didn't exit successfully` line, the
script's `cargo:warning=` lines and its `--- stderr` section, capped at 60
lines with a count of what was dropped. The self-test feeds it cargo's own
failure shape, captured from a failing build script, and asserts the
reason survives and the `rerun-if-changed` noise does not.

### 2. The cause, reproduced in the CI image

The sweep was rerun by hand in `nano-ros-ci-local:humble`, the same jammy
base and apt list as `nano-ros-ci:humble`, which this host cannot pull:

- **With the store's pinned `riscv-none-elf-gcc` 14.2-nros1 hidden**, as on
  the runner (the job provisions ThreadX *sources* and no compiler), the
  build falls back to the image's apt `riscv64-unknown-elf-gcc` 10.2.0 and
  fails, rc=101:

  ```
  tx_port.h:89:10: fatal error: string.h: No such file or directory
  ```

- **With it present**, rc=0, and cc-rs's debug output shows 736 compiles
  through `~/.nros/sdk/riscv-none-elf-gcc/14.2-nros1/bin/riscv-none-elf-gcc`.
  That is why the workstation passed: its store holds the dist, and
  `nros_build_paths::riscv64::tool` prefers the store to `PATH`.

The `string.h` error is only the first layer. The image's apt compiler ships
picolibc and no newlib. jammy's picolibc specs add its headers with
`-isystem`, so `--specs=picolibc.specs -print-sysroot` prints an empty line
and `arch_flags::riscv64::picolibc_sysroot()` adds no include. Pointing it at
those headers was tried; the build then stops in
`nros-platform-threadx/src/platform.c:724`:

```
error: '_impure_ptr' undeclared
error: invalid application of 'sizeof' to incomplete type 'struct _reent'
```

The RISC-V64 port is newlib-only by design: it swaps newlib's per-thread
`_impure_ptr` in the scheduler (issue 0680, `NROS_TX_PORT_HAS_REENT`). So no
probe change can make a picolibc-only compiler build this board, and the
header fallback was reverted rather than shipped. Issue 0678's rule stands
as it was.

### The fix

`host-tests.yml`'s `feature-sweep` job now runs `nros setup --tool
riscv-none-elf-gcc` beside its ThreadX sources. That is the toolchain the
port needs (the dist bundles newlib) and the one every other lane that
builds `threadx_riscv64` provisions through `nros setup` for the board.

Still to see: the first scheduled `host-tests` run with this change, where
`nros-board-threadx-qemu-riscv64` should be one more `ok` line. If it is
not, the sweep now prints why.
