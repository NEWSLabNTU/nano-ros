---
id: 1420
title: "A refused resolve leaves the previous system_model.yaml on disk, and
  entity-inventory, codegen-system and codegen entry derive from it - the
  provenance check lives only in run_sync"
status: open
type: bug
area: [cli, cmake, build]
severity: high
found: 2026-09-21
related: [issue-1121, issue-0320, issue-0427, phase-460, rfc-0063]
---

## What was observed

Brief D experiment E7b on the Autoware Safety Island (nano-ros pin f8655e9b7,
2026-09-18): a contract edit that the `rate-hierarchy` rule refuses. `just
sync` on the island exits 1 with `refusing to emit a SystemModel: 2 contract
error(s)`. The `build/nros/models/safety_island_bringup/system_model.yaml`
left by the PREVIOUS experiment (E6b, which had added a string parameter)
stayed on disk, and `nros ws entity-inventory`, `nros codegen-system` and
`nros codegen entry` all ran against it: the inventory diff showed
`NROS_DERIVED_MAX_PARAMETERS 26`, E6b's number, for a tree whose authored
contract no longer said that.

Inside a cmake configure the failing sync stops the build. A hand-run
configure, a CI step that runs the verbs separately, or an `nros sync` whose
error scrolled past does not notice.

## Where the check is, and where it is not (verified at 783cdfa14)

* `nros-launch-resolve` writes its output only after a successful resolve
  (`packages/cli/nros-launch-resolve/src/main.rs:172`). On refusal nothing is
  written and nothing is removed.
* `run_sync` stages, stamps the resolver pin and renames on success
  (`packages/cli/nros-cli-core/src/cmd/ws.rs:2338-2348`); on refusal the error
  propagates at `ws.rs:2268` and the previous file is untouched.
* `model_provenance_stale` (`ws.rs:1448`) re-hashes the inputs the model
  RECORDS in `meta.inputs`. Those are the inputs of the resolve that succeeded.
  After a refused edit the recorded hashes still match the file the refused
  edit was applied to only if the edit was reverted; while the edit is in
  place the hash differs, so `run_sync` would re-resolve - and be refused
  again. `run_sync` is the only caller.
* `nros model-path` (`packages/cli/nros-cli-core/src/cmd/model_path.rs`)
  maps input coordinates to the path and checks nothing about the file;
  `nano_ros_add_executable` takes that path (`cmake/NanoRosEntry.cmake:286-314`).
* `ws entity-inventory`, `codegen-system` (`model_ingest::load_model`) and
  `codegen entry` (`plan_from_model`) load whatever the path holds.

So a model whose producer last said no is current by every check a consumer
performs.

## Why it is a bug and not a workflow note

The model is the single source for entity counts, buffer bounds, the parameter
store and the entry's node list (RFC-0063: a build artifact, never committed).
Every downstream refusal - `ExecutorFull`, `DECLARED_DEPTH_MISMATCH`,
`DECLARED_PARAM_MISMATCH` - compares the code against THIS file. A stale file
turns those refusals into confirmations of the wrong contract. Issue 1121 is
the same model going stale from the other direction (a new sidecar is not a
recorded input); this one is the producer refusing and the consumers not
knowing.

## What would fix it

phase-460 W1, in one sentence: quarantine on refusal, verify at every door.

1. On refusal `run_sync` renames the previous model to
   `<model>.refused-<utc-stamp>.yaml` and writes a `<model>.refused` marker
   naming the check and the input. Kept for diffing, not deleted.
2. `model_provenance_stale` becomes a shared `model_gate` and gains the
   current hashes of every launch-tree input, not only the recorded ones.
3. Every consumer calls it: `model-path` (so cmake refuses at configure),
   `ws entity-inventory`, `codegen-system`, `codegen entry`,
   `nros::main!`'s `load_for_build_script`. A `.refused` marker refuses until
   the next successful sync removes it.

## Acceptance

A fixture test edits a contract to an unknown key, runs sync (refused), and
asserts that `model-path`, `entity-inventory`, `codegen-system` and
`codegen entry` exit non-zero naming the marker; reverting and syncing makes
all four pass. Fast tier.
