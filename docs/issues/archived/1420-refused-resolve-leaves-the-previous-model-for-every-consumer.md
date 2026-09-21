---
id: 1420
title: "A refused resolve leaves the previous system_model.yaml on disk, and
  entity-inventory, codegen-system and codegen entry derive from it - the
  provenance check lives only in run_sync"
status: resolved
type: bug
area: [cli, cmake, build]
severity: high
found: 2026-09-21
resolved: 2026-09-21
resolved_in: "PR #1165 (phase-460 W1) - quarantine on refusal, one model gate at every door"
related: [issue-1121, issue-0320, issue-0427, phase-460, rfc-0063]
---

> **RESOLVED 2026-09-21 by phase-460 W1 (PR #1165).** Digest; the full
> observation is in the phase doc and brief D (E7b).

## What was observed

Brief D experiment E7b on the Autoware Safety Island (nano-ros pin f8655e9b7):
a contract edit the `rate-hierarchy` rule refused. `just sync` exited 1 with
`refusing to emit a SystemModel: 2 contract error(s)`, and the
`build/nros/models/safety_island_bringup/system_model.yaml` left by the
PREVIOUS experiment stayed on disk. `nros ws entity-inventory`,
`nros codegen-system` and `nros codegen entry` all ran against it: the
inventory said `NROS_DERIVED_MAX_PARAMETERS 26`, the previous experiment's
number, for a tree whose authored contract no longer said that.

The cause, verified at 783cdfa14: `nros-launch-resolve` writes only after a
successful resolve, `run_sync` propagated the error and left the previous
file untouched, and `model_provenance_stale` re-hashed the inputs the model
RECORDS - those of the resolve that succeeded - with `run_sync` as its only
caller. `nros model-path` checked nothing; the three consumers loaded
whatever the path held. A model whose producer last said no was current by
every check a consumer performed.

## What fixed it

phase-460 W1: quarantine on refusal, verify at every door.

1. On refusal `run_sync` moves the previous model to
   `<stem>.refused-<utc-stamp>.yaml` beside it (kept for diffing) and writes
   a two-line `<stem>.refused` marker naming the refusing check and the
   input that changed. Only the next SUCCESSFUL resolve clears the marker;
   editing the inputs back does not.
2. `model_provenance_stale` moved into `nros_cli_core::model_gate` as
   `provenance_stale` and gained the second hash input: a file the launch
   tree names (launch file, includes, their contract sidecars,
   `system.toml`) that `meta.inputs` never recorded - issue 1121 made
   visible, not fixed.
3. `model_gate::verify` (marker, resolver pin, recorded hashes, launch
   tree) and `verify_search` (the marker on every rung of the model ladder)
   are called by `model-path` (cmake fails at configure), `ws
   entity-inventory`, `codegen-system` on both roads, and `codegen entry`.

Not covered: the `nros::main!` macro opens the model through
`model_location::ensure_model` in `nros-macros`, which cannot depend on the
CLI crate. It re-resolves from the inputs when the artifact is absent (what
quarantine leaves), so it never reads a quarantined model, but it does not
read the marker. `nros build` (`cmd/build.rs`) reaches `plan_from_model`
without the gate as well.

## Acceptance, as measured

`packages/cli/nros-cli-core/tests/refused_resolve_leaves_no_model.rs`
(`cli-tests`, `just ci l1`): the fixture resolves and all four doors
accept; its contract is edited to `qos: { depht: 4 }` and sync is refused
(`[manifest-parse] unknown key in qos`); the model is gone, the quarantined
copy and the marker exist, the marker names `launch/system.contract.yaml`,
and all four doors refuse naming the marker; edited back without a sync,
all four still refuse; synced, all four accept, the marker is gone and the
copy is kept. `test result: ok. 1 passed`.
