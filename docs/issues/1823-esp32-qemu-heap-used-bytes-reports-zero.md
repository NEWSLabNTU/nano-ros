---
id: 1823
title: "`nros-platform-esp32-qemu` does not implement `heap_used_bytes` / `heap_total_bytes`, so the platform ABI reports 0 there"
status: open
type: tech-debt
area: [platform, memory]
severity: low
found: 2026-10-12
related: [phase-230, rfc-0034, 1525, phase-394]
---

**Home: [phase-394](../roadmap/phase-394-memory-campaign-ledger.md)** — homed 2026-10-17; listed in that phase's "Issues homed here (2026-10-17)".

## What is left

[phase-230](../roadmap/archived/phase-230-platform-layer-split.md) added
`nros_platform_heap_{used,total}_bytes` to the platform ABI with per-port
implementations. Its archive note (2026-10-11) says the gap left over is "the
bare-metal `FreeListHeap::used` still returns 0". Re-measured 2026-10-12, that
sentence is out of date in one place and still true in another:

- **mps2-an385 and stm32f4 report real numbers.** Both override
  `PlatformAlloc::heap_used_bytes` with `memory::used()`, which reads
  `FreeListHeap::used()` from `zpico-alloc` built with `features = ["stats"]`
  (`packages/platform/nros-platform-{mps2-an385,stm32f4}/src/{lib,memory}.rs`).
- **esp32-qemu does not.** `packages/platform/nros-platform-esp32-qemu/src/`
  overrides neither `heap_used_bytes` nor `heap_total_bytes`, so it falls back
  to the trait default in `nros-platform-api/src/lib.rs`, which returns `0`
  ("unknown").

`0` is a legal answer in the ABI ("this port does not instrument it"), so this
is a missing measurement, not a wrong one.

## What closes it

Implement both methods on esp32-qemu from its heap, the way mps2-an385 does.
ESP32 is dormant (issue 1525), so this is low priority. Do it when that platform
comes back, or close this as wontfix if it is retired.
