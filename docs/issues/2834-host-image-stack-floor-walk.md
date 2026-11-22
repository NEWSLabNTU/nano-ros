---
id: 2834
title: "`nros build`'s stack-floor walk never reaches a host image, so threadx-linux states a stack nothing checks"
status: open
type: limitation
severity: low
area: [threadx, build, cli]
related: [issue-2800, issue-2400, phase-557]
found: 2026-11-22
---
## What
Since issue 2800, ThreadX's Linux port runs each thread on its ThreadX stack,
and the threadx-linux board states its app stack (`stack_stated::emit`, 64 KiB
by default, knob `NROS_MEMORY_APP_STACK_BYTES`). Nothing checks that
statement:

- `stack_floor::plan_one` (`packages/cli/nros-cli-core/src/cmd/stack_floor.rs`)
  returns before reading the platform lists when the plan's cargo output has
  no cross triple. That is every host image, so a threadx-linux build prints no
  stack-floor verdict, not even "not checked". Measured: `nros build
  condvar_contract_threadx_linux_bringup:threadx-linux` prints none.
- threadx-linux therefore stays on `UNCHECKABLE_PLATFORMS` and off
  `STATING_PLATFORMS`.

## Why it is more than a list move
The walk (`scripts/nros-stack-floor.py`) sums frames from the image's own
code. On a host image the app thread's deepest paths run through the host's
glibc, whose frames the walk has no sizes for. A walk that skips them reads
low, which is the unsafe direction.

## Fix (not started)
1. Let `plan_one` arm a host image whose platform states a stack, instead of
   returning on a missing cross triple.
2. Give the walk the host libc's frames, or refuse a path that enters a
   function it has no size for, naming it.
3. Move threadx-linux to `STATING_PLATFORMS`.

Until then the measured number comes from the image itself: each
threadx-linux image prints `nros: stack peak name=… used=N of=M` for every
thread when it ends.
