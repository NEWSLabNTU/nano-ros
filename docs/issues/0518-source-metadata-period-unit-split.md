---
id: 518
title: "`SourceMetadata` cannot be parsed at all — the timer period is `period_us`
  on the producer and `period_ms` on the reader"
status: open
type: bug
area: orchestration
related: [issue-0505, phase-307]
---

## Symptom

`cargo test -p nros-cli-core --test plan_pipeline_e2e` — two reds on `main`:

```
metadata_build_discovers_missing_sources ... FAILED
metadata_mode_build_emits_source_metadata_for_component ... FAILED

valid SourceMetadata: Error("unknown field `period_us`, expected one of
  `id`, `declaration_slot`, `period_ms`, `callback`, `callback_slot`")
```

Reproduced on a clean tree (`git stash` of all unrelated work), so this is
not an interaction with in-flight changes.

`SourceTimer` is `#[serde(deny_unknown_fields)]`, so this is a hard parse
failure of the whole sidecar, not a dropped field.

## Cause

`#505` ("microsecond timer resolution end to end") moved the producer to
microseconds:

* `packages/core/nros-macros/src/main_macro.rs:2916` emits `period_us`
* `packages/cli/nros-cli-core/src/orchestration/source_metadata.rs:96`
  still declares `pub period_ms: u64`

`source_metadata.rs` was last touched in `1a2eb87cb`, long before #505 — the
migration changed the writer and not this reader.

## Why the one-line fix is NOT the fix

Renaming the field to `period_us` makes those two tests fail the other way:

```
unknown field `period_ms`, expected one of `id`, `declaration_slot`, `period_us`, …
```

because a committed fixture still uses the old spelling —
`nros-cli-core/tests/fixtures/orchestration/source_metadata_talker.json:45`
(`"period_ms": 100`). So the tree currently holds **both** spellings and the
reader can satisfy only one of them at a time.

A complete fix needs, together:

1. `SourceTimer.period_us: u64`;
2. `source_metadata_talker.json` regenerated (or hand-updated) to `period_us`,
   with the value converted — `100` ms is `100000` us, and a straight rename
   would silently make the fixture assert a 100 µs timer;
3. a sweep of the other committed `tests/fixtures/orchestration/plan_*.json`
   to confirm their `period_ms` belongs to `sched_contexts` (a different
   struct, `Option<u64>`, apparently still milliseconds) and is genuinely out
   of scope — nine files match the grep and only one is a `SourceTimer`.

Left to #505's author rather than guessed at here: whether `sched_contexts`
is also meant to move to microseconds is a design question, and step 2's unit
conversion is exactly the kind of thing that is silently wrong if someone
outside the change makes it.

## Note on scope

`SourceTimer` has no field readers anywhere — it is only declared and held as
`Vec<SourceTimer>`. That is why the mismatch survived review: nothing except
the deserializer itself ever looks at the field, so only a test that parses a
real sidecar catches it.

Found while landing phase-348 W3; unrelated to that work.
