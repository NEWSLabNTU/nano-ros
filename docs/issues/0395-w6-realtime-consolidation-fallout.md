---
id: 395
title: phase-331 W6 consolidated ws-realtime-cpp-mps2 into realtime-cpp but left the
  freertos tiers behind — the freertos fixture row points at a bringup that does not exist
status: open
type: bug
area: testing
related: [rfc-0066, phase-331, phase-330, 0380]
---

## Problem

`a92778843` (phase-331 W6, "consolidate realtime, rename the survivors, drop
ws-") moved the `workspace-cpp-freertos-realtime` fixture onto a workspace that
cannot satisfy it, and dropped the tier declarations it was built to exercise.

`examples/fixtures.toml`:

| | before W6 | after W6 |
| --- | --- | --- |
| `dir` | `examples/workspaces/ws-realtime-cpp-mps2` | `examples/workspaces/realtime-cpp` |
| `bringup` | `src/demo_bringup` | `src/deploy_bringup` |

`realtime-cpp` has no `src/deploy_bringup` — the only `deploy_bringup` in the
tree belongs to `realtime-cpp-subnode-portable`, a different workspace with
`fast`/`bulk` tiers. So `check-fixtures-manifest` fails:

```
fixtures-manifest.py: workspace-cpp-freertos-realtime:
  missing bringup dir: examples/workspaces/realtime-cpp/src/deploy_bringup
```

## The part that is not just a path

Retargeting the row at `realtime-cpp/src/demo_bringup` would make the gate pass
and the fixture meaningless. **That bringup declares no freertos tiers at all.**
The row exists to exercise "one RTOS task per tier over one shared session —
ctrl (high tier, prio=5, 10 ms) + telem (low tier, prio=2, 100 ms)", and those
`[tiers.*.freertos]` blocks did not survive the consolidation.

Independent confirmation from the model-dims baseline, which is the gate that
exists precisely for this: `high.freertos.priority` was recorded for
`realtime-c` and `realtime-cpp` and is now present only in the
`orchestration_tiers_freertos` fixture. Two of three instances disappeared.

This is the issue-0380 shape once more — a declaration lost in a move, with the
loss visible only because a gate remembered it.

## Also in the same commit

`realtime-cpp-subnode-portable` lost its `README.md`: the rename deleted
`ws-realtime-cpp-subnode-portable/README.md` (38 lines) without recreating it
under the new name, so `check-example-matrix` failed. Restored from
`a92778843^` with the `ws-` prefixes updated — content is the original author's,
not invented.

## Fix

1. Decide where the mps2 freertos realtime system now lives — either restore
   `[tiers.high.freertos]` / `[tiers.low.freertos]` into
   `realtime-cpp/src/demo_bringup/system.toml`, or keep a separate deploy
   bringup as before.
2. Point the `workspace-cpp-freertos-realtime` row at whichever it is.
3. Re-record the dims baseline once the tiers are back, so
   `high.freertos.priority` returns to three instances rather than one.

Until (1) is answered the baseline should NOT be re-recorded as-is: writing it
now accepts the loss silently, which is what the gate is meant to prevent.
