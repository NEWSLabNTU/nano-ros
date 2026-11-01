---
id: 2198
title: "NuttX rv-virt images fail to link: the POSIX handle park needs `pipe` and `__wrap_poll`"
status: open
type: bug
severity: high
area: [nuttx, platform, fixtures]
related: [phase-521]
found: 2026-11-01
---

## Symptom

Nightly 41283477252 (2026-11-01 07:07 UTC, `fc06d58870`) ran job 124780499659,
`nuttx` (GitHub-hosted). The job built every fixture row and failed on these
four, all `nuttx-riscv`:

```
fixture-rows-keep-going: 4 fixture row(s) FAILED to build; every other row was built (issue 1671):
  - fixtures-build nuttx-riscv c zenoh: examples/rv-virt-nuttx/c/talker	rc=1
  - workspace-fixtures-build nuttx-riscv c: workspace-c-nuttx-riscv-realtime (examples/workspaces/realtime-c)	rc=2
  - workspace-fixtures-build nuttx-riscv cpp: workspace-cpp-nuttx-riscv-realtime (examples/workspaces/realtime-cpp)	rc=2
  - workspace-fixtures-build nuttx-riscv rust: workspace-rust-nuttx-riscv-realtime (examples/workspaces/realtime-rust)	rc=101
```

Each of the four links fails with the same undefined reference:

```
platform.c:(.text.nros_platform_wake_park_handles_us+0x48): undefined reference to `pipe'
```

Three link steps in the same log also report:

```
third-party/nuttx/nuttx/sched/signal/sig_ppoll.c:121:(.text.ppoll+0x94): undefined reference to `__wrap_poll'
```

## Cause (located, not yet fixed)

`89c24caf5f` (phase-521 W3/W4, "the readiness slot and the POSIX handle park",
2026-11-01 03:08 UTC) added `nros_platform_wake_park_handles_us` to
`packages/platform/nros-platform-posix/src/platform.c`. The NuttX images build
that file. The park creates its wake pipe with `pipe()`
(`nros_posix_wake_make_pipe`), and with `NROS_POSIX_HAVE_PPOLL` it waits in
`ppoll`. The rv-virt NuttX images provide neither:

- there is no `pipe` symbol, presumably because their defconfig lacks
  `CONFIG_PIPES`;
- NuttX's `ppoll` calls `poll`, which these images link with `--wrap=poll`,
  but nothing defines `__wrap_poll`.

The nightly's commit includes `89c24caf5f`. The merge-queue gates passed
because they build no NuttX image.

## What it is NOT

- It is not a stale fixture: the commit is a direct ancestor of the run's
  head.
- It is not an NuttX Arm failure: no `nuttx-arm` row is in the failed list.

## What would close it

Pick one, deliberately:

- give the rv-virt NuttX configuration `CONFIG_PIPES` and a `__wrap_poll`
  (or stop wrapping `poll` there);
- or give the NuttX build of the POSIX platform a park that needs neither,
  as the FreeRTOS and ThreadX ports do;
- or leave the park off NuttX and have it refuse by name.

Acceptance: all four rows build in the nightly `nuttx` job, and a NuttX
riscv cell runs the park.
