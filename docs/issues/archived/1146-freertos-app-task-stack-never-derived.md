---
id: 1146
title: "The FreeRTOS app-task stack is 384 KiB by bisection, its C/C++ mirror says
  512 KiB, and the reason all three documents gave for the number stopped being
  true five phases ago"
status: resolved
type: tech-debt
area: boards
related: [phase-392, phase-76, 0271, 0739, 1145, 1187, 1197, 1330, 1348, phase-448]
resolved_in: phase-448 W3
---

## Problem

`nros_board_common::freertos_config::DEFAULT_APP_STACK_BYTES` is **393216**
(384 KiB), drawn from the FreeRTOS heap. It has never been derived from
anything. Its own doc comment is a bisection log:

> The default is 384 KiB: the Rust zenoh executor can exceed 160 KiB opening a
> FreeRTOS session with lwIP up, and the phase-212 Entry / run-plan Executor
> open overflows the older 256 KiB (issue #46). […] A 10-node macro entry's
> register pass overflows even 384 KiB, hence the override.

Three separate things follow from that, and all three are open:

1. **The number is unattributed.** Nobody knows how much of the 384 KiB is the
   zenoh-pico session open, how much is the executor-open call chain, and how
   much is headroom. No high-water measurement exists.
2. **The C/C++ carrier disagrees.** `cmake/templates/freertos_app_config.c.in`
   sets `.app_stack_bytes = 524288u` — 512 KiB, a third independent number.
   The divergence is documented there but not justified by a measurement
   either.
3. **Nothing sets the override.** `NROS_FREERTOS_APP_STACK_KB` exists and no
   file in the tree uses it, so every FreeRTOS example runs at the default.

## The stale explanation, corrected

phase-392 W6 was to "re-derive one over-large task stack against the new
figure — the FreeRTOS action examples' 64 KB app task". **Every noun in that
sentence was stale**, which is why W6 could not carry it:

| the claim | the fact |
| --- | --- |
| `APP_TASK_STACK` is 64 KB | `APP_TASK_STACK` was **deleted in phase-76**. The live knob is `app_stack_bytes` and the default is 384 KiB — six times larger, in the opposite direction from the one the wave predicted. |
| the executor's arena is inline on that stack | `Executor` has held `arena: &'s mut [MaybeUninit<u8>]` since phase-271 (issue 0110). On FreeRTOS the backing is a `.bss` static (phase-392 W6). Nothing arena-sized is in that frame. |
| the action examples pin `NROS_EXECUTOR_ARENA_SIZE=8192` | No example in the tree sets that knob. It is pre-phase-271 prose in `docs/guides/freertos-lan9118-debugging.md`. |

So the wave's hypothesis — "the stack is carrying the arena, drop it and the
number falls" — is **already false**: the arena left that frame five phases
before the wave was written. Whatever the 384 KiB is holding, it is not the
arena, and lowering the knob on that reasoning would have been a guess dressed
as a measurement.

---

## MEASURED 2026-09-07 — parts 1 and 3 resolved, part 2 stays open

Method: `uxTaskGetStackHighWaterMark(NULL)` (the portable
`nros_platform_task_stack_unused_bytes`, live on this port because the shared
`FreeRTOSConfig.h` sets `INCLUDE_uxTaskGetStackHighWaterMark 1`), read at four
points of bring-up. 14 images on `qemu-system-arm -machine mps2-an385`, each
run 45–90 s against a live `rmw_zenohd` — **not linked, run**, publishing or
serving.

### Rust path — `app_stack_bytes` = 393216 reserved

| image | after boot bringup | after `Executor::open` | after register pass | steady spin |
| --- | --- | --- | --- | --- |
| rust/talker | 5 040 | 8 952 | 25 160 | 25 160 |
| rust/listener | 5 040 | 8 952 | 24 784 | 24 784 |
| rust/service-server | 5 040 | 8 952 | 25 936 | 25 936 |
| rust/service-client | 5 040 | 8 952 | 26 992 | 26 992 |
| rust/action-server | 5 040 | 8 952 | **36 152** | 36 152 |
| rust/action-client | 5 040 | 8 952 | 33 584 | 33 584 |
| workspaces/rust (talker+listener) | 3 208 | 10 752 | 23 296 | 23 296 |
| workspaces/realtime-rust boot tier | 3 208 | — | 22 184 | 22 184 |
| workspaces/realtime-rust spawned tier | 3 160 | — | — | 22 368 |

The register pass is the peak; **90 s of spinning never moved any of them**.

### C carrier — `.app_stack_bytes` = 524288 reserved

| image | app-task peak |
| --- | --- |
| c/talker | 5 416 |
| c/listener | 6 976 |
| c/service-server | 8 304 |
| c/service-client | 8 616 |
| c/action-server | **18 176** |
| c/action-client | 16 312 |

Shallower than Rust, because that carrier keeps the executor and every node
struct in `.bss`.

### What the three inherited claims were worth

- *"the Rust zenoh executor can exceed 160 KiB opening a FreeRTOS session with
  lwIP up"* — **8 952 bytes**, identical on all eight Rust images. Off by 18x.
- *"the phase-212 Entry / run-plan Executor open overflows the older 256 KiB"* —
  nothing on this path comes within 7x of 256 KiB.
- *"a 10-node macro entry's register pass overflows even 384 KiB, hence the
  override"* — TRUE, and about an **out-of-tree** consumer: `a60b80da3`'s commit
  message names the "sentinel" entry, which ships 896 KiB via the override. It
  is an argument for the override and never was one for the default — an entry
  that overflows 384 KiB is not served by a 384 KiB default either. `issue-0274
  follow-up`, which the doc comment cites, does not mention stacks at all.

### The change

`DEFAULT_APP_STACK_BYTES` 393216 → **131072** (128 KiB): 3.6x the worst measured
in-tree peak, 7.2x the worst C one. It is charged **per task** — a spawned tier
whose `stack_bytes` is 0 takes the app default too — so a 2-tier FreeRTOS image
stops reserving 768 KiB of its 2 MiB heap_4 and reserves 256 KiB. That is a heap
BUDGET saving; it becomes a `.bss` saving only when `configTOTAL_HEAP_SIZE`
follows, which is issue 1145's half.

And the number is now **derivable without patching the board**:
`report_stack_peak` (`nros-board-freertos/src/entry.rs`) prints

```
nros: app task stack peak 36160 of 131072 bytes (94912 free) — raise with NROS_FREERTOS_APP_STACK_KB / `[node.rt] app_stack_bytes`
```

once per task at the end of the register pass. Once, deliberately:
`uxTaskGetStackHighWaterMark` walks the unpainted region, so on a spin tick it
costs tens of thousands of byte compares per iteration on an emulated
Cortex-M3 — which is also why the executor's `stack-headroom-runtime` rule is
not wired to it (see below).

### Verification — it runs at the new default

All six Rust role examples rebuilt through the real lane
(`scripts/build/fixtures-build.sh freertos rust zenoh`) and run: **5 of 6 green**
(talker publishes, service pair round-trips, action pair serves goals). The
sixth, `listener`, fails on the **arena**, not the stack —
`arena exhausted: 3844 more bytes needed, 0/8192 in use` — and a control build
with this change stashed fails identically, so it is a pre-existing red of the
issue-0900 family, unrelated. Reproduce: build `listener` into a private
`--target-dir` and run it.

### Bracketing run — and the failure is MISATTRIBUTED

Same image (`action-server`), same router:

| `NROS_FREERTOS_APP_STACK_KB` | result |
| --- | --- |
| 40 (40 960 B) | boots, serves goals, reports `peak 36160 of 40960 (4800 free)` |
| 32 (32 768 B) | `*** MALLOC FAILED ***`, hang |

The measurement predicts the boundary to within 5 KiB. But note **what it does
NOT print**: `*** STACK OVERFLOW: <task> ***`. The shared `FreeRTOSConfig.h`
sets `configCHECK_FOR_STACK_OVERFLOW 2` and `vApplicationStackOverflowHook` is
wired, yet heap_4 hands out the task stack, so the overflow lands in the
adjacent heap block's header and the next `pvPortMalloc` fails before any
context switch can check the pattern. `*** MALLOC FAILED ***`, `Invalid mbox`
and `*** STACK OVERFLOW ***` are three faces of one fault here. The `lwIP
ASSERT: Invalid mbox` this issue predicted is a possible landing, not the
landing.

## RESOLVED 2026-09-12 — part 2, the C/C++ carrier's 512 KiB

`cmake/templates/freertos_app_config.c.in` is **65 536**, and it is set from
BOTH halves rather than from the one that ran.

The blocker was that the C++ half could not be BUILT: every embedded C++
FreeRTOS target died in `<string>` -> `requires_hosted.h` under `-ffreestanding`
(issue 1187), and behind that issue 1330 — `nros_log_severity_t` packed to one
byte by the AAPCS `-fshort-enums` default. Both are fixed, so the number was
taken.

Method as part 1: `uxTaskGetStackHighWaterMark` on the app task, one-shot at the
end of bring-up, every image RUN on `qemu-system-arm -machine mps2-an385
-icount shift=auto` against a live `rmw_zenohd` — the pubsub and service pairs
delivering, the action pairs serving a goal to completion.

| role | C | C++ |
| --- | ---: | ---: |
| talker | 6 096 | 8 832 |
| listener | 7 840 | 8 832 |
| service-server | 9 320 | 8 832 |
| service-client | 9 344 | 8 832 |
| action-server | **18 792** | **19 112** |
| action-client | 17 112 | 17 472 |
| `examples/workspaces/cpp` entry | — | 10 136 |

**The two halves agree to within 2 % at their worst**, and that is the finding
that makes one shared number legitimate here rather than merely convenient. It
could not be known until the C++ half ran, which is exactly why this issue
refused to move the default on the C half alone.

65 536 is **3.4x** the worse of the two (19 112), the same margin the Rust
default carries over its own worst (131 072 / 36 936 = 3.5x). The MARGIN is the
shared decision; the numbers differ because the measurements do.

The C numbers moved slightly from the 2026-09-07 table above (talker 5 416 ->
6 096, action-server 18 176 -> 18 792). That is not drift in the measurement: the
shipped reporter has a frame of its own, and this run took the peak from a
watcher task rather than a throwaway probe. Same note as part 1's 36 152/36 160.

### The action images needed a throwaway probe, and that is issue 1348

Every action SERVER — Rust, C and C++ — currently fails to register on the zenoh
backend: the QoS shim refuses `DURABILITY_TRANSIENT_LOCAL` and the ROS action
`/<action>/_action/status` publisher is required by the action spec to ask for
it (regression from `b0ea5a04b`, 2026-09-11). Filed as **issue 1348**, not fixed
here.

The peaks above were taken with a throwaway local patch granting that policy, so
the action pairs ran to completion. Worth recording: the failing path and the
working path reported the SAME peak to the byte (C++ 19 112 either way), because
the deepest frame is reached before the refusal — so the numbers would have been
right without the probe. That was not knowable in advance, which is why the
probe was taken rather than assumed away.

## Verification — every image RUN at the new default

All twelve C/C++ role images plus the `workspaces/cpp` entry rebuilt through the
real lane (`just freertos build-fixtures`) at `.app_stack_bytes = 65536` and run
against a live router: every one reports the same peak it did at 512 KiB, the
pubsub and service pairs deliver, and no image reports `*** MALLOC FAILED ***`
or `*** STACK OVERFLOW ***`.

The one overflow this work did produce was the INSTRUMENT's own: the reporter
task's 512-word stack was thin enough that adding the heap line crossed it, and
`examples/workspaces/cpp` died with `*** STACK OVERFLOW: stkpeak ***` after
printing both lines. It is 1024 words now. A reporter that faults the image it
is measuring is worse than no reporter.

## Also found, not fixed here

- **`examples/workspaces/c` and `examples/workspaces/mixed` stop producing
  output at t=3 s** on mps2-an385, after one publish that IS delivered and
  received. Three controls say it is not this work: identical at a 512 KiB and a
  64 KiB app stack, and identical with the reporter task removed entirely. Not
  investigated further.

## Also found, not fixed

- **`Executor::set_min_stack_headroom_bytes` has no caller anywhere in the
  tree**, so the `stack-headroom-runtime` monitor rule (`min_bytes == 0`
  disables it) has never fired and cannot. Its doc says the entry that spawned
  the thread is the party that must set it. Wiring it needs a probe that is
  cheap per tick, which `uxTaskGetStackHighWaterMark` is not — it is O(unused
  bytes). A per-task cached bound, or a port-side watermark that is maintained
  rather than scanned, is the prerequisite.
- **`just freertos talker` (and its five siblings) named binaries that do not
  exist** — `_run-qemu "talker" "qemu-freertos-talker"`, while every leaf's
  `[[bin]] name` is the bare role (`talker`). Dev-convenience recipes only (the
  fixtures and tests resolve the real name), and FIXED here since the six call
  sites are one line each.
