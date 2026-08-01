---
id: 0380
title: Model regeneration destroys hand-authored execution dims — 17 realtime
  e2e tests silently lost their subject
status: open
severity: high
created: 2026-08-01
tags: [orchestration, system-model, realtime, process]
related: [0320, 0361, 0368]
rfcs: [RFC-0047, RFC-0052]
phases: [phase-296, phase-327]
---

## What happened

The ws-realtime committed SystemModels carry **hand-authored** phase-296 W5
scheduling/placement dims — `class: real_time`, zephyr-scoped `deadline_us`,
nuttx-scoped `budget_us`/`period_us` (sporadic), threadx-scoped
`preempt_threshold`/`time_slice_us`, per-platform `core` pins — with long
doc comments explaining each arm. `system.toml` deliberately does NOT carry
them (its `nros_orchestration_ir` schema is stricter; the model YAML was the
declared SSoT for these dims — see the comment block in
`ws-realtime-rust/src/demo_bringup/system.toml`).

Two regeneration commits deleted the committed models and re-resolved from
`launch + system.toml`, which cannot reproduce content that never existed in
the inputs:

- `07650d0a1` (#320, 2026-07-28) stripped ws-realtime-cpp-mps2 (freertos
  `core` pin, from f38cbea5c).
- `6071bd150` (#361 Part2 gap, 2026-07-31, "Force-regenerated — delete
  committed system_model.yaml + nros ws sync") stripped ws-realtime-rust
  (12 dims) and ws-realtime-cpp (5 dims).

Every fixture rebuilt since bakes tier tables with priorities only. The
first full `test-all` on a clean host (2026-08-01) failed ~17 tests across
the family: `*_core_pin_applied` (posix/zephyr/freertos/nuttx/threadx),
`threadx_preempt_threshold_applied`, `threadx_time_slice_applied`,
`nuttx_sporadic_budget_applied`, `zephyr_edf_deadline_applied` (×3),
`realtime_tiers_e2e` native + nuttx cases — all reporting the RFC-0052
fail-loud violation they exist to catch: the dim was silently dropped
(this time at REGENERATION, not at boot).

## Restoration

The dims were restored surgically (tiers block from git history grafted
into the current files, keeping the newer `deploy:` tables and provenance
headers) for ws-realtime-rust, ws-realtime-cpp, ws-realtime-cpp-mps2 in
the commit that references this issue.

## The class problem (why it WILL happen again)

Two live conventions contradict each other:

1. **#320 content-addressed staleness**: a committed model must be
   `resolve(inputs)` with `meta.inputs` sha256 provenance; when inputs
   drift, the remedy is "delete + re-resolve" — which several sessions now
   do routinely (`7401468fb` converted another hand-written model the same
   night).
2. **phase-296 W5**: the model YAML is the SSoT for scheduling dims the
   resolver inputs cannot express.

Anything that is hand-authored INTO a file whose maintenance procedure is
"regenerate from inputs" is data waiting to be deleted.

## Fix directions (pick one, then gate it)

- **A (preferred): give the dims a resolver input.** Extend the system.toml
  schema (`nros_orchestration_ir` + ros-launch-resolve `sched_loader`) to
  carry per-platform scoped tier dims (`zephyr.deadline_us`,
  `threadx.preempt_threshold`/`time_slice_us`, `nuttx.budget_us`/
  `period_us`, per-platform `core`), so `resolve(inputs)` round-trips them
  and regeneration is always safe. The resolver already carries
  `posix.core`/`sched_class`; this widens the same table.
- **B: overlay file.** A committed `config/system_model.overlay.yaml`
  (hand-authored, merged at ingest) that regeneration never touches.
- **C: regeneration guard.** `nros sync` refuses to shrink an existing
  model's `execution.tiers` key set (same shape as the #368/W5 patch-table
  narrowing guard) unless `--allow-dim-loss` is passed.

In all cases: add a gate that diffs baked tier dims in the realtime
fixtures against the committed model, so a stripped model fails the BUILD,
not a QEMU e2e three tiers later. (Issue-0196 rule: the gate must watch
the same inputs the tests consume.)
