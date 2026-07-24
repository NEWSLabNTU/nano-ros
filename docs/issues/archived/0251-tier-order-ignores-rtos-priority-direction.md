---
id: 251
title: "resolve_tiers ignores per-RTOS priority direction — the boot tier is the LOWEST-priority tier on ThreadX/Zephyr"
status: resolved
type: bug
severity: low
area: orchestration
related: [rfc-0053, rfc-0047]
---

## Resolution (2026-07-24) — option (b), comment/doc truthing

The descending-raw-number sort is KEPT as the deliberate v1 semantics (the
system owner authors numbers in the target RTOS's direction; `resolve_tiers`
does not invert — its own comment already said so). The defect was every
comment claiming "boot tier = highest priority": fixed in
`nros-orchestration-ir` (`ResolvedTierTable.tiers` doc now states the
direction caveat explicitly, naming this issue), the zephyr board
(`entry_tiers.rs` module + `run_tiers` docs, `zephyr_run_tiers.c` header)
and the threadx board (`entry.rs` W4 block; the W5 work had already truthed
the inner comments + phase/book docs). posix/FreeRTOS/NuttX comments were
already true (bigger-number-wins). A direction-aware sort (option a) remains
possible later but changes which tier BOOTS on Zephyr/ThreadX — rejected for
now to avoid destabilizing the in-flight phase-296 EDF/realizer work whose
fixtures embed the current order.

## Finding (2026-07-23, phase-297 W5 acceptance work)

`resolve_tiers` sorts the tier table **descending by raw priority number**
without inverting per RTOS direction
(`packages/core/nros-orchestration-ir/src/lib.rs` — "Highest RTOS priority
first. (The system owner authors numbers correct for the target RTOS's
direction; v1 does not invert.)"):

```rust
out.sort_by(|a, b| b.priority.cmp(&a.priority).then(a.name.cmp(&b.name)));
```

On **lower-number-is-higher-priority** RTOSes (ThreadX, Zephyr) this puts the
numerically-largest = **lowest**-priority tier at `tiers[0]` — which is the
BOOT tier in every `run_tiers` implementation. Consequences:

1. **The "boot tier = highest priority" comments are wrong on those RTOSes**
   (`nros-board-threadx/src/entry.rs`, and the equivalent assumption anywhere
   the #144 chain-spawn ordering is justified by "highest priority declares
   first"). On ThreadX/Zephyr the LOWEST tier declares first and the highest
   tier is chain-spawned last.
2. **The #144 rationale inverts**: the intent was that the most latency-
   critical tier's entity declares complete before lower tiers pile on; on
   inverted-direction RTOSes the critical tier declares LAST, eating the
   startup transient.
3. No correctness failure observed — the shipped zephyr rows (5/10) and the
   phase-297 threadx rows (5/15) both pass their ratio proofs — but the
   ordering is semantically backwards and every new board copies the wrong
   comment.

## Fix directions

Either (a) direction-aware sort: a per-RTOS `priority_ascending` flag in the
platform manifest, applied in `resolve_tiers` so `tiers[0]` is ALWAYS the
semantically-highest tier; or (b) keep v1 no-invert but fix every "boot tier =
highest priority" comment and document the per-RTOS boot-order in RFC-0053 /
RFC-0047. (a) touches zephyr + threadx `run_tiers` expectations at once —
cross-board, coordinate with the phase-296 realizer work.

Phase-297 W5 chose (b)-style doc corrections locally for threadx
(`entry.rs` boot-tier comment + phase doc); zephyr's comments are untouched.
