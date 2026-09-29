---
id: 1573
title: "The boot-report decoder named a PLATFORM HEAP failure as an executor ARENA failure and told the reader to raise the wrong knob"
status: resolved
type: bug
area: [boot-report, diagnostics]
severity: medium
found: 2026-09-29
related: [1036, 1424, 1425]
resolved_in: "branch fix/boot-report-names-the-heap"
---

## What

The boot record (`packages/core/nros-node/src/boot_report.rs`) has ONE pair
of words for a failed allocation, `failed_alloc_size` /
`failed_alloc_shortfall`, and two writers: `note_alloc_failed` (the executor
arena) and `note_heap_alloc_failed` (the platform heap, called from
`nros_platform_alloc`'s exhaustion path in `nros-platform-zephyr`). First
writer wins, which is right -- the first failure stops the boot -- but nothing
in the record said WHICH allocator wrote the pair. `scripts/read-boot-report.py`
read every failure as the arena's:

```
ARENA EXHAUSTED: an allocation of 236 bytes did not fit,
  set NROS_EXECUTOR_ARENA_SIZE >= ...
```

## Measured

Autoware Safety Island (phase-8 W1), QEMU `mps2/an385`, at the conf heap. The
console printed

```
nros: HEAP EXHAUSTED: request 236 bytes, arena 94720 bytes, caller 0x297bb
```

(`caller` resolves to zenoh-pico's `_z_slist_new`), and the decoded boot report
said ARENA EXHAUSTED for 236 bytes while the executor arena was 28% used. The
knob that fixes it is `CONFIG_NROS_ZEPHYR_HEAP_SIZE`; the one the decoder named
could not.

## Fix

* A 26th record word, `failed_alloc_arena`, APPENDED (record `VERSION` 7 -> 8,
  104 bytes), holding a `boot_report::AllocArena` code: 1 executor arena, 2
  platform heap. Both writers go through one `record_alloc_failure`, which
  stores it in the same first-writer branch as the pair it qualifies.
* The decoder reads it: a heap failure prints `HEAP EXHAUSTED`, says the
  executor arena is not the cause, and names
  `raise CONFIG_NROS_ZEPHYR_HEAP_SIZE >= <capacity + request>` (the request is a
  floor on the increase, since the heap is fragmented when it refuses). An
  unknown code refuses to name a knob.

## Tests

* `read-boot-report.py --self-test` gains `self_test_alloc_verdicts`: the
  island's shape (236 bytes, heap 94,720, arena about a quarter used) must say
  HEAP and name `CONFIG_NROS_ZEPHYR_HEAP_SIZE >= 94956` and must NOT say ARENA
  EXHAUSTED; an arena failure must still say ARENA; an unknown code must say
  neither. Run against the old verdict logic (both new branches disabled):

  ```
    self-test FAIL platform heap failure: exit 1 (want 1), missing ['HEAP EXHAUSTED', 'CONFIG_NROS_ZEPHYR_HEAP_SIZE >= 94956'], wrongly present ['ARENA EXHAUSTED', 'set NROS_EXECUTOR_ARENA_SIZE']
    self-test FAIL unknown allocator: exit 1 (want 1), missing ['ALLOCATION FAILED'], wrongly present ['ARENA EXHAUSTED']
  read-boot-report --self-test: FAILED
  ```

  and with the fix: `read-boot-report --self-test: OK`.
* `boot_report.rs`: `a_heap_failure_is_recorded_as_the_heap_and_an_arena_one_as_the_arena`
  (on records of its own, so it does not race the image static),
  `the_alloc_arena_codes_match_the_record`, the layout test renamed to
  `the_record_is_twenty_six_packed_u32s`.
* `check-boot-report-layout`: `BootReport 26 fields / 104 bytes ... all agreed
  with the decoder`.

## Note for readers of older dumps

A version-7 dump is refused by the new decoder, as every version change is.
Rebuild the image to read a heap failure correctly.
