# Phase 448 — the executor backing, sized exactly and paid for once, on every port

**Status (2026-09-11). Opened to give the exact-size work a home. Nothing in
this phase has landed; W1–W8 are open. Zephyr's pairing (issue 1145, landed
2026-09-06) predates the phase and is the template the other ports follow.**

## Why this phase exists

[Phase 392 W6](phase-392-static-memory-space-campaign.md) moved the executor
arena into a named `.bss` static, `EXECUTOR_BACKING`, so the campaign's largest
allocation stopped being invisible to its own instrument. On a host that move is
free. On an RTOS it is not: the allocator arena the backing used to come out of
is itself a fixed static, and unless it is lowered by the same amount the image
reserves the bytes TWICE.

Zephyr was fixed within a day — twelve confs paired, the knob
`CONFIG_NROS_EXECUTOR_BACKING_U64S`, the gate
`check-executor-backing-arena-pairing`, and the subtrahend DERIVED rather than
copied from `nm` ([issue 1171](../issues/archived/1171-arena-backing-pairing-is-hand-maintained.md)).
Every other port is untouched, and sizing the backing exposed three more costs
along the way.

That work then continued as seven loose issues. No phase owned any of them,
which is the state phase-412 names as the failure: *a mention is not an owner —
an issue with no work item is an issue nobody is accountable for.* This doc is
the owner.

## What "exact" means here

The backing is the arena plus a set of fixed tables, and its size depends on the
knobs AND the target: **87,256 B on mps2_an385 against 88,328 B on
native_sim/native/64 for one conf.** So the number is always derived per target
(`nros-executor-layout` computes the offsets for a target's padding and
alignment), and a subtrahend is always the derivation, never a literal. A figure
copied from one board's `nm` is wrong on the next board and drifts on the next
knob move.

## Work items

### W1 — answer which allocator a Zephyr XRCE image has

[Issue 1189](../issues/1189-xrce-leaf-sets-an-arena-knob-its-image-lacks.md).
The six Zephyr XRCE Rust leaves set `CONFIG_COMMON_LIBC_MALLOC_ARENA_SIZE`, and
their images do not select picolibc, so the knob never reaches the resolved
`.config` and the comment above it describes another image. The issue refuses
the one-line fix on purpose: *should* an XRCE image be on picolibc like its
zenoh siblings? A silent libc difference between RMW variants of one example is
a bigger finding than a dead config line.

It comes first because it decides which heap W2 has to size.

- [ ] The question answered, with the reason recorded in the issue.
- [ ] Either the dead line removed and its comment corrected, or picolibc
      selected and the knob kept — and one XRCE leaf's resolved `.config` shows
      whichever was chosen.

### W2 — every Zephyr XRCE image boots

[Issue 1010](../issues/1010-zephyr-xrce-executor-arena-exceeds-heap.md). The
only item in this phase where an image does not run: every Zephyr XRCE example
dies at boot. The 2026-09-04 correction located the allocation — it is not the
executor arena but `xrce_session_state`, dominated by
`subscriber_slots[MAX_SUBSCRIBERS]` at `RING_DEPTH x (BUFFER_SIZE + 16)`.

- [ ] Size the session (or the heap it comes from) so it fits — the issue
      measured that ring depth alone does not.
- [ ] The nine Zephyr XRCE cells boot, confirmed by running them.

### W3 — the FreeRTOS C/C++ carrier's stack is measured

[Issue 1146](../issues/1146-freertos-app-task-stack-never-derived.md), part 2.
`cmake/templates/freertos_app_config.c.in` keeps `.app_stack_bytes = 524288u`
for both typed carriers. Its C half measures 18,176 B at worst. Its C++ half was
unmeasurable because no embedded C++ FreeRTOS image built; that blocker
([issue 1187](../issues/archived/1187-freertos-embedded-cpp-freestanding-string.md))
is resolved, so the number can now be taken and has not been.

Part 1 (the Rust app task, 384 KiB → 128 KiB, measured on 14 running images) is
done.

- [ ] A `examples/mps2-an385-freertos/cpp/*` image measured the way part 1 was:
      `uxTaskGetStackHighWaterMark` at the end of the register pass, on a running
      image against a live router.
- [ ] The carrier default set from both halves, and the image's boot print
      confirming the peak.

### W4 — FreeRTOS reserves the backing once

[Issue 1197](../issues/1197-freertos-heap-cannot-learn-the-backing-size.md), and
the FreeRTOS half of [issue 1145](../issues/1145-executor-backing-static-unpaired-with-rtos-heap.md).
Measured: every FreeRTOS Rust image reserves its 20,608–32,512 B backing twice,
because `configTOTAL_HEAP_SIZE` is still budgeted for an arena that moved to
`.bss`. Neither Zephyr mechanism transfers. The 2026-09-07 attempt established
that the board crate cannot be the consumer: it cannot see the size, and stating
the size would fight the per-leaf derivation. `nros-node` now publishes the
backing size for probing (`936064f25`), and the board still cannot read it.

W3 comes first, for the reason the issue gives: the app stack and the backing
both come out of the SAME heap budget, so re-deriving that budget twice in two
PRs is how it ends up wrong.

- [ ] The layering decided — route the backing size to whatever computes
      `configTOTAL_HEAP_SIZE`, or move that computation to a layer that can see
      it — and the decision recorded in issue 1197.
- [ ] The heap default re-derived ONCE, against both W3's and this item's
      reductions, as a derivation that tracks the backing rather than a literal
      (so W6 cannot re-stale it).
- [ ] Every affected FreeRTOS image RUN on QEMU against a live router. A too-small
      heap is a runtime allocation failure, and running is the only bar that
      catches it.

### W5 — NuttX, ThreadX, ESP32

The rest of [issue 1145](../issues/1145-executor-backing-static-unpaired-with-rtos-heap.md).
Untouched. **One platform per commit**, as the issue specifies: the failure mode
is a runtime allocation failure, and a three-platform diff makes it
unattributable. The knob is Zephyr-only so far; on the other ports it is spelled
through the `NROS_*` build environment.

- [ ] NuttX paired, measured, and a running image.
- [ ] ThreadX paired, measured, and a running image.
- [ ] ESP32 paired, measured, and a running image.
- [ ] `check-executor-backing-arena-pairing` covers each paired port, or records
      why that port cannot be checked.

### W6 — the fixed tables shrink to what the image declares

[Issue 1198](../issues/archived/1198-executor-node-and-sc-slots-are-undeclared-defaults.md).
`MAX_NODES` and `MAX_SC` are undeclared defaults. Every FreeRTOS backing carries
a constant **12,416 B** of tables, identical to the byte across leaves with
different declarations. `MAX_NODES` is the heavier of the two: it multiplies
seven tables.

This is a design question before it is code, and the issue lists it:

- [ ] Find which artifact already states the node count — the model, the
      contract sidecar, or the entry. An entry may create several `Node`s, and
      tiered boot opens one executor per tier. This is close to issue 0973's
      answer: wiring is authored.
- [ ] Decide where `MAX_SC` comes from. Scheduling contexts are the tier model's
      (RFC-0016), not the entity inventory's, so they may belong to a different
      declaration than `MAX_CBS`.
- [ ] Both derived, with a FreeRTOS `nm -S` before and after.

### W7 — each subscription priced at its own type

[Issue 1255](../issues/archived/1255-arena-prices-every-subscription-slot-at-one-global-type-bound.md).
`subs_arena()` bills every declared subscription at one image-wide bound — 880 B
on the reference island — although the per-type bound is generated and reaches
the build script: the declared-depth triples are `type|topic=depth`, and the
parse keeps only the depth. Headroom, not a blocker (DTCM is at 74.8 %). It
becomes a blocker when the next subscription is added to a part with 128 KB of
DTCM.

- [ ] `subs_arena()` sums `buffered_region(depth_i, bound(type_i)) + entry_struct`,
      and falls back to the global bound when a type's bound is absent (absent
      is not zero).
- [ ] The host oracle (`cargo:arena_size`) and the island's linked
      `EXECUTOR_BACKING` agree, measured before and after.

### W8 — the per-entry arena scales by a ratio that stopped being the model

[Issue 1290](../issues/archived/1290-arena-size-for-halves-a-declared-model.md), found by
phase-412 item 4's oracle on the day it landed — which is the argument for
having built the oracle at all.

`nros_node::config::arena_size_for(cbs)` sizes a per-entry executor's arena as
`ARENA_SIZE * cbs / MAX_CBS`, floored at `ARENA_SIZE / MAX_CBS`. Its doc says
why: *"the same per-slot arena budget the global default used"*. That was true
while `ARENA_SIZE` WAS a per-slot budget. Since phase-403 step 3 it is not on an
image that declares its entities: `build.rs` sums the model per KIND and
`MAX_CBS` is a separate knob, so `ARENA_SIZE / MAX_CBS` means nothing and
scaling it by `cbs` can hand an image's ONE executor a fraction of what its own
entities need.

MEASURED on `nros-bench/large-msg-baremetal` with one declared subscription:

| | bytes |
| --- | ---: |
| `cargo:arena_size` (the derived default, = the model) | 14,424 |
| `arena_size_for(2)` with `MAX_CBS` = 4 | **7,212** |

Half. Nothing in the tree is broken TODAY because the bench declares no
entities, and the oracle now refuses the build rather than letting it die at
registration with `BufferTooSmall`. So this is a latent trap for the next image
that declares entities and sizes a per-entry executor — which is what W6 and W7
both make more common.

- [ ] `arena_size_for` asks the model when the image declares one, and keeps the
      ratio only for the undeclared case (where `ARENA_SIZE` still IS a per-slot
      budget) — or the ratio goes and the undeclared case states its own floor.
- [ ] The bench declares its entities, so the path is exercised rather than
      merely reasoned about.
- [ ] A positive control: the oracle fires on the old arithmetic and does not on
      the new.

Take it WITH W7 — both are "the arena's number stopped matching the model", one
per entry and one per subscription, and they touch the same derivation.

## Order

W1 and then W2 first, because an image that does not boot outranks one that
wastes bytes. Then W3 and then W4, together, because they reduce one budget.
W5 is one port at a time, in any order. W6, W7 and W8 are tightness work and can
go in parallel with the rest — W7 and W8 together, since both are the arena's
number having stopped matching the model.

## Not owned here

- **Arena exhaustion diagnostics** — issue 1036, homed in
  [phase 412](phase-412-derived-counts-and-sizes.md), whose second rule is that a
  derived count is safe only where exhaustion names the knob.
- **The runtime stack-headroom rule** — phase 436.
- **Main-stack and Zephyr-heap derivation** — phase 412 W2's table, which records
  the blocker for each.

## Acceptance

1. On every port, `nm -S` shows `EXECUTOR_BACKING` and an allocator arena that no
   longer also budgets for it. Each is reported as a before/after pair, and the
   subtrahend is a derivation, never a copied literal.
2. Every Zephyr XRCE cell boots.
3. No executor table is sized by a default the image did not declare, unless the
   phase records why that count cannot be known.
