---
id: 626
title: "The zenoh transport tasks' priority is unsettable on ThreadX and Zephyr —
  the board never asks, and two of the three layers would drop it if it did"
status: open
type: bug
area: boards, rmw-zenoh, platform
related: [issue-0623, issue-0506, issue-0579, phase-358, phase-364]
---

## Symptom

There is no way to say what priority the zenoh read/lease tasks run at on
ThreadX or Zephyr. Not "the knob is ignored" — there is no knob, and the three
layers that would carry one each drop it independently.

The consequence is that issue 0506's question — *what preempts the RT tiers
under inbound load?* — cannot even be asked on two of the four RTOSes. On
FreeRTOS it is answerable (and #0623 made the answer legible); here the transport
band's priority is whatever a default happens to be.

## What is actually there, layer by layer

Checked in the tree rather than assumed — an earlier draft of #0623 guessed
these two platforms shared FreeRTOS's *units* problem, and that guess was wrong.

**1. The board never asks.** `zpico_set_task_config` — the process-wide setter
for read/lease priority and stack — has exactly one caller in the tree:

```
packages/boards/nros-board-freertos/src/entry.rs:312    (Rust entry)
packages/boards/nros-board-freertos/c/freertos_c_entry.c:218  (C entry)
```

The ThreadX and Zephyr boards never call it. So `zp_task_read_options_t
.task_attributes` stays `NULL` and the tasks are spawned with every default —
stack size included, not just priority.

**2. The shim would drop it anyway.** `zpico_set_task_config` branches on the
platform macro:

| platform | branch | priority |
| --- | --- | --- |
| FreeRTOS (`ZENOH_FREERTOS_LWIP`) | native attr | **honoured** |
| Zephyr (`ZENOH_ZEPHYR`, set by `zephyr/cmake/nros_rmw_zenoh.cmake:56`) | POSIX | `pthread_attr` stack only — `(void)read_priority`, since priority needs `SCHED_FIFO` |
| ThreadX (`ZENOH_THREADX`) | `#else` | `(void)`'d entirely |

The `#else` branch's comment — *"ThreadX, generic, and other platforms:
`z_task_attr_t` is `void*` and zenoh-pico ignores it. Config stored for future
platform support"* — is the honest record of a deliberate stub. It has since
stopped being true (see below).

**3. On native Zephyr the PORT drops it too.** `nros-platform-zephyr` has two
`nros_platform_task_init` implementations:

* `#ifdef CONFIG_POSIX_API` (line 192) — accepts `attr`, phase-364 W3;
* `#else` + `CONFIG_DYNAMIC_THREAD` (line 547) — `(void) attr;` and a hardcoded
  `K_PRIO_PREEMPT(5)`.

So a native-Zephyr image runs its transport tasks at cooperative-preempt 5 by
construction, and no configuration anywhere can move it.

## Why it is worth fixing now rather than when someone trips on it

The ABI half is already built and is going unused. phase-364 W3/W5 gave
`nros_platform_task_attr_t` a real `priority` on a NORMALISED band (0 = least
urgent, larger = more urgent, `NROS_PLATFORM_PRIORITY_INHERIT`,
`NROS_PLATFORM_PRIORITY_RAW(n)` to bypass), precisely so that *"the same number
meant 'run me first' on one board and 'run me last' on another"* would stop
being true. **Both** the ThreadX port and Zephyr's POSIX port already accept and
honour that attr:

```c
/* threadx: phase-364 W1/W3 — INVALID for a caller-side impossibility. `attr` is
   NO LONGER among them: a NULL means every default, as on every other port. */
const nros_platform_task_attr_t *a = (const nros_platform_task_attr_t *) attr;
```

What is missing is the wiring between the shim and that ABI. Which also means
this is the one place in the tree where the *unified* vocabulary #0623 wants
could be adopted without reinterpreting anything already written — there is no
existing value to reinterpret.

## Correction to a claim in #0623

Two things that issue said, which examining the code disproves:

* *"ThreadX and Zephyr have the same two vocabularies meeting in one
  scheduler."* They do not. The priority is not settable at all there, which is
  the "declared value silently ignored" class (#0579), not the units class.
* *"`z_task_attr_t` has THREE definitions in this tree, so implementing it is a
  fix-the-class change."* There are three, but they are **identical** —
  `typedef void *z_task_attr_t;` in the generic, bare-metal and threadx headers.
  They agree; the problem is what they agree ON. No ABI split, and the fix is
  correspondingly smaller than that sentence implied.

## Sketch of a fix

Under `ZENOH_GENERIC` the chain already reaches the right function:
`_z_task_init` → `nros_platform_task_init(task, attr, entry, arg)`. The blocker
is only that `z_task_attr_t` is `void *`, so nothing can be carried in it.

Typedef it to `nros_platform_task_attr_t` (the generic header already includes
`<nros/platform.h>` for `NROS_PLATFORM_TASK_STORAGE_SIZE`), populate it in
`zpico_set_task_config`'s `#else` branch on the normalised band, and give the
ThreadX/Zephyr boards the call the FreeRTOS board already makes.

Two cautions for whoever takes it:

* `zp_task_read_options_t.task_attributes` is a `z_task_attr_t *`, so widening
  the typedef from a pointer to a struct changes what the pointer POINTS AT, not
  the option struct's layout — but every TU must still agree on the typedef, and
  a mismatched pair here is issue 0135's shape.
* Native Zephyr (layer 3 above) needs its own change; wiring the shim is not
  enough there while `task_init` hardcodes `K_PRIO_PREEMPT(5)`.

## Acceptance

* the zenoh read/lease task priority is settable on ThreadX and on Zephyr, in
  the phase-364 normalised band, and the value reaches the kernel spawn call;
* a value that cannot be honoured fails or reports — it does not silently become
  a default (that is the whole of this issue);
* #0506's question is answerable on those platforms: something states what the
  transport band's priority is relative to the tiers.

## Found by

Phase-358 W3 → #0623. Asking what "bound the read task priority" means on
FreeRTOS surfaced the units collision there; checking whether the other RTOSes
shared it turned up this instead.
