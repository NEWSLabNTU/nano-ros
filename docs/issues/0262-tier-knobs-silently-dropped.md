---
id: 262
title: "Declared tier knobs silently dropped: threadx core, zephyr stack_bytes, posix priority/stack/core, freertos uniproc pin"
status: open
type: bug
severity: medium
area: boards
related: [rfc-0052, phase-296, phase-302]
---

## Finding (implementation-completeness audit, 2026-07-25)

RFC-0052's rule: an unconsumed `Some(..)` tier knob is a fail-loud
violation. Four live silent drops:

1. **threadx `core`** — caps claim `affinity: true` but `TierSpec.core`
   is never read in `nros-board-threadx/src/entry.rs`; no
   `tx_thread_smp_core_exclude` consumer exists.
2. **zephyr `stack_bytes`** — `nros_zephyr_tier_task_create` has no
   stack parameter (fixed `NROS_ZEPHYR_TIER_STACK_SIZE` 16 KiB pool);
   a declared per-tier stack is ignored without warning.
3. **posix `priority`/`stack_bytes`/`core`** — run_tiers consumes only
   groups/spin_period/policy; priority is documented-advisory but none
   of the three is surfaced at bake or boot.
4. **freertos uniprocessor core-pin** — the `(void)task` branch drops a
   declared pin silently (already acknowledged in phase-296 as a
   fail-loud follow-up; folded here).

## Fix direction

Per knob: bake-time reject where the platform can never honor it
(threadx core until an SMP consumer exists; posix priority-as-advisory
gets a bake-time NOTE), loud boot-time warning where it is
config-dependent (freertos uniproc pin), or implement the consumer
(zephyr per-tier stack: size the pool slots or take a stack param).
Phase-302 W2.
