---
id: 1877
title: "`zephyr_cortex_m_c_boot_report_names_arena_exhaustion` asserted one build's byte counts, so it failed when the first entity's arena claim grew from 48 to 56 bytes, with the decoder still right"
status: resolved
type: bug
severity: low
area: [testing, zephyr]
related: [phase-492, 1874]
found: 2026-10-13
---

## Measured

Tier 2 on 2026-10-13: `zephyr_cortex_m_qemu::zephyr_cortex_m_c_boot_report_names_arena_exhaustion`
failed with "the decode does not say `short by 16 bytes`". The decode it
printed was complete and correct:

```
arena capacity                32
ARENA EXHAUSTED: an allocation of 56 bytes did not fit,
short by 24 bytes.
  set NROS_EXECUTOR_ARENA_SIZE >= 56
```

The test asserted the literals `short by 16 bytes` and
`set NROS_EXECUTOR_ARENA_SIZE >= 48`, which were the first entity's claim on
the day they were recorded. That claim is now 56 bytes after `main`'s changes
since; phase-492 reshaped the C entity structs.

## Cause

A decoder test pinned to one struct's size. Its question is "does the boot
report name the exhaustion, the shortfall and the knob". It went red when an
unrelated struct grew, so it was really guarding the struct's size, and a
`sizeof` assertion is not this test's job.

## Fix

The test now checks the decoder's arithmetic, which holds whatever the claim's
size:

- the claim exceeds the arena capacity;
- the shortfall equals the claim minus the capacity;
- the knob the report names equals the claim.

All three numbers are parsed from the decode. Against the failing run's decode:
56 − 32 = 24, and the knob says 56.
