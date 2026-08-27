---
id: 857
title: "ComponentCell's inline registries cost worst-case × biggest-payload heap per component"
status: open
area: api
severity: high
found: 2026-08-28
phase: phase-391
related: [0843, 0816]
---

# ComponentCell's inline registries cost worst-case × biggest-payload heap per component

## Symptom

`test_esp32_workspace_entry_e2e` red 3/3 solo after the phase-391 W5 heapless
port: the image dies with

```
memory allocation of 17468 bytes failed
```

right after `Ethernet ready.` — a clean-looking OOM in the #184 class. The
single-node esp32 examples stay green (same `.stack` = 60.4 KB, so the #64/#190
stack-overflow explanation is dead; this is genuine heap exhaustion).

Symbolized backtrace (addr2line on the exact fixture ELF) puts the failing
`Box::new_uninit` inside `listener_pkg::register` — it is the **second**
component's `Arc<ComponentCell>` allocation. The first cell plus the executor
arena backing (`NROS_EXECUTOR_ARENA_SIZE` 16384 + 1084 table overhead — the
same 17468, a coincidence that misdirected the first hour of triage) had
already consumed the 48 KB esp-alloc heap.

## Cause

W5 ported the cell registries from `alloc::Vec` to inline
`heapless::Vec<_, CELL_REG_CAP>`. That moved the cost model from
**pay-per-actual-entity** to **pay-worst-case-always**, and the worst case
multiplies by the biggest payload type:

```
publishers: heapless::Vec<(IdStr, EmbeddedRawPublisher), 8>
```

`EmbeddedRawPublisher` embeds `TxArena<DEFAULT_LOAN_BUF = 1024>`, so one entry
is ~1.35 KB and the vec alone is ~10.8 KB. With the three other registries
(~140 B × 8 × 3) the cell reaches ~17.5 KB — and it is still `Arc::new`'d, so
the whole thing lands on the Rust heap per component. A listener with ZERO
publishers pays for eight.

Pre-W5 the same cell cost ~a few hundred bytes plus per-actual-entity growth.

## Fix

- **Structural (phase-391 W5 endgame, task: per-class exact cells):** the
  macro knows the class's declared entity set at emit time; emit a cell whose
  registries are sized to the DECLARED counts, placed in the per-class static
  beside the slot store — no `Arc`, no heap, no worst-case padding. This issue
  is the measured motivation for that wave.
- **Interim (landed with this issue):** per-image knob. The esp32 workspace
  fixture row sets `NROS_RUNTIME_MAX_CELL_ENTITIES=2` (its two components
  declare one publisher + one subscription); cell drops ~17.5 KB → ~3.5 KB.

## Sweep

Any embedded image that registers components through `node_runtime` pays
CELL_REG_CAP × ~1.35 KB per cell. Zephyr images absorb it in the 64 KB rlsf
arena today; esp32's 48 KB esp-alloc heap was the first to fail. Re-measure
per-image after the endgame lands (`just mem-report`).
