---
id: 395
title: phase-331 W6 consolidated ws-realtime-cpp-mps2 into realtime-cpp but left the
  freertos tiers behind — the freertos fixture row points at a bringup that does not exist
status: resolved  # fixed 2026-08-03
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

## Fixed 2026-08-03

The `[tiers.*.freertos]` blocks are restored into BOTH bringups, with the values
recovered from `a92778843^` rather than guessed:

| | `realtime-c` | `realtime-cpp` |
| --- | --- | --- |
| `high.freertos.priority` | 5 | 5 |
| `low.freertos.priority` | 2 | 2 |
| `low.freertos.core` | — | 0 |

`tests/freertos_core_pin_applied.rs` is what settled the shape: its own header
says "the realtime-cpp `low` tier pins to core 0", so that dim has a live
consumer and the two-tier reading is the right one.

The fixture row now points at `src/demo_bringup` (the bringup that exists).

**Two baseline entries were W6 rename errors, not losses**, and are corrected by
the re-record:

* `realtime-cpp/src/deploy_bringup` — the `fast`/`bulk` dims are intact under
  `realtime-cpp-subnode-portable/src/deploy_bringup`; only the recorded PARENT
  was wrong;
* `mid.*` on `realtime-cpp/src/demo_bringup` — the three-tier mps2 system
  (ctrl/aux/telem) that W6 chose not to keep. No test consumes a `mid` tier and
  the fixture row's own comment describes two, so this one is a deliberate drop
  rather than an accident. `aux_pkg` still sits in the workspace unreferenced —
  worth a follow-up decision, not a silent restore.

Baseline re-recorded only after the genuine losses were back: 91 dims across 8
models, up from 80.

## Original fix plan

1. Decide where the mps2 freertos realtime system now lives — either restore
   `[tiers.high.freertos]` / `[tiers.low.freertos]` into
   `realtime-cpp/src/demo_bringup/system.toml`, or keep a separate deploy
   bringup as before.
2. Point the `workspace-cpp-freertos-realtime` row at whichever it is.
3. Re-record the dims baseline once the tiers are back, so
   `high.freertos.priority` returns to three instances rather than one.

Until (1) is answered the baseline should NOT be re-recorded as-is: writing it
now accepts the loss silently, which is what the gate is meant to prevent.

## Follow-up (2026-08-03) — the `mid` tier DOES have a consumer

The fix above restored two tiers and left `aux_pkg` unreferenced, on the reading
that "no test consumes a mid tier and the row's own comment describes two". The
comment was stale; the test is not:

```rust
// packages/testing/nros-tests/tests/realtime_tiers_e2e.rs — case::freertos_cpp
// THREE tiers — [aux] (50 ms, spawned BY a spawned tier) is the #144
// chained-spawn regression signal ...
proof: Proof::SerialTicks(&["ctrl", "aux", "telem"]),
```

Before W6 the row pointed at `ws-realtime-cpp-mps2/src/demo_bringup`, whose
launch is 3-node and whose `system.toml` carried `[tiers.mid]` — so the fixture
really did run three tiers, and the two-tier comment beside the row described
the OTHER workspace. Landing the two-tier reading would have left the #144
signal quietly unexercised: `SerialTicks` waits for an `aux` tick that a
two-tier image never emits, so the failure surfaces a QEMU tier away, which is
the distance issue 0380 is about.

Restored: `[tiers.mid]` (50 ms, posix 40, freertos 3) and the `aux_node`
component bound to it via `group_tiers = { aux = "mid" }`.

`aux_pkg` builds only on the mps2 board, so the shared `system.launch.xml` stays
2-node and the 3-node resolve is a VARIANT — `launch/freertos_system.launch.xml`
-> `config/freertos_system_model.yaml`, declared as a `[[model]]` block, which is
the pattern the `rclcpp` / `subnode` entries in this same bringup already use.
`freertos_entry` consumes that model. No second bringup, so the fold W6 made
still holds.

Verified: the generated `NativeTierSpec` table is three entries with the
spawn-parent chain the #144 fix serializes —

```
{ "high", …, 5LL, 0u,  10000ull, …, parent 0u }
{ "mid",  …, 3LL, 0u,  50000ull, …, parent 0u }   // spawned by high
{ "low",  …, 2LL, 0u, 100000ull, …, parent 1u }   // spawned by mid — two hops
```

Baseline re-recorded with the new model tracked first (the gate enumerates via
`git ls-files`, so an untracked model is invisible to it): 118 dims across 9
models.
