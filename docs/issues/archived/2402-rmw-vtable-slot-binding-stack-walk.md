---
id: 2402
title: "The stack walk charges every indirect call the deepest address-taken target, because it cannot bind a call through the RMW vtable to the slot it loads"
status: resolved
resolved_in: "PR #PRNUM (phase-557 W7): --indirect bind follows every indirect call; measured peaks are [[observed]] rows"
type: tech-debt
severity: low
area: [memory, tooling, rmw]
found: 2026-11-10
related: [issue-2400, issue-2360, issue-2398, issue-2516, issue-2517, phase-548, phase-557]
---

**Home: [phase-548](../roadmap/phase-548-three-languages-every-board.md)**, with
issue 2400, whose per-backend indirect allowances this issue would retire.

## What

`scripts/nros-stack-floor.py` (issue 2400) sums a thread role's deepest stack
path from the compiler's frames. It cannot follow a call through a register
(`blx rN` and the like). Under `--indirect fold1` it charges every such site
the same bytes: the deepest path among the address-taken functions in scope.

So a call site gets charged for targets it can never reach. On the nucleo-f767zi
Rust zenoh action server, the charge comes from the graph query
`ZenohSession::names_and_types_filtered` (19,656 B frame). The image never
calls it. The nros-log sink's call site is charged that same amount.

Measured against the Renode twin peak of 29,184 B, a static floor cannot be
both sound and tight:

| walk | OPEN | vs 29,184 B |
| --- | ---: | --- |
| direct calls only | 18,112 B | under |
| fold1, targets scoped to zenoh/zpico | 26,232 B | under by 2,952 B |
| fold1, every address-taken target | 45,776 B | 57 % over |

Issue 2400 therefore takes the floor as max(scoped static walk, measured twin
peak) + a stated margin. A role with no twin (PX4's executor-pass thread) gets
the static walk plus a stated per-backend indirect allowance. This issue is
what would remove those allowances.

## Proposal

The one indirect call that dominates is the call through the C RMW vtable
(`nros_rmw_vtable_t`, issue 0331's ABI). Each backend fills a static instance
of it. Every slot is a relocation in that static's data section, at a known
offset, naming the backend function. The call site loads a function pointer
from `vtable + offset` and branches to it.

1. From the relocations of the backend's vtable static, build the map
   slot offset → target function.
2. At each indirect site in `nros-rmw-cffi`, recover the offset the branch
   register was loaded from. On Thumb-2 this is a `ldr rN, [rM, #off]`
   feeding the `blx rN` in the same basic block. Other targets need their
   own load pattern.
3. Charge that site only the slot's target. A site whose load is not
   recovered keeps today's charge and is counted in the output.

Only that one table. Rust `dyn` vtables and C callback tables stay on the
allowance until this one is measured to pay off.

## Acceptance

- On the nucleo action server, the static walk with slot binding is at or
  above 29,184 B, and within a stated margin of it.
- The per-backend indirect allowance in issue 2400's table for that
  target/role is removed, and the floor still holds against the twin.
- A self-test with a synthetic vtable and load sequence binds the site to its
  slot, and refuses it if the load is not found.

## LTO: the allowance moves the opposite way from the static walk (2026-11-10)

Measured on the nucleo-f767zi Rust zenoh action server, from one tree, built
with and without LTO (issue 2407). The twin peak is the thread analyzer's
maximum over 7 rounds on the Renode twin:

| | static OPEN (scoped fold1) | twin peak | allowance (peak − static) |
| --- | ---: | ---: | ---: |
| without LTO | 26,232 B | 29,224 B | 2,992 B |
| with LTO | 22,564 B | 30,256 B | **7,692 B** |

LTO lowers the static walk by 3,668 B and raises the measured peak by
1,032 B. Inlining merges frames along the open path. Most of that path runs
behind the RMW vtable, which the static walk charges at the deepest scoped
target, not at the frame that actually runs. So one code change moved the two
numbers in opposite directions, and the allowance more than doubled.

This is the strongest evidence so far that the allowance cannot be derived
from the static number. It has to be measured per (rmw, target, role) and per
build mode, and it has to be measured again when the build mode changes. Slot
binding (this issue) is what would make the static walk follow the real
target and close the gap.

## Allowances are per image now (issue 2400 ruling (A), 2026-11-10)

Because the indirect charge is wrong in both directions (+2,992 to +7,692 B
on Rust images, −12,616 B on the C talker), an allowance is measured per
image shape and may be negative, and an unmeasured image is never enforced.
Slot binding is what would let one measurement stand for a target: once the
walk follows the real vtable target, the static number becomes a bound, and
per-image rows can retire.

## Measured: the walk's worst path is not the run-time one (2026-11-10)

Issue 2401 (#3481) moved the session-property buffers off the stack:
`CffiSession::open_marshalling_properties` went from 8,472 B to 240 B. That
frame was on every nucleo image's static OPEN worst path. Re-measured on the
nucleo-f767zi twin (main-thread peak over 7 rounds):

| image | walk before | walk after | peak before | peak after |
| --- | ---: | ---: | ---: | ---: |
| C talker (no LTO) | 24,312 | 16,112 | 11,696 | 11,696 |
| Rust talker (LTO) | 23,220 | 22,212 | 28,888 | 28,888 |
| Rust action server (LTO) | 22,564 | 21,780 | 30,256 | 30,256 |

Not one peak moved, so on these images the main thread does not peak in
open. The walk only put open on its worst path because of this issue's
mechanism: it charges each indirect call its deepest address-taken target
(8,392 B at 51 sites on the C talker). On PX4, by contrast, open IS the
peak; 2401's Renode `mm_free` fault was STACK_MAIN overflowing in open.

Each allowance rose by its walk's drop (C talker −12,616 → −4,416). A row
left stale would have given the C talker a 3,848 B floor against its
11,696 B peak.

## Resolution (2026-11-14): bind every indirect call, and record peaks as observations

**The cause was wider than the vtable.** A gdb watchpoint caught the real
peak of `examples/mps2-an385-baremetal/rust/talker`. A 1 KiB window between
31 and 32 KiB below the stack top was pre-filled, and the first write into it
stopped the run.

- The deepest write was at 31,848 B, inside smoltcp's TCP dispatch.
- Its path crossed FOUR indirect calls: a Rust `dyn` call into
  `ExecutorSink::create_entity`, this issue's RMW vtable, zenoh-pico's link
  write, and the smoltcp poll below it.
- `fold1` charged the first of them a flat 9,520 B and walked nothing behind
  it. That gave 23,968 B, and the image overflowed its MPU guard at 31 KiB.

So binding the RMW vtable alone would not have fixed it.

**`--indirect bind`** turns every indirect site into call edges, so one walk
follows them all. Targets come from the Rust codegen IR, which `llvm-dis`
reads from the bitcode the frames were compiled from:

1. **Slot.** The functions at the loaded byte offset in every constant
   function table (a `dyn` vtable, the C RMW vtable), plus every function of
   the site's type whose address escapes anywhere else. The second half keeps
   a runtime-filled table sound.
2. **Type.** Otherwise, every address-taken function of the site's LLVM
   function type.
3. **What C can hold.** A site in C (no IR), or a Rust site no function of its
   type matches, reaches every function whose address C code takes, plus
   every Rust function whose address escapes.

Two loader fixes go with it:

- A function whose address CODE loads (`movw`/literal pool) now counts as
  address-taken.
- A frameless function the linked image does not define (the link dropped it)
  is a 0-byte leaf, and is named.

The cmake roads now pass `--cfi-elf` too.

**Measured.** The top of the mps2 path is now the gdb-measured run, frame for
frame, down to `ZenohPublisher::new`.

| image | fold1 (before) | bind | observed peak |
| --- | ---: | ---: | ---: |
| mps2 bare-metal Rust talker | 23,968 | 51,240 | 31-32 KiB |
| nucleo Rust action server | 21,780 | 64,712 | 30,256 |
| nucleo Rust talker | 22,212 | 63,272 | 28,888 |
| nucleo C talker | 16,112 | 123,412 | 11,876 |

**Allowances retire under `bind`.** Ruled 2026-11-14: no negative bytes,
because a negative allowance puts the floor back at one observed run. The
floor is the bind static plus the margin, never lower. A measured peak becomes
an `[[observed]]` row:

- it makes the image MEASURED, so a short stack fails the build;
- it is evidence: a walk below its peak refuses, as a walker defect;
- it carries the `static` that `nros new` stamps a template's stack from.

`--validate-table` refuses an `[[allowance]]` under a `bind` rmw. The twin
lane checks `[[observed]]` rows.

**What it costs.** The C talker reads 10x its run. The excess is C: zenoh-pico
calls through untyped pointers, and each such site reaches every C callback.
Tightening is tracked separately:

- [issue 2516](../2516-stack-floor-bind-c-site-types.md): C call-site types
  from clang IR.
- [issue 2517](../2517-stack-floor-prune-by-declared-entities.md): pruning
  entity kinds an image never declares, and the 25 KB action-client frame.
