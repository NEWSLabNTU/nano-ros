---
id: 2689
title: "The CLI's `SourceMetadata` rejects every schema v6 probe sidecar, and the C/C++ refresh swallows the error, so every sidecar reads as stale"
status: resolved
resolved_in: "PR #4190 (9afb5c8a19)"
type: bug
area: [cli, metadata]
severity: high
found: 2026-11-18
related: [2650, 0518, phase-307, phase-478]
---

**Home:** [phase-478](../../roadmap/phase-478-island-developer-experience-findings.md)

Found on the safety island's CI against nano-ros main `a49a8dacaf`: every
Zephyr C++ configure fails at `NanoRosPromise.cmake:402`.

## What happens

Issue 2650 (`d8c4a1383f`) moved the probe recorder to schema v6: the sidecar
gains top-level `parameter_description_max_len` and
`parameter_constraints_max_len`. The CLI's typed mirror,
`orchestration::source_metadata::SourceMetadata`, is
`#[serde(deny_unknown_fields)]` and did not gain them. So:

1. `stamp_provenance` fails to parse the sidecar the probe just wrote.
2. The C/C++ batch in `metadata_refresh` called it as
   `let _ = stamp_provenance(...)`, so the failure vanished and the sidecar
   stayed unstamped.
3. `sidecar_is_fresh` parses through the same struct and returns `false` on
   any parse error. Every sidecar is stale on every `nros configure`, so the
   counts and registration observations it carries are never used, and the
   Zephyr C++ promise check refuses.

Issue 0518 was the same shape for v2. A recorder schema bump that does not
touch the reader is invisible to every test, because the producer
(`packages/api/nros`) and the reader (`packages/cli/nros-cli-core`) share no
cargo workspace, and the only cross-check
(`the_recorder_schema_version_matches_the_recorder`) reads the version
number as text.

## Fix

- `SourceMetadata` carries both keys as `Option<u64>` with
  `#[serde(default)]`: an absent key means nobody watched, distinct from `0`.
- `stamp_provenance`'s error propagates from the C/C++ batch the way it
  already did from the Rust one. Its message names the serde error, which
  names the unknown field.
- A GOLDEN sidecar,
  `packages/cli/nros-cli-core/tests/fixtures/orchestration/probe_sidecar_golden.json`,
  is written by the real producer (`MetadataRecorder::write_source_metadata_json`)
  over a recorder holding every row kind and both v6 keys. The producer's test
  compares its output to the file (`NROS_BLESS_PROBE_SIDECAR=1` regenerates
  it); the CLI's test parses it strictly through `SourceMetadata` and round-trips
  it. A new key or a changed type on either side now fails a test.
- `fresh_probe_rows` reads `executable` and `parameter_description_max_len`
  through `SourceMetadata` instead of a lenient `serde_json::Value`, so the
  strict reader is the only reader of the sidecar.

## Sweep

`rg -n 'deny_unknown_fields' packages --glob '*.rs'`, then each hit checked
for whether it reads probe-written or C/C++-written JSON. The only such reader
is `SourceMetadata` and the `schema.rs` types it nests (`SourceName`,
`InterfaceRef`, `QosProfile`, `SourceLocation`), all covered by the golden
round-trip. The other probe reader, `leaf_entity_env::declaration_from_probe`,
is lenient by design (it refuses only a row it cannot read). The cmake-written
`nros-metadata.json` (`codegen/entry/metadata.rs`) is not `deny_unknown_fields`.

## Resolution (2026-11-18)

Fixed by PR #4190 (`9afb5c8a19`, merged 2026-11-18), which landed the four
changes under "Fix" above. Checked on main:

- `SourceMetadata` carries `parameter_description_max_len` and
  `parameter_constraints_max_len` as `Option<u64>`
  (`packages/cli/nros-cli-core/src/orchestration/source_metadata.rs:26-31`).
- No `let _ = stamp_provenance` is left under `packages/cli`
  (`git grep -nE 'let _ = stamp_provenance' -- packages/cli` is empty).
- `cargo test -p nros-cli-core --lib source_metadata` passes 3 of 3 on a
  tree containing `9afb5c8a19`, including
  `golden_probe_sidecar_round_trips_through_source_metadata`, which parses
  the producer-written golden sidecar strictly.
- The symptom reproduces without the fix and goes away with it: a
  `lane=tier2` fixture build on a branch based before `9afb5c8a19` failed
  every Zephyr C++ violation-entry configure at `NanoRosPromise.cmake:402`
  (`_nros_promise_check_unseeded`), the failure this issue names.
