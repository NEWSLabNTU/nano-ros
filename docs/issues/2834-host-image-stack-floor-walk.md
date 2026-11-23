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

Each threadx-linux image prints `nros: stack peak name=… used=N of=M` for
every thread when it ends (any termination signal included).

## Progress (2026-11-23): sized from the measured peak; the libc walk is deferred

Ruled by the manager, following the user's peak-sizing ruling for measured
rows (the rule issue 2783 applies to a row whose walk is an upper bound):

- `nros build` now arms a threadx-linux cargo image
  (`HOST_MEASURED_PLATFORMS` in `stack_floor.rs`, replacing
  `UNCHECKABLE_PLATFORMS`). Cargo's command runs unchanged (no walk flags);
  afterwards `nros-stack-floor.py --measured-only` reads the board's stated
  stack from cargo's messages and holds it to the image's `[[observed]]` row:
  floor = ceil8(peak x (100 + margin) / 100). A stack below it fails, naming
  `NROS_MEMORY_APP_STACK_BYTES`. An image with no row is reported as not
  checked, with how to measure it.
- A host row carries `unwalked = "<reason>"` instead of `static`; the
  validator refuses one that also records a walk (`static`, `walk_floor`,
  `upper_bound`) and one with no reason. Its `target` is the host triple it
  was measured on. The drift check and the twin lane skip it (nothing to
  re-walk, no twin), and `nros new` stamps a template's stack from its peak.
- The first row: the threadx-linux Rust talker, measured on x86_64 Linux.

**Deferred: steps 1-3 of the fix above (the libc walk).** Estimate 1-2 days:
the walk needs glibc's frames, which the host's `libc.so.6` holds only as
`.eh_frame` CFA rules (no `-fstack-usage`), plus a policy for the calls that
leave glibc into the kernel; and `nros build` without `--target` applies
target rustflags to build scripts too, so the walk's flags need a
build-script-safe carrier on a host image. Until that lands a host image is
MEASURED, never walked: a path the run did not take is not covered.

The cmake road does not check a threadx-linux C/C++ image either; its walk
checks the FreeRTOS carrier only (`cmake/NanoRosStackFloor.cmake`).
