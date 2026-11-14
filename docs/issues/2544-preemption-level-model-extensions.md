---
id: 2544
title: "Preemption levels: EDF across levels and per-callback preemption thresholds are deferred with a decision point"
status: open
type: enhancement
severity: low
area: [executor, scheduling]
related: [rfc-0108, phase-555]
found: 2026-11-14
---

## What

RFC-0108 runs one ready set per core over N preemption levels in contiguous
bands. Two extensions were deferred with a stated trigger
(RFC-0108 "Decisions" 1 and "Deferred"):

1. **EDF across levels.** Today EDF orders callbacks within a level only.
   Ordering the levels themselves by deadline (Zephyr's deadline scheduling
   mode, for example) is decided when an image first needs EDF with more
   than one level.
2. **Per-callback preemption thresholds.** Bands cannot express overlapping
   non-preemptive groups (RFC-0108 D3's worked example). Thresholds are a
   backward-compatible extension, taken when a contract can state
   non-preemption or shared-state relations between callbacks, or for a
   kernel with native thresholds (ThreadX `tx_thread_preemption_change`).

## Why it is filed now

Phase-555 W3 normalized RFC-0108's status to Stable; deferred work goes to
an issue rather than staying only in the RFC.

## Acceptance

Each item is either taken up by a phase (its trigger met, named), or closed
with the trigger restated in RFC-0108 if the project decides it will not
need it.
