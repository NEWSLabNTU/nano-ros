---
id: 3008
title: "Executor::open inlines about 5.9 KB of stack into its caller on FreeRTOS"
status: open
type: tech-debt
severity: low
area: [executor, memory]
found: 2026-11-27
related: [2815, 3007]
---

## What happens

`Executor::open` is inlined into whatever opens an executor. Its internals
then land in the caller's frame: the opened session (a 528 B `CffiSession`
copy through `refuse_without_readiness`), the carved tables'
`compute_offsets_with` layout, and the `Result` temporaries. Measured on
`freertos-isr-wake-mps2` (thumbv7m, FreeRTOS):

- Inlined, the image's run closure (`app_task_entry_bare<.., main::{closure}>`)
  had a 10,064 B frame.
- With the open moved into an `#[inline(never)]` helper in the bin, the closure
  drops to about 4.2 KB and the helper takes 5,512 B, so the peak falls by about
  360 B.

The layout inside the inlined open is also unstable: an unrelated commit
(cfff3395ab, phase-579 W3a) moved the closure's `sub sp` by +64 B with no
change to what it holds.

## Why it matters

Every user binary that opens an executor pays this in its own frame, and on an
RTOS that frame is usually a task stack the user sized. The release fixed only
the one test bin, out of line (issue 2815's max-frame gate).

## Fix direction

Split `Executor::open` in the library: a small inlined shell around an
out-of-line body that builds the executor in place (or into the caller's
return slot), so the session and layout temporaries live in one bounded
frame that the max-frame gate can see and cap. Measure the body's frame per
target, and keep it under `max_frame`.
