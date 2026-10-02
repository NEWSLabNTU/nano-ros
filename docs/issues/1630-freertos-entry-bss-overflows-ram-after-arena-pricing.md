---
id: 1630
title: "`freertos_entry`'s `.bss` overflows `RAM` by 151,016 bytes on
  mps2-an385-freertos — the first nightly after the per-kind arena pricing landed,
  on a lane that was green the three nightlies before"
status: open
type: bug
area: [freertos, executor, build]
severity: high
found: 2026-10-02
related: [issue-1145, issue-0810, issue-1340, issue-1171]
---

## Measured

Nightly run **36977810939** (schedule 07:17:45, the 07:13 RTOS-ports arm), head
**`27070bdbd`**, job **110745933441** `freertos`, step `Build (freertos)`:

```
ld: freertos_entry section `.bss' will not fit in region `RAM'
ld: region `RAM' overflowed by 151016 bytes
collect2: error: ld returned 1 exit status
error: recipe `build-examples` failed with exit code 2
```

The link is the `mps2-an385-freertos` board — the objects immediately before the
failure are `packages/boards/nros-board-mps2-an385-freertos/c/board_mps2.c.obj`
and `_nano_ros_link/freertos_entry/nros_app_register_backends.c.obj`.

## It is a regression, and the lane was green three nightlies running

| nightly | head | `freertos` job |
| --- | --- | --- |
| 2026-09-22 07:13 | `17cd52fbc` | success |
| 2026-09-23 07:13 | `fcba471be` | success |
| 2026-09-25 07:14 | `364c5b3fd` | success |
| **2026-10-02 07:17** | **`27070bdbd`** | **failure** |

## The leading candidate, stated as a candidate

`27070bdbd` is the **tip of PR #1538**, merged at about 03:38 today — the
per-kind executor-arena pricing (`fix(#0810)`: *price each declared entity kind
at what its entry holds*, with `fix(#1496, #1036)`, `fix(#1370)` and
`fix(#1623)`). This is the first nightly to run on it.

Three things make it the first place to look, and none of them is proof:

1. **The arena lives in `.bss`.** RFC-0002 §4.4b and the phase-392 W6 correction
   put the Rust executor backing in a named `.bss` static
   (`nros_node::executor::backing::EXECUTOR_BACKING`) rather than on the task
   stack or the heap. A `.bss` overflow is exactly where an arena-sizing change
   shows up on a RAM-constrained target.
2. **That PR's own commit message quotes arena sizes moving** — "(arena 74,240 ->
   8,192)" for the three native realtime workspaces. Per-kind pricing can move a
   figure either way: an image whose declared entities are costlier than the old
   worst-case shape gets a BIGGER arena, not a smaller one.
3. **151,016 bytes is the right order** for this knob. CLAUDE.md records the
   derived default as 87,256 B on mps2_an385 and 88,328 B on native_sim — so an
   overflow of ~151 KB is a plausible arena delta on this board rather than, say,
   a stray buffer.

## What this is NOT

- **Not 1145.** That is `NROS_EXECUTOR_BACKING_U64S` **below** the default, which
  is a compile error naming the knob. This is a link-time region overflow with the
  knob unset, i.e. the derived value itself is too big for the board.
- **Not 1353.** No disk text anywhere in the job; the build ran to the link.
- **Not 1628.** That is `rust-rtos-link-check` refusing because the FreeRTOS
  kernel source is unprovisioned, on `host-tests`. Here the kernel is present and
  the image compiles — it is the linker's region that fails.
- Not the ROM overflows of archived 0477/0511 (NuttX, `.text`): this is `.bss`
  on FreeRTOS.

## What would close this

First, confirm or kill the candidate cheaply, since the whole diagnosis rests on
it:

```
just setup-platform freertos           # arm-none-eabi + kernel
just freertos build                    # at 27070bdbd, then at 27070bdbd~7
just mem-report <elf> --json           # per-symbol, per-crate, declared pools
```

`mem-report`'s `--baseline` comparison across those two builds names the symbol
that grew, which either shows `EXECUTOR_BACKING` and confirms the candidate, or
points somewhere else and retires it. If it is the arena, the decision is whether
this board states `NROS_EXECUTOR_BACKING_U64S` (and pays the knob-and-guard
discipline issues 1145/1171 describe, including that the subtrahend is STATED
rather than measured) or whether the per-kind pricing needs a cap on a board this
small.

Acceptance: the 07:13 nightly's `freertos` job links again, and whatever number
changed is written down next to the board rather than left derived — issue 1146's
lesson about `app_stack_bytes`, where a bisected constant stood for phases before
anyone measured it.
