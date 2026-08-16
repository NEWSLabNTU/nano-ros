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


## 2026-08-16 — Zephyr half implemented; ThreadX untouched; runtime not observed

**Zephyr is now settable.** `CONFIG_NROS_ZENOH_READ_PRIORITY` /
`CONFIG_NROS_ZENOH_LEASE_PRIORITY` (normalised 0–31, default 16 — the value the
FreeRTOS board already used) resolve through `_nros_resolve_knob` alongside every
other `ZPICO_*` knob and reach `zpico.c` as `ZPICO_{READ,LEASE}_TASK_PRIORITY`.

Three things had to change, matching the three drop points above:

* the shim's POSIX branch now sets the priority under `ZENOH_ZEPHYR` —
  `pthread_attr_setschedpolicy(SCHED_RR)` + `setschedparam`, mapped onto
  `sched_get_priority_min/max` rather than an assumed range, since that range is
  a build-configuration property (`CONFIG_NUM_PREEMPT_PRIORITIES`);
* **`PTHREAD_EXPLICIT_SCHED` with it** — the default is `PTHREAD_INHERIT_SCHED`,
  under which both calls are silently ignored and the thread takes the creator's
  priority. A scheduling attribute quietly dropped one layer down is this issue
  repeating itself, so it is set explicitly;
* `zpico_open` applies the compile-time default when no board called
  `zpico_set_task_config`, because on Zephyr none does. A board that DOES call
  it first still wins.

Stack sizes are passed as 0 on that path, and 0 now means "leave the port
default alone" — the convention `nros_platform_task_attr_t.stack_bytes` already
uses. Stating a priority should not require inventing a stack size.

`SCHED_RR` rather than `SCHED_FIFO`: the transport tasks share their level with
other work, and a FIFO thread that does not block holds the CPU. Round-robin
keeps the priority ORDER — which is what was being asked for — without making a
busy transport a starvation source.

### Verified

* the knob reaches the TU: `ZPICO_READ_TASK_PRIORITY=16` on `zpico.c`'s compile
  line;
* the branch compiles and links: the object carries undefined refs to
  `pthread_attr_setinheritsched`, `pthread_attr_setschedpolicy`,
  `pthread_attr_setschedparam`, `sched_get_priority_{min,max}` and defines
  `zpico_posix_set_priority`;
* the Zephyr C workspace entry builds green.

### NOT verified, and it matters

**The priority has not been observed taking effect at runtime.** A bare run of
`build-ws-c-entry-zenoh/zephyr/zephyr.exe` boots and then dumps core — but that
is PRE-EXISTING, not this change: the same run with the change stashed and the
image rebuilt cores identically. That entry needs the harness's launch setup,
not a bare invocation. `entry_e2e`'s `zephyr_c` cell is the right check and is
filtered out of this host's default scope (`0 tests run, 1 skipped`).

So what is proven is that the value is plumbed and the code is linked. Whether
the resulting thread priority is what the number says needs the e2e cell, or a
one-line log of the resolved value at session open — worth adding, since nothing
today makes this observable from the outside, which is how it stayed unsettable
without anyone noticing.

### ThreadX: unchanged (superseded — see below)

Still `(void)`'d. It needs the `z_task_attr_t` widening described above, which
touches the generic and bare-metal headers as well, and it has no board caller
either. Left for a separate change rather than bundled in behind a Zephyr
verification gap.


## 2026-08-16 (later) — ThreadX done, and verified at RUNTIME

Both platforms are now settable. ThreadX turned out to be the easier of the two
to PROVE, which is the opposite of what the Zephyr entry above expected.

### What ThreadX actually had

Narrower than "no knob": `c/platform/threadx/platform.h` already carried a
compile-time `Z_TASK_PRIORITY` (14), `#ifndef`-guarded and overridable. But
`_z_task_init` opened with `(void)attr;` and passed that one constant to
`tx_thread_create` for EVERY zenoh task — read, lease and tx-flush alike — so
`zpico_set_task_config`'s per-task arguments reached nothing and no
configuration path in nano-ros set the constant either.

### The change

* **`z_task_attr_t` widened to `nros_platform_task_attr_t`** in BOTH headers a
  ThreadX build sees. That pairing is load-bearing: `task.c` does a TU-local
  `#undef NROS_PLATFORM_ALIASES` to reach the concrete TX_THREAD-flavoured
  `_z_task_t`, so it reads `threadx/platform.h` while every other TU reads
  `nros_zenoh_generic_platform.h`. The shim ALLOCATES the attr and `task.c`
  DEREFERENCES it — one header changed alone would be a silent type confusion
  across that seam (issue 0135's shape). `bare-metal/platform.h` deliberately
  stays `void *`: it is single-threaded, creates no task, and lacks the include.
* **`_z_task_init` honours the attr**, inverting the band because ThreadX
  documents priority as "0 through (TX_MAX_PRIORITIES-1), where a value of 0
  represents the highest priority" while the band counts larger as more urgent.
  Scaled against `TX_MAX_PRIORITIES` rather than a literal 32, since it is
  configurable (32..1024).
* **`preempt_threshold` tracks the resolved priority.** ThreadX requires it to
  be `<= priority`; leaving the old fixed `Z_TASK_PREEMPT_THRESHOLD` would make
  any attr-supplied priority numerically below it ILLEGAL, and
  `tx_thread_create` would fail with `TX_PRIORITY_ERROR` — a scheduling knob
  that breaks thread creation when used.
* **`stack_bytes` is refused, at both ends.** The ThreadX `_z_task_t` embeds its
  stack at the compile-time `Z_TASK_STACK_SIZE`, so there is no larger region to
  point at; silently accepting a bigger number would be worse than ignoring it.
* **The knob lives in `config/threadx/nros-platform.toml`** (`defines_env`,
  so environment-settable with a default), because ThreadX's shim is compiled by
  the manifest-driven unified builder — `build_c_shim` is explicitly skipped for
  this platform, so a define added there would never have reached it. Listed in
  `rerun_if_env_changed` too: without that, changing the value in the
  environment would not rebuild the shim and the old one would silently persist,
  which is this issue's own failure mode.

### Default 17, not 16, on purpose

17 is the band value that maps back to ThreadX **14** — the `Z_TASK_PRIORITY`
every zenoh task took before. 16 would land on 15. Plumbing a value through
should not also retune it: a one-step scheduling shift on an RTOS is exactly
the kind of change that resurfaces later as a timing flake nobody connects back
to this commit. Retuning is now a one-line edit, which is the point.

### Verified

* `just threadx_riscv64 build-examples` green;
* **runtime**: the ThreadX RISC-V talker on QEMU opens its zenoh session and
  publishes (88 messages on the first run, 74 on the re-run with the
  behaviour-preserving default, 0 errors both times). This is a real check
  rather than a plumbing one — a priority `tx_thread_create` rejects returns
  `TX_PRIORITY_ERROR` and there would be no session at all;
* `check-kconfig-knob-forwarding` green.

### Two things the build corrected mid-change

* **The gate caught a wrong home for the Zephyr knob.** Routing it through
  `_nros_resolve_knob` looked tidier, and `check-kconfig-knob-forwarding`
  rejected it: that resolver is for knobs with a *Rust* reader, because a Zephyr
  Rust image inherits none of cmake's env exports (issue 0460). A priority gates
  no layout and has one consumer, so it forwards from `CONFIG_*` directly.
* **`size_probe.c` needed the same include.** Widening the typedef made the
  platform headers pull `<nros/platform.h>`, and the probe compiled with a
  narrower include set — `fatal error: nros/platform.h: No such file or
  directory` on the ThreadX cross build until it was added.

### Still open

`build-fixture-extras` on this lane fails at link with `rust-lld: unable to find
library -lnosys`, from `cmake/toolchain/riscv64-threadx.cmake`'s
`if(EXISTS .../libnosys.a)` conditional. This change adds no link flags and the
Rust examples link fine, so it is not from here — but no clean control was run
for it, and it wants its own issue.
