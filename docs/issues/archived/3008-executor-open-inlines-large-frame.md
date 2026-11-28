---
id: 3008
title: "Executor::open inlines about 5.9 KB of stack into its caller on FreeRTOS"
status: resolved
resolved: 2026-11-28
resolved_in: "the change that adds this line"
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

## Fix (2026-11-28)

The open is split in the library, as the fix direction asked: a small shell
around one out-of-line body, `Executor::open_owning`. Its temporaries live in
that one bounded frame, which the max-frame gate sees, and it returns the
executor straight into the caller's result. Four changes make the frame hold
less, not only sit elsewhere:

- `open_sized` is out of line and tail-calls the body. The body takes the
  `default_backing` block itself (`take_default_backing`), and it gives the
  block back on a failed open (`give_back_on_failure`, the rule
  `with_default_backing` applies). `with_default_backing` used to patch the
  executor into a local `Result` it then returned, which kept an executor-sized
  copy in whatever frame inlined it.
- The open's three log lines (backend unresolved, session open failed,
  session open) are `#[cold]`/`#[inline(never)]` helpers. Inlined, each put a
  format buffer in the open's frame on the path that does not log.
- `nros_rmw_cffi::refuse_without_readiness` borrows the session for its check
  and logs from a cold helper. It used to take and return the 528 B session by
  value with its log line inlined.
- `isr-wake-freertos-mps2`'s `#[inline(never)] open_executor` wrapper is gone.
  The library makes it unnecessary, and its 5,512 B max-frame baseline row is
  deleted.

Two shapes were measured and rejected. Building the executor into an `out`
slot put the slot in the shell's frame, which made the bare-metal open deeper
than main. A separate out-of-line session open stacked a 1,144 B frame under
the body, which also made the chain deeper.

## Measured (2026-11-28)

Frame sizes from each function's prologue (pushes plus `sub sp`), in the two
ISR-wake images: thumbv7m, FreeRTOS at `nros-minsizerel` and bare metal at
`nros-relwithdebinfo`.

| | main | fixed |
|---|---|---|
| FreeRTOS run closure (the caller) | 4,224 B with the bin's wrapper, ~10 KB without | 4,224 B, no wrapper |
| FreeRTOS open chain | `open_executor` 5,392 B | `open_sized` 160 + `open_owning` 4,064 = 4,224 B |
| bare-metal run closure | 4,168 B | 4,168 B |
| bare-metal open chain (on the walk's deepest path) | `open_sized` 2,152 + `open_in_with_session_slot` 3,376 = 5,528 B | `open_sized` 104 + `open_owning` 3,344 = 3,448 B |

So the open path is 1,168 B shallower on FreeRTOS and 2,080 B shallower on
bare metal. No frame on it is over `max_frame` (4,096 B) any more.

The images' walk floors do not show this one for one. The deepest path below
the session open is chosen among indirect-call bindings, and which one wins
moves with an unrelated change: 53,528 -> 54,352 B on FreeRTOS, where the
floor path does not pass through the open at all, and 55,416 -> 57,088 B on
bare metal, where it does but now ends in a rosout pump instead of a serial
link. That band is issue 2550's, not this change.

## Not done

`SessionStore::Owned` still holds a 528 B session inline in every `Executor`,
even when the session sits in a carved slot. Removing that variant would shrink
every executor copy by about 0.5 KB. It touches the C/C++ opaque executor size
and the constructors whose sizing carves no slot, so it is its own change.
