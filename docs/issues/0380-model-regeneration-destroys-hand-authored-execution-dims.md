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

## Landed (2026-08-02) — direction C + the gate

**C, the regeneration guard.** `nros sync` resolved straight over the
committed model (`-o <model>`), which made destruction the default AND the
check impossible — once overwritten there is nothing left to compare. It now
resolves to a side file, compares `execution.tiers` dim KEY SETS, and refuses
to commit a narrower model:

```
Error: sync: re-resolving `demo_bringup` would DROP 10 hand-authored execution
dim(s) from config/system_model.yaml:
  - execution.tiers.high.zephyr.deadline_us
  - execution.tiers.low.threadx.preempt_threshold
  …
```

Key sets, not values: a changed value is a re-resolve doing its job, while a
key that DISAPPEARS is content the inputs could never have produced.
`--allow-dim-loss` is the deliberate escape hatch.

Verified against the real regression rather than a synthetic one — running
`nros sync` on `ws-realtime-rust` reproduces it exactly: refused, model left
byte-identical, no stray side file. With `--allow-dim-loss` the same run
strips the model from 15 dim-lines to 2, which is what the two historical
commits did.

**The gate.** `check-model-dims` (in `check-fast`) compares every committed
model's dim set against `scripts/model-dims-baseline.txt` and fails in both
directions — a lost dim is data loss, a new one must be recorded so the
baseline keeps meaning something. 86 dims across 11 models today.

It asks the CLI (`nros ws model-dims`, hidden) rather than re-parsing YAML in
shell, so the gate and the sync-time guard share one definition of "a dim".
That matters concretely: `spin_period_us` is a tier dim and `nuttx.period_us`
is the sporadic one, so a shell `grep period_us` would conflate them.

**`--allow-dim-loss` was removed (2026-08-02).** It was a one-flag path to the
exact data loss the guard exists to prevent, and the legitimate workflow never
needed it: to retire a dim you remove it from the model, at which point there
is nothing to drop and the change lands as a reviewable diff. It was also
nearly inert before the delete-path fix — anyone blocked could simply delete
the model instead.

**Resolution direction settled: RFC-0063.** The model becomes a DERIVED BUILD
ARTIFACT, generated on the fly into `build/` (colcon-style) and exposed for
inspection, with the user maintaining launch file + project config + system
config. That makes direction A mandatory rather than optional — the dims must
become expressible in user-maintained config, because a derived artifact cannot
hold hand-authored content.

The guards here are therefore TRANSITIONAL: they protect data that will no
longer live in a committed file, and should be removed in the same change that
deletes the last committed model — not before.
